//! Cell families: linear algebra, graph, PDE, probability, optimization, polynomial, control, category.

use super::*;

/// The linear-algebra registry cells (B35) as quoted terms:
/// `(name, term, signature, params, guards)` tuples over the closed
/// vocabulary. L2 norm is the generic `norm` name; L1 composes the abs
/// map with the sum reduce; Linf composes abs with the vmax reduce; the
/// inner product is the generic dot. All are guarded AllFinite (the
/// vector contract).
pub(super) fn linear_algebra_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let vector_param = || vec![("v".to_string(), ParamShape::Vector)];
    let two_vector_params = || {
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ]
    };
    let all_finite = |count: usize| (0..count).map(ArgGuard::AllFinite).collect();
    // L2: norm(v) — the generic norm name lowers to VectorNorm.
    let l2 = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("norm".into()), 1)
            .expect("norm signature is conflict-free");
        (
            Term::Apply {
                operator: SymbolId("norm".into()),
                arguments: vec![Term::Variable(VariableId("v".into()))],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // L1: sum(map(abs, v)) — abs over the vector, then the sum reduce.
    let l1 = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("abs", 1usize), ("sum", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("norm1 signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("abs".into()),
                    arguments: vec![Term::Variable(VariableId("v".into()))],
                }],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // Linf: vmax(map(abs, v)) — abs over the vector, then the max reduce.
    let linf = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("abs", 1usize), ("vmax", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("norminf signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("abs".into()),
                    arguments: vec![Term::Variable(VariableId("v".into()))],
                }],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // Inner product: dot(u, v) — the generic dot.
    let inner = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("dot".into()), 2)
            .expect("dot signature is conflict-free");
        (
            Term::Apply {
                operator: SymbolId("dot".into()),
                arguments: vec![
                    Term::Variable(VariableId("u".into())),
                    Term::Variable(VariableId("v".into())),
                ],
            },
            signature,
            two_vector_params(),
            all_finite(2),
        )
    };
    let direct = |operator: &'static str, params: Vec<(String, ParamShape)>| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), params.len())
            .expect("linear algebra signature is conflict-free");
        let arguments = params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect();
        let guards = all_finite(params.len());
        (
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments,
            },
            signature,
            params,
            guards,
        )
    };
    let solve = direct(
        "solve_linear",
        vec![
            ("A".to_string(), ParamShape::Matrix),
            ("b".to_string(), ParamShape::Vector),
        ],
    );
    let lu = direct("lu", vec![("A".to_string(), ParamShape::Matrix)]);
    let qr = direct("qr", vec![("A".to_string(), ParamShape::Matrix)]);
    let outer = direct(
        "outer_product",
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ],
    );
    vec![
        ("std.linalg.norm", l2),
        ("std.linalg.norm1", l1),
        ("std.linalg.norminf", linf),
        ("std.linalg.inner_product", inner),
        ("std.linalg.solve_linear", solve),
        ("std.linalg.lu", lu),
        ("std.linalg.qr", qr),
        ("std.linalg.outer_product", outer),
    ]
    .into_iter()
    .map(|(name, (term, signature, params, guards))| (name, term, signature, params, guards))
    .collect()
}

/// The graph algorithm cells: registry DATA
/// over the slice-1 EMIR ops — zero per-op VM code (the
/// anti-LOC law). `std.graph.shortest_distances` declares the
/// all-finite weight guard so a NaN/Inf weight refuses typed
/// (`E-GRAPH-004` at the VM seam) — never silent NaN distances.
pub(super) fn graph_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let adjacency_param = vec![("adj".to_string(), ParamShape::Matrix)];
    let traversal_params = vec![
        ("adj".to_string(), ParamShape::Matrix),
        ("source".to_string(), ParamShape::Scalar),
    ];
    let finite_adjacency = || vec![ArgGuard::AllFinite(0)];
    let cell = |name: &'static str, operator: &str, arity: usize| {
        let variable_names: &[&str] = if arity == 1 {
            &["adj"]
        } else {
            &["adj", "source"]
        };
        let arguments: Vec<Term> = variable_names
            .iter()
            .map(|variable| Term::Variable(VariableId((*variable).into())))
            .collect();
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), arity)
            .expect("graph cell signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments,
            },
            signature,
            if arity == 1 {
                adjacency_param.clone()
            } else {
                traversal_params.clone()
            },
            finite_adjacency(),
        )
    };
    vec![
        cell("std.graph.reachability", "reachability", 2),
        cell("std.graph.bfs_order", "bfs_order", 2),
        cell("std.graph.shortest_distances", "shortest_distances", 2),
        cell("std.graph.out_degrees", "out_degrees", 1),
        // Spectral basics: the Laplacian as registry DATA;
        // the spectrum composes through the existing symmetric eigen
        // op.
        cell("std.graph.laplacian", "graph_laplacian", 1),
        // Directed → spectral path: symmetrization as
        // registry DATA (weight-preserving (A+Aᵀ)/2 convention).
        cell("std.graph.symmetrize", "graph_symmetrize", 1),
        // Negative-edge methods: Bellman-Ford as registry
        // DATA; negative weights ADMITTED, reachable negative cycles
        // refuse E-GRAPH-005 at the kernel/wrapper layer.
        cell("std.graph.bellman_ford", "bellman_ford", 2),
        // Sparse storage: COO extraction/build as registry
        // DATA (duplicates SUM; malformed streams refuse E-GRAPH-006).
        // The build cell has MIXED param shapes (scalar n, vector
        // triplets) and guards the triplet stream (index 1), so it
        // bypasses the adjacency-cell helper.
        cell("std.graph.sparse_triplets", "sparse_triplets", 1),
        {
            let mut signature = Signature::default();
            signature
                .insert(SymbolId("sparse_from_triplets".into()), 2)
                .expect("graph cell signature is conflict-free");
            (
                "std.graph.sparse_from_triplets",
                Term::Apply {
                    operator: SymbolId("sparse_from_triplets".into()),
                    arguments: vec![
                        Term::Variable(VariableId("n".into())),
                        Term::Variable(VariableId("triplets".into())),
                    ],
                },
                signature,
                vec![
                    ("n".to_string(), ParamShape::Scalar),
                    ("triplets".to_string(), ParamShape::Vector),
                ],
                vec![ArgGuard::AllFinite(1)],
            )
        },
    ]
    .into_iter()
    .chain(optimization_cells())
    .chain(polynomial_cells())
    .chain(control_cells())
    .chain(category_cells())
    .chain(pde_cells())
    .chain(probability_cells())
    .collect()
}

/// The PDE cells (thin nucleus): registry DATA over the
/// spectral Poisson op — zero per-op VM code (the anti-LOC
/// law). The all-finite guard on the load keeps NaN out of the
/// transform at the cell seam (the kernel's own E-PDE-002 guards the
/// bare-op path).
pub(super) fn pde_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let sine = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poisson_sine".into()), 1)
            .expect("poisson_sine signature is conflict-free");
        (
            "std.pde.poisson_sine",
            Term::Apply {
                operator: SymbolId("poisson_sine".into()),
                arguments: vec![Term::Variable(VariableId("load".into()))],
            },
            signature,
            vec![("load".to_string(), ParamShape::Vector)],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![sine]
}

/// The probability cells (thin nucleus): registry DATA
/// over the sampling/density EMIR ops — zero per-op VM code (the
/// anti-LOC law). The all-finite guard on the params keeps
/// NaN out of the generators at the cell seam (the kernel's own
/// E-PROB-001/002 codes guard the bare-op path). The seed→stream
/// mapping (f64 bits → SplitMix64 state) is PROVISIONAL: the
/// stream contract owns the seed/stream semantics above this layer.
pub(super) fn probability_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let sample_cell = |name: &'static str, operator: &'static str, kind: ProbKind| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), 3)
            .expect("sample signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments: vec![
                    Term::Variable(VariableId("params".into())),
                    Term::Variable(VariableId("seed".into())),
                    Term::Variable(VariableId("draws".into())),
                ],
            },
            signature,
            vec![
                ("params".to_string(), ParamShape::Vector),
                ("seed".to_string(), ParamShape::Scalar),
                ("draws".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    let density_cell = |name: &'static str, operator: &'static str| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), 2)
            .expect("density signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments: vec![
                    Term::Variable(VariableId("params".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("params".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![
        sample_cell("std.prob.normal_sample", "normal_sample", ProbKind::Normal),
        sample_cell(
            "std.prob.uniform_sample",
            "uniform_sample",
            ProbKind::Uniform,
        ),
        sample_cell(
            "std.prob.bernoulli_sample",
            "bernoulli_sample",
            ProbKind::Bernoulli,
        ),
        density_cell("std.prob.normal_density", "normal_density"),
        density_cell("std.prob.uniform_density", "uniform_density"),
        density_cell("std.prob.bernoulli_pmf", "bernoulli_pmf"),
    ]
}

/// The optimization cells: registry DATA over
/// the LP/Pareto EMIR ops — zero per-op VM code (the anti-LOC
/// law). Both declare the all-finite guard on every numeric argument
/// (the strict-f64 finite policy; `E-CELL-006` at the seam, the
/// kernel's own E-LP/E-PARETO codes guard the bare-op path).
pub(super) fn optimization_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let lp_params = vec![
        ("A".to_string(), ParamShape::Matrix),
        ("b".to_string(), ParamShape::Vector),
        ("c".to_string(), ParamShape::Vector),
    ];
    let pareto_params = vec![("points".to_string(), ParamShape::Matrix)];
    let lp = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("lp_minimize".into()), 3)
            .expect("lp signature is conflict-free");
        (
            "std.optimize.lp",
            Term::Apply {
                operator: SymbolId("lp_minimize".into()),
                arguments: vec![
                    Term::Variable(VariableId("A".into())),
                    Term::Variable(VariableId("b".into())),
                    Term::Variable(VariableId("c".into())),
                ],
            },
            signature,
            lp_params,
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let pareto = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("pareto_front".into()), 1)
            .expect("pareto signature is conflict-free");
        (
            "std.optimize.pareto_front",
            Term::Apply {
                operator: SymbolId("pareto_front".into()),
                arguments: vec![Term::Variable(VariableId("points".into()))],
            },
            signature,
            pareto_params,
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![lp, pareto]
}

/// The polynomial cells: registry
/// DATA over the poly EMIR ops — zero per-op VM code (the
/// anti-LOC law). Addition needs no cell (it binds the generic vector
/// add at call level).
pub(super) fn polynomial_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let two_vectors = || {
        vec![
            ("a".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
        ]
    };
    let mul = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poly_mul".into()), 2)
            .expect("poly_mul signature is conflict-free");
        (
            "std.poly.mul",
            Term::Apply {
                operator: SymbolId("poly_mul".into()),
                arguments: vec![
                    Term::Variable(VariableId("a".into())),
                    Term::Variable(VariableId("b".into())),
                ],
            },
            signature,
            two_vectors(),
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
        )
    };
    let eval = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poly_eval".into()), 2)
            .expect("poly_eval signature is conflict-free");
        (
            "std.poly.eval",
            Term::Apply {
                operator: SymbolId("poly_eval".into()),
                arguments: vec![
                    Term::Variable(VariableId("p".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("p".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![mul, eval]
}

/// The control cells (thin B43): registry
/// DATA over the control EMIR ops — zero per-op VM code (the
/// anti-LOC law). The all-finite guards keep NaN out of the cell seam
/// (the kernels' own E-CONTROL-001..005 codes guard the bare-op path).
pub(super) fn control_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let transfer = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("transfer_eval".into()), 3)
            .expect("transfer_eval signature is conflict-free");
        (
            "std.control.transfer_eval",
            Term::Apply {
                operator: SymbolId("transfer_eval".into()),
                arguments: vec![
                    Term::Variable(VariableId("num".into())),
                    Term::Variable(VariableId("den".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("num".to_string(), ParamShape::Vector),
                ("den".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
        )
    };
    let dc_gain = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("dc_gain".into()), 3)
            .expect("dc_gain signature is conflict-free");
        (
            "std.control.dc_gain",
            Term::Apply {
                operator: SymbolId("dc_gain".into()),
                arguments: vec![
                    Term::Variable(VariableId("A".into())),
                    Term::Variable(VariableId("b".into())),
                    Term::Variable(VariableId("c".into())),
                ],
            },
            signature,
            vec![
                ("A".to_string(), ParamShape::Matrix),
                ("b".to_string(), ParamShape::Vector),
                ("c".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let poles_stable = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poles_stable".into()), 1)
            .expect("poles_stable signature is conflict-free");
        (
            "std.control.poles_stable",
            Term::Apply {
                operator: SymbolId("poles_stable".into()),
                arguments: vec![Term::Variable(VariableId("den".into()))],
            },
            signature,
            vec![("den".to_string(), ParamShape::Vector)],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![transfer, dc_gain, poles_stable]
}

/// The category cells (thin B39):
/// registry DATA over the category EMIR ops — zero per-op VM code (the
/// anti-LOC law). The all-finite guards keep NaN out of the
/// cell seam (the kernels' own E-CAT-001..007 codes guard the bare-op
/// path; the law certification still runs inside the kernel).
pub(super) fn category_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let check = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("category_check".into()), 3)
            .expect("category_check signature is conflict-free");
        (
            "std.category.check",
            Term::Apply {
                operator: SymbolId("category_check".into()),
                arguments: vec![
                    Term::Variable(VariableId("dom".into())),
                    Term::Variable(VariableId("cod".into())),
                    Term::Variable(VariableId("comp".into())),
                ],
            },
            signature,
            vec![
                ("dom".to_string(), ParamShape::Vector),
                ("cod".to_string(), ParamShape::Vector),
                ("comp".to_string(), ParamShape::Matrix),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let commutative = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("diagram_commutative".into()), 4)
            .expect("diagram_commutative signature is conflict-free");
        (
            "std.category.commutative",
            Term::Apply {
                operator: SymbolId("diagram_commutative".into()),
                arguments: vec![
                    Term::Variable(VariableId("dom".into())),
                    Term::Variable(VariableId("cod".into())),
                    Term::Variable(VariableId("comp".into())),
                    Term::Variable(VariableId("faces".into())),
                ],
            },
            signature,
            vec![
                ("dom".to_string(), ParamShape::Vector),
                ("cod".to_string(), ParamShape::Vector),
                ("comp".to_string(), ParamShape::Matrix),
                ("faces".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
                ArgGuard::AllFinite(3),
            ],
        )
    };
    vec![check, commutative]
}
