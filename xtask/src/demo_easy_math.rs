//! `cargo xtask demo easy-math`.

use std::path::Path;

use emath_store::{PackBudgets, PackEntry, PackReader, PackWriter};

const SOURCE: &str = "language/examples/intro/easy-math.emath";

pub fn demo() -> u8 {
    println!("== demo easy-math ==");
    match run_demo() {
        Ok(()) => {
            println!("easy-math demo: ok");
            0
        }
        Err(error) => {
            eprintln!("easy-math demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    let source = std::fs::read_to_string(SOURCE).map_err(|error| format!("read: {error}"))?;
    let expansion = emath_syntax::expand_scratch(&source);
    if expansion.diagnostics.has_errors() {
        return Err(format!("desugar refused: {:?}", expansion.diagnostics));
    }
    let (_, diagnostics) = emath_syntax::parse_str(&expansion.expanded);
    if diagnostics.has_errors() {
        return Err(format!("expanded source refused: {diagnostics:?}"));
    }
    println!(
        "easy-math|desugar|{}",
        if expansion.rewritten() {
            "expanded"
        } else {
            "already-explicit"
        }
    );

    let temp = std::env::temp_dir().join(format!("emath-easy-math-{}", std::process::id()));
    std::fs::create_dir_all(&temp).map_err(|error| format!("temp: {error}"))?;
    let frozen = temp.join("easy-math.emath");
    let frozen_source = expansion.expanded.as_bytes();
    std::fs::write(&frozen, frozen_source).map_err(|error| format!("freeze source: {error}"))?;
    let lock = frozen.with_extension("freeze.lock.json");
    let lock_document = format!(
        "{{\n  \"schema\": \"emath.freeze.lock.v1\",\n  \"meaning_id\": \"{}\",\n  \"authority_raised\": false\n}}\n",
        emath_core::MeaningId::from_bytes(frozen_source)
    );
    std::fs::write(&lock, lock_document).map_err(|error| format!("freeze lock: {error}"))?;
    if !Path::new(&lock).is_file() {
        return Err("freeze lock missing".to_string());
    }
    println!("easy-math|freeze|ok");

    let frozen_bytes = std::fs::read(&frozen).map_err(|error| format!("frozen: {error}"))?;
    let lock_bytes = std::fs::read(&lock).map_err(|error| format!("lock: {error}"))?;
    let entries = vec![
        PackEntry::new("easy-math.emath", &frozen_bytes),
        PackEntry::new("easy-math.freeze.lock.json", &lock_bytes),
    ];
    let budgets = PackBudgets::draft();
    let pack = PackWriter::new(budgets)
        .write(&entries, None)
        .map_err(|error| format!("share: {error}"))?;
    if PackReader::new(budgets)
        .read(&pack, None)
        .map_err(|error| format!("verify share: {error}"))?
        != entries
    {
        return Err("shared .emlib did not round-trip".to_string());
    }
    println!("easy-math|share|{} bytes|ok", pack.len());
    Ok(())
}
