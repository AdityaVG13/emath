//! emath-option-result-graph-field-aj8d (thin slice): Option/Result
//! value semantics at the compute layer.
//!
//! The bead's Option/Result need, thinned to TOTAL value semantics on
//! a real option carrier (`Value::Option(Option<Box<Value>>)` — a
//! None genuinely carries nothing, never a hidden zero):
//! - Constructors: `OptionSome` / `OptionNone` / `ResultOk` /
//!   `ResultErr` (the error payload is a real value, preserved).
//! - Polarity: `OptionIsSome` / `ResultIsOk` → Bool.
//! - The honesty gate: `OptionUnwrapOr` / `ResultUnwrapOr` — total
//!   unwraps; NO panicking unwrap exists at this layer (a missing
//!   value yields the caller's default, evaluated eagerly by the
//!   register discipline).
//! - Composition: `ResultErrorOf` yields the error as an OPTION
//!   (`None` when Ok, `Some(error)` when Err) — Result errors compose
//!   with the Option ops.
//! - Laws: polarity (kills tag-flip mutants), value-vs-default
//!   (kills always-default and always-value mutants), shape
//!   preservation through the carrier (F64/Vector/Matrix), the
//!   Some(None) tag-vs-content distinction, and the error round-trip.
//! - NO refusal surface: the semantics are total BY DESIGN; the
//!   negative-seed law does not bind here (documented in the pack).
//!   Registry cells ride the type admission (option-typed parameters
//!   have no ParamShape yet — named follow-up, not this slice).
//! - Graph is NOT reworked here (masa s1–5 delivered the adjacency
//!   compute layer); Field/GF is the disjoint follow-up.

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{compile_reference, ParamShape, TermCompileError};
use emath_exec_ir::{EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn scalar_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
}

fn bool_of(value: &Value) -> bool {
    let Value::Bool(b) = value else {
        panic!("expected a bool, got {value:?}")
    };
    *b
}

/// Helper: evaluate a one-operand op over `inputs`.
fn eval1(op: EmirOp, input: Value) -> Result<Value, EvalFault> {
    eval(vec![op], &[input])
}

#[test]
fn aj8d_option_polarity_and_unwrap_laws() {
    // Some(5): IsSome = true; UnwrapOr(7) = 5 (the VALUE, not the
    // default — kills an always-default mutant).
    let some = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(5.0))
        .expect("Some computes");
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), some.clone())
        .expect("IsSome computes");
    assert!(bool_of(&is_some), "Some(5).is_some()");
    let unwrapped = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[some, Value::F64(7.0)],
    )
    .expect("UnwrapOr computes");
    assert_eq!(scalar_of(&unwrapped), 5.0, "Some(5).unwrap_or(7) = 5");

    // None: IsSome = false; UnwrapOr(7) = 7 (the DEFAULT — kills an
    // always-value mutant).
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), none.clone())
        .expect("IsSome computes");
    assert!(!bool_of(&is_some), "None.is_some() = false");
    let unwrapped = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[none, Value::F64(7.0)],
    )
    .expect("UnwrapOr computes");
    assert_eq!(scalar_of(&unwrapped), 7.0, "None.unwrap_or(7) = 7");
}

#[test]
fn aj8d_result_polarity_error_preserved() {
    // Ok(3): IsOk = true; UnwrapOr(9) = 3; ErrorOf = Option NONE.
    let ok = eval1(EmirOp::ResultOk(EmirValue(0)), Value::F64(3.0))
        .expect("Ok computes");
    let is_ok = eval1(EmirOp::ResultIsOk(EmirValue(0)), ok.clone())
        .expect("IsOk computes");
    assert!(bool_of(&is_ok), "Ok(3).is_ok()");
    let unwrapped = eval(
        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
        &[ok.clone(), Value::F64(9.0)],
    )
    .expect("UnwrapOr computes");
    assert_eq!(scalar_of(&unwrapped), 3.0, "Ok(3).unwrap_or(9) = 3");
    let error_of = eval1(EmirOp::ResultErrorOf(EmirValue(0)), ok)
        .expect("ErrorOf computes");
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), error_of)
        .expect("IsSome computes");
    assert!(!bool_of(&is_some), "Ok(3).error_of() = None (composition)");

    // Err(42): IsOk = false; UnwrapOr(9) = 9; ErrorOf = Option SOME(42)
    // — the error payload SURVIVES (never swallowed into the default).
    let err = eval1(EmirOp::ResultErr(EmirValue(0)), Value::F64(42.0))
        .expect("Err computes");
    let is_ok = eval1(EmirOp::ResultIsOk(EmirValue(0)), err.clone())
        .expect("IsOk computes");
    assert!(!bool_of(&is_ok), "Err(42).is_ok() = false");
    let unwrapped = eval(
        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
        &[err.clone(), Value::F64(9.0)],
    )
    .expect("UnwrapOr computes");
    assert_eq!(scalar_of(&unwrapped), 9.0, "Err(42).unwrap_or(9) = 9");
    let error_of = eval1(EmirOp::ResultErrorOf(EmirValue(0)), err)
        .expect("ErrorOf computes");
    let recovered = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[error_of, Value::F64(-1.0)],
    )
    .expect("composition unwraps");
    assert_eq!(
        scalar_of(&recovered),
        42.0,
        "Err(42).error_of().unwrap_or(-1) = 42 (error round-trip)"
    );
}

#[test]
fn aj8d_shape_preservation_through_carrier() {
    // The carrier preserves ANY payload shape: Vector and Matrix
    // payloads round-trip through Some/UnwrapOr bit-for-bit (kills
    // shape-losing mutants that would coerce to scalar).
    let vector = Value::Vector(vec![1.5, -2.5, 3.5]);
    let some = eval1(EmirOp::OptionSome(EmirValue(0)), vector.clone())
        .expect("Some(vector) computes");
    let unwrapped = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[some, Value::Vector(vec![0.0; 3])],
    )
    .expect("UnwrapOr computes");
    assert_eq!(unwrapped, vector, "vector payload round-trips");

    let matrix = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![1.0, 2.0, 3.0, 4.0],
    };
    let some = eval1(EmirOp::OptionSome(EmirValue(0)), matrix.clone())
        .expect("Some(matrix) computes");
    let unwrapped = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[some, Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![0.0; 4],
        }],
    )
    .expect("UnwrapOr computes");
    assert_eq!(unwrapped, matrix, "matrix payload round-trips");
}

#[test]
fn aj8d_some_of_none_is_some() {
    // Tag-vs-content distinction: Some(None) IS Some (the polarity op
    // reads the TAG, not the content). A mutant that probes content
    // fails.
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let some_of_none = eval1(EmirOp::OptionSome(EmirValue(0)), none)
        .expect("Some(None) computes");
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), some_of_none)
        .expect("IsSome computes");
    assert!(bool_of(&is_some), "Some(None).is_some() = true (the tag)");
}

#[test]
fn aj8d_bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct OptionWorld;
    impl emath_genesis::FirstOrderWorld for OptionWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let some = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(5.0))
                .ok()
                .and_then(|some| {
                    eval(
                        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[some, Value::F64(7.0)],
                    )
                    .ok()
                });
            let none = eval1(EmirOp::OptionNone, Value::F64(0.0))
                .ok()
                .and_then(|none| {
                    eval(
                        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[none, Value::F64(7.0)],
                    )
                    .ok()
                });
            match (some, none) {
                (Some(Value::F64(a)), Some(Value::F64(b))) if a == 5.0 && b == 7.0 => {
                    Ok("option-result-semantics".to_string())
                }
                _ => Ok("option-result-diverged".to_string()),
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "option-result-value-semantics",
                &["option-carrier", "total-unwrap", "error-as-option"],
            )
        }
    }

    let term = Term::Constant(SymbolId("values[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &OptionWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "option-result-value-semantics");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}

// ── Pass 3: the term-compile CALL surface (nine names) ────────────────
//
// The nine Option/Result names bind the already-landed value-semantics
// ops through the PUBLIC `compile_reference` seam (the graph-call-
// surface precedent): the same CompiledCell → EmirProgram → reference
// interpreter path, with NO sema lowering (the `.emath`-text surface is
// the named follow-up lane). Carriers are opaque at the shape level;
// payload/default slots admit the concrete Scalar/Vector/Matrix shapes.

fn opt_apply(operator: &str, arguments: Vec<Term>) -> Term {
    Term::Apply {
        operator: SymbolId(operator.into()),
        arguments,
    }
}

fn opt_const(text: &str) -> Term {
    Term::Constant(SymbolId(text.into()))
}

/// The nine call-surface operators with their declared arities.
const CALL_SURFACE_DECLS: &[(&str, usize)] = &[
    ("option_some", 1),
    ("option_none", 0),
    ("option_is_some", 1),
    ("option_unwrap_or", 2),
    ("result_ok", 1),
    ("result_err", 1),
    ("result_is_ok", 1),
    ("result_unwrap_or", 2),
    ("result_error_of", 1),
    // Pass 7: the prime-field call surface. `field_inv(a, p)` lowers to
    // the interpreter's exact modular inverse `ModInv(a, p)`; generic
    // modular ADD/MUL EmirOps do not exist, so `field_add`/`field_mul`
    // are deliberately NOT registered here (handoff spec, never
    // half-wired names).
    ("field_inv", 2),
];

fn call_signature(constants: &[&str]) -> Signature {
    let mut signature = Signature::default();
    for (name, arity) in CALL_SURFACE_DECLS {
        signature
            .insert(SymbolId(name.to_string()), *arity)
            .expect("call-surface declarations are conflict-free");
    }
    for constant in constants {
        signature
            .insert(SymbolId(constant.to_string()), 0)
            .expect("constant declarations are conflict-free");
    }
    signature
}

/// Compile a constant-only call-surface program (no params) and
/// evaluate it through the reference interpreter.
fn call_eval(term: Term, constants: &[&str]) -> Result<Value, EvalFault> {
    let cell = compile_reference(
        &term,
        &call_signature(constants),
        &[],
        Vec::new(),
        "test.option-result-call",
    )
    .expect("call-surface program compiles");
    evaluate_with_budget(&cell.program, &[], &[], EvalBudget::default())
}

/// Compile a call-surface program over declared params and evaluate it
/// over input values (vector/matrix payload fixtures).
fn call_eval_params(
    term: Term,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
    constants: &[&str],
) -> Result<Value, EvalFault> {
    let cell = compile_reference(
        &term,
        &call_signature(constants),
        &params,
        Vec::new(),
        "test.option-result-call",
    )
    .expect("call-surface program compiles");
    evaluate_with_budget(&cell.program, inputs, &[], EvalBudget::default())
}

#[test]
fn aj8d_call_surface_option_constructors_polarity_unwrap() {
    // is_some(some(5)) == true — the polarity op over the compiled
    // constructor.
    let value = call_eval(
        opt_apply(
            "option_is_some",
            vec![opt_apply("option_some", vec![opt_const("5")])],
        ),
        &["5"],
    )
    .expect("is_some(some(5)) evaluates");
    assert_eq!(value, Value::Bool(true), "is_some(some(5)) = true");

    // unwrap_or(some(2), 9) == 2 — the VALUE, not the default (kills an
    // always-default mutant).
    let value = call_eval(
        opt_apply(
            "option_unwrap_or",
            vec![
                opt_apply("option_some", vec![opt_const("2")]),
                opt_const("9"),
            ],
        ),
        &["2", "9"],
    )
    .expect("unwrap_or(some(2), 9) evaluates");
    assert_eq!(value, Value::F64(2.0), "Some(2).unwrap_or(9) = 2");

    // unwrap_or(none, 9) == 9 — the DEFAULT (the honesty gate; no
    // panicking unwrap exists at this layer).
    let value = call_eval(
        opt_apply(
            "option_unwrap_or",
            vec![opt_apply("option_none", Vec::new()), opt_const("9")],
        ),
        &["9"],
    )
    .expect("unwrap_or(none, 9) evaluates");
    assert_eq!(value, Value::F64(9.0), "None.unwrap_or(9) = 9");

    // The zero-arg constructor alone evaluates to Option None (None
    // carries NOTHING).
    let value = call_eval(opt_apply("option_none", Vec::new()), &[]).expect("none evaluates");
    assert_eq!(value, Value::Option(None), "none = Option::None");
}

#[test]
fn aj8d_call_surface_result_polarity_unwrap_error_of() {
    // is_ok(ok(3.5)) == true; is_ok(err(7)) == false.
    let value = call_eval(
        opt_apply(
            "result_is_ok",
            vec![opt_apply("result_ok", vec![opt_const("3.5")])],
        ),
        &["3.5"],
    )
    .expect("is_ok(ok(3.5)) evaluates");
    assert_eq!(value, Value::Bool(true), "Ok(3.5).is_ok() = true");
    let value = call_eval(
        opt_apply(
            "result_is_ok",
            vec![opt_apply("result_err", vec![opt_const("7")])],
        ),
        &["7"],
    )
    .expect("is_ok(err(7)) evaluates");
    assert_eq!(value, Value::Bool(false), "Err(7).is_ok() = false");

    // unwrap_or(ok(3.5), 9) == 3.5; unwrap_or(err(7), 9) == 9.
    let value = call_eval(
        opt_apply(
            "result_unwrap_or",
            vec![
                opt_apply("result_ok", vec![opt_const("3.5")]),
                opt_const("9"),
            ],
        ),
        &["3.5", "9"],
    )
    .expect("unwrap_or(ok(3.5), 9) evaluates");
    assert_eq!(value, Value::F64(3.5), "Ok(3.5).unwrap_or(9) = 3.5");
    let value = call_eval(
        opt_apply(
            "result_unwrap_or",
            vec![opt_apply("result_err", vec![opt_const("7")]), opt_const("9")],
        ),
        &["7", "9"],
    )
    .expect("unwrap_or(err(7), 9) evaluates");
    assert_eq!(value, Value::F64(9.0), "Err(7).unwrap_or(9) = 9");

    // error_of(err(7)) composes as an Option and the error payload
    // round-trips through a SECOND unwrap_or: == 7 (never swallowed
    // into the default).
    let value = call_eval(
        opt_apply(
            "option_unwrap_or",
            vec![
                opt_apply(
                    "result_error_of",
                    vec![opt_apply("result_err", vec![opt_const("7")])],
                ),
                opt_const("-1"),
            ],
        ),
        &["7", "-1"],
    )
    .expect("err(7).error_of().unwrap_or(-1) evaluates");
    assert_eq!(value, Value::F64(7.0), "the error payload round-trips through error_of");

    // error_of(ok(1)) == none: Ok carries no error, so the composed
    // Option is empty.
    let value = call_eval(
        opt_apply(
            "option_is_some",
            vec![opt_apply(
                "result_error_of",
                vec![opt_apply("result_ok", vec![opt_const("1")])],
            )],
        ),
        &["1"],
    )
    .expect("ok(1).error_of() evaluates");
    assert_eq!(value, Value::Bool(false), "Ok(1).error_of() = none");
}

#[test]
fn aj8d_call_surface_vector_payload_round_trips() {
    // option_some(v) wraps a Vector payload; unwrap_or over a
    // vector-shaped default returns the Vector shape and the payload
    // round-trips bit-for-bit (the opaque-carrier shape law).
    let term = opt_apply(
        "option_unwrap_or",
        vec![
            opt_apply("option_some", vec![Term::Variable(VariableId("v".into()))]),
            Term::Variable(VariableId("w".into())),
        ],
    );
    let vector = Value::Vector(vec![1.5, -2.5, 3.5]);
    let out = call_eval_params(
        term,
        vec![
            ("v".to_string(), ParamShape::Vector),
            ("w".to_string(), ParamShape::Vector),
        ],
        &[vector.clone(), Value::Vector(vec![0.0; 3])],
        &[],
    )
    .expect("vector payload compiles and evaluates");
    assert_eq!(out, vector, "vector payload round-trips through the call surface");

    // The Result twin: err(v) wraps the payload slot; error_of then
    // yields Some(v) and the unwrap recovers it.
    let term = opt_apply(
        "option_unwrap_or",
        vec![
            opt_apply(
                "result_error_of",
                vec![opt_apply("result_err", vec![Term::Variable(VariableId("v".into()))])],
            ),
            Term::Variable(VariableId("w".into())),
        ],
    );
    let out = call_eval_params(
        term,
        vec![
            ("v".to_string(), ParamShape::Vector),
            ("w".to_string(), ParamShape::Vector),
        ],
        &[vector.clone(), Value::Vector(vec![0.0; 3])],
        &[],
    )
    .expect("result error payload compiles and evaluates");
    assert_eq!(out, vector, "result error payload round-trips through error_of");
}

#[test]
fn aj8d_call_surface_arity_refuses_typed() {
    // some() — zero args where one is declared: the arity refusal is a
    // TYPED TermCompileError (the emath-term signature check), never a
    // panic and never an empty lowering.
    let zero_arg = opt_apply("option_some", Vec::new());
    let error = compile_reference(
        &zero_arg,
        &call_signature(&[]),
        &[],
        Vec::new(),
        "test.option-result-call",
    )
    .expect_err("option_some() refuses at compile");
    assert!(
        matches!(error, TermCompileError::ArityMismatch { .. }),
        "zero-arg some must be a typed arity refusal, got {error:?}"
    );

    // unwrap_or(some(1)) — one argument where two are declared.
    let short = opt_apply(
        "option_unwrap_or",
        vec![opt_apply("option_some", vec![opt_const("1")])],
    );
    let error = compile_reference(
        &short,
        &call_signature(&["1"]),
        &[],
        Vec::new(),
        "test.option-result-call",
    )
    .expect_err("option_unwrap_or(some(1)) refuses at compile");
    assert!(
        matches!(error, TermCompileError::ArityMismatch { .. }),
        "short unwrap_or must be a typed arity refusal, got {error:?}"
    );
}

#[test]
fn aj8d_call_surface_shape_law_refuses_typed() {
    // is_some(5): a Scalar in the Option carrier slot refuses at
    // COMPILE (ShapeMismatch — the closed vocabulary's shape law),
    // never a silent mis-lowering and never a panic.
    let term = opt_apply("option_is_some", vec![opt_const("5")]);
    let error = compile_reference(
        &term,
        &call_signature(&["5"]),
        &[],
        Vec::new(),
        "test.option-result-call",
    )
    .expect_err("is_some(5) refuses at compile");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "a scalar in the Option carrier slot must refuse typed, got {error:?}"
    );

    // unwrap_or(err(7), none): an opaque Option carrier in the DEFAULT
    // slot refuses (defaults must be concrete payloads).
    let term = opt_apply(
        "result_unwrap_or",
        vec![
            opt_apply("result_err", vec![opt_const("7")]),
            opt_apply("option_none", Vec::new()),
        ],
    );
    let error = compile_reference(
        &term,
        &call_signature(&["7"]),
        &[],
        Vec::new(),
        "test.option-result-call",
    )
    .expect_err("unwrap_or with an option default refuses at compile");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "an Option carrier in the default slot must refuse typed, got {error:?}"
    );
}

// ── Pass 4: interp TOTAL-VALUE laws (test-only) ──────────────────────
//
// Four assertions of the total value semantics on the real carrier:
// (1) Some(None) / Some(Some(7)) STRUCTURAL nesting is preserved (the
// content is never flattened into the tag); (2) payload SHAPE
// preservation for Matrix and nested-Option payloads; (3) the honesty
// gate — none/err UNWRAP_OR returns the EAGER DEFAULT, never a fault,
// never a panicking unwrap (grep-verified in interp.rs:1418–1484; those
// arms use let-else/match, no `.unwrap()`/`expect(`/`panic!`); the only
// two `expect` calls in interp.rs are in series sampling, not the
// carrier ops); (4) polarity TOTALITY — every carrier answers both
// polarities, unwrap_or, and error_of (table-driven). TypeConfusion is
// the sole fault class for cross-carrier misuse.

#[test]
fn aj8d_law_nested_none_structural_identity() {
    // Some(None) is NOT flattened to None and NOT a hidden zero: the
    // Some tag and its None content are distinct (kills a "flatten
    // Some(None) → None" mutant at the structural assert below).
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let some_of_none = eval1(EmirOp::OptionSome(EmirValue(0)), none.clone())
        .expect("Some(None) computes");
    assert_eq!(
        some_of_none,
        Value::Option(Some(Box::new(Value::Option(None)))),
        "Some(None) keeps the outer Some and the inner None"
    );
    assert_ne!(
        some_of_none, none,
        "Some(None) is NOT the same value as None"
    );

    // The polarity op reads the OUTER tag: Some(None) is Some.
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), some_of_none.clone())
        .expect("is_some computes");
    assert!(bool_of(&is_some), "Some(None).is_some() = true (the tag)");

    // unwrap_or through Some(None) yields the CONTENT (None), not the
    // default — the carrier payload passes through untouched.
    let out = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[some_of_none, Value::F64(9.0)],
    )
    .expect("unwrap_or(Some(None)) computes");
    assert_eq!(
        out,
        Value::Option(None),
        "Some(None).unwrap_or(9) = the content (None), NOT the default"
    );
}

#[test]
fn aj8d_law_double_some_nesting() {
    let seven = Value::F64(7.0);
    let inner = eval1(EmirOp::OptionSome(EmirValue(0)), seven).expect("Some(7) computes");
    let outer = eval1(EmirOp::OptionSome(EmirValue(0)), inner.clone())
        .expect("Some(Some(7)) computes");
    assert_eq!(
        outer,
        Value::Option(Some(Box::new(Value::Option(Some(Box::new(Value::F64(
            7.0
        ))))))),
        "two wrapper levels preserve structural nesting"
    );

    // Polarity reads only the outermost tag.
    let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), outer.clone())
        .expect("is_some computes");
    assert!(bool_of(&is_some), "Some(Some(7)).is_some() = true");

    // unwrap_or unwraps ONE level → the inner Option.
    let out = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[outer, Value::F64(9.0)],
    )
    .expect("unwrap_or(Some(Some(7))) computes");
    assert_eq!(out, inner, "Some(Some(7)).unwrap_or(9) = Some(7)");
}

#[test]
fn aj8d_law_display_distinguishes_some_none_from_none() {
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let some_of_none = eval1(EmirOp::OptionSome(EmirValue(0)), none.clone())
        .expect("Some(None) computes");
    assert_eq!(
        some_of_none.to_string(),
        "some(none)",
        "Some(None) renders some(none), so display is not conflated with None"
    );
    assert_eq!(none.to_string(), "none", "None renders none");

    // Some(Some(7)) renders nested, not flattened.
    let inner = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(7.0)).expect("Some(7) computes");
    let outer = eval1(EmirOp::OptionSome(EmirValue(0)), inner).expect("Some(Some(7)) computes");
    assert_eq!(outer.to_string(), "some(some(7.0))");

    // The Result display keeps ok/err tags for the round-trip evidence.
    let ok = eval1(EmirOp::ResultOk(EmirValue(0)), Value::F64(3.0)).expect("Ok(3) computes");
    let err = eval1(EmirOp::ResultErr(EmirValue(0)), Value::F64(42.0)).expect("Err(42) computes");
    assert_eq!(ok.to_string(), "ok(3.0)");
    assert_eq!(err.to_string(), "err(42.0)");
}

#[test]
fn aj8d_law_matrix_payload_round_trip_through_carriers() {
    // Matrix payloads round-trip EXACTLY through Some/Ok/Err-UnwrapOr
    // at the term call surface (the Vector precedent extended to
    // Matrix; kills shape-losing mutants that coerce to scalar).
    let matrix = Value::Matrix {
        rows: 3,
        cols: 2,
        data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
    };
    let zero = Value::Matrix {
        rows: 3,
        cols: 2,
        data: vec![0.0; 6],
    };

    let term = opt_apply(
        "option_unwrap_or",
        vec![
            opt_apply("option_some", vec![Term::Variable(VariableId("v".into()))]),
            Term::Variable(VariableId("w".into())),
        ],
    );
    let out = call_eval_params(
        term,
        vec![
            ("v".to_string(), ParamShape::Matrix),
            ("w".to_string(), ParamShape::Matrix),
        ],
        &[matrix.clone(), zero.clone()],
        &[],
    )
    .expect("Matrix payload through Some/UnwrapOr");
    assert_eq!(out, matrix, "Some matrix round-trip exact");

    let term = opt_apply(
        "result_unwrap_or",
        vec![
            opt_apply("result_ok", vec![Term::Variable(VariableId("v".into()))]),
            Term::Variable(VariableId("w".into())),
        ],
    );
    let out = call_eval_params(
        term,
        vec![
            ("v".to_string(), ParamShape::Matrix),
            ("w".to_string(), ParamShape::Matrix),
        ],
        &[matrix.clone(), zero.clone()],
        &[],
    )
    .expect("Matrix payload through Ok/UnwrapOr");
    assert_eq!(out, matrix, "Ok matrix round-trip exact");

    // Err matrix → error_of → unwrap: the error payload survives.
    let term = opt_apply(
        "option_unwrap_or",
        vec![
            opt_apply(
                "result_error_of",
                vec![opt_apply(
                    "result_err",
                    vec![Term::Variable(VariableId("v".into()))],
                )],
            ),
            Term::Variable(VariableId("w".into())),
        ],
    );
    let out = call_eval_params(
        term,
        vec![
            ("v".to_string(), ParamShape::Matrix),
            ("w".to_string(), ParamShape::Matrix),
        ],
        &[matrix.clone(), zero],
        &[],
    )
    .expect("Matrix error payload through Err/ErrorOf/UnwrapOr");
    assert_eq!(out, matrix, "Err matrix round-trip exact");
}

#[test]
fn aj8d_law_polarity_totality_table() {
    // Every carrier answers BOTH polarities, unwrap_or, and (for
    // Result) error_of — no partial match leaves a carrier unhandled.
    // Table-driven over the (constructor × observer) matrix.
    type Probe = Box<dyn Fn() -> Result<Value, EvalFault>>;
    let some = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(5.0)).expect("Some computes");
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let ok = eval1(EmirOp::ResultOk(EmirValue(0)), Value::F64(3.0)).expect("Ok computes");
    let err = eval1(EmirOp::ResultErr(EmirValue(0)), Value::F64(42.0)).expect("Err computes");

    let rows: Vec<(&str, &str, Probe, Value)> = vec![
        (
            "some",
            "is_some",
            {
                let c = some.clone();
                Box::new(move || eval1(EmirOp::OptionIsSome(EmirValue(0)), c.clone()))
            },
            Value::Bool(true),
        ),
        (
            "some",
            "unwrap_or",
            {
                let c = some.clone();
                Box::new(move || {
                    eval(
                        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[c.clone(), Value::F64(9.0)],
                    )
                })
            },
            Value::F64(5.0),
        ),
        (
            "none",
            "is_some",
            {
                let c = none.clone();
                Box::new(move || eval1(EmirOp::OptionIsSome(EmirValue(0)), c.clone()))
            },
            Value::Bool(false),
        ),
        (
            "none",
            "unwrap_or",
            {
                let c = none.clone();
                Box::new(move || {
                    eval(
                        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[c.clone(), Value::F64(9.0)],
                    )
                })
            },
            Value::F64(9.0),
        ),
        (
            "ok",
            "is_ok",
            {
                let c = ok.clone();
                Box::new(move || eval1(EmirOp::ResultIsOk(EmirValue(0)), c.clone()))
            },
            Value::Bool(true),
        ),
        (
            "ok",
            "unwrap_or",
            {
                let c = ok.clone();
                Box::new(move || {
                    eval(
                        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[c.clone(), Value::F64(9.0)],
                    )
                })
            },
            Value::F64(3.0),
        ),
        (
            "ok",
            "error_of",
            {
                let c = ok.clone();
                Box::new(move || eval1(EmirOp::ResultErrorOf(EmirValue(0)), c.clone()))
            },
            Value::Option(None),
        ),
        (
            "err",
            "is_ok",
            {
                let c = err.clone();
                Box::new(move || eval1(EmirOp::ResultIsOk(EmirValue(0)), c.clone()))
            },
            Value::Bool(false),
        ),
        (
            "err",
            "unwrap_or",
            {
                let c = err.clone();
                Box::new(move || {
                    eval(
                        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
                        &[c.clone(), Value::F64(9.0)],
                    )
                })
            },
            Value::F64(9.0),
        ),
        (
            "err",
            "error_of",
            {
                let c = err.clone();
                Box::new(move || eval1(EmirOp::ResultErrorOf(EmirValue(0)), c.clone()))
            },
            Value::Option(Some(Box::new(Value::F64(42.0)))),
        ),
    ];
    for (carrier, observer, probe, expected) in rows {
        let value = probe()
            .unwrap_or_else(|fault| panic!("{carrier}.{observer} must be total, got {fault:?}"));
        assert_eq!(value, expected, "{carrier}.{observer} = {:?}", expected);
    }
}

#[test]
fn aj8d_law_none_and_err_return_default_total() {
    // The honesty gate: none/err UNWRAP_OR returns the EAGER DEFAULT.
    // Never a fault, never a panicking unwrap.
    let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
    let out = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[none, Value::F64(7.0)],
    )
    .expect("none.unwrap_or(7) computes (no panic, no fault)");
    assert_eq!(out, Value::F64(7.0), "None.unwrap_or(7) = 7");

    let err = eval1(EmirOp::ResultErr(EmirValue(0)), Value::F64(42.0)).expect("Err computes");
    let out = eval(
        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
        &[err, Value::F64(9.0)],
    )
    .expect("err.unwrap_or(9) computes (no panic, no fault)");
    assert_eq!(out, Value::F64(9.0), "Err(42).unwrap_or(9) = 9");
}

#[test]
fn aj8d_law_typeconfusion_is_some_on_result() {
    // is_some on a Result carrier is a TYPED evaluation error (a
    // TypeConfusion fault), never a panic, never a silent wrong answer.
    let result = eval1(EmirOp::ResultOk(EmirValue(0)), Value::F64(3.0)).expect("Ok computes");
    let fault = eval1(EmirOp::OptionIsSome(EmirValue(0)), result)
        .expect_err("is_some on a Result carrier must refuse");
    assert!(
        matches!(fault, EvalFault::TypeConfusion { .. }),
        "expected a TypeConfusion fault, got {fault:?}"
    );
}

#[test]
fn aj8d_law_typeconfusion_is_ok_on_option() {
    let option = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(5.0)).expect("Some computes");
    let fault = eval1(EmirOp::ResultIsOk(EmirValue(0)), option)
        .expect_err("is_ok on an Option carrier must refuse");
    assert!(
        matches!(fault, EvalFault::TypeConfusion { .. }),
        "expected a TypeConfusion fault, got {fault:?}"
    );
}

#[test]
fn aj8d_law_typeconfusion_unwrap_or_wrong_carrier() {
    // unwrap_or checks the carrier kind strictly too: an Option unwrap
    // over a Result carrier (and vice versa) is TypeConfusion — the
    // eager default is NOT silently handed out on the wrong carrier.
    let ok = eval1(EmirOp::ResultOk(EmirValue(0)), Value::F64(3.0)).expect("Ok computes");
    let fault = eval(
        vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
        &[ok, Value::F64(9.0)],
    )
    .expect_err("OptionUnwrapOr on a Result carrier must refuse");
    assert!(
        matches!(fault, EvalFault::TypeConfusion { .. }),
        "expected a TypeConfusion fault, got {fault:?}"
    );

    let some = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(5.0)).expect("Some computes");
    let fault = eval(
        vec![EmirOp::ResultUnwrapOr(EmirValue(0), EmirValue(1))],
        &[some, Value::F64(9.0)],
    )
    .expect_err("ResultUnwrapOr on an Option carrier must refuse");
    assert!(
        matches!(fault, EvalFault::TypeConfusion { .. }),
        "expected a TypeConfusion fault, got {fault:?}"
    );
}

#[test]
fn aj8d_nested_carrier_in_payload_compiles_and_evaluates() {
    // Pass 3 lift (emath-option-result-graph-field-aj8d): a carrier is a
    // valid PAYLOAD for the constructors. `option_some(option_none())`
    // now COMPILES at the term surface (TermCompileError::ShapeMismatch
    // is gone) and evaluates to the nested value `Some(None)` — the
    // tag-vs-content distinction proven from a compiled term, not just
    // the raw-value laws. This REPLACES the earlier pass-4 pin test
    // `aj8d_law_carrier_in_payload_refuses_compile`, which asserted the
    // OLD restriction; the capability was deliberately extended, so the
    // pin is rewritten to the stronger positive contract (fail-first
    // law — never silently weakened, only re-scoped to the new rule).
    let term = opt_apply("option_some", vec![opt_apply("option_none", Vec::new())]);
    let out = call_eval(term, &[]).expect("option_some(option_none()) compiles and evaluates");
    assert!(
        matches!(
            &out,
            Value::Option(Some(inner))
                if matches!(&**inner, Value::Option(None))
        ),
        "option_some(option_none()) must evaluate to Some(None), got {out:?}"
    );
}

#[test]
fn aj8d_nested_double_some_compiles_and_evaluates() {
    // `option_some(option_some(5.0))` compiles and yields Some(Some(5)):
    // the inner carrier survives intact through the outer constructor.
    let term = opt_apply(
        "option_some",
        vec![opt_apply("option_some", vec![opt_const("5")])],
    );
    let out = call_eval(term, &["5"]).expect("double Some compiles and evaluates");
    assert!(
        matches!(
            &out,
            Value::Option(Some(inner))
                if matches!(&**inner, Value::Option(Some(n)) if matches!(n.as_ref(), Value::F64(x) if *x == 5.0))
        ),
        "option_some(option_some(5.0)) must evaluate to Some(Some(5.0)), got {out:?}"
    );
}

// ── Pass 7: Field/GF<p> exact modular value semantics ──────────────
//
// The prime-field VALUE layer is the exact-i64 modular ops that already
// exist in the interpreter: `ModInv(a, m)` (interp.rs:2004 →
// emath_rt::mod_inv_checked — extended GCD; typed Arithmetic refusal when
// the modulus is non-positive or the value non-invertible). The term
// surface `field_inv(a, p)` lowers to `ModInv(a, p)` (term_compile.rs).
// Generic modular ADD/MUL EmirOps do NOT exist (inventory: only ModInv /
// Congruence / PolyEvalMod / RSEncode), so field_add/field_mul are a
// HANDOFF SPEC, not half-wired names.
//
// Exactness: results are `Value::I64` and the equality asserts pin the
// I64 variant — a float cast would produce `Value::F64` and fail.

#[test]
fn aj8d_field_inv_value_law() {
    // field_inv(a, p) = a^-1 mod p over the prime field, exactly:
    // 3^-1 ≡ 5 (mod 7) and 2^-1 ≡ 3 (mod 5).
    let out = eval(
        vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
        &[Value::I64(3), Value::I64(7)],
    )
    .expect("field_inv(3, 7) computes");
    assert_eq!(out, Value::I64(5), "3^-1 ≡ 5 (mod 7) — exact I64");

    let out = eval(
        vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
        &[Value::I64(2), Value::I64(5)],
    )
    .expect("field_inv(2, 5) computes");
    assert_eq!(out, Value::I64(3), "2^-1 ≡ 3 (mod 5) — exact I64");
}

#[test]
fn aj8d_field_inv_refusals_typed() {
    // A non-invertible a (gcd(a, p) ≠ 1) and a non-positive modulus are
    // TYPED Arithmetic refusals from the kernel, never a panic and never
    // a silent answer.
    let fault = eval(
        vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
        &[Value::I64(2), Value::I64(4)],
    )
    .expect_err("field_inv(2, 4) must refuse: gcd(2,4) = 2 ≠ 1");
    assert!(
        matches!(fault, EvalFault::Arithmetic { .. }),
        "a non-invertible value must be a typed Arithmetic fault, got {fault:?}"
    );

    let fault = eval(
        vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
        &[Value::I64(3), Value::I64(0)],
    )
    .expect_err("field_inv(3, 0) must refuse: modulus must be positive");
    assert!(
        matches!(fault, EvalFault::Arithmetic { .. }),
        "a non-positive modulus must be a typed Arithmetic fault, got {fault:?}"
    );
}

#[test]
fn aj8d_field_inv_term_surface_values() {
    // The closed call surface: `field_inv(a, p)` compiles to ModInv and
    // evaluates exactly. RED before the term_compile arm landed (the
    // operator was unknown to the closed vocabulary).
    let value = call_eval(
        opt_apply("field_inv", vec![opt_const("3"), opt_const("7")]),
        &["3", "7"],
    )
    .expect("field_inv(3, 7) compiles and evaluates");
    assert_eq!(
        value,
        Value::I64(5),
        "field_inv(3, 7) = 5 through the call surface"
    );
}

#[test]
fn aj8d_field_inv_arity_shape_refused_typed() {
    // field_inv() with zero args refuses as a typed ARITY refusal; a
    // Vector in the scalar operand slot refuses as a typed SHAPE refusal.
    // Never a panic, never an empty lowering.
    let zero = opt_apply("field_inv", Vec::new());
    let error = compile_reference(
        &zero,
        &call_signature(&[]),
        &[],
        Vec::new(),
        "test.field-call",
    )
    .expect_err("field_inv() refuses at compile");
    assert!(
        matches!(error, TermCompileError::ArityMismatch { .. }),
        "zero-arg field_inv must be a typed arity refusal, got {error:?}"
    );

    let term = opt_apply(
        "field_inv",
        vec![Term::Variable(VariableId("v".into())), opt_const("7")],
    );
    let error = compile_reference(
        &term,
        &call_signature(&["7"]),
        &[("v".to_string(), ParamShape::Vector)],
        Vec::new(),
        "test.field-call",
    )
    .expect_err("field_inv over a Vector refuses at compile");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "a Vector in the scalar slot must refuse typed, got {error:?}"
    );
}

// ── Pass 9: metamorphic / property-encoded laws + mutation kills ─────
//
// TEST-ONLY pass (no production edits). Each law is table-driven over a
// value set (deterministic loops, no proptest — the test crate's
// Cargo.toml has no such dep and heavy deps are banned). Every law is
// written in the strongest DISCRIMINATING form: a row that currently
// passes and cannot be failed by any documented mutant is a tautology and
// is DELETED rather than kept (anti-slop RULE 0.1).
//
// - Mutable multiple-of-p and zero operands must FAULT typed (kills a
//   "total silence" mutant that returns an answer for a zero divisor).
// - Concrete expected values (field_inv(3,7)==5) fail under a "wrong
//   inverse" mutant that returns e.g. inv(a)=a-1.
// - Graph relabel uses DIFFERENT-reachability graphs so the metamorphic
//   value changes (not writes a coincidentally-equal mask).

//
// (1) Option/Result laws — metamorphic identity + error channel.
//

#[test]
fn aj8d_meta_unwrap_or_identity_law() {
    // unwrap_or(some(x), d) == x (the payload, never the default) and
    // unwrap_or(none, d) == d, over a payload vector. The interpreter's
    // default is EAGER (register discipline); both branches agree on the
    // VALUE equality here, so the law asserts the value, not laziness.
    let payloads: [f64; 7] = [
        -1e9, -0.5, 0.0, 1.5, 42.0, 9007199254740992.0, // 2^53 exact
        -0.0,
    ];
    for &x in &payloads {
        let some = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(x))
            .expect("Some(x) computes");
        let out = eval(
            vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
            &[some, Value::F64(7.0)],
        )
        .expect("some(x).unwrap_or(d) computes (total)");
        assert_eq!(out, Value::F64(x), "Some({x}).unwrap_or(7) == {x} (identity)");
    }
    for &d in &payloads {
        let none = eval1(EmirOp::OptionNone, Value::F64(0.0)).expect("None computes");
        let out = eval(
            vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
            &[none, Value::F64(d)],
        )
        .expect("none.unwrap_or(d) computes (total)");
        assert_eq!(out, Value::F64(d), "None.unwrap_or({d}) == {d} (identity)");
    }
}

#[test]
fn aj8d_meta_result_error_channel_law() {
    // Metamorphic duality: is_some(error_of(r)) == !is_ok(r) over both
    // branches, plus the error round-trip err(x).error_of().unwrap_or(y)
    // == x. Table-driven over a payload set.
    let payloads: [f64; 4] = [-2e9, 0.0, 7.5, 4503599627370496.0];
    for &x in &payloads {
        for &ok in &[true, false] {
            let ctor = if ok {
                EmirOp::ResultOk(EmirValue(0))
            } else {
                EmirOp::ResultErr(EmirValue(0))
            };
            let r = eval1(ctor, Value::F64(x)).expect("constructor computes");
            let error_of = eval1(EmirOp::ResultErrorOf(EmirValue(0)), r.clone())
                .expect("error_of computes");
            let is_some_eo =
                eval1(EmirOp::OptionIsSome(EmirValue(0)), error_of.clone()).expect("is_some computes");
            let is_ok = eval1(EmirOp::ResultIsOk(EmirValue(0)), r).expect("is_ok computes");
            assert_eq!(
                bool_of(&is_some_eo),
                !bool_of(&is_ok),
                "is_some(error_of(r)) == !is_ok(r) for payload {x}"
            );
            if !ok {
                let rec = eval(
                    vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
                    &[error_of, Value::F64(-1.0)],
                )
                .expect("error round-trip computes");
                assert_eq!(
                    rec,
                    Value::F64(x),
                    "err({x}).error_of().unwrap_or(-1) == {x} (round-trip)"
                );
            }
        }
    }
}

#[test]
fn aj8d_meta_double_wrap_associativity() {
    // Carrier associativity: some(some(x)).unwrap_or(d) == some(x) — one
    // unwrap pops ONE wrapper, not two (kills a double-unwrap mutant) and
    // not zero (kills a no-unwrap mutant); the result stays Some.
    let payloads: [f64; 3] = [3.0, -5.5, 1e308];
    for &x in &payloads {
        let inner = eval1(EmirOp::OptionSome(EmirValue(0)), Value::F64(x)).expect("Some(x)");
        let outer = eval1(EmirOp::OptionSome(EmirValue(0)), inner.clone()).expect("Some(Some(x))");
        let out = eval(
            vec![EmirOp::OptionUnwrapOr(EmirValue(0), EmirValue(1))],
            &[outer, Value::F64(9.0)],
        )
        .expect("unwrap computes");
        assert_eq!(
            out, inner,
            "Some(Some({x})).unwrap_or(9) == Some({x}) — one wrapper popped"
        );
        let is_some = eval1(EmirOp::OptionIsSome(EmirValue(0)), out).expect("is_some computes");
        assert!(bool_of(&is_some), "double-wrap unwrap_or keeps Some for {x}");
    }
}

//
// (2) Graph relabel — metamorphic permutation equivariance.
//
// `Graph` is the dense `Matrix<Float64>` adjacency alias. Relabel =
// permute rows AND cols. The reachability/out_degrees ops are reachable
// through `compile_reference`'s compile_call even though they are not in
// the Option/Result CALL_SURFACE_DECLS above; a local graph signature
// declares their arity so the shared const stays untouched (released P3
// surface). No `graph_algorithms.rs` (other agents' file) is edited.
//

/// Local signature declaring the graph term-surface operators we probe.
fn graph_signature() -> Signature {
    let mut signature = Signature::default();
    for (name, arity) in [("reachability", 2usize), ("out_degrees", 1usize)] {
        signature
            .insert(SymbolId(name.to_string()), arity)
            .expect("graph decls are conflict-free");
    }
    signature
}

/// Compile + evaluate a graph-call term over declared params.
fn graph_eval(
    term: Term,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    let cell = compile_reference(
        &term,
        &graph_signature(),
        &params,
        Vec::new(),
        "test.graph-relabel",
    )
    .expect("graph relabel program compiles");
    evaluate_with_budget(&cell.program, inputs, &[], EvalBudget::default())
}

/// new_mask[P[i]] = old_mask[i]: a permutation P (old→new) applied to a
/// per-vertex vector.
fn permute_vector(p: &[usize], value: &Value) -> Value {
    let Value::Vector(data) = value else {
        panic!("expected a vector, got {value:?}")
    };
    let mut out = vec![0.0; data.len()];
    for (i, &v) in data.iter().enumerate() {
        out[p[i]] = v;
    }
    Value::Vector(out)
}

/// A'[P[i]][P[j]] = A[i][j]: relabel (conjugate) the adjacency matrix.
fn permute_matrix(p: &[usize], value: &Value) -> Value {
    let Value::Matrix { rows, cols, data } = value else {
        panic!("expected a matrix, got {value:?}")
    };
    assert_eq!(rows, cols, "relabel needs a square adjacency");
    let n = *rows as usize;
    let mut out = vec![0.0; data.len()];
    for i in 0..n {
        for j in 0..n {
            let a = data[i * n + j];
            out[p[i] * n + p[j]] = a;
        }
    }
    Value::Matrix {
        rows: *rows,
        cols: *cols,
        data: out,
    }
}

#[test]
fn aj8d_meta_graph_relabel_reachability_equivariance() {
    // Adjacency: edges 0->1, 1->3, 2->3 — vertex 2 is NOT reachable
    // from 0 and vertex 3 is not a fork from 0. Relative reachability
    // differs per vertex so a relabel that permutes ENDPOINTS changes the
    // reachability mask (not coincidentally equal). LAW: for every
    // permutation P (old->new) and every source,
    //   reachability(A', P(src)) == P ⊳ reachability(A, src)
    let adj = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 0.0, //
        ],
    };
    let permutations: &[&[usize]] = &[
        &[1, 2, 3, 0], // rotation
        &[3, 1, 0, 2], // derangement
        &[0, 1, 2, 3], // identity (control)
    ];
    let term = opt_apply(
        "reachability",
        vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("s".into())),
        ],
    );
    for &p in permutations {
        for src in 0u32..4 {
            let orig = graph_eval(
                term.clone(),
                vec![
                    ("a".to_string(), ParamShape::Matrix),
                    ("s".to_string(), ParamShape::Scalar),
                ],
                &[adj.clone(), Value::F64(src as f64)],
            )
            .expect("reachability(A, src) computes");
            let expected = permute_vector(p, &orig);
            let a_perm = permute_matrix(p, &adj);
            let got = graph_eval(
                term.clone(),
                vec![
                    ("a".to_string(), ParamShape::Matrix),
                    ("s".to_string(), ParamShape::Scalar),
                ],
                &[a_perm, Value::F64(p[src as usize] as f64)],
            )
            .expect("reachability(A', P(src)) computes");
            assert_eq!(
                got, expected,
                "relabel P={p:?}: reachability(A', {0}) must equal P ⊳ \
                 reachability(A, {0})",
                p[src as usize]
            );
        }
    }
}

#[test]
fn aj8d_meta_graph_relabel_out_degrees_equivariance() {
    // out_degrees(A')[u] = out_degrees(A)[Pinv(u)] — row sums permute
    // under a relabel: P ⊳ out_degrees(A). Values CHANGE under the
    // rotation (the discriminant is a real permutation of the degree
    // vector, not a coincidental fixpoint).
    let adj = Value::Matrix {
        rows: 4,
        cols: 4,
        data: vec![
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            0.0, 0.0, 0.0, 0.0, //
        ],
    };
    let p: &[usize] = &[1, 2, 3, 0];
    let term = opt_apply("out_degrees", vec![Term::Variable(VariableId("a".into()))]);
    let orig = graph_eval(
        term.clone(),
        vec![("a".to_string(), ParamShape::Matrix)],
        &[adj.clone()],
    )
    .expect("out_degrees(A) computes");
    let expected = permute_vector(p, &orig);
    let a_perm = permute_matrix(p, &adj);
    let got = graph_eval(
        term.clone(),
        vec![("a".to_string(), ParamShape::Matrix)],
        &[a_perm],
    )
    .expect("out_degrees(A') computes");
    assert_eq!(
        got, expected,
        "relabel P={p:?}: out_degrees(A') must equal P ⊳ out_degrees(A)"
    );
}

//
// (3) Finite-field algebra — metamorphic involution + totality + controls.
//
// WITHIN the no-claim boundary (only field_inv + generic i64 integer
// family; field_add/field_mul not registered). The generic-i64 `mul` is
// reachable, but there is NO integer remainder/mod at the term surface
// and NO `congruence` name, so the inverse-product law a*inv(a) ≡ 1 (mod p)
// is NOT reachable — reported, not implemented. The reduced form is
// asserted instead: involution, range, totality, and concrete anchors.
//

#[test]
fn aj8d_meta_field_involution_range_totality() {
    // For every prime p in scope and every nonzero a in 1..p-1:
    //  (a) totality: field_inv(a,p) computes with NO EvalFault;
    //  (b) range:   field_inv(a,p) in 1..p-1 (never 0, never ≥ p);
    //  (c) anchor:  field_inv(1,p) == 1;
    //  (d) involution: field_inv(field_inv(a,p), p) == a.
    // (a) discriminates "total silence" (a fault-suppressing mutant); (b)
    // and (d) discriminate wrong-inverse and out-of-range mutants.
    let primes = [3i64, 5, 7, 13];
    for &p in &primes {
        let inv1 = eval(
            vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
            &[Value::I64(1), Value::I64(p)],
        )
        .expect("field_inv(1, p) computes");
        assert_eq!(inv1, Value::I64(1), "1^-1 == 1 (mod {p}) — exact I64");
        for a in 1..p {
            let inv = eval(
                vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
                &[Value::I64(a), Value::I64(p)],
            )
            .expect("field_inv(a,p) computes (totality)");
            let Value::I64(b) = inv else {
                panic!("field_inv must be exact I64 for a={a} p={p}, got {inv:?}")
            };
            assert!(
                1 <= b && b < p,
                "range: field_inv({a},{p}) = {b} must lie in 1..{p}-1"
            );
            let inv_inv = eval(
                vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
                &[Value::I64(b), Value::I64(p)],
            )
            .expect("field_inv(inv(a),p) computes");
            assert_eq!(
                inv_inv,
                Value::I64(a),
                "involution: field_inv(field_inv({a},{p}),{p}) == {a}"
            );
        }
    }
}

#[test]
fn aj8d_meta_field_negative_controls_discriminate() {
    // NEGATIVE CONTROLS: operands with gcd(a, p) ≠ 1 — a equal to the
    // modulus and a zero representative — must FAULT typed, never return
    // a silent answer. These fail every "total silence" mutant: an
    // implementation that returned *something* (e.g. 0, or a / a = 1) for
    // a zero divisor without faulting would pass naive positive laws yet
    // FAIL these rows. (p prime: gcd(a,p)≠1 iff a ≡ 0 mod p.)
    let primes = [3i64, 5, 7, 13];
    for &p in &primes {
        for &a in &[p, 0i64] {
            let fault = eval(
                vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
                &[Value::I64(a), Value::I64(p)],
            )
            .expect_err("field_inv({a}, {p}) must fault: gcd != 1");
            assert!(
                matches!(fault, EvalFault::Arithmetic { .. }),
                "field_inv({a}, {p}) must be a TYPED Arithmetic fault — \
                 a silent answer would be a correctness bug; got {fault:?}"
            );
        }
    }
}

#[test]
fn aj8d_meta_field_concrete_anchors_discriminate() {
    // CONCRETE anchors: hand-computed inverses that fail under a
    // "wrong inverse" mutant (inv(a) := a-1, or inv(a) := 2a mod p).
    // 3^{-1} ≡ 5 (mod 7): 3*5 = 15 ≡ 1 (mod 7).
    // 2^{-1} ≡ 3 (mod 5): 2*3 = 6 ≡ 1 (mod 5).
    // 5^{-1} ≡ 3 (mod 7): 5*3 = 15 ≡ 1 (mod 7).
    // 3^{-1} ≡ 5 (mod 13): 3*5 = 15 ≡ 2? no — 3*9=27 ≡ 1 (mod 13).
    for (a, p, want) in [
        (3i64, 7i64, 5i64),
        (2, 5, 3),
        (5, 7, 3),
        (3, 13, 9),
    ] {
        let out = eval(
            vec![EmirOp::ModInv(EmirValue(0), EmirValue(1))],
            &[Value::I64(a), Value::I64(p)],
        )
        .expect("field_inv(a,p) computes");
        assert_eq!(
            out,
            Value::I64(want),
            "concrete anchor: {a}^-1 == {want} (mod {p})"
        );
    }
}

// --- Pass 6: universal int_rem (aj8d pass 6) ---
// Exact-Euclidean remainder `a.rem_euclid(m)` on i64. Result is always
// Value::I64 (no float cast anywhere — the 2^31 / 2 exactness case proves
// i64 path). m <= 0 is a typed EvalFault::Arithmetic, never a panic.

/// int_rem concrete exact values (Euclidean, non-negative): 7 rem 7 = 0,
/// 5 rem 7 = 5, and the sign law int_rem(-1, 7) = 6.
#[test]
fn aj8d_int_rem_value_law() {
    for (a, m, want) in [(7i64, 7i64, 0), (5, 7, 5), (-1, 7, 6), (13, 7, 6)] {
        let out = eval(
            vec![EmirOp::IntRem(EmirValue(0), EmirValue(1))],
            &[Value::I64(a), Value::I64(m)],
        )
        .expect("int_rem(a, m) computes");
        assert_eq!(out, Value::I64(want), "int_rem({a}, {m}) = {want} (Euclidean)");
    }
}

/// Exactness: int_rem(2^31, 2) = 0 as a REAL i64 (no value may fall back
/// to f64 in the exact-integer path).
#[test]
fn aj8d_int_rem_exact_i64_large() {
    let out = eval(
        vec![EmirOp::IntRem(EmirValue(0), EmirValue(1))],
        &[Value::I64(2_147_483_648), Value::I64(2)],
    )
    .expect("int_rem(2^31, 2) computes");
    assert_eq!(
        out,
        Value::I64(0),
        "int_rem(2^31, 2) must be I64-exact 0 (no float path), got {out:?}"
    );
}

/// m <= 0 is a TYPED Arithmetic fault (modulus must be positive), never a
/// panic and never a silent truncated result.
#[test]
fn aj8d_int_rem_zero_modulus_faults_ir() {
    let fault = eval(
        vec![EmirOp::IntRem(EmirValue(0), EmirValue(1))],
        &[Value::I64(5), Value::I64(0)],
    )
    .expect_err("int_rem(5, 0) must refuse: modulus must be positive");
    assert!(
        matches!(fault, EvalFault::Arithmetic { .. }),
        "int_rem(5, 0) must be a typed Arithmetic fault, got {fault:?}"
    );
    let neg = eval(
        vec![EmirOp::IntRem(EmirValue(0), EmirValue(1))],
        &[Value::I64(5), Value::I64(-3)],
    )
    .expect_err("int_rem(5, -3) must refuse: modulus must be positive");
    assert!(
        matches!(neg, EvalFault::Arithmetic { .. }),
        "int_rem(5, -3) must be a typed Arithmetic fault, got {neg:?}"
    );
}
