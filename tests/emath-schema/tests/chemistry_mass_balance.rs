//! Chemistry expressed as
//! `.emath` field-pack DATA over the existing generic nucleus — no Rust
//! chemistry crate, no domain-named evaluator, no chemistry VM branch.
//!
//! The `std.chem.mass_balance` cell is ONE registry entry: its
//! reference term is the generic `matvec(S, s)` (a name binding on the
//! EXISTING dense matrix×vector op), its params are the signed
//! stoichiometric composition matrix S (elements × species) and the
//! signed coefficient vector s (reactants positive, products negative).
//! The result is the per-element mass-balance residual `S·s`.
//!
//! - balanced `2 H2 + O2 -> 2 H2O`: residual is EXACTLY `[0, 0]` —
//!   the zero residual is the mass-balance evidence;
//! - unbalanced `2 H2 + O2 -> H2O`: residual `[2, 1]` refuses typed
//!   `MassImbalance` at the capability seam (the zero-certificate
//!   result guard, cell data — never a silent value).
//!
//! Failure-first: this file was written and run BEFORE the cell or the
//! result guard existed and FAILED exactly where the refusal/zero
//! assertions now guard (the gap the cell closes).

use emath_core::limits::Limits;
use emath_core::Span;
use emath_exec_ir::install::{PackRegistry, install_pack};
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::std_cell_registry;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// The capability path of the chemistry mass-balance cell.
const MASS_BALANCE: &str = "std.chem.mass_balance";

/// The capability path of the chemistry balancing cell (auto-derives
/// primitive positive integer coefficients from the species composition
/// matrix; second milestone).
const BALANCE: &str = "std.chem.balance";

// ===========================================================================
// Metamorphic testing — MR
// strength matrix. The oracle: for an ARBITRARY composition matrix the
// canonical balanced equation is not directly known (multiple null
// vectors, sign conventions), so we verify RELATIONS instead. Every MR
// below scores >= 2.0 = fault-sensitivity × independence / cost.
//
// | MR | Category | F (1-5) | I (1-5) | C (1-5) | Score |
// |----|----------|--------:|--------:|--------:|------:|
// | balance∘mass_balance chain (derived coefficients certified) | compose | 5 | 4 | 2 | 10 |
// | primitivity/gcd-1 of derived vector | equivalence | 4 | 4 | 2 | 8 |
// | canonical sign (first nonzero > 0); negation-consistent | invertive | 4 | 3 | 2 | 6 |
// | species permutation → vector permuted identically | permutative | 4 | 4 | 2 | 8 |
// | element row scaling invariance | multiplicative | 3 | 4 | 2 | 6 |
// | k·s scaling of the CERTIFICATE still admits | multiplicative | 3 | 3 | 2 | 4.5 |
// | underdetermined (dim>1) refusal, typed | inclusive/exclusive | 4 | 4 | 2 | 8 |
// | impossible (dim 0) refusal, typed | inclusive/exclusive | 4 | 4 | 2 | 8 |
// | non-integral / overflow inputs refuse typed | boundary | 3 | 3 | 1 | 9 |
// | mutation kills (sign/gcd/pivot/guard mutants) | validate | 4 | 4 | 2 | 8 |
//
// Dropped (score < 2.0): none — every candidate above is implemented or
// covered by an equal-strength sibling.
// ===========================================================================

/// The chemistry field-pack source (the `.emath` deliverable): exports
/// the EXISTING mass-balance and balancing cells; nothing is forked or
/// reimplemented.
const CHEM_PACK: &str = "\
package std

emath field_pack chemistry:
    exports:
        cell mass_balance
        cell balance
    metadata:
        description stoichiometric mass balance certificate plus balancing via generic ops
";

/// Admit the pack source at the language layer and return the entry.
fn admitted_pack(source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("chemistry-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    let messages: Vec<String> = result
        .diagnostics
        .items()
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        codes.is_empty(),
        "the chemistry pack admits at the language layer, got {messages:?}"
    );
    let mut packs = result.package.field_packs;
    assert_eq!(packs.len(), 1, "one field_pack admitted");
    packs.remove(0)
}

/// Hand-built EMIR applying the mass-balance cell to its two inputs:
/// register 0 = S (composition matrix), register 1 = s (coefficients).
fn balance_program() -> EmirProgram {
    let span = Span::default();
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), span),
            (EmirOp::LoadInput(1), span),
            (
                EmirOp::ApplyCapability {
                    capability: MASS_BALANCE.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0), EmirValue(1)],
                },
                span,
            ),
        ],
        result: EmirValue(2),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Combustion: species order H2, O2, H2O. Composition rows are H and O.
fn combustion_matrix() -> Value {
    Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
    }
}

/// The pack admits, installs against the EXISTING std registry, and
/// `use std.chemistry` resolves the installed artifact — the field-pack
/// compiler path, end to end.
#[test]
fn chemistry_pack_admits_installs_and_resolves() {
    let entry = admitted_pack(CHEM_PACK);
    let installed = install_pack(&entry, &["std".to_string()], std_cell_registry())
        .expect("the chemistry pack installs from the existing registry");
    assert_eq!(installed.pack, "chemistry");
    assert_eq!(
        installed.exports,
        vec![MASS_BALANCE.to_string(), BALANCE.to_string()],
        "the exports resolve to the canonical registry cells"
    );
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
    let cells = installed.image.load("cells").expect("cells page");
    assert!(
        cells.contains("cell:std.chem.mass_balance"),
        "image cells page carries the mass-balance cell: {cells}"
    );
    assert!(
        cells.contains("cell:std.chem.balance"),
        "image cells page carries the balancing cell: {cells}"
    );
    let mut registry = PackRegistry::new();
    registry.install(installed);
    let used = registry
        .resolve_use(&["std".to_string(), "chemistry".to_string()])
        .expect("use std.chemistry resolves");
    assert_eq!(used.pack, "chemistry");
}

/// Balanced `2 H2 + O2 -> 2 H2O` (`s = [2, 1, -2]`) admits and the cell
/// returns the EXACT all-zero mass-balance residual — the
/// stoichiometric evidence, bit-identical zeros, not a tolerance.
#[test]
fn balanced_combustion_admits_with_exact_zero_evidence() {
    let out = evaluate_with_budget(
        &balance_program(),
        &[combustion_matrix(), Value::Vector(vec![2.0, 1.0, -2.0])],
        &[],
        EvalBudget::default(),
    )
    .expect("balanced combustion evaluates in the reference VM");
    let residual = match out {
        Value::Vector(values) => values,
        other => panic!("expected a residual vector, got {other:?}"),
    };
    assert_eq!(residual, vec![0.0, 0.0], "H and O residuals vanish");
    assert!(
        residual.iter().all(|x| *x == 0.0),
        "residual entries are exact zeros: {residual:?}"
    );
}

/// Unbalanced `2 H2 + O2 -> H2O` (`s = [2, 1, -1]`) refuses typed
/// `MassImbalance` at the capability seam — never a silent value, never
/// an untyped arithmetic fault. Residual `[2, 1]`: H is the first
/// violating element with residual 2.
#[test]
fn unbalanced_combustion_refuses_typed_mass_imbalance() {
    match evaluate_with_budget(
        &balance_program(),
        &[combustion_matrix(), Value::Vector(vec![2.0, 1.0, -1.0])],
        &[],
        EvalBudget::default(),
    ) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, MASS_BALANCE);
            assert_eq!(
                code, "MassImbalance(element 0, residual 2)",
                "typed refusal names the violating element and its exact residual"
            );
        }
        ok => panic!(
            "unbalanced combustion must refuse typed MassImbalance, got {ok:?}"
        ),
    }
}

/// A second nonzero case with a different imbalance shape: `H2 + O2 ->
/// H2O` (`s = [1, 1, -1]`) leaves H residual 0 but O residual 1 — the
/// refusal must name element 1 (O), not element 0.
#[test]
fn second_unbalanced_case_names_the_other_element() {
    match evaluate_with_budget(
        &balance_program(),
        &[combustion_matrix(), Value::Vector(vec![1.0, 1.0, -1.0])],
        &[],
        EvalBudget::default(),
    ) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, MASS_BALANCE);
            assert_eq!(
                code, "MassImbalance(element 1, residual 1)",
                "H is balanced here; O is the violating element"
            );
        }
        ok => panic!("second unbalanced case must refuse typed, got {ok:?}"),
    }
}

// ===========================================================================
// Balancing: `std.chem.balance(S)` auto-derives the canonical primitive
// integer coefficient vector s (reactants positive, products negative)
// from the sign-blind species composition matrix S (rows = elements,
// cols = species, entry = atoms of the element in the species). The
// oracle problem applies: for arbitrary S the canonical null vector is
// unknown a priori, so relations (permutation, scaling, sign, chain)
// verify it, plus pinned hand-derived fixtures. Only the GENERIC op
// `int_nullspace` is new in the nucleus; the balance cell is registry
// data over it.
// ===========================================================================

/// Hand-built EMIR applying the balancing cell to its one input:
/// register 0 = S (composition matrix).
fn balance_cell_program() -> EmirProgram {
    let span = Span::default();
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), span),
            (
                EmirOp::ApplyCapability {
                    capability: BALANCE.to_string(),
                    class: CellClass::Pure,
                    args: vec![EmirValue(0)],
                },
                span,
            ),
        ],
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Run the balance cell on a composition matrix, returning the derived
/// coefficient vector or the refusal.
fn call_balance(matrix: Value) -> Result<Value, EvalFault> {
    evaluate_with_budget(
        &balance_cell_program(),
        &[matrix],
        &[],
        EvalBudget::default(),
    )
}

/// Composition matrix for hydrogen combustion; element rows H, O and
/// species columns H2, O2, H2O.
fn combustion_composition() -> Value {
    Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
    }
}

/// MR1 (fixture, not MR): combustion balances to `2 H2 + O2 -> 2 H2O`,
/// coefficients `[2, 1, -2]`, primitive and canonically signed.
#[test]
fn balance_combustion_derives_primitive_vector() {
    match call_balance(combustion_composition()) {
        Ok(Value::Vector(s)) => {
            assert_eq!(s, vec![2.0, 1.0, -2.0], "hand-derived null vector");
            assert!(
                s.iter().any(|x| *x != 0.0) && s[0] > 0.0,
                "canonical sign: first nonzero entry positive, got {s:?}"
            );
        }
        other => panic!("combustion must balance, got {other:?}"),
    }
}

/// MR the chain (score 10): the derived coefficients are CERTIFIED by
/// the mass-balance cell — `mass_balance(S, balance(S))` must admit with
/// the exact zero residual. Composition of the two cells.
#[test]
fn mr_chain_balance_then_certify() {
    let s = match call_balance(combustion_composition()) {
        Ok(Value::Vector(s)) => s,
        other => panic!("combustion must balance first, got {other:?}"),
    };
    let out = evaluate_with_budget(
        &balance_program(),
        &[combustion_composition(), Value::Vector(s)],
        &[],
        EvalBudget::default(),
    )
    .expect("the derived coefficients certify");
    match out {
        Value::Vector(residual) => {
            assert_eq!(residual, vec![0.0, 0.0], "S·s == 0 exactly");
        }
        other => panic!("expected a residual vector, got {other:?}"),
    }
}

/// Nonzero target: thermite `Fe2O3 + 2 Al -> Al2O3 + 2 Fe`, four
/// species. Rows Fe, O, Al; columns Fe2O3, Al, Al2O3, Fe. Hand-derived
/// null vector `[1, 2, -1, -2]`.
#[test]
fn balance_thermite_nontrivial_reaction() {
    let matrix = Value::Matrix {
        rows: 3,
        cols: 4,
        data: vec![2.0, 0.0, 0.0, 1.0, 3.0, 0.0, 3.0, 0.0, 0.0, 1.0, 2.0, 0.0],
    };
    match call_balance(matrix.clone()) {
        Ok(Value::Vector(s)) => {
            assert_eq!(s, vec![1.0, 2.0, -1.0, -2.0], "thermite hand-derived");
            let out = evaluate_with_budget(
                &balance_program(),
                &[matrix, Value::Vector(s)],
                &[],
                EvalBudget::default(),
            )
            .expect("thermite coefficients certify");
            assert_eq!(out, Value::Vector(vec![0.0, 0.0, 0.0]));
        }
        other => panic!("thermite must balance, got {other:?}"),
    }
}

/// A second nonzero reaction shape: `C2H4 + H2 -> C2H6` balances to
/// `[1, 1, -1]` (rows C, H; columns C2H4, H2, C2H6).
#[test]
fn balance_hydrogenation_derives_vector() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 4.0, 2.0, 6.0],
    };
    match call_balance(matrix.clone()) {
        Ok(Value::Vector(s)) => {
            assert_eq!(s, vec![1.0, 1.0, -1.0], "hydrogenation hand-derived");
            let out = evaluate_with_budget(
                &balance_program(),
                &[matrix, Value::Vector(s)],
                &[],
                EvalBudget::default(),
            )
            .expect("hydrogenation coefficients certify");
            assert_eq!(out, Value::Vector(vec![0.0, 0.0]));
        }
        other => panic!("hydrogenation must balance, got {other:?}"),
    }
}

/// MR underdetermined (score 8): adding H2O2 to the combustion species
/// set leaves TWO independent balance equations in FOUR species — a
/// one-dimensional nullspace no longer exists, so balancing MUST refuse
/// typed rather than guess a basis vector.
#[test]
fn mr_underdetermined_system_refuses_typed() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 4,
        data: vec![2.0, 0.0, 2.0, 0.0, 0.0, 2.0, 1.0, 2.0],
    };
    match call_balance(matrix.clone()) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "int-nullspace");
            assert!(
                detail.contains("E-NULLSPACE-002"),
                "underdetermined balance refuses with the typed code, got {detail}"
            );
        }
        other => panic!("underdetermined system must refuse typed, got {other:?}"),
    }
}

/// MR impossible (score 8): `H2 + He` — each element appears in exactly
/// one species, so the only null vector is zero; balancing refuses
/// (dimension 0) rather than emitting an all-zero equation.
#[test]
fn mr_impossible_system_refuses_typed() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![2.0, 0.0, 0.0, 1.0],
    };
    match call_balance(matrix) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "int-nullspace");
            assert!(
                detail.contains("E-NULLSPACE-002"),
                "impossible balance refuses with the typed code, got {detail}"
            );
        }
        other => panic!("impossible system must refuse typed, got {other:?}"),
    }
}

/// MR non-integral (score 9): fractional composition entries have no
/// integer nullspace meaning — refuse typed before any arithmetic.
#[test]
fn mr_non_integral_composition_refuses_typed() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![1.5, 0.0, 0.0, 1.0],
    };
    match call_balance(matrix) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "int-nullspace");
            assert!(
                detail.contains("E-NULLSPACE-001"),
                "non-integral input refuses with the typed code, got {detail}"
            );
        }
        other => panic!("non-integral composition must refuse typed, got {other:?}"),
    }
}

/// MR canonical sign (score 6): reordering the species columns may turn
/// the raw null vector negative; the canonical form must keep the FIRST
/// nonzero entry positive, so [H2O, H2, O2] still reports the same
/// reaction `2 H2 + O2 -> 2 H2O` (coefficients [2, -1, -2]).
#[test]
fn mr_canonical_sign_is_first_nonzero_positive() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![2.0, 0.0, 2.0, 1.0, 2.0, 0.0],
    };
    match call_balance(matrix) {
        Ok(Value::Vector(s)) => {
            assert_eq!(s, vec![2.0, -1.0, -2.0], "canonical sign flip");
            assert!(s[0] > 0.0, "first nonzero entry is positive");
        }
        other => panic!("canonical sign must balance, got {other:?}"),
    }
}

/// MR scaled-equivalence (score 4.5): DERIVED coefficients are
/// canonical; USER-supplied integer multiples of them must still
/// certify through the mass-balance cell (2x combustion coefficients
/// have zero residual too — the certificate is scaling-invariant).
#[test]
fn mr_scaled_coefficients_still_certify() {
    for s in [vec![2.0, 1.0, -2.0], vec![4.0, 2.0, -4.0], vec![-2.0, -1.0, 2.0]]
    {
        let out = evaluate_with_budget(
            &balance_program(),
            &[combustion_composition(), Value::Vector(s.clone())],
            &[],
            EvalBudget::default(),
        )
        .expect("scaled coefficients certify");
        assert_eq!(out, Value::Vector(vec![0.0, 0.0]), "s = {s:?}");
    }
}

/// MR species permutation (score 8): permuting the SPECIES columns
/// permutes the derived coefficient vector in exactly the same way.
#[test]
fn mr_species_permutation_permutes_vector() {
    let base = combustion_composition();
    let Value::Matrix { rows, cols, data } = base else {
        unreachable!("combustion composition is a matrix")
    };
    let perm: [usize; 3] = [2, 0, 1]; // H2O, H2, O2
    let mut permuted_data = vec![0.0; data.len()];
    for r in 0..rows {
        for c in 0..cols {
            permuted_data[r * cols + c] = data[r * cols + perm[c]];
        }
    }
    let permuted = Value::Matrix {
        rows,
        cols,
        data: permuted_data,
    };
    let s = match call_balance(permuted.clone()) {
        Ok(Value::Vector(s)) => s,
        other => panic!("permuted system must balance, got {other:?}"),
    };
    // The raw null vector in the permuted column order is [-2, 2, 1],
    // which canonicalizes (first nonzero positive) to [2, -2, -1]: the
    // original [2, 1, -2] permuted by the same column swap.
    assert_eq!(s, vec![2.0, -2.0, -1.0], "vector permuted with the columns");
    // And the permuted coefficients still certify against the permuted
    // matrix — balance and certificate commute.
    let out = evaluate_with_budget(
        &balance_program(),
        &[permuted, Value::Vector(s)],
        &[],
        EvalBudget::default(),
    )
    .expect("permuted coefficients certify against the permuted matrix");
    assert_eq!(out, Value::Vector(vec![0.0, 0.0]), "permutation preserves balance");
}

/// MR element permutation (score 8): permuting the ELEMENT rows leaves
/// the derived coefficient vector unchanged — the nullspace is
/// row-order independent.
#[test]
fn mr_element_permutation_keeps_vector() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![0.0, 2.0, 1.0, 2.0, 0.0, 2.0], // O row first, then H
    };
    let s = match call_balance(matrix) {
        Ok(Value::Vector(s)) => s,
        other => panic!("element-permuted system must balance, got {other:?}"),
    };
    assert_eq!(s, vec![2.0, 1.0, -2.0], "row order does not matter");
}

/// MR row scaling (score 6): multiplying an element's row by any
/// positive INTEGER preserves the nullspace exactly (the integrality
/// gate admits integer multiples only).
#[test]
fn mr_row_scaling_invariance() {
    // H row doubled (x2), O row tripled (x3) — same nullspace.
    let matrix = Value::Matrix {
        rows: 2,
        cols: 3,
        data: vec![4.0, 0.0, 4.0, 0.0, 6.0, 3.0],
    };
    let s = match call_balance(matrix) {
        Ok(Value::Vector(s)) => s,
        other => panic!("row-scaled system must balance, got {other:?}"),
    };
    assert_eq!(s, vec![2.0, 1.0, -2.0], "row scaling keeps the null vector");
}

/// MR zero-column (score 8): a species with zero atoms in every element
/// (a column of zeros) adds a free direction — the nullspace is at
/// least two-dimensional and balancing must refuse typed
/// (E-NULLSPACE-002), never silently ignore the species.
#[test]
fn mr_zero_column_species_refuses_typed() {
    let matrix = Value::Matrix {
        rows: 2,
        cols: 4,
        data: vec![2.0, 0.0, 2.0, 0.0, 0.0, 2.0, 1.0, 0.0],
    };
    match call_balance(matrix) {
        Err(EvalFault::Arithmetic { op, detail }) => {
            assert_eq!(op, "int-nullspace");
            assert!(
                detail.contains("E-NULLSPACE-002"),
                "zero-column species refuses typed, got {detail}"
            );
        }
        other => panic!("zero-column species must refuse typed, got {other:?}"),
    }
}
