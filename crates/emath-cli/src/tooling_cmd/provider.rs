//! Provider and fork management commands.

use super::*;

/// `provider list|inspect <id>|test <id>`.
pub(crate) fn provider_cmd(request: ProviderRequest) -> CliExit {
    match request {
        ProviderRequest::List { json } => {
            if json {
                let mut rows = Vec::new();
                for (id, capability, status) in PROVIDERS {
                    let mut row = JsonWriter::object();
                    row.string("id", id);
                    row.string("capability", capability);
                    row.string("status", status);
                    rows.push(row.finish());
                }
                let mut object = JsonWriter::object();
                object.string("schema", "emath.provider-list");
                object.objects("providers", &rows);
                println!("{}", object.finish());
            } else {
                for (id, capability, status) in PROVIDERS {
                    println!("provider {id}: {capability} [{status}]");
                }
            }
            EXIT_OK
        }
        ProviderRequest::Inspect { id } => {
            let Some((_, capability, status)) =
                PROVIDERS.iter().find(|(candidate, _, _)| *candidate == id)
            else {
                eprintln!("error: E-TLT-016: unknown provider `{id}`");
                if let Some(hint) = suggest_provider(&id) {
                    eprintln!("did you mean `emath provider inspect {hint}`?");
                }
                return EXIT_USAGE;
            };
            // Descriptor is a JSON document either way (`--json` is independently legal).
            let mut object = JsonWriter::object();
            object.string("schema", "emath.provider-descriptor");
            object.string("id", &id);
            object.string("capability", capability);
            object.string("status", status);
            println!("{}", object.finish());
            EXIT_OK
        }
        ProviderRequest::Test { id, json } => {
            if !PROVIDERS.iter().any(|(candidate, _, _)| *candidate == id) {
                eprintln!("error: E-TLT-016: unknown provider `{id}`");
                return EXIT_USAGE;
            }
            // No in-CLI battery exists; printing "ok" without running
            // anything would be a fake success (same as bench E-TLT-004).
            if json {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.provider-test");
                object.string("id", &id);
                object.string("code", "E-TLT-013");
                object.bool("ok", false);
                println!("{}", object.finish());
            }
            eprintln!(
                "error: E-TLT-013: provider `{id}` has no in-CLI negative-control battery; run `cargo test` against tests/emath-adapter-rumoca in the workspace"
            );
            EXIT_REFUSED
        }
    }
}

/// `fork status|sync [--dry-run]`.
pub(crate) fn fork_cmd(request: ForkRequest) -> CliExit {
    let lock = upstream_lock_path();
    let Ok(bytes) = std::fs::read(&lock) else {
        eprintln!(
            "error: E-TLT-007: upstream lock missing at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    if bytes.is_empty() {
        eprintln!(
            "error: E-TLT-007: upstream lock is empty at {}",
            lock.display()
        );
        return EXIT_USAGE;
    }
    let Ok(text) = String::from_utf8(bytes) else {
        eprintln!(
            "error: E-TLT-007: upstream lock is not valid UTF-8 at {}",
            lock.display()
        );
        return EXIT_USAGE;
    };
    match request {
        ForkRequest::Status { json } => {
            let ids = lock_ids(&text);
            let lock_id = content_id_of_str(&text).0;
            if json {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.fork-status");
                object.string("lock_id", &lock_id);
                object.int("pins", ids.len() as u64);
                object.strings("ids", &ids);
                object.bool("offline", true);
                println!("{}", object.finish());
            } else {
                for id in ids {
                    println!("fork {id}: pinned (lock {lock_id})");
                }
            }
            EXIT_OK
        }
        ForkRequest::Sync { dry_run, json } => {
            let pins = lock_ids(&text).len();
            if dry_run {
                if json {
                    let mut object = JsonWriter::object();
                    object.string("schema", "emath.fork-sync");
                    object.bool("dry_run", true);
                    object.int("pins", pins as u64);
                    object.bool("offline", true);
                    println!("{}", object.finish());
                } else {
                    println!("sync: dry-run: {pins} upstream pins unchanged (offline)");
                }
                EXIT_OK
            } else {
                if json {
                    let mut object = JsonWriter::object();
                    object.string("schema", "emath.fork-sync");
                    object.bool("dry_run", false);
                    object.bool("ok", false);
                    object.string("code", "E-TLT-006");
                    println!("{}", object.finish());
                }
                eprintln!(
                    "error: E-TLT-006: network/source sync is disabled in Phase 1 (offline-first); use --dry-run"
                );
                EXIT_REFUSED
            }
        }
    }
}

/// Extracts quoted `"id": "..."` values from the lock document.
pub(super) fn lock_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("\"id\": \"") else {
            continue;
        };
        if let Some(end) = rest.find('"') {
            ids.push(rest[..end].to_string());
        }
    }
    ids
}

pub(super) fn suggest_provider(unknown: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for (id, _, _) in PROVIDERS {
        if id == unknown {
            return Some(id);
        }
        let distance = id
            .chars()
            .zip(unknown.chars())
            .filter(|(a, b)| a != b)
            .count()
            + id.len().abs_diff(unknown.len());
        if distance <= 4 && best.is_none_or(|(_, current)| distance < current) {
            best = Some((id, distance));
        }
    }
    best.map(|(id, _)| id)
}

/// Maps a build error to the conventional exit class.
pub(crate) fn classify_build_error(error: &dyn std::fmt::Display) -> CliExit {
    let text = error.to_string();
    if text.contains("admission refused") {
        EXIT_REFUSED
    } else {
        EXIT_USAGE
    }
}
