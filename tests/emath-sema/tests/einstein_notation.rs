//! Einstein notation — C11/C10 Phase-1 fence pins + einsum
//! canonical surface.
//!
//! Probed state on HEAD (this file's contracts are regression pins; the
//! discriminating-power mutation is logged in the compliance pack):
//! - `einsum("ik,kj->ij", A, B)` on Matrix inputs parses, types, admits —
//!   the explicit canonical form is already the admitted spelling.
//! - Bare index `S[ij]` on a rank-2 Matrix refuses `E-SHAPE-006`
//!   ("index requires 2 subscript(s), found 1"): the comma spelling
//!   `S[i, j]` is the only way to name two indices. No token surgery.
//! - A free/unknown index name on a rank-1 vector refuses `E-TYPE-002`
//!   (unknown variable) — never a silent single-index reinterpretation.
//! - Index-notation assignment `C[i, j] = A[i, k] * B[k, j]` refuses
//!   `E-SEQ-RECURRENCE` (a definition target carries at most one index —
//!   the sequence/recurrence law owns indexed definitions); pack-gated
//!   contraction semantics land with the einstein pack, whose `use`
//!   resolution is the imports lane.
//! - Concrete integer indexing `S[1, 2]` on a Matrix admits.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("einstein", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
            )
        })
        .collect()
}

fn errors(out: &[(String, String)]) -> Vec<&(String, String)> {
    out.iter()
        .filter(|(severity, _)| severity == "Error")
        .collect()
}

#[test]
fn einsum_canonical_form_admits() {
    let out = check(
        "emath function MatMul:\n    inputs:\n        A: Matrix[Float64]\n        B: Matrix[Float64]\n    outputs:\n        C: Matrix[Float64]\n    definitions:\n        C = einsum(\"ik,kj->ij\", A, B)\n",
    );
    assert!(
        errors(&out).is_empty(),
        "einsum canonical form must admit on Matrix inputs, got {:?}",
        out
    );
}

/// A bare index on rank 2 must refuse, and
/// the message must name the subscript-count mismatch (never a silent
/// two-index reading).
#[test]
fn bare_index_on_rank2_refuses_e_shape_006() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/einstein_bare_index.emath"
    ));
    assert!(
        fixture.contains("expect: E-SHAPE-006"),
        "fixture must pin E-SHAPE-006"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-SHAPE-006"),
        "bare index on rank-2 must refuse E-SHAPE-006, got {errs:?}"
    );
    assert!(
        out.iter().any(|(_, code)| code == "E-SHAPE-006"),
        "subscript-count rule must fire"
    );
}

/// Free/unknown index on rank-1: unknown name refuses as E-TYPE-002 —
/// there is no ambient index namespace (sampling/indices are declared,
/// never ambient — same doctrine as the pack's opt-in rule).
#[test]
fn unknown_index_name_on_rank1_refuses() {
    let out = check(
        "emath function Vec:\n    inputs:\n        v: Vector[Float64]\n    outputs:\n        t: Float64\n    definitions:\n        t = v[ij]\n",
    );
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-TYPE-002"),
        "unknown index name must refuse E-TYPE-002, got {errs:?}"
    );
}

/// Index-notation assignment (`C_idx[i, j] = A[i, k] * B[k, j]`) is
/// fenced by the sequence law: a definition target carries at most one
/// index (E-SEQ-RECURRENCE); pack-gated contraction lands later.
#[test]
fn index_notation_assignment_is_phase1_fenced() {
    let out = check(
        "emath function MatMul2:\n    inputs:\n        A: Matrix[Float64]\n        B: Matrix[Float64]\n    outputs:\n        C: Matrix[Float64]\n    definitions:\n        C[i, j] = A[i, k] * B[k, j]\n",
    );
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-SEQ-RECURRENCE"),
        "multi-index definition target must refuse E-SEQ-RECURRENCE, got {errs:?}"
    );
}

/// Concrete integer indexing keeps admitting — C11 must not over-fire.
#[test]
fn concrete_integer_indexing_still_admits() {
    let out = check(
        "emath function Pick:\n    inputs:\n        S: Matrix[Float64]\n    outputs:\n        T: Float64\n    definitions:\n        T = S[1, 2]\n",
    );
    assert!(
        errors(&out).is_empty(),
        "concrete integer indexing must admit, got {:?}",
        out
    );
}
