//! Artifact inspection: verify, inspect, diff, fingerprinting.

use super::*;

/// `verify <dir>`: independent artifact re-verification.
pub(crate) fn verify_cmd(dir: &Path) -> CliExit {
    artifact_check(dir)
}

/// `inspect <dir>`: print the committed artifact manifest; refuses
/// non-UTF-8 manifests instead of substituting lossy text.
pub(crate) fn inspect_cmd(dir: &Path, json: bool) -> CliExit {
    let root = dir.join("emath");
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!(
            "error: E-TLT-005: no `emath/` state directory under {}",
            dir.display()
        );
        return EXIT_USAGE;
    };
    let mut inspected: u64 = 0;
    let mut manifests = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let manifest = entry.path().join("emath/artifact-manifest.json");
        let bytes = match std::fs::read(&manifest) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!(
                    "error: E-TLT-005: cannot read manifest at {}: {error}",
                    manifest.display()
                );
                return EXIT_REFUSED;
            }
        };
        let Ok(text) = String::from_utf8(bytes) else {
            eprintln!(
                "error: E-EVID-114: manifest is not valid UTF-8 at {}",
                manifest.display()
            );
            return EXIT_REFUSED;
        };
        if json {
            manifests.push(text);
        } else {
            println!("artifact {}:", entry.file_name().to_string_lossy());
            println!("{text}");
        }
        inspected += 1;
    }
    if inspected == 0 {
        eprintln!("error: E-TLT-005: no artifacts under {}", root.display());
        EXIT_USAGE
    } else if json {
        let mut object = JsonWriter::object();
        object.string("schema", "emath.inspect");
        object.string("dir", &dir.display().to_string());
        object.int("count", inspected);
        object.objects("manifests", &manifests);
        println!("{}", object.finish());
        EXIT_OK
    } else {
        EXIT_OK
    }
}

/// `diff <a.emath> <b.emath>`: fingerprint comparison of parse-admitted sources.
pub(crate) fn diff_cmd(a: &Path, b: &Path, json: bool) -> CliExit {
    let id_a = fingerprint(a);
    let id_b = fingerprint(b);
    match (id_a, id_b) {
        (Ok(id_a), Ok(id_b)) => {
            let identical = id_a == id_b;
            if json {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.diff");
                object.string("a", &a.display().to_string());
                object.string("a_id", &id_a.0);
                object.string("b", &b.display().to_string());
                object.string("b_id", &id_b.0);
                object.bool("identical", identical);
                println!("{}", object.finish());
            } else {
                println!("diff: {} {}", a.display(), id_a.0);
                println!("diff: {} {}", b.display(), id_b.0);
                println!("diff: {}", if identical { "identical" } else { "differ" });
            }
            if identical { EXIT_OK } else { EXIT_REFUSED }
        }
        (Err(()), _) | (_, Err(())) => EXIT_REFUSED,
    }
}

/// Content id of a parse-admitted source; diagnostics printed on refusal.
pub(super) fn fingerprint(file: &Path) -> Result<emath_core::ContentId, ()> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(file) else {
        eprintln!("error: cannot read {}", file.display());
        return Err(());
    };
    let result = session.check(package.file);
    print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return Err(());
    }
    let Ok(bytes) = std::fs::read(file) else {
        eprintln!("error: cannot read {}", file.display());
        return Err(());
    };
    // Bind the bytes, not a lossy decode: non-UTF-8 stays distinct.
    Ok(emath_core::bootstrap_content_id(&bytes))
}
