//! `emath meaning list|set|unset|explain`: project-local interpretation locks.

use super::genesis_cmd::{self, Analysis};
use super::{usage, EXIT_OK, EXIT_REFUSED};
use emath_artifact::JsonWriter;
use emath_portfolio::{
    evaluate, refuse_disqualified, Authority, InterpretationPolicy, LockEntry, LockError, LockKey,
    MeaningLock, MetricAxis, MetricPolarity, SelectionMethod, WorldCandidate,
    DEFAULT_PORTFOLIO_CAP, PROVENANCE_USER_LOCKED, WHOLE_TERM_HOLE,
};
use emath_world_ir::WorldIr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Dispatch `emath meaning …`.
pub fn dispatch(args: &[String]) -> u8 {
    match args.first().map(String::as_str) {
        Some("list") => list_cmd(&args[1..]),
        Some("set") => set_cmd(&args[1..]),
        Some("unset") => unset_cmd(&args[1..]),
        Some("explain") => explain_cmd(&args[1..]),
        _ => usage("meaning list|set|unset|explain"),
    }
}

/// Refuse when an existing lock file next to `source` does not parse.
pub fn refuse_malformed_project_lock(source: &Path) -> Option<u8> {
    let root = MeaningLock::discover_project_root(source);
    match MeaningLock::load(&root) {
        Ok(_) => None,
        Err(error) => {
            eprintln!("{error}");
            Some(EXIT_REFUSED)
        }
    }
}

fn list_cmd(args: &[String]) -> u8 {
    let dir = flag_path("--dir", args).unwrap_or_else(|| PathBuf::from("."));
    let json = args.iter().any(|arg| arg == "--json");
    match MeaningLock::load(&dir) {
        Ok(None) => {
            if json {
                print!("{}", empty_lock_json(&dir));
            } else {
                println!(
                    "no meaning lock under {}",
                    MeaningLock::path(&dir).display()
                );
            }
            EXIT_OK
        }
        Ok(Some(lock)) => {
            if json {
                print!("{}", lock.encode());
            } else {
                println!(
                    "lock_id {:016x} cap {} provenance {PROVENANCE_USER_LOCKED}",
                    lock.lock_id, lock.portfolio_cap
                );
                if lock.entries.is_empty() {
                    println!("entries 0");
                }
                for (key, entry) in &lock.entries {
                    println!(
                        "  {} {} -> {:016x} receipt {:016x} method {} source {}",
                        hex(key.declaration_id),
                        key.hole_id,
                        entry.world_fingerprint,
                        entry.portfolio_receipt_id,
                        entry.selection_method.as_str(),
                        entry.source
                    );
                    println!(
                        "    emath meaning set {} --world {:016x}",
                        entry.source, entry.world_fingerprint
                    );
                }
            }
            EXIT_OK
        }
        Err(error) => {
            eprintln!("{error}");
            EXIT_REFUSED
        }
    }
}

fn set_cmd(args: &[String]) -> u8 {
    let Some(file) = first_positional(args) else {
        return usage("meaning set <file.emath> --world <name-or-fingerprint> [--dir <dir>]");
    };
    let Some(world) = flag_value("--world", args) else {
        return usage("meaning set <file.emath> --world <name-or-fingerprint> [--dir <dir>]");
    };
    let dir = flag_path("--dir", args).unwrap_or_else(|| MeaningLock::discover_project_root(&file));
    let hole = flag_value("--hole", args).unwrap_or_else(|| WHOLE_TERM_HOLE.to_string());
    let cap = match flag_value("--cap", args) {
        Some(text) => match text.parse::<u32>() {
            Ok(value) if value > 0 => value,
            _ => {
                eprintln!("error: E-LOCK-001: --cap must be an integer >= 1");
                return EXIT_REFUSED;
            }
        },
        None => DEFAULT_PORTFOLIO_CAP,
    };
    let analysis = match genesis_cmd::analyze(&file) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let chosen = match find_world(&worlds, &world) {
        Ok(world) => world,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    let fingerprint = chosen.identity().0;
    let candidates = world_candidates(&worlds);
    let receipt = match evaluate(
        candidates,
        vec![MetricAxis::new("cost", MetricPolarity::Minimize)],
        InterpretationPolicy::Portfolio,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    if let Err(error) = refuse_disqualified(fingerprint, &receipt) {
        eprintln!("{error}");
        return EXIT_REFUSED;
    }
    let mut lock = match MeaningLock::load(&dir) {
        Ok(Some(lock)) => lock,
        Ok(None) => MeaningLock::with_cap(cap),
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    if flag_value("--cap", args).is_some() {
        lock.portfolio_cap = cap;
    }
    lock.upsert(
        LockKey {
            declaration_id: analysis.term_id,
            hole_id: hole,
        },
        LockEntry {
            source: path_label(&file),
            source_hash: analysis.source_hash,
            world_fingerprint: fingerprint,
            portfolio_receipt_id: receipt.receipt_id,
            selection_method: SelectionMethod::CliSet,
            selected_at: now_secs(),
        },
    );
    if let Err(error) = lock.save(&dir) {
        eprintln!("{error}");
        return EXIT_REFUSED;
    }
    println!(
        "locked {} -> {:016x} ({}) lock_id {:016x} provenance {PROVENANCE_USER_LOCKED}",
        path_label(&file),
        fingerprint,
        chosen.name,
        lock.lock_id
    );
    println!(
        "hint: teams MAY commit {} to share one interpretation; default is local-side",
        MeaningLock::path(&dir).display()
    );
    EXIT_OK
}

fn unset_cmd(args: &[String]) -> u8 {
    let dir = flag_path("--dir", args).unwrap_or_else(|| {
        first_positional(args)
            .as_ref()
            .map(|path| MeaningLock::discover_project_root(path))
            .unwrap_or_else(|| PathBuf::from("."))
    });
    if let Some(declaration) = flag_value("--declaration", args) {
        let hole = flag_value("--hole", args).unwrap_or_else(|| WHOLE_TERM_HOLE.to_string());
        let declaration_id = match u64::from_str_radix(&declaration, 16) {
            Ok(value) if declaration.len() == 16 => value,
            _ => {
                eprintln!("error: E-LOCK-001: --declaration must be 16 hex digits");
                return EXIT_REFUSED;
            }
        };
        let mut lock = match MeaningLock::load(&dir) {
            Ok(Some(lock)) => lock,
            Ok(None) => {
                println!(
                    "no meaning lock under {}",
                    MeaningLock::path(&dir).display()
                );
                return EXIT_OK;
            }
            Err(error) => {
                eprintln!("{error}");
                return EXIT_REFUSED;
            }
        };
        let key = LockKey {
            declaration_id,
            hole_id: hole,
        };
        if !lock.unset(&key) {
            eprintln!(
                "error: E-LOCK-006: no lock entry for {} {}",
                hex(declaration_id),
                key.hole_id
            );
            return EXIT_REFUSED;
        }
        if lock.entries.is_empty() {
            if let Err(error) = MeaningLock::remove_file(&dir) {
                eprintln!("{error}");
                return EXIT_REFUSED;
            }
        } else if let Err(error) = lock.save(&dir) {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
        println!("unset {} {}", hex(declaration_id), key.hole_id);
        return EXIT_OK;
    }
    if let Err(error) = MeaningLock::remove_file(&dir) {
        eprintln!("{error}");
        return EXIT_REFUSED;
    }
    println!("unset {}", MeaningLock::path(&dir).display());
    EXIT_OK
}

fn explain_cmd(args: &[String]) -> u8 {
    let file = first_positional(args);
    let dir = flag_path("--dir", args).unwrap_or_else(|| {
        file.as_ref()
            .map(|path| MeaningLock::discover_project_root(path))
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let json = args.iter().any(|arg| arg == "--json");
    let lock = match MeaningLock::load(&dir) {
        Ok(None) => {
            if json {
                print!("{}", empty_lock_json(&dir));
            } else {
                println!("no meaning lock; genesis/eval/compile use the interpretation portfolio");
            }
            return EXIT_OK;
        }
        Ok(Some(lock)) => lock,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    if json {
        print!("{}", lock.encode());
        return EXIT_OK;
    }
    println!(
        "lock_id {:016x} cap {} provenance {PROVENANCE_USER_LOCKED} (authority is never raised)",
        lock.lock_id, lock.portfolio_cap
    );
    if let Some(file) = file {
        let analysis = match genesis_cmd::analyze(&file) {
            Ok(analysis) => analysis,
            Err(error) => {
                eprintln!("error: {error}");
                return EXIT_REFUSED;
            }
        };
        match lock.resolve(analysis.term_id, WHOLE_TERM_HOLE, &path_label(&file)) {
            Ok(Some(entry)) => {
                println!(
                    "matches {} -> {:016x} receipt {:016x}",
                    path_label(&file),
                    entry.world_fingerprint,
                    entry.portfolio_receipt_id
                );
            }
            Ok(None) => println!(
                "{} is not locked; portfolio ranking still applies",
                path_label(&file)
            ),
            Err(error) => {
                eprintln!("{error}");
                return EXIT_REFUSED;
            }
        }
    } else {
        for (key, entry) in &lock.entries {
            println!(
                "  {} {} source {} fp {:016x}",
                hex(key.declaration_id),
                key.hole_id,
                entry.source,
                entry.world_fingerprint
            );
        }
    }
    println!("drift or a tampered fingerprint is a typed E-LOCK refusal, never a silent fallback");
    EXIT_OK
}

fn find_world<'a>(worlds: &'a [WorldIr], token: &str) -> Result<&'a WorldIr, String> {
    if let Some(world) = worlds.iter().find(|world| world.name == token) {
        return Ok(world);
    }
    if token.len() == 16 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let fingerprint = u64::from_str_radix(token, 16)
            .map_err(|_| format!("error: E-LOCK-001: invalid world fingerprint `{token}`"))?;
        worlds
            .iter()
            .find(|world| world.identity().0 == fingerprint)
            .ok_or_else(|| LockError::UnknownCandidate { fingerprint }.to_string())
    } else {
        Err(format!("error: E-GEN-092: unknown world `{token}`"))
    }
}

fn world_candidates(worlds: &[WorldIr]) -> Vec<WorldCandidate> {
    worlds
        .iter()
        .map(|world| {
            WorldCandidate::bag_member(world.identity().0, "builtin-seed", Authority::Structural)
        })
        .collect()
}

fn empty_lock_json(dir: &Path) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.meaning-lock");
    object.string("path", &MeaningLock::path(dir).display().to_string());
    object.bool("present", false);
    object.finish()
}

fn first_positional(args: &[String]) -> Option<PathBuf> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            return args.get(index + 1).map(PathBuf::from);
        }
        if arg.starts_with('-') {
            if flag_takes_value(arg) {
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        return Some(PathBuf::from(arg));
    }
    None
}

fn flag_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--dir" | "--world" | "--hole" | "--declaration" | "--cap" | "--out" | "-o"
    )
}

fn flag_value(name: &str, args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn flag_path(name: &str, args: &[String]) -> Option<PathBuf> {
    flag_value(name, args).map(PathBuf::from)
}

fn path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| path.display().to_string(), ToOwned::to_owned)
}

fn hex(value: u64) -> String {
    format!("{value:016x}")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Shared lock resolution for genesis/eval/compile.
pub(crate) fn resolve_locked_worlds(
    path: &Path,
    analysis: &Analysis,
    all_worlds: Vec<WorldIr>,
) -> Result<LockedWorlds, String> {
    let root = MeaningLock::discover_project_root(path);
    let loaded = MeaningLock::load(&root).map_err(|error| error.to_string())?;
    let cap = match (&loaded, analysis.file.keep_pareto) {
        (_, Some(keep)) => keep,
        (Some(lock), None) => lock.portfolio_cap,
        (None, None) => DEFAULT_PORTFOLIO_CAP,
    };
    let Some(lock) = loaded else {
        return Ok(LockedWorlds {
            worlds: all_worlds,
            cap,
            lock: None,
        });
    };
    match lock.resolve(analysis.term_id, WHOLE_TERM_HOLE, &path_label(path)) {
        Ok(None) => Ok(LockedWorlds {
            worlds: all_worlds,
            cap,
            lock: None,
        }),
        Ok(Some(entry)) => {
            let world = all_worlds
                .iter()
                .find(|world| world.identity().0 == entry.world_fingerprint)
                .cloned()
                .ok_or_else(|| {
                    LockError::Drifted {
                        fingerprint: entry.world_fingerprint,
                        detail: "locked fingerprint is not among current world identities"
                            .to_string(),
                    }
                    .to_string()
                })?;
            Ok(LockedWorlds {
                worlds: vec![world],
                cap,
                lock: Some(ResolvedLock {
                    lock_id: lock.lock_id,
                    origin_receipt_id: entry.portfolio_receipt_id,
                    fingerprint: entry.world_fingerprint,
                    method: entry.selection_method.as_str().to_string(),
                }),
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

#[derive(Clone)]
pub(crate) struct LockedWorlds {
    pub worlds: Vec<WorldIr>,
    pub cap: u32,
    pub lock: Option<ResolvedLock>,
}

#[derive(Clone)]
pub(crate) struct ResolvedLock {
    pub lock_id: u64,
    pub origin_receipt_id: u64,
    pub fingerprint: u64,
    pub method: String,
}
