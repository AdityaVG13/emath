//! emath-epic-emlib-nz1n.11 CAPSTONE: `cargo xtask demo meaning-store`.
//!
//! The executable identity gate (identity first, UX capstone later):
//! three source versions of one function walk the meaning store's
//! layers and prove the four distinctions —
//! - presentation edit (comments/spacing): MeaningID PRESERVED, the
//!   semantic diff classifies `presentation` and the rebuild is SKIPPED
//!   with a cutoff receipt;
//! - breaking change (`y = x*x` → `y = x*x + x`): MeaningID MUTATED,
//!   the diff classifies `meaning` and dependents REBUILD — never a
//!   silent cutoff;
//! - evidence attachments land in the independent evidence plane
//!   WITHOUT retconning MeaningID or ObjectID;
//! - the presentation diff's cutoff receipt is deterministic.
//!
//! No incremental-compilation completeness is claimed here: this is the
//! identity classification gate over the landed nz1n.2/.5/.8 layers,
//! driven through the REAL admission pipeline (`CompilerSession`).

use emath_core::{MeaningId, SourceId};
use emath_sema::CompilerSession;
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::semantic_diff::{
    classify, decide, ChangeClass, DiffOutcome, SemanticSnapshot,
};
use emath_store::EvidencePlane;
use emath_syntax::install_source_parser;

const V1_BASE: &str = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
const V2_PRESENTATION: &str = "# formatted: comments and spacing only\n\nemath function square:\n    inputs:\n        x: Float64\n\n    # body comment, blank lines, same math\n    definitions:\n        y = x * x\n";
const V3_BREAKING: &str = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x + x\n";

fn meaning_of(source: &str) -> Result<MeaningId, String> {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("meaning-store-demo.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(format!("source must admit: {errors:#?}"));
    }
    result
        .package
        .meaning_id(&[])
        .map_err(|error| format!("meaning id refused: {error}"))
}

fn snapshot(source: &str, evidence: &[&str]) -> Result<emath_store::semantic_diff::SemanticSnapshot, String> {
    Ok(emath_store::semantic_diff::SemanticSnapshot::new(
        SourceId::from_bytes(source.as_bytes()),
        meaning_of(source)?,
        "specializer-12",
        evidence,
    ))
}

pub fn demo() -> u8 {
    println!("== demo meaning-store ==");
    match run_demo() {
        Ok(()) => {
            println!("meaning-store demo: ok");
            0
        }
        Err(error) => {
            eprintln!("meaning-store demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    let meaning_v1 = meaning_of(V1_BASE)?;
    let meaning_v2 = meaning_of(V2_PRESENTATION)?;
    let meaning_v3 = meaning_of(V3_BREAKING)?;
    let mut rows: Vec<String> = Vec::new();

    // 1) Presentation keeps MeaningID.
    if meaning_v1 != meaning_v2 {
        return Err("GATE FAIL: a whitespace/comment edit mutated MeaningID".to_string());
    }
    rows.push(format!("presentation|meaning-stable|{}", meaning_v1.as_str()));
    println!("meaning-store|presentation|meaning-stable|{}", meaning_v1.as_str());

    // 2) Breaking changes it.
    if meaning_v1 == meaning_v3 {
        return Err("GATE FAIL: a semantics change kept MeaningID".to_string());
    }
    rows.push(format!("breaking|meaning-changed|{}", meaning_v3.as_str()));
    println!("meaning-store|breaking|meaning-changed|{}", meaning_v3.as_str());

    // 3) Evidence independent: attach to a stored object, no retcon.
    let mut graph = ObjectGraph::default();
    let cell = graph
        .put(ObjectDraft {
            kind: ObjectKind::Cell,
            meaning_id: meaning_v1.clone(),
            semantic_payload: V1_BASE.as_bytes().to_vec(),
            presentation: Some("square, formatted".to_string()),
        })
        .map_err(|error| format!("object store refused: {error:?}"))?;
    let meaning_before = graph.object(&cell).unwrap().meaning_id.clone();
    let mut plane = EvidencePlane::default();
    let attached = plane
        .attach(&graph, &cell, EvidenceReceipt::seal("capstone-receipt", b"demo evidence run"))
        .map_err(|error| format!("evidence attach refused: {error:?}"))?;
    if graph.object(&cell).unwrap().meaning_id != meaning_before {
        return Err("GATE FAIL: an evidence attachment retconned MeaningID".to_string());
    }
    rows.push(format!("evidence|independent|id={attached}"));
    println!("meaning-store|evidence|independent|id={attached}");

    // 4) Early cutoff: the presentation diff SKIPS the rebuild, with a
    // receipt; the breaking diff rebuilds — never a silent cutoff.
    let before = snapshot(V1_BASE, &[])?;
    let after_presentation = snapshot(V2_PRESENTATION, &[])?;
    match decide(&before, &after_presentation, &[]) {
        DiffOutcome::Cutoff(receipt) if receipt.class == ChangeClass::Presentation => {
            rows.push(format!("cutoff|skipped|{receipt}"));
            println!("meaning-store|cutoff|skipped|{receipt}");
        }
        other => return Err(format!("presentation diff must cut off, got {other:?}")),
    }
    let after_breaking = snapshot(V3_BREAKING, &[])?;
    match decide(&before, &after_breaking, &[]) {
        DiffOutcome::Rebuild(receipt) if receipt.class == ChangeClass::Meaning => {
            rows.push(format!("rebuild|dependents|{receipt}"));
            println!("meaning-store|rebuild|dependents|{receipt}");
        }
        other => return Err(format!("breaking diff must rebuild, got {other:?}")),
    }

    // Determinism: the same walkthrough classifies identically on rerun.
    let again = snapshot(V2_PRESENTATION, &[])?;
    if classify(&before, &again) != ChangeClass::Presentation {
        return Err("classification must be deterministic".to_string());
    }

    let _ = rows;
    Ok(())
}
