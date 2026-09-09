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

use emath_test_harness::{Probe, Source, boot};

#[test]
fn einstein_notation() {
    boot();
    let mut p = Probe::new("einsum admits, bare/unknown/multi-index refuse typed, concrete admits");
    p.case("einsum-admits", |p| {
        Source::from_str(
            "einsum",
            "emath function MatMul:\n    inputs:\n        A: Matrix[Float64]\n        B: Matrix[Float64]\n    outputs:\n        C: Matrix[Float64]\n    definitions:\n        C = einsum(\"ik,kj->ij\", A, B)\n",
        )
        .must_admit(p);
    });
    p.case("bare-index-refuses", |p| {
        Source::from_workspace("tests/invalid/einstein_bare_index.emath")
            .must_refuse(p, &["E-SHAPE-006"]);
    });
    p.case("unknown-index-refuses", |p| {
        Source::from_str(
            "vec-unknown-index",
            "emath function Vec:\n    inputs:\n        v: Vector[Float64]\n    outputs:\n        t: Float64\n    definitions:\n        t = v[ij]\n",
        )
        .must_refuse(p, &["E-TYPE-002"]);
    });
    p.case("index-assign-fenced", |p| {
        Source::from_str(
            "matmul-index-assign",
            "emath function MatMul2:\n    inputs:\n        A: Matrix[Float64]\n        B: Matrix[Float64]\n    outputs:\n        C: Matrix[Float64]\n    definitions:\n        C[i, j] = A[i, k] * B[k, j]\n",
        )
        .must_refuse(p, &["E-SEQ-RECURRENCE"]);
    });
    p.case("concrete-index-admits", |p| {
        Source::from_str(
            "pick",
            "emath function Pick:\n    inputs:\n        S: Matrix[Float64]\n    outputs:\n        T: Float64\n    definitions:\n        T = S[1, 2]\n",
        )
        .must_admit(p);
    });
    p.finish();
}
