//! `cargo xtask demo portable-emlib`.
//!
//! The offline share-unit walkthrough: create (canonical bytes),
//! verify (corruption refuses by name), mount into a fresh space,
//! thin-pack against a parent, then REJECT a corrupt pack. No network,
//! no daemon: every layer is in-memory (format + spaces
//! + graph).

use std::sync::Arc;

use emath_core::MeaningId;
use emath_store::Space;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::pack::{PackBudgets, PackEntry, PackReader, PackWriter};

const MAGIC: &[u8] = b"EMATHLIB\0";

fn draft(meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind: ObjectKind::Cell,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

pub fn demo() -> u8 {
    println!("== demo portable-emlib ==");
    match run_demo() {
        Ok(()) => {
            println!("portable-emlib demo: ok");
            0
        }
        Err(error) => {
            eprintln!("portable-emlib demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    let budgets = PackBudgets::draft();

    // 1) CREATE: canonical pack from the library objects.
    let entries = vec![
        PackEntry::new("emath:meaning:v1:cell-a", b"payload-a"),
        PackEntry::new("emath:meaning:v1:cell-b", b"payload-b"),
    ];
    let pack = PackWriter::new(budgets)
        .write(&entries, None)
        .map_err(|error| format!("create refused: {error}"))?;
    if !pack.starts_with(MAGIC) {
        return Err("created pack must carry the .emlib magic".to_string());
    }
    println!("portable-emlib|create|{} bytes|canonical", pack.len());

    // 2) VERIFY: the pack reads back identically.
    let read = PackReader::new(budgets)
        .read(&pack, None)
        .map_err(|error| format!("verify refused a fresh pack: {error}"))?;
    if read != entries {
        return Err("verify must round-trip the created entries".to_string());
    }
    println!("portable-emlib|verify|round-trip|ok");

    // 3) MOUNT: entries materialize in a FRESH space (offline).
    let mut graph = ObjectGraph::default();
    let mut mounted = Vec::new();
    for entry in &read {
        let id = graph
            .put(draft(&entry.id, "mounted from .emlib"))
            .map_err(|error| format!("mount refused: {error:?}"))?;
        mounted.push(id);
    }
    let space = Space::new("fresh-workbench", Arc::new(graph.clone()))
        .map_err(|error| format!("space refused: {error:?}"))?;
    let lock = emath_store::LibraryLock::from_snapshot(
        &space
            .snapshot()
            .map_err(|error| format!("snapshot refused: {error:?}"))?,
        mounted.clone(),
    );
    lock.verify(&graph)
        .map_err(|error| format!("mounted lock refused: {error:?}"))?;
    println!(
        "portable-emlib|mount|space={}|objects={}",
        space.name(),
        mounted.len()
    );

    // 4) THIN-PACK: delta against the parent; refuses without closure.
    let thin = PackWriter::new(budgets)
        .write(
            &[PackEntry::new("emath:meaning:v1:cell-c", b"payload-c")],
            Some("emath:meaning:v1:cell-a"),
        )
        .map_err(|error| format!("thin-pack refused: {error}"))?;
    if PackReader::new(budgets).read(&thin, None).is_ok() {
        return Err("a thin pack must refuse to read without its parent closure".to_string());
    }
    let merged = PackReader::new(budgets)
        .read(&thin, Some(&pack))
        .map_err(|error| format!("thin-pack merge refused: {error}"))?;
    if merged.len() != 3 {
        return Err(format!(
            "thin merge must carry 3 cells, got {}",
            merged.len()
        ));
    }
    println!("portable-emlib|thin-pack|merged|{}", merged.len());

    // 5) CORRUPT REJECT: a truncated pack refuses by name; a mutated
    // pack's identity moves (re-serialization cannot reproduce the
    // original bytes).
    let truncated = &pack[..pack.len() - 5];
    match PackReader::new(budgets).read(truncated, None) {
        Err(error) => println!("portable-emlib|corrupt|refused|{error:.60}"),
        Ok(_) => return Err("a truncated pack must refuse — silent mount is a FAIL".to_string()),
    }
    let mut mutated = pack.clone();
    let last = mutated.len() - 1;
    mutated[last] ^= 0x01;
    let mutated_read = PackReader::new(budgets)
        .read(&mutated, None)
        .map_err(|error| format!("mutated pack refused: {error}"))?;
    let resealed = PackWriter::new(budgets)
        .write(&mutated_read, None)
        .map_err(|error| format!("re-seal refused: {error}"))?;
    if resealed == pack {
        return Err("a mutated pack must not re-seal to the original bytes".to_string());
    }
    println!("portable-emlib|tamper|visible|identity-moved");

    Ok(())
}
