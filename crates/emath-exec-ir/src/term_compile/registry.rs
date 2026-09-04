//! Compiled std-cell registry: geometry, balance, cycle, rewrite, mass-balance cells.

use super::*;

/// Compiled std-cell registry: cells ship as DATA (quoted reference term
/// + guards), compiled once to generic bytecode. Adding a pure cell is
/// one registry entry — the VM seam and the op set never grow per-op.
pub fn std_cell_registry() -> &'static HashMap<String, CompiledCell> {
    static REGISTRY: OnceLock<HashMap<String, CompiledCell>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut map = HashMap::new();
        // The cohort (7 migrated ops + softmax). Each entry is
        // standalone data (independent rollback): quoted term + guards,
        // compiled by the SAME closed vocabulary — zero per-op VM code.
        // Scalar ops declare the unguarded-scalar policy (NaN propagates
        // — the declared strict-f64 behavior for unguarded scalars).
        let mut insert = |cell: CompiledCell| {
            map.insert(cell.capability.clone(), cell);
        };
        for (name, op) in [
            ("std.math.sin", "sin"),
            ("std.math.exp", "exp"),
            ("std.math.sqrt", "sqrt"),
        ] {
            let (term, signature) = scalar_unary_term(op);
            match compile_reference(
                &term,
                &signature,
                &[("x".to_string(), ParamShape::Scalar)],
                Vec::new(),
                name,
            ) {
                Ok(cell) => insert(cell),
                Err(error) => panic!("std scalar cell failed to compile: {error}"),
            }
        }
        // Chemistry: the Boltzmann softmax
        // reference term as `std.chem.softmax` cell data — a pure vector
        // cell (shift-invariant exp-normalize), compiled through the SAME
        // closed vocabulary, zero per-op VM code.
        let (term, signature) = softmax_reference_term();
        match compile_reference(
            &term,
            &signature,
            &[("x".to_string(), ParamShape::Vector)],
            vec![ArgGuard::NonEmpty(0), ArgGuard::AllFinite(0)],
            "std.chem.softmax",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.softmax reference failed to compile: {error}"),
        }
        // Chemistry: the
        // stoichiometric mass-balance cell is registry DATA over the
        // EXISTING dense matrix×vector op. `matvec(S, s)` is the
        // per-element residual S·s (S = signed composition matrix,
        // elements × species; s = signed coefficients, reactants
        // positive). The zero-certificate result guard refuses typed
        // `MassImbalance` when any residual is nonzero. f64 represents
        // small-integer stoichiometry EXACTLY, so an all-zero residual
        // is an exact mass-balance certificate, not a tolerance.
        let (term, signature) = mass_balance_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("S".to_string(), ParamShape::Matrix),
                ("s".to_string(), ParamShape::Vector),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
            ResultGuard::AllZero {
                code: "MassImbalance",
            },
            "std.chem.mass_balance",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.mass_balance reference failed to compile: {error}"),
        }
        // Chemistry balancing: derive the canonical
        // primitive coefficient vector from the sign-blind species
        // composition matrix, as registry DATA over the generic
        // `int_nullspace` op. No domain/logic code lives in the seam —
        // the op is the generic exact-integer primitive. (No result
        // guard: the coefficient vector is legitimately nonzero; the
        // mass-balance cell certifies it.)
        let (term, signature) = balance_reference_term();
        match compile_reference(
            &term,
            &signature,
            &[("S".to_string(), ParamShape::Matrix)],
            vec![ArgGuard::AllFinite(0)],
            "std.chem.balance",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.balance reference failed to compile: {error}"),
        }
        // Molecular-graph rewrite checker:
        // valence preservation across the (L, K, R) span as registry
        // DATA over generic ops, with the scalar-capable AllZero
        // certificate guard.
        let (term, signature) = rewrite_preserve_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("L".to_string(), ParamShape::Matrix),
                ("K".to_string(), ParamShape::Matrix),
                ("R".to_string(), ParamShape::Matrix),
                ("u".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
                ArgGuard::AllFinite(3),
            ],
            ResultGuard::AllZero {
                code: "ValenceImbalance",
            },
            "std.chem.graph_rewrite_preserve",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.graph_rewrite_preserve failed to compile: {error}"),
        }
        // Thermo-equilibrium: Wegscheider cycle
        // consistency as registry DATA over the generic exact product
        // delta op, with the AllZero scalar certificate guard.
        let (term, signature) = cycle_consistent_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("P".to_string(), ParamShape::Vector),
                ("Q".to_string(), ParamShape::Vector),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
            ResultGuard::AllZero {
                code: "CycleInconsistency",
            },
            "std.chem.cycle_consistent",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.cycle_consistent failed to compile: {error}"),
        }
        for (name, op) in [
            ("std.math.add", "add"),
            ("std.math.mul", "mul"),
            ("std.math.lt", "lt"),
        ] {
            let (term, signature) = scalar_binary_term(op);
            match compile_reference(
                &term,
                &signature,
                &[
                    ("x".to_string(), ParamShape::Scalar),
                    ("y".to_string(), ParamShape::Scalar),
                ],
                Vec::new(),
                name,
            ) {
                Ok(cell) => insert(cell),
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // The sum reduction (vector → scalar, guarded finite policy).
        match scalar_unary_sum() {
            Ok(cell) => {
                map.insert(cell.capability.clone(), cell);
            }
            Err(error) => panic!("std.tensor.sum reference failed to compile: {error}"),
        }
        // The linear-algebra norm family + inner product (B35):
        // registry DATA over the closed vector vocabulary the interp
        // already executes — L2 is the generic VectorNorm op; L1/Linf
        // compose the abs map with the sum/max reduces; the inner
        // product is the generic dot. Zero per-op VM code.
        for (name, term, signature, params, guards) in linear_algebra_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // Graph algorithms: registry DATA over
        // the slice-1 EMIR ops, Matrix-typed params, all-finite weight
        // guard (E-GRAPH-004 at the seam).
        for (name, term, signature, params, guards) in graph_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // Geometry primitives: registry DATA over the
        // closed vector vocabulary the interp already executes — cross
        // via bit-exact dot-with-basis component extraction, normalize
        // as the generic vector-scalar divide, distance as
        // norm(a-b). Zero per-op VM code, no geometry kernel.
        for (name, term, signature, params, guards) in geometry_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        match compile_std_softmax() {
            Ok(cell) => {
                map.insert(cell.capability.clone(), cell);
            }
            // The std formula is statically validated against its
            // signature; a failure here is a build-time contract break,
            // not a runtime condition.
            Err(error) => panic!("std.tensor.softmax reference failed to compile: {error}"),
        }
        map
    })
}

/// The geometry primitive cells: `std.geometry.cross` /
/// `std.geometry.normalize` / `std.geometry.distance` as registry DATA
/// over the SAME closed vector vocabulary — no geometry kernel, no new
/// op, no index operator. Component extraction inside `cross` is
/// bit-exact dot-with-basis: `dot(u, e_i) == u[i]` exactly for finite
/// inputs (`x·1 = x`, `x·0 = ±0`, and `x + ±0 = x`), so the compiled
/// formula is the textbook cross product over the extracted components.
/// Zero per-op VM code.
pub(super) fn geometry_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let two_vector_params = || {
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ]
    };
    let a_b_vector_params = || {
        vec![
            ("a".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
        ]
    };
    let one_vector_param = || vec![("v".to_string(), ParamShape::Vector)];
    let guarded = |count: usize| {
        (0..count)
            .flat_map(|index| [ArgGuard::NonEmpty(index), ArgGuard::AllFinite(index)])
            .collect::<Vec<_>>()
    };
    let axis = |x: &str, y: &str, z: &str| Term::Apply {
        operator: SymbolId("vec".into()),
        arguments: vec![
            Term::Constant(SymbolId(x.into())),
            Term::Constant(SymbolId(y.into())),
            Term::Constant(SymbolId(z.into())),
        ],
    };
    let e1 = || axis("1.0", "0.0", "0.0");
    let e2 = || axis("0.0", "1.0", "0.0");
    let e3 = || axis("0.0", "0.0", "1.0");
    let dot = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("dot".into()),
        arguments: vec![a, b],
    };
    let mul = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("mul".into()),
        arguments: vec![a, b],
    };
    let sub = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![a, b],
    };
    let u = || Term::Variable(VariableId("u".into()));
    let v = || Term::Variable(VariableId("v".into()));

    // cross(u, v): the three components assembled from bit-exact
    // basis-dot extractions; right-handed orientation is the term's
    // data (the permutation laws in tests/emath-sema/tests/geometry3d.rs
    // discriminate it).
    let cross = {
        let mut signature = Signature::default();
        // The basis-vector coordinates are nullary constant symbols and
        // must be declared like any other symbol (arity 0).
        for (symbol, arity) in [
            ("vec", 3usize),
            ("dot", 2),
            ("mul", 2),
            ("sub", 2),
            ("0.0", 0),
            ("1.0", 0),
        ] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("cross signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("vec".into()),
                arguments: vec![
                    sub(
                        mul(dot(u(), e2()), dot(v(), e3())),
                        mul(dot(u(), e3()), dot(v(), e2())),
                    ),
                    sub(
                        mul(dot(u(), e3()), dot(v(), e1())),
                        mul(dot(u(), e1()), dot(v(), e3())),
                    ),
                    sub(
                        mul(dot(u(), e1()), dot(v(), e2())),
                        mul(dot(u(), e2()), dot(v(), e1())),
                    ),
                ],
            },
            signature,
            two_vector_params(),
            guarded(2),
        )
    };
    // normalize(v): v / norm(v) — the generic vector-scalar divide.
    // A zero-norm input divides by zero: IEEE gives NaN/Inf under the
    // declared strict-f64 unguarded policy (the geometric no-claim —
    // never a synthesized unit vector).
    let normalize = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("div", 2usize), ("norm", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("normalize signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("div".into()),
                arguments: vec![
                    v(),
                    Term::Apply {
                        operator: SymbolId("norm".into()),
                        arguments: vec![v()],
                    },
                ],
            },
            signature,
            one_vector_param(),
            guarded(1),
        )
    };
    // distance(a, b): norm(a - b) — the generic vector subtract + norm.
    let distance = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("norm", 1usize), ("sub", 2)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("distance signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("norm".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("sub".into()),
                    arguments: vec![
                        Term::Variable(VariableId("a".into())),
                        Term::Variable(VariableId("b".into())),
                    ],
                }],
            },
            signature,
            a_b_vector_params(),
            guarded(2),
        )
    };
    vec![
        ("std.geometry.cross", cross.0, cross.1, cross.2, cross.3),
        (
            "std.geometry.normalize",
            normalize.0,
            normalize.1,
            normalize.2,
            normalize.3,
        ),
        (
            "std.geometry.distance",
            distance.0,
            distance.1,
            distance.2,
            distance.3,
        ),
    ]
}

/// The chemistry mass-balance reference term: `matvec(S, s)` — the
/// per-element residual of the signed stoichiometric system.
/// The chemistry balancing reference term: `int_nullspace(S)` — the
/// canonical primitive coefficient vector of the sign-blind species
/// composition matrix.
pub(super) fn balance_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("int_nullspace".into()), 1)
        .expect("int_nullspace signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("int_nullspace".into()),
            arguments: vec![Term::Variable(VariableId("S".into()))],
        },
        signature,
    )
}

/// The chemistry rewrite-preservation reference term (molecular-graph
/// slice): the valence certificate
/// `sum(abs(matvec(L,u)-matvec(K,u))) + sum(abs(matvec(K,u)-matvec(R,u)))`
/// over a rule triple (L, K, R) of context-row × union-column matrices
/// with `u` the all-ones vector (row sums = bond-order valences). The
/// guard refuses typed `ValenceImbalance` when the certificate is
/// nonzero. Pure registry data over generic ops; no domain code.
/// The cycle-consistency reference term (thermo slice): the exact
/// rational product difference `exact_product_delta(P, Q)` — the
/// Wegscheider certificate `∏P − ∏Q` — guarded AllZero with the
/// `CycleInconsistency` typed refusal.
pub(super) fn cycle_consistent_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("exact_product_delta".into()), 2)
        .expect("exact_product_delta signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("exact_product_delta".into()),
            arguments: vec![
                Term::Variable(VariableId("P".into())),
                Term::Variable(VariableId("Q".into())),
            ],
        },
        signature,
    )
}

pub(super) fn rewrite_preserve_reference_term() -> (Term, Signature) {
    let violation = |a: &str, b: &str| Term::Apply {
        operator: SymbolId("sum".into()),
        arguments: vec![Term::Apply {
            operator: SymbolId("abs".into()),
            arguments: vec![Term::Apply {
                operator: SymbolId("sub".into()),
                arguments: vec![
                    Term::Apply {
                        operator: SymbolId("matvec".into()),
                        arguments: vec![
                            Term::Variable(VariableId(a.into())),
                            Term::Variable(VariableId("u".into())),
                        ],
                    },
                    Term::Apply {
                        operator: SymbolId("matvec".into()),
                        arguments: vec![
                            Term::Variable(VariableId(b.into())),
                            Term::Variable(VariableId("u".into())),
                        ],
                    },
                ],
            }],
        }],
    };
    let term = Term::Apply {
        operator: SymbolId("add".into()),
        arguments: vec![violation("L", "K"), violation("K", "R")],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("matvec", 2usize),
        ("sub", 2),
        ("abs", 1),
        ("sum", 1),
        ("add", 2),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("rewrite preserve signature is conflict-free");
    }
    (term, signature)
}

/// The chemistry mass-balance reference term: `matvec(S, s)` — the
/// per-element residual of the signed stoichiometric system.
pub(super) fn mass_balance_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("matvec".into()), 2)
        .expect("matvec signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("matvec".into()),
            arguments: vec![
                Term::Variable(VariableId("S".into())),
                Term::Variable(VariableId("s".into())),
            ],
        },
        signature,
    )
}

/// The `std.tensor.sum` reference: `sum(x)` over the declared vector,
/// guarded AllFinite (the finite policy is the reduction's contract).
pub(super) fn scalar_unary_sum() -> Result<CompiledCell, TermCompileError> {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("sum".into()), 1)
        .expect("sum signature is conflict-free");
    compile_reference(
        &Term::Apply {
            operator: SymbolId("sum".into()),
            arguments: vec![Term::Variable(VariableId("x".into()))],
        },
        &signature,
        &[("x".to_string(), ParamShape::Vector)],
        vec![ArgGuard::AllFinite(0)],
        "std.tensor.sum",
    )
}
