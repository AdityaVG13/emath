//! `emath freeze`, `why`, and assumptions commands.

use super::*;

pub(super) fn freeze_lock_json(
    source: &str,
    frozen: &str,
    ledger: &emath_syntax::ExactnessLedger,
    meaning_id: &emath_core::MeaningId,
) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.freeze.lock.v1");
    object.string(
        "source_content_id",
        &emath_core::content_id_of_str(source).0,
    );
    object.string(
        "frozen_content_id",
        &emath_core::content_id_of_str(frozen).0,
    );
    object.string("meaning_id", meaning_id.as_str());
    object.bool("authority_raised", false);
    object.string("prelude", "scratch-v1");
    let none: Vec<String> = Vec::new();
    object.strings("packages", &none);
    object.strings("methods", &none);
    object.string("numeric_policy", "strict-f64");
    object.strings("providers", &["native.rust".to_string()]);
    let open: Vec<String> = ledger
        .open_holes()
        .into_iter()
        .map(|entry| format!("{}:{}", entry.dimension.as_str(), entry.name))
        .collect();
    object.strings("open", &open);
    let rows: Vec<String> = ledger
        .entries
        .iter()
        .map(|entry| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("id", &entry.inference_id);
            row.string("dimension", entry.dimension.as_str());
            row.string("status", entry.status.as_str());
            row.string("name", &entry.name);
            row.finish().trim_end().to_string()
        })
        .collect();
    object.objects("ledger", &rows);
    object.finish()
}

pub(super) fn write_via_rename(path: &Path, bytes: &str) -> bool {
    let mut tmp = path.to_path_buf();
    tmp.as_mut_os_string().push(".tmp");
    let ok = std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
    ok
}

pub(super) fn sidecar_lock_path(out: &Path) -> PathBuf {
    let mut lock_path = out.to_path_buf();
    match lock_path.extension().and_then(|ext| ext.to_str()) {
        Some("emath") | Some("lock") => {
            lock_path.set_extension("freeze.lock.json");
        }
        Some(ext) => {
            lock_path.set_extension(format!("{ext}.freeze.lock.json"));
        }
        None => {
            lock_path.set_extension("freeze.lock.json");
        }
    }
    lock_path
}

pub(super) enum FreezeRequest {
    Ready {
        path: PathBuf,
        out: Option<PathBuf>,
        json: bool,
    },
}

pub(super) fn parse_freeze_request(args: &[String]) -> Option<FreezeRequest> {
    let mut path = None;
    let mut out = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let path = path?;
    Some(FreezeRequest::Ready { path, out, json })
}

pub(super) fn freeze_cmd(request: FreezeRequest) -> CliExit {
    let FreezeRequest::Ready { path, out, json } = request;
    let source = match read_emath_source("freeze", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let expansion = emath_syntax::expand_scratch(&source);
    if expansion
        .diagnostics
        .items()
        .iter()
        .any(|item| item.code == "E-SYN-147")
    {
        eprintln!(
            "E-SYN-147 claiming exactness while holes remain open is refused; freeze does not upgrade authority"
        );
        return EXIT_REFUSED;
    }
    if expansion.diagnostics.has_errors() {
        print_diagnostics(&expansion.diagnostics);
        return EXIT_REFUSED;
    }
    let ledger = emath_syntax::exactness_ledger(&source);
    let Some(meaning_id) = admitted_meaning_id(&path, &expansion.expanded) else {
        eprintln!(
            "error: freeze requires admitted meaning; fix semantic diagnostics before freezing"
        );
        return EXIT_REFUSED;
    };
    let mut frozen = String::new();
    frozen.push_str("# emath freeze: does not raise evidence authority\n");
    for entry in ledger.open_holes() {
        frozen.push_str(&format!(
            "# emath freeze: open {} ({})\n",
            entry.dimension.as_str(),
            entry.name
        ));
    }
    frozen.push_str(&expansion.expanded);
    let lock = freeze_lock_json(&source, &frozen, &ledger, &meaning_id);
    if let Some(ref out) = out {
        if !write_via_rename(out, &frozen) {
            eprintln!("error: cannot write {}", out.display());
            return EXIT_USAGE;
        }
        let lock_path = sidecar_lock_path(out);
        if !write_via_rename(&lock_path, &lock) {
            eprintln!("error: cannot write {}", lock_path.display());
            let _ = std::fs::remove_file(out);
            return EXIT_USAGE;
        }
    }
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "freeze");
        object.bool("ok", !expansion.diagnostics.has_errors());
        object.bool("authority_raised", false);
        object.int(
            "open_holes",
            ledger.count(emath_syntax::ExactnessStatus::Open) as u64,
        );
        object.string("source", &source);
        object.string("frozen", &frozen);
        object.string("lock", &lock);
        println!("{}", object.finish());
    } else if out.is_none() {
        print!("{frozen}");
        print_missing_newline(&frozen);
        println!("--- emath.freeze.lock.v1 ---");
        print!("{lock}");
        print_missing_newline(&lock);
    }
    exit_from_diagnostics(expansion.diagnostics.has_errors())
}

pub(super) enum WhyRequest {
    Ready {
        path: PathBuf,
        needle: String,
        json: bool,
    },
}

pub(super) fn parse_why_request(args: &[String]) -> Option<WhyRequest> {
    let mut path = None;
    let mut json = false;
    let mut needle = None;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("inference:") => {
                assign_once(&mut needle, other.to_string())?
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
    }
    Some(WhyRequest::Ready {
        path: path?,
        needle: needle?,
        json,
    })
}

pub(super) fn why_cmd(request: WhyRequest) -> CliExit {
    let WhyRequest::Ready { path, needle, json } = request;
    let source = match read_emath_source("why", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let notes = emath_syntax::explanation_notes(&source);
    let Some(note) = notes.iter().find(|note| {
        note.inferred.starts_with(&needle) || note.inferred.contains(&format!(" {needle} "))
    }) else {
        let index = needle
            .strip_prefix("inference:")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1));
        if let Some(note) = index.and_then(|i| notes.get(i)) {
            print_why(note, json);
            return EXIT_OK;
        }
        eprintln!("error: no such inference `{needle}`");
        return EXIT_REFUSED;
    };
    print_why(note, json);
    EXIT_OK
}

pub(super) fn print_why(note: &emath_syntax::ScratchNote, json: bool) {
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "why");
        object.string("inferred", &note.inferred);
        object.string("rationale", &note.rationale);
        object.string("replacement", &note.replacement);
        object.string("stability", note.stability.as_str());
        println!("{}", object.finish());
    } else {
        println!("{} ({})", note.inferred, note.stability.as_str());
        println!("{}", note.rationale);
        println!("write: {}", note.replacement.replace('\n', " / "));
    }
}

pub(super) fn assumptions_cmd(path: &Path, json: bool) -> CliExit {
    let source = match read_emath_source("assumptions", path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let notes: Vec<_> = emath_syntax::explanation_notes(&source)
        .into_iter()
        .filter(|note| note.stability == ExactnessStatus::Inferred)
        .collect();
    if json {
        let mut rows = Vec::new();
        for note in &notes {
            let mut object = emath_artifact::JsonWriter::object();
            object.string("inferred", &note.inferred);
            object.string("rationale", &note.rationale);
            object.string("stability", note.stability.as_str());
            rows.push(object.finish().trim_end().to_string());
        }
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "assumptions");
        out.objects("notes", &rows);
        println!("{}", out.finish());
    } else {
        for note in &notes {
            println!("{} — {}", note.inferred, note.rationale);
        }
    }
    EXIT_OK
}
