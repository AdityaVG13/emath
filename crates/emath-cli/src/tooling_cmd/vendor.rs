//! The `emath vendor` dependency-staging command.

use super::*;

/// `vendor --out <dir>`: offline dependency snapshot (zero third-party deps).
pub(crate) fn vendor_cmd(out: &Path) -> CliExit {
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
    let entry_count = text.matches("      \"id\": \"").count();
    let mut object = JsonWriter::object();
    object.string("schema", "emath.vendor");
    object.string("source", UPSTREAM_LOCK_REL);
    let source_id = content_id_of_str(&text).0;
    object.string("source_id", &source_id);
    object.int("upstream_pins", u64::try_from(entry_count).unwrap_or(0));
    object.int("third_party_deps", 0);
    object.bool("offline", true);
    if std::fs::create_dir_all(out).is_err() {
        eprintln!("error: cannot create {}", out.display());
        return EXIT_USAGE;
    }
    let target = out.join("vendor-manifest.json");
    let body = object.finish();
    if std::fs::write(&target, body.clone()).is_err() {
        eprintln!("error: cannot write {}", target.display());
        return EXIT_USAGE;
    }
    println!("vendor: wrote {body}");
    EXIT_OK
}
