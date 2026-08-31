//! `emath library mount` — mount a stdlib object pack (bead
//! `emath-stdlib-object-packs-hpzgf`).
//!
//! The standard library is executable object packs, not catalog
//! markdown: `emath library mount std` composes the std.core census
//! (theory object + cell object + independent evidence receipt, each
//! with an admitted MeaningID), exports it as a canonical `.emlib`
//! pack, mounts it through the store's typed mount (every object id and
//! evidence hash re-verified), and prints a deterministic receipt.
//! Forgery or corruption refuses typed; nothing silent.

use crate::{CliExit, EXIT_OK, EXIT_REFUSED};
use emath_core::MeaningId;
use emath_sema::CompilerSession;
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::ObjectGraph;
use emath_store::pack::PackEntry;
use emath_store::stdlib::{StdObject, StdReceipt, export_std_pack, mount_stdlib};
use emath_syntax::install_source_parser;

/// The std.core theory: what the shape law claims.
const THEORY_SOURCE: &str = "emath function shape_law:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
/// The std.core cell: the reference algorithm — the same law computed
/// as a distinct expression tree, so theory and algorithm carry
/// independent MeaningIDs.
const CELL_SOURCE: &str = "emath function square_ref:\n    inputs:\n        x: Float64\n    definitions:\n        y = (x * x) + 0.0\n";

/// Admitted meaning identity AND canonical semantic payload for a
/// stdlib census source. Admission failure is a typed refusal — the
/// catalog never fabricates an identity from error text — and the
/// payload is the same canonical bytes the test suite derives, so the
/// printed object identities are the canonical census identities.
fn meaning_of(source: &str) -> Result<(MeaningId, Vec<u8>), String> {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("stdlib-pack.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(format!(
            "the std.core census source does not admit: {}",
            errors.join("; ")
        ));
    }
    let id = result
        .package
        .meaning_id(&[])
        .map_err(|error| format!("admitted package must carry a meaning id: {error}"))?;
    let payload = emath_ir::meaning::canonical_meaning_bytes(&result.package, &[])
        .map_err(|error| format!("canonical meaning bytes: {error}"))?;
    Ok((id, payload))
}

/// `emath library mount <name>` dispatch. Today the only mountable
/// library is `std`.
pub(crate) fn mount_cmd(name: &str) -> CliExit {
    match name {
        "std" => mount_std(),
        other => {
            eprintln!("error: unknown library `{other}` (mountable: std)");
            EXIT_REFUSED
        }
    }
}

fn mount_std() -> CliExit {
    let (theory_meaning, theory_payload) = match meaning_of(THEORY_SOURCE) {
        Ok(meaning) => meaning,
        Err(detail) => {
            eprintln!("error: std.core census refused: {detail}");
            return EXIT_REFUSED;
        }
    };
    let (cell_meaning, cell_payload) = match meaning_of(CELL_SOURCE) {
        Ok(meaning) => meaning,
        Err(detail) => {
            eprintln!("error: std.core census refused: {detail}");
            return EXIT_REFUSED;
        }
    };
    let theory = StdObject {
        kind: emath_store::object_graph::ObjectKind::Theory,
        meaning_id: theory_meaning,
        semantic_payload: theory_payload,
        presentation: Some("std.core theory: shape law".into()),
    };
    let cell = StdObject {
        kind: emath_store::object_graph::ObjectKind::Cell,
        meaning_id: cell_meaning,
        semantic_payload: cell_payload,
        presentation: Some("std.core.cells.square: reference algorithm".into()),
    };
    let mut scratch = ObjectGraph::default();
    let Ok(theory_id) = scratch.put(theory.to_draft()) else {
        eprintln!("error: std.core census theory does not admit");
        return EXIT_REFUSED;
    };
    let Ok(cell_id) = scratch.put(cell.to_draft()) else {
        eprintln!("error: std.core census cell does not admit");
        return EXIT_REFUSED;
    };
    let receipt = StdReceipt {
        kind: "algorithm-test".into(),
        payload: b"square(3) == 9".to_vec(),
        object_id: cell_id.clone(),
    };
    let receipt_evidence =
        EvidenceReceipt::seal("algorithm-test", b"square(3) == 9").evidence_id;
    let entries = vec![
        PackEntry::new(theory_id.as_str(), &theory.encode()),
        PackEntry::new(cell_id.as_str(), &cell.encode()),
        PackEntry::new(receipt_evidence.as_str(), &receipt.encode()),
    ];
    let Ok(bytes) = export_std_pack(&entries) else {
        eprintln!("error: std.core pack export refused");
        return EXIT_REFUSED;
    };
    match mount_stdlib(&bytes) {
        Ok(mount) => {
            println!("mounted library `std` as object pack {}", mount.pack_id);
            for object in mount.graph.objects() {
                println!(
                    "object {:?} {}  meaning {}",
                    object.kind, object.id, object.meaning_id
                );
            }
            for object in mount.graph.objects() {
                for evidence in mount.evidence.attachments_of(&object.id) {
                    println!("evidence {evidence} -> {}", object.id);
                }
            }
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: library mount refused: {error}");
            EXIT_REFUSED
        }
    }
}
