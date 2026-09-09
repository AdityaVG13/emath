use std::collections::BTreeMap;
use std::process::Command;

use emath_core::{QualifiedName, Span};
use emath_ir::{
    BinaryOp, BinderKind, BinderVariable, CompileSpec, Constructor, Declaration, DeclarationId,
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExprId, ExprNode, Extent, FallbackPolicy,
    Field, Goal, GoalId, GoalKind, GoalPayload, GoalRequirements, Literal, ObligationClass,
    ObligationKind, SemanticPackage, SliceAxis, TargetProfile, TestCase, TypeId, TypeNode, UnaryOp,
    Visibility,
};
use emath_rust_backend::BackendInput;
use emath_rust_backend::rust_ir::ast::{Item, StructDef};
use emath_rust_backend::rust_ir::render::render_module;
use emath_test_harness::Probe;

/// A minimal package: one declaration `named` with an `x: Float64`
/// input, nothing else. Enough to exercise struct emission, which is
/// where declaration names become Rust source.
fn package_for(named: &str) -> SemanticPackage {
    let mut package = SemanticPackage::new();
    package.types.push(TypeNode::Float64);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName(named.to_string()),
        kind: QualifiedName("policy".to_string()),
        kind_label: "policy".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: Vec::new(),
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    package
}

fn eval_requirements() -> GoalRequirements {
    GoalRequirements {
        evidence: EvidenceLevel::E1,
        exactness: ExactnessPolicy::Exact,
        determinism: DeterminismPolicy::Required,
        target: TargetProfile {
            family: "rust-library".to_string(),
            triple: None,
            features: Vec::new(),
        },
        fallback: FallbackPolicy::NativeOnly,
        produce: "rust.library".to_string(),
    }
}

fn generate_fn(
    name: &str,
    inputs: &[&str],
    y_def: ExprId,
    package: &mut SemanticPackage,
) -> String {
    let ty = package.push_type(TypeNode::Float64);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single(name),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: inputs
            .iter()
            .map(|input| Field {
                name: (*input).to_string(),
                ty,
                visibility: Visibility::Public,
                source: Span::default(),
            })
            .collect(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package,
        crate_name: name.to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .unwrap_or_else(|err| panic!("{name} must generate: {err}"));
    output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs")
        .clone()
}

fn extract_fn(src: &str, name: &str) -> String {
    let marker = format!("pub fn {name}");
    match src.find(&marker) {
        Some(start) => src[start..].chars().take(800).collect(),
        None => panic!("missing `{marker}` in:\n{src}"),
    }
}

/// Single-use inlining must keep non-associative grouping: `a - (b - c)`
/// is not `a - b - c`, and `(a + b) * c` is not `a + b * c`.

/// Flattening used to emit braceless `if cond then else`, which is not
/// valid Rust and dropped the Select from generated crates. Arms must be
/// blocks so the value matches eager SSA (taken arm) and the crate compiles.

/// Method receivers must parenthesize inlined sums: `(a + b).sin()`, not
/// `a + b.sin()`.

fn generate_typed(
    name: &str,
    output_ty: TypeNode,
    y_def: ExprId,
    package: &mut SemanticPackage,
) -> String {
    let ty = package.push_type(output_ty);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single(name),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package,
        crate_name: name.to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .unwrap_or_else(|err| panic!("{name} must generate: {err}"));
    output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs")
        .clone()
}

/// `2^53+1` cannot round-trip through f64. rust.library must emit an i64
/// literal, matching interp `Value::I64`, not `(value as f64)`.

/// Mixed Int/Float64 `==` used to widen through `as f64`, so this
/// constant pair folded to `true`. Exact compare folds to `false`.

/// `factorial(20)` is exact i64 in interp and emath-rt; codegen must call
/// the i64 kernel and not cast the result to f64.

/// `einsum("ik,kj->ij", A, B)` must call the emath-rt kernel, not emit
/// `panic!("einsum ... not yet implemented")`.

/// rust.library used to emit panicking `v[i as usize]`. OOB is a typed
/// `Result` via `vec_index_checked`.

/// `t[0, :, :]` used to clone the whole tensor. It must emit the slice
/// kernel and produce a matrix (tensor-face.emath identity).

/// `product i in 1..=20: i` stays on the exact i64 fold (interp
/// `Value::I64(20!)`), not f64 `fold_mul`.

/// Folded IEEE non-finite constants (`sqrt(-1)` → NaN, `1/0` → Inf) must
/// render as valid Rust, not Debug `NaN`/`inf` identifiers.

/// `sign(0)` is mathematical 0, not IEEE `signum` (±1 at ±0). rust.library
/// must emit the same zero-check the interp builtin uses.

// ──: strict Rust backend Option/Result parity ─────────────
// The nine carrier ops must lower through the REAL generation path
// (SemanticPackage → emitter → EmirProgram → BackendInput::generate →
// rust-ir) into executable native Option<T>/Result<T, E> code with typed
// shape errors. Behavior is proven by compiling the generated crate and
// RUNNING it (rustc-direct, no new cargo crate; test-only infra).

fn carrier_int(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(name, TypeNode::Int, y_expr, package);
}

fn carrier_bool(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(name, TypeNode::Bool, y_expr, package);
}

fn carrier_vec(name: &str, y_expr: ExprId, package: &mut SemanticPackage) {
    carrier_decl(
        name,
        TypeNode::Vector {
            element: Box::new(TypeNode::Float64),
            extent: None,
        },
        y_expr,
        package,
    );
}

fn carrier_decl(name: &str, output_ty: TypeNode, y_expr: ExprId, package: &mut SemanticPackage) {
    let ty = package.push_type(output_ty);
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_expr),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_expr);
    package.declarations.push(Declaration {
        id: DeclarationId(package.declarations.len() as u32),
        name: QualifiedName::single(name),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
}

/// Real-path generation of the test package → generated src/lib.rs.
fn generate_carrier_lib(package: &SemanticPackage) -> String {
    BackendInput {
        package,
        crate_name: "opt_result_carrier".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("option/result carrier package must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone()
}

/// Generated user code (everything before the embedded `emath_rt` module).
fn user_section(lib: &str) -> &str {
    lib.split("mod emath_rt").next().unwrap_or(lib)
}

/// Compile the generated crate and run `main_body` against it with
/// rustc-direct (edition 2024, std-only generated crate; the embedded
/// emath_rt kernel source is pure std). Behavior failures surface as
/// non-zero exit (assert!) with the message in stderr.

fn demand_run(p: &mut Probe, name: &str, result: Result<(), String>) {
    match result {
        Ok(()) => {
            p.demand(name, true, "generated crate ran");
        }
        Err(error) => {
            p.fail(name, error);
        }
    }
}

fn run_generated(lib: &str, main_body: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "emath_opt_carrier_run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdtemp: {e}"))?;
    let result = run_generated_in(&dir, lib, main_body);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn run_generated_in(dir: &std::path::Path, lib: &str, main_body: &str) -> Result<(), String> {
    let lib_path = dir.join("generated.rs");
    std::fs::write(&lib_path, lib).map_err(|e| format!("write lib: {e}"))?;
    let driver = format!(
        "#[path = \"{}\"]\nmod generated;\nfn main() {{\n{main_body}\n}}\n",
        lib_path.display()
    );
    let main_path = dir.join("main.rs");
    let bin_path = dir.join("run");
    std::fs::write(&main_path, driver).map_err(|e| format!("write driver: {e}"))?;
    let comp = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-name")
        .arg("opt_carrier_run")
        .current_dir(dir)
        .arg(&main_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .map_err(|e| format!("rustc spawn failed (is rustc on PATH?): {e}"))?;
    if !comp.status.success() {
        return Err(format!(
            "generated crate failed to compile:\n{}\n--- generated lib ---\n{lib}",
            String::from_utf8_lossy(&comp.stderr)
        ));
    }
    let run = Command::new(&bin_path)
        .output()
        .map_err(|e| format!("run generated binary: {e}"))?;
    if !run.status.success() {
        return Err(format!(
            "generated behavior failed (exit {:?}):\n{}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(())
}

/// some(some carrier) round trips its payload; none returns the eager
/// default; is_some polarity holds — all in EXECUTED generated Rust.

/// ok/err carry their payloads, is_ok polarity holds, unwrap_or's
/// default is taken only on Err, and error_of composes the error as an
/// Option (Ok → None, Err → Some(payload)) — in EXECUTED generated Rust.

/// Wrong-carrier use is a TYPED lowering refusal (interp TypeConfusion
/// parity), surfaced as `BackendError::Lowering`, never a panic and
/// never a silent scalar shadow.

// ──: hardened backend typed refusals ────────────────────

fn literal_int(p: &mut SemanticPackage, s: &str) -> ExprId {
    p.push_expr(
        ExprNode::Literal(Literal::Integer(s.to_string())),
        Span::default(),
    )
}

/// A carrier passed into the eager-default slot of an `unwrap_or` is a
/// typed payload-kind conflict, never a silent scalar shadow of the
/// default. (Backend `CarrierPayloadTypes` resolves i64 from the carrier
/// producer vs f64 from the carrier default on the SAME register → REFUSE.)

/// A `Field<7>` output field is not representable as a built-in Phase 1
/// Rust type; rust.library refuses it typed (naming the prime-field
/// spelling), never treating it as f64. (Sema ADMITS Field<7>; the
/// backend is the enforcement boundary for field-FIELD types.)

/// A `Field<7>` input field refuses typed for the same reason a scalar
/// Step is admitted — the field spelling is not a native Phase 1 scalar.

/// A `Option<Int>` / `Result<Int, Bool>` input FIELD also refuses typed
/// at the backend (Phase 1 has no native carrier RUST type on decl
/// fields; carrier VALUES only exist in expressions via the nine ops,
/// confirmed in ). This pins the boundary: no field of a carrier
/// or field spelling can silently lower to f64.

// ──: TEXT-driven carrier parity + nested carrier parity ───
// Build the SemanticPackage by parsing REAL .emath source (sema →
// package), then generate + execute the native Option/Result Rust. These
// close the loop the hand-built tests start: the USER surface's
// executable carriers carry through to generated native types, and
// nested carriers now resolve to native nested types (Option<Option<T>>)
// instead of collapsing the inner payload to f64.

fn text_package(p: &mut Probe, source: &str) -> SemanticPackage {
    emath_syntax::install_source_parser();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let file = session.load_text("rust-backend-text.emath", source);
    let planned = session.plan(file);
    let errors: Vec<String> = planned
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect();
    p.demand("text_package", errors.is_empty(), format!("text must admit: {errors:?}\n{source}"));
    planned.package
}

/// Generate src/lib.rs from real .emath text (sema → package → backend).
fn text_lib(p: &mut Probe, source: &str) -> String {
    let package = text_package(p, source);
    BackendInput {
        package: &package,
        crate_name: "opt_result_carrier".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("text package must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone()
}

/// A declared `Option<Int>` OUTPUT whose definition lifts its input into
/// option_some: the generated free function carries the native Option<i64>
/// OUT of the declaration, and the runtime value is Some(5).

/// NESTED carrier: option_some(option_some(k)) typed as
/// Option<Option<Int>> emits the native nested type and round-trips to
/// Some(Some(5)).

/// Nested Some(None): outer Some carries an inner None (the tag-vs-content
/// distinction is preserved through the nested native type). The declared
/// input names the I/O surface (L3 E-SEC-130) while the nested carrier
/// stays constant.

/// map-by-declared-composition over a scalar input: the generated Rust
/// shows a native Option produced by the composed if/else (no carrier
/// input field — inputs stay scalar, the carrier is the output).

// ──: int_rem exact-Euclidean remainder; field ops as data ──
// Field +/*/inverse are user capability-cell DATA over the universal
// int_rem primitive. Generated Rust must emit exact `.rem_euclid(` and the
// runtime values must match the interpreter (field7_add(3,4)=0,
// field7_mul(3,4)=5, int_rem(-1,7)=6 — the negative case the truncated `%`
// mutant must kill).

/// field7_add: `c = int_rem(a + b, 7)` with an Int output and an evaluate
/// goal — the generated free fn computes the exact Field<7> sum.

/// field7_mul: `c = int_rem(a * b, 7)`. Generated Rust matches.

/// Euclid sign law in generated Rust: int_rem(-1, 7) == 6. This assertion
/// FAILS under the truncated-`%` mutant (which yields -1), so it must be
/// present BEFORE the remainder-sign mutation runs.

// ──: emath-bibqi — vec-index-by-binder inside a sum-fold guard ───
// CopperGorge's compiled-probe repro (mail 206 /
// internal/proximity-prize/census/admit-probe.emath, fn idx_by_binder):
// the i64 fold closure is a PLAIN `Fn(i64) -> i64`, so a checked
// vec-index inside the fold guard rendered `.map_err(|e| e.to_string())?`
// and rustc refused with E0277 ("the ? operator can only be used in a
// closure that returns Result or Option"). `emath check` passes, so the
// user hit raw rustc text. Failure-first: these tests ran RED against
// that binary (E0277 at generated lib.rs) and only turn green once the
// fold context renders checked ops as panic-with-real-runtime-message —
// the documented i64 fold fault channel (fold_add_i64 overflow panic).

/// The exact repro source (fn idx_by_binder of the census admit probe).
fn fold_guard_repro_source() -> &'static str {
    "emath function idx_by_binder:\n    inputs:\n        p: Int\n        xs: Vector[Int]\n    outputs:\n        c: Int\n    definitions:\n        c = sum i in 0..4 if int_rem(xs[i], p) == 0: 1\n    goals:\n        evaluate <c>:\n            produce rust.library\n"
}

/// The second repro fn (inline_coeff_poly): nested i64 folds whose INNER
/// guard indexes xs[i] and w[i] by binder variables, with an inline
/// coefficient vector in poly_eval_mod. Every fold closure here is
/// plain-valued, so no `?` may render anywhere in the user code.

/// A non-i64 fold whose body faults (f64 sum of 1.0 over a binder index)
/// renders the checked-fold call as panic-with-real-message when nested
/// inside a plain fold closure (its `?` would otherwise land in an i64
/// closure). Value parity with the interpreter: inner counts never reach
/// 2.5, so c is 0.

#[test]
fn probe() {
    let mut p = Probe::new("rust backend escapes keywords, emits real evaluators, and refuses wrong carriers");
    p.case("keyword_declaration_name_is_escaped_in_generated_rust", |p| case_keyword_declaration_name_is_escaped_in_generated_rust(p));
    p.case("generated_constructor_carries_its_construction_receipt", |p| case_generated_constructor_carries_its_construction_receipt(p));
    p.case("keyword_crate_name_is_escaped_in_manifest", |p| case_keyword_crate_name_is_escaped_in_manifest(p));
    p.case("expect_less_example_generates_computation_without_assert", |p| case_expect_less_example_generates_computation_without_assert(p));
    p.case("constant_only_declaration_generates_parameterless_method", |p| case_constant_only_declaration_generates_parameterless_method(p));
    p.case("stateless_declaration_emits_free_function", |p| case_stateless_declaration_emits_free_function(p));
    p.case("chained_definitions_emit_let_bindings_in_source_order", |p| case_chained_definitions_emit_let_bindings_in_source_order(p));
    p.case("causalized_model_emits_newton_step_methods", |p| case_causalized_model_emits_newton_step_methods(p));
    p.case("model_emits_explicit_step_methods", |p| case_model_emits_explicit_step_methods(p));
    p.case("flatten_preserves_non_associative_grouping", |p| case_flatten_preserves_non_associative_grouping(p));
    p.case("flatten_select_emits_blocked_if", |p| case_flatten_select_emits_blocked_if(p));
    p.case("flatten_method_receiver_parenthesizes_sum", |p| case_flatten_method_receiver_parenthesizes_sum(p));
    p.case("const_i64_past_f64_mantissa_stays_i64", |p| case_const_i64_past_f64_mantissa_stays_i64(p));
    p.case("mixed_i64_f64_eq_folds_false_not_widened_true", |p| case_mixed_i64_f64_eq_folds_false_not_widened_true(p));
    p.case("factorial_twenty_calls_i64_kernel", |p| case_factorial_twenty_calls_i64_kernel(p));
    p.case("einsum_codegen_calls_rt_kernel_not_panic_stub", |p| case_einsum_codegen_calls_rt_kernel_not_panic_stub(p));
    p.case("vector_index_codegen_uses_checked_helper_not_index", |p| case_vector_index_codegen_uses_checked_helper_not_index(p));
    p.case("tensor_face_slice_codegen_is_not_a_clone", |p| case_tensor_face_slice_codegen_is_not_a_clone(p));
    p.case("integer_product_fold_uses_i64_kernel", |p| case_integer_product_fold_uses_i64_kernel(p));
    p.case("folded_nonfinite_constants_emit_valid_rust", |p| case_folded_nonfinite_constants_emit_valid_rust(p));
    p.case("sign_zero_uses_mathematical_sgn", |p| case_sign_zero_uses_mathematical_sgn(p));
    p.case("option_carrier_generated_behaviors", |p| case_option_carrier_generated_behaviors(p));
    p.case("result_carrier_generated_behaviors", |p| case_result_carrier_generated_behaviors(p));
    p.case("wrong_carrier_use_refuses_typed", |p| case_wrong_carrier_use_refuses_typed(p));
    p.case("unwrap_or_carrier_default_refuses_payload_conflict", |p| case_unwrap_or_carrier_default_refuses_payload_conflict(p));
    p.case("field_prime_output_decl_refuses_typed", |p| case_field_prime_output_decl_refuses_typed(p));
    p.case("field_prime_input_decl_refuses_typed", |p| case_field_prime_input_decl_refuses_typed(p));
    p.case("carrier_field_decl_refuses_typed", |p| case_carrier_field_decl_refuses_typed(p));
    p.case("text_option_int_output_carrier_out", |p| case_text_option_int_output_carrier_out(p));
    p.case("text_nested_option_option_int", |p| case_text_nested_option_option_int(p));
    p.case("text_nested_some_none", |p| case_text_nested_some_none(p));
    p.case("text_map_by_composition_emits_native_option", |p| case_text_map_by_composition_emits_native_option(p));
    p.case("text_int_rem_field_add_executes", |p| case_text_int_rem_field_add_executes(p));
    p.case("text_int_rem_field_mul_executes", |p| case_text_int_rem_field_mul_executes(p));
    p.case("text_int_rem_sign_law_generated", |p| case_text_int_rem_sign_law_generated(p));
    p.case("text_fold_guard_vec_index_by_binder_compiles_and_counts", |p| case_text_fold_guard_vec_index_by_binder_compiles_and_counts(p));
    p.case("text_nested_fold_guards_vec_index_compiles", |p| case_text_nested_fold_guards_vec_index_compiles(p));
    p.case("text_fault_fold_nested_in_plain_fold_compiles", |p| case_text_fault_fold_nested_in_plain_fold_compiles(p));
    p.finish();
}

fn case_keyword_declaration_name_is_escaped_in_generated_rust(p: &mut Probe) {
    // `type` is a Rust keyword; the generated struct must be `type_`
    // so the crate compiles (`emath custom <type>` negative control
    // from the earlier repair).
    let package = package_for("type");
    let output = BackendInput {
        package: &package,
        crate_name: "type".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("keyword-named declaration must generate");
    let struct_items: Vec<&StructDef> = output
        .module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(struct_def) => Some(struct_def),
            _ => None,
        })
        .collect();
    p.demand("keyword_declaration_name_is_escaped_in_generated_rust/1", struct_items.iter().all(|def| def.name != "type"), format!("raw keyword must never be emitted: {struct_items:?}"));
    p.demand("keyword_declaration_name_is_escaped_in_generated_rust/2", struct_items.iter().any(|def| def.name == "type_"), format!("expected escaped struct `type_`, got {struct_items:?}"));
    let rendered = render_module(&output.module).code;
    p.demand("keyword_declaration_name_is_escaped_in_generated_rust/3", rendered.contains("struct type_"), format!("rendered module must name the escaped struct, got:\n{rendered}"));
    p.demand("keyword_declaration_name_is_escaped_in_generated_rust/4", output
            .module
            .items
            .iter()
            .all(|item| !matches!(item, Item::Struct(def) if def.name == "type")), "no unescaped keyword struct may reach the output");

}

fn case_generated_constructor_carries_its_construction_receipt(p: &mut Probe) {
    let mut package = package_for("Scorer");
    // exprs: 0 = `scale` (param), 1 = `0.0`, 2 = `scale > 0.0`.
    package
        .exprs
        .push(ExprNode::Variable(QualifiedName("scale".to_string())));
    package
        .exprs
        .push(ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())));
    package.exprs.push(ExprNode::Binary {
        operation: BinaryOp::Greater,
        left: ExprId(0),
        right: ExprId(1),
    });
    package.expr_spans = vec![Span::default(); package.exprs.len()];
    let declaration = &mut package.declarations[0];
    declaration.state = vec![Field {
        name: "scale".to_string(),
        ty: TypeId(0),
        visibility: Visibility::Private,
        source: Span::default(),
    }];
    declaration.constructors.push(Constructor {
        name: "new".to_string(),
        parameters: vec![Field {
            name: "scale".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        preconditions: vec![ExprId(2)],
        assignments: BTreeMap::from([("scale".to_string(), ExprId(0))]),
        postconditions: vec![ExprId(2)],
        defaults: BTreeMap::new(),
        error_type: None,
        is_public: true,
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "scorer".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("constructor package must generate");
    p.eq("generated_constructor_carries_its_construction_receipt/1", output.receipts.len(), 1);
    let receipt = &output.receipts[0];
    p.eq("generated_constructor_carries_its_construction_receipt/2", receipt.declaration.as_str(), "Scorer");
    p.eq("generated_constructor_carries_its_construction_receipt/3", receipt.obligations.len(), 2);
    p.demand("generated_constructor_carries_its_construction_receipt/4", receipt
            .obligations
            .iter()
            .all(|obligation| obligation.class == ObligationClass::Runtime), "Phase 1 discharges every textual obligation at runtime");
    p.eq("generated_constructor_carries_its_construction_receipt/5", receipt.obligations[0].kind, ObligationKind::Precondition);
    p.eq("generated_constructor_carries_its_construction_receipt/6", receipt.obligations[1].kind, ObligationKind::Postcondition);
    p.demand("generated_constructor_carries_its_construction_receipt/7", receipt.open_obligations().is_empty(), "no obligation may remain open on a runtime-discharged receipt");

}

fn case_keyword_crate_name_is_escaped_in_manifest(p: &mut Probe) {
    // Cargo package names may be keywords, but the generated crate
    // must keep a sane rust-identifier crate name for `lib.rs`
    // (`extern crate`/name collisions in dev builds).
    let package = package_for("Demo");
    let output = BackendInput {
        package: &package,
        crate_name: "fn".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("keyword crate name must generate");
    let manifest = output
        .files
        .get("Cargo.toml")
        .expect("generate must emit a manifest");
    p.demand("keyword_crate_name_is_escaped_in_manifest/1", manifest.contains("name = \"fn_\""), format!("keyword crate name must be escaped in the manifest, got:\n{manifest}"));
    p.demand("keyword_crate_name_is_escaped_in_manifest/2", !manifest.contains("name = \"fn\""), "unescaped keyword crate name must not reach the manifest");

}

fn case_expect_less_example_generates_computation_without_assert(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: x,
            right: x,
        },
        Span::default(),
    );
    let four = package.push_expr(
        ExprNode::Literal(Literal::Integer("4".to_string())),
        Span::default(),
    );
    let mut given = BTreeMap::new();
    given.insert("x".to_string(), four);
    let test_id = package.push_test(TestCase {
        name: "four_squared".to_string(),
        given,
        expect: None,
        source: Span::default(),
    });
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Square"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: vec![test_id],
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "square".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("worked example must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("expect_less_example_generates_computation_without_assert/1", lib.contains("let actual") || lib.contains("fn y"), format!("worked example must execute the definition, got:\n{lib}"));
    p.demand("expect_less_example_generates_computation_without_assert/2", lib.contains("let _ ="), format!("worked example must bind the computed value without asserting, got:\n{lib}"));
    // The embedded `emath_rt` module may legitimately contain `assert!`
    // kernel guards; the invariant applies to the generated user code.
    let user_code = lib.split("mod emath_rt").next().unwrap_or(lib);
    p.demand("expect_less_example_generates_computation_without_assert/3", !user_code.contains("assert!("), format!("worked example must not assert, got:\n{lib}"));

}

fn case_constant_only_declaration_generates_parameterless_method(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let three = package.push_expr(
        ExprNode::Literal(Literal::Integer("3".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: three,
            right: seven,
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("TwentyOne"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "twenty_one".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("constant-only declaration must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("constant_only_declaration_generates_parameterless_method/1", lib.contains("fn TwentyOne()") || lib.contains("fn TwentyOne(&self)"), format!("no-input declaration must generate a parameterless evaluator, got:\n{lib}"));
    p.demand("constant_only_declaration_generates_parameterless_method/2", !lib.contains("fn TwentyOne(&self,")
            && !lib.contains("fn TwentyOne(&self ,")
            && !lib.contains("fn TwentyOne(,"), format!("no-input evaluator must not take extra parameters, got:\n{lib}"));

}

fn case_stateless_declaration_emits_free_function(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: x,
            right: x,
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "square".to_string(),
        expression: Some(y_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("square".to_string(), y_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("square"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "square".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "square".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("stateless square must generate");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("stateless_declaration_emits_free_function/1", lib.contains("pub fn square(") && lib.contains("x: f64"), format!("stateless case must emit a free function, got:\n{lib}"));
    p.demand("stateless_declaration_emits_free_function/2", !lib.contains("struct square")
            && !lib.contains("fn square(&self")
            && !lib.contains("fn square(self"), format!("stateless case must not emit a unit struct + method, got:\n{}", extract_fn(lib, "square")));
    p.demand("stateless_declaration_emits_free_function/3", output
            .anchors
            .iter()
            .any(|anchor| anchor.label == "fn square"), format!("source map must anchor the free function, got {:?}", output.anchors));

}

fn case_chained_definitions_emit_let_bindings_in_source_order(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let file = Span::default().file;
    let two = package.push_expr(
        ExprNode::Literal(Literal::Integer("2".to_string())),
        Span::new(file, 1, 2),
    );
    let a_var = package.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::new(file, 3, 4),
    );
    let b_def = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: a_var,
            right: a_var,
        },
        Span::new(file, 5, 6),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "b".to_string(),
        expression: Some(b_def),
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".to_string(),
                triple: None,
                features: Vec::new(),
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".to_string(),
        },
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("a".to_string(), two);
    definitions.insert("b".to_string(), b_def);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Chain"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: Vec::new(),
        outputs: vec![Field {
            name: "b".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "chain".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("chained definitions must lower");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("chained_definitions_emit_let_bindings_in_source_order/1", lib.contains("let a =") && lib.contains("pub fn Chain("), format!("evaluate b must let-bind earlier definition a, got:\n{lib}"));

}

fn case_causalized_model_emits_newton_step_methods(p: &mut Probe) {
    // Fully implicit DAEs (Newton-solved residuals) now codegen the same
    // causalized Newton solve the interpreter runs: embedded Gaussian
    // helpers, residual closures over the flat solve vector, the 30/1e-9
    // budget mirrored from `causal_newton`, and Result-typed steps.
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let v = package.push_expr(
        ExprNode::Variable(QualifiedName::single("V")),
        Span::default(),
    );
    let i = package.push_expr(
        ExprNode::Variable(QualifiedName::single("I")),
        Span::default(),
    );
    let q = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.q")),
        Span::default(),
    );
    // `(V - I) - q`
    let v_minus_i = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: v,
            right: i,
        },
        Span::default(),
    );
    let residual_expr = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: v_minus_i,
            right: q,
        },
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_q".to_string(), i);
    package
        .expr_spans
        .resize(package.exprs.len(), Span::default());
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Causalized"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: vec![Field {
            name: "V".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: Vec::new(),
        state: vec![Field {
            name: "q".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        algebraic: vec![Field {
            name: "I".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    package.residuals.insert(
        DeclarationId(0),
        vec![emath_ir::ModelResidual {
            expr: residual_expr,
            components: 1,
            algebraic: vec!["I".to_string()],
            rates: Vec::new(),
        }],
    );
    let output = BackendInput {
        package: &package,
        crate_name: "causalized".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("causalized model must now codegen its causalized Newton solve");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("causalized_model_emits_newton_step_methods/1", lib.contains("fn __emath_gaussian_solve") && lib.contains("fn __emath_max_abs"), format!("causalized codegen must embed the Newton helpers, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/2", lib.contains("fn step_euler(") && lib.contains("fn step_rk4("), format!("causalized model must emit both step methods, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/3", lib.contains("Result<Self, String>"), format!("causalized steps must surface non-convergence as a typed Result, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/4", lib.contains("for _ in 0..30u32") && lib.contains("__emath_max_abs(&__f) < 0.000000001"), format!("causalized steps must mirror the interpreter Newton budget and tolerance, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/5", lib.contains("_rates[") && lib.contains("x[0]"), format!("causalized stages must drive residual closures through the solve vector, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/6", lib.contains("__emath_gaussian_solve"), format!("causalized Jacobian solve must call the embedded Gaussian helper, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/7", lib.contains("I: f64") && lib.contains("q: f64"), format!("causalized struct must hold algebraic I with state q so a step can return a consistent DAE point, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/8", lib.contains("__proj_alg") && lib.contains("__advanced"), format!("causalized steps must re-solve algebraic unknowns at the accepted state, got:\n{lib}"));
    p.demand("causalized_model_emits_newton_step_methods/9", lib.contains("internal: Newton rate vector has the wrong width"), format!("causalized steps must refuse a wrong-width rate vector as Result, not panic, got:\n{lib}"));

}

fn case_model_emits_explicit_step_methods(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.x")),
        Span::default(),
    );
    let rate = package.push_expr(
        ExprNode::Unary {
            operation: emath_ir::UnaryOp::Negate,
            value: x,
        },
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_x".to_string(), rate);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Decay"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "decay".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("model must emit step methods");
    let lib = output
        .files
        .get("src/lib.rs")
        .expect("generated crate has src/lib.rs");
    p.demand("model_emits_explicit_step_methods/1", lib.contains("fn der_x(") && lib.contains("fn step_euler(") && lib.contains("fn step_rk4("), format!("model must emit der_x/step_euler/step_rk4, got:\n{lib}"));

}





fn case_flatten_preserves_non_associative_grouping(p: &mut Probe) {
    let mut nested_sub = SemanticPackage::new();
    let a = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let c = nested_sub.push_expr(
        ExprNode::Variable(QualifiedName::single("c")),
        Span::default(),
    );
    let inner = nested_sub.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: b,
            right: c,
        },
        Span::default(),
    );
    let y = nested_sub.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatSub,
            left: a,
            right: inner,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("nested_sub", &["a", "b", "c"], y, &mut nested_sub),
        "nested_sub",
    );
    p.demand("flatten_preserves_non_associative_grouping/1", src.contains("(a) - ((b) - (c))") || src.contains("a - (b - c)"), format!("right-assoc subtraction must stay grouped, got:\n{src}"));

    let mut grouped_mul = SemanticPackage::new();
    let a = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let c = grouped_mul.push_expr(
        ExprNode::Variable(QualifiedName::single("c")),
        Span::default(),
    );
    let sum = grouped_mul.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatAdd,
            left: a,
            right: b,
        },
        Span::default(),
    );
    let y = grouped_mul.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatMul,
            left: sum,
            right: c,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("grouped_mul", &["a", "b", "c"], y, &mut grouped_mul),
        "grouped_mul",
    );
    p.demand("flatten_preserves_non_associative_grouping/2", src.contains("((a) + (b)) * (c)") || src.contains("(a + b) * c"), format!("add-then-mul must stay grouped, got:\n{src}"));

}

fn case_flatten_select_emits_blocked_if(p: &mut Probe) {
    let mut abs_if = SemanticPackage::new();
    let a = abs_if.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let zero = abs_if.push_expr(
        ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())),
        Span::default(),
    );
    let cond = abs_if.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::Greater,
            left: a,
            right: zero,
        },
        Span::default(),
    );
    let neg = abs_if.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Negate,
            value: a,
        },
        Span::default(),
    );
    let y = abs_if.push_expr(
        ExprNode::If {
            condition: cond,
            then_value: a,
            else_value: neg,
        },
        Span::default(),
    );
    let src = extract_fn(&generate_fn("abs_if", &["a"], y, &mut abs_if), "abs_if");
    p.demand("flatten_select_emits_blocked_if/1", src.contains("if") && src.contains('{') && src.contains("else"), format!("Select must render as `if cond {{ t }} else {{ e }}`, got:\n{src}"));
    p.demand("flatten_select_emits_blocked_if/2", !src.contains(") (a)") && !src.contains(") a\n"), format!("braceless `if cond (a)` is invalid Rust, got:\n{src}"));

}

fn case_flatten_method_receiver_parenthesizes_sum(p: &mut Probe) {
    let mut sin_add = SemanticPackage::new();
    let a = sin_add.push_expr(
        ExprNode::Variable(QualifiedName::single("a")),
        Span::default(),
    );
    let b = sin_add.push_expr(
        ExprNode::Variable(QualifiedName::single("b")),
        Span::default(),
    );
    let sum = sin_add.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatAdd,
            left: a,
            right: b,
        },
        Span::default(),
    );
    let y = sin_add.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Sin,
            value: sum,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_fn("sin_add", &["a", "b"], y, &mut sin_add),
        "sin_add",
    );
    p.demand("flatten_method_receiver_parenthesizes_sum/1", src.contains("((a) + (b)).sin()") || src.contains("(a + b).sin()"), format!("sin of a sum must parenthesize the receiver, got:\n{src}"));

}

fn case_const_i64_past_f64_mantissa_stays_i64(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let y = package.push_expr(
        ExprNode::Literal(Literal::Integer("9007199254740993".to_string())),
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("past_mantissa", TypeNode::Int, y, &mut package),
        "past_mantissa",
    );
    p.demand("const_i64_past_f64_mantissa_stays_i64/1", src.contains("9007199254740993i64") || src.contains("9007199254740993"), format!("ConstI64 past the f64 mantissa must stay i64, got:\n{src}"));
    p.demand("const_i64_past_f64_mantissa_stays_i64/2", !src.contains("9007199254740992") && !src.contains("9007199254740993.0"), format!("must not round 2^53+1 through f64, got:\n{src}"));
    p.demand("const_i64_past_f64_mantissa_stays_i64/3", src.contains("-> i64"), format!("Int output must return i64, got:\n{src}"));

}

fn case_mixed_i64_f64_eq_folds_false_not_widened_true(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let n = package.push_expr(
        ExprNode::Literal(Literal::Integer("9007199254740993".to_string())),
        Span::default(),
    );
    let x = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(((1i64 << 53) as f64).to_bits())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::Equal,
            left: n,
            right: x,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("mixed_eq", TypeNode::Bool, y, &mut package),
        "mixed_eq",
    );
    let body = src.split("\npub fn").next().unwrap_or(&src);
    p.demand("mixed_i64_f64_eq_folds_false_not_widened_true/1", body.contains("false"), format!("2^53+1 == 2^53.0 must fold to false, got:\n{body}"));
    p.demand("mixed_i64_f64_eq_folds_false_not_widened_true/2", !body.contains("true"), format!("widening as f64 would fold this pair to true, got:\n{body}"));

}

fn case_factorial_twenty_calls_i64_kernel(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let n = package.push_expr(
        ExprNode::Literal(Literal::Integer("20".to_string())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("factorial"),
            arguments: vec![n],
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("fact20", TypeNode::Int, y, &mut package),
        "fact20",
    );
    p.demand("factorial_twenty_calls_i64_kernel/1", src.contains("emath_rt::factorial"), format!("factorial must call the i64 kernel, got:\n{src}"));
    p.demand("factorial_twenty_calls_i64_kernel/2", !src.contains("as f64"), format!("factorial(20) must not be recast to f64, got:\n{src}"));
    p.demand("factorial_twenty_calls_i64_kernel/3", src.contains("-> i64"), format!("factorial Int output must return i64, got:\n{src}"));

}



fn case_einsum_codegen_calls_rt_kernel_not_panic_stub(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let f = |p: &mut SemanticPackage, v: f64| {
        p.push_expr(
            ExprNode::Literal(Literal::FloatBits(v.to_bits())),
            Span::default(),
        )
    };
    let a11 = f(&mut package, 1.0);
    let a12 = f(&mut package, 2.0);
    let a21 = f(&mut package, 3.0);
    let a22 = f(&mut package, 4.0);
    let b11 = f(&mut package, 5.0);
    let b12 = f(&mut package, 6.0);
    let b21 = f(&mut package, 7.0);
    let b22 = f(&mut package, 8.0);
    let a = package.push_expr(
        ExprNode::Matrix(vec![vec![a11, a12], vec![a21, a22]]),
        Span::default(),
    );
    let b = package.push_expr(
        ExprNode::Matrix(vec![vec![b11, b12], vec![b21, b22]]),
        Span::default(),
    );
    let sub = package.push_expr(
        ExprNode::Literal(Literal::Text("ik,kj->ij".to_string())),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("einsum"),
            arguments: vec![sub, a, b],
        },
        Span::default(),
    );
    let src = generate_typed(
        "ein_mm",
        TypeNode::Matrix {
            element: Box::new(TypeNode::Float64),
            rows: None,
            cols: None,
        },
        y,
        &mut package,
    );
    p.demand("einsum_codegen_calls_rt_kernel_not_panic_stub/1", !src.contains("not yet implemented"), format!("einsum must not emit a panic stub, got:\n{src}"));
    p.demand("einsum_codegen_calls_rt_kernel_not_panic_stub/2", src.contains("einsum_as_matrix") && src.contains("EinsumIn::einsum_operand"), format!("einsum must call the rt kernel, got:\n{src}"));

}

fn case_vector_index_codegen_uses_checked_helper_not_index(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let v_ty = package.push_type(TypeNode::Vector {
        element: Box::new(TypeNode::Float64),
        extent: None,
    });
    let f_ty = package.push_type(TypeNode::Float64);
    let v = package.push_expr(
        ExprNode::Variable(QualifiedName::single("v")),
        Span::default(),
    );
    let i = package.push_expr(
        ExprNode::Variable(QualifiedName::single("i")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Index {
            value: v,
            indices: vec![i],
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("idx"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![
            Field {
                name: "v".to_string(),
                ty: v_ty,
                visibility: Visibility::Public,
                source: Span::default(),
            },
            Field {
                name: "i".to_string(),
                ty: f_ty,
                visibility: Visibility::Public,
                source: Span::default(),
            },
        ],
        outputs: vec![Field {
            name: "y".to_string(),
            ty: f_ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let src = BackendInput {
        package: &package,
        crate_name: "idx".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("index must generate")
    .files
    .get("src/lib.rs")
    .expect("generated crate has src/lib.rs")
    .clone();
    let fn_src = extract_fn(&src, "idx");
    p.demand("vector_index_codegen_uses_checked_helper_not_index/1", fn_src.contains("vec_index_checked"), format!("vector index must call the checked helper, got:\n{fn_src}"));
    p.demand("vector_index_codegen_uses_checked_helper_not_index/2", !fn_src.contains("as usize") && !fn_src.contains("]["), format!("must not emit panicking [], got:\n{fn_src}"));
    p.demand("vector_index_codegen_uses_checked_helper_not_index/3", fn_src.contains("Result<") && fn_src.contains("String"), format!("index evaluate must return Result, got:\n{fn_src}"));

}

fn case_tensor_face_slice_codegen_is_not_a_clone(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let f = |p: &mut SemanticPackage, v: f64| {
        p.push_expr(
            ExprNode::Literal(Literal::FloatBits(v.to_bits())),
            Span::default(),
        )
    };
    let elems: Vec<_> = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        .into_iter()
        .map(|v| f(&mut package, v))
        .collect();
    let t = package.push_expr(
        ExprNode::Tensor {
            shape: vec![2, 2, 2],
            elements: elems,
        },
        Span::default(),
    );
    let zero = f(&mut package, 0.0);
    let two = f(&mut package, 2.0);
    let y = package.push_expr(
        ExprNode::Slice {
            value: t,
            axes: vec![
                SliceAxis::Point(zero),
                SliceAxis::Range {
                    start: zero,
                    end: two,
                },
                SliceAxis::Range {
                    start: zero,
                    end: two,
                },
            ],
        },
        Span::default(),
    );
    let src = generate_typed(
        "tensor_face",
        TypeNode::Matrix {
            element: Box::new(TypeNode::Float64),
            rows: None,
            cols: None,
        },
        y,
        &mut package,
    );
    let fn_src = extract_fn(&src, "tensor_face");
    p.demand("tensor_face_slice_codegen_is_not_a_clone/1", !fn_src.contains("tensor slice axes") && !fn_src.contains("t.clone()"), format!("tensor slice must not be a no-op clone, got:\n{fn_src}"));
    p.demand("tensor_face_slice_codegen_is_not_a_clone/2", fn_src.contains("tensor_slice_as_matrix") && fn_src.contains("SliceAxis"), format!("t[0, :, :] must call the slice kernel, got:\n{fn_src}"));
    p.demand("tensor_face_slice_codegen_is_not_a_clone/3", fn_src.contains("emath_rt::Tensor") && fn_src.contains("shape: vec![2, 2, 2]"), format!("rank-3 literal must keep shape, got:\n{fn_src}"));

}

fn case_integer_product_fold_uses_i64_kernel(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let start = package.push_expr(
        ExprNode::Literal(Literal::Integer("1".to_string())),
        Span::default(),
    );
    // EMIR fold is half-open; inclusive 1..=20 is the vector [1, 21].
    let end = package.push_expr(
        ExprNode::Literal(Literal::Integer("21".to_string())),
        Span::default(),
    );
    let domain = package.push_expr(ExprNode::Vector(vec![start, end]), Span::default());
    let body = package.push_expr(
        ExprNode::Variable(QualifiedName::single("i")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Binder {
            kind: BinderKind::Product,
            variables: vec![BinderVariable {
                name: "i".to_string(),
                domain,
            }],
            body,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("prod20", TypeNode::Int, y, &mut package),
        "prod20",
    );
    p.demand("integer_product_fold_uses_i64_kernel/1", src.contains("fold_mul_i64"), format!("integer product must use fold_mul_i64, got:\n{src}"));
    p.demand("integer_product_fold_uses_i64_kernel/2", !src.contains("fold_mul(") || src.contains("fold_mul_i64"), format!("integer product must not use the f64 fold_mul kernel, got:\n{src}"));

}

fn case_folded_nonfinite_constants_emit_valid_rust(p: &mut Probe) {
    let mut sqrt_neg = SemanticPackage::new();
    let neg1 = sqrt_neg.push_expr(
        ExprNode::Literal(Literal::Integer("-1".to_string())),
        Span::default(),
    );
    let y = sqrt_neg.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Sqrt,
            value: neg1,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("sqrt_neg", TypeNode::Float64, y, &mut sqrt_neg),
        "sqrt_neg",
    );
    p.demand("folded_nonfinite_constants_emit_valid_rust/1", src.contains("f64::from_bits("), format!("sqrt(-1) must emit from_bits, not Debug NaN, got:\n{src}"));
    p.demand("folded_nonfinite_constants_emit_valid_rust/2", !src.contains("NaN"), format!("bare `NaN` is not valid Rust, got:\n{src}"));

    let mut div0 = SemanticPackage::new();
    let one = div0.push_expr(
        ExprNode::Literal(Literal::Integer("1".to_string())),
        Span::default(),
    );
    let zero = div0.push_expr(
        ExprNode::Literal(Literal::Integer("0".to_string())),
        Span::default(),
    );
    let y = div0.push_expr(
        ExprNode::Binary {
            operation: BinaryOp::StrictFloatDiv,
            left: one,
            right: zero,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("div0", TypeNode::Float64, y, &mut div0),
        "div0",
    );
    p.demand("folded_nonfinite_constants_emit_valid_rust/3", src.contains("f64::from_bits("), format!("1/0 must emit from_bits, not Debug inf, got:\n{src}"));
    p.demand("folded_nonfinite_constants_emit_valid_rust/4", !src.contains("inf") && !src.contains("Inf"), format!("bare `inf` is not valid Rust, got:\n{src}"));

    let mut log0 = SemanticPackage::new();
    let zero = log0.push_expr(
        ExprNode::Literal(Literal::Integer("0".to_string())),
        Span::default(),
    );
    let y = log0.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Log,
            value: zero,
        },
        Span::default(),
    );
    let src = extract_fn(
        &generate_typed("log0", TypeNode::Float64, y, &mut log0),
        "log0",
    );
    p.demand("folded_nonfinite_constants_emit_valid_rust/5", src.contains("f64::from_bits("), format!("log(0) must emit from_bits, not Debug -inf, got:\n{src}"));

}

fn case_sign_zero_uses_mathematical_sgn(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x")),
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("sign"),
            arguments: vec![x],
        },
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "y".to_string(),
        expression: Some(y),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), y);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("sgn"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "y".to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let output = BackendInput {
        package: &package,
        crate_name: "sgn".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect("sign must generate");
    let src = extract_fn(
        output
            .files
            .get("src/lib.rs")
            .expect("generated crate has src/lib.rs"),
        "sgn",
    );
    p.demand("sign_zero_uses_mathematical_sgn/1", src.contains("== 0.0") && src.contains("signum"), format!("sign must use mathematical sgn (0 at 0), got:\n{src}"));

}

fn case_option_carrier_generated_behaviors(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let five = package.push_expr(
        ExprNode::Literal(Literal::Integer("5".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let some = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let roundtrip = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![some, seven],
        },
        Span::default(),
    );
    carrier_int("opt_roundtrip", roundtrip, &mut package);
    let none = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_none"),
            arguments: Vec::new(),
        },
        Span::default(),
    );
    let default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![none, seven],
        },
        Span::default(),
    );
    carrier_int("opt_default", default, &mut package);
    let some2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let pol_some = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![some2],
        },
        Span::default(),
    );
    carrier_bool("opt_polarity_some", pol_some, &mut package);
    let none2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_none"),
            arguments: Vec::new(),
        },
        Span::default(),
    );
    let pol_none = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![none2],
        },
        Span::default(),
    );
    carrier_bool("opt_polarity_none", pol_none, &mut package);
    let v1 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
        Span::default(),
    );
    let v2 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(2.0_f64.to_bits())),
        Span::default(),
    );
    let vec_payload = package.push_expr(ExprNode::Vector(vec![v1, v2]), Span::default());
    let some_vec = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some"),
            arguments: vec![vec_payload],
        },
        Span::default(),
    );
    let v9 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(9.0_f64.to_bits())),
        Span::default(),
    );
    let v8 = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(8.0_f64.to_bits())),
        Span::default(),
    );
    let vec_default = package.push_expr(ExprNode::Vector(vec![v9, v8]), Span::default());
    let vec_pick = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![some_vec, vec_default],
        },
        Span::default(),
    );
    carrier_vec("vec_pick", vec_pick, &mut package);

    let lib = generate_carrier_lib(&package);
    let user = user_section(&lib);
    p.demand("option_carrier_generated_behaviors/1", lib.contains("Option::<i64>::Some") && lib.contains("Option::<i64>::None"), format!("generated carriers must be native Option<i64>, got:\n{lib}"));
    p.demand("option_carrier_generated_behaviors/2", lib.contains("Option::<Vec<f64>>::Some") && lib.contains(".unwrap_or("), format!("vector payload and eager unwrap_or gate must be emitted, got:\n{lib}"));
    p.demand("option_carrier_generated_behaviors/3", !user.contains(".unwrap()") && !user.contains("expect(") && !user.contains("panic!"), format!("generated user code must contain no panicking unwrap / expect / panic, got:\n{user}"));
    demand_run(p, "run", run_generated(
        &lib,
        r#"
        assert_eq!(generated::opt_roundtrip(), 5i64, "some carrier round trip");
        assert_eq!(generated::opt_default(), 7i64, "none returns eager default");
        assert_eq!(generated::opt_polarity_some(), true, "option_is_some(some) is true");
        assert_eq!(generated::opt_polarity_none(), false, "option_is_some(none) is false");
        assert_eq!(generated::vec_pick(), vec![1.0_f64, 2.0_f64], "vector payload round trip");
    "#,
    ));

}

fn case_result_carrier_generated_behaviors(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let five = package.push_expr(
        ExprNode::Literal(Literal::Integer("5".to_string())),
        Span::default(),
    );
    let six = package.push_expr(
        ExprNode::Literal(Literal::Integer("6".to_string())),
        Span::default(),
    );
    let seven = package.push_expr(
        ExprNode::Literal(Literal::Integer("7".to_string())),
        Span::default(),
    );
    let nine = package.push_expr(
        ExprNode::Literal(Literal::Integer("9".to_string())),
        Span::default(),
    );
    let ok = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let ok_carry = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or"),
            arguments: vec![ok, seven],
        },
        Span::default(),
    );
    carrier_int("res_ok_carry", ok_carry, &mut package);
    let err = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let err_default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or"),
            arguments: vec![err, seven],
        },
        Span::default(),
    );
    carrier_int("res_err_default", err_default, &mut package);
    let ok2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let is_ok_true = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_is_ok"),
            arguments: vec![ok2],
        },
        Span::default(),
    );
    carrier_bool("res_is_ok_true", is_ok_true, &mut package);
    let err2 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let is_ok_false = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_is_ok"),
            arguments: vec![err2],
        },
        Span::default(),
    );
    carrier_bool("res_is_ok_false", is_ok_false, &mut package);
    let ok3 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok"),
            arguments: vec![five],
        },
        Span::default(),
    );
    let err_of_ok = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![ok3],
        },
        Span::default(),
    );
    let err_of_ok_default = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![err_of_ok, nine],
        },
        Span::default(),
    );
    carrier_int("err_of_ok", err_of_ok_default, &mut package);
    let err3 = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_err"),
            arguments: vec![six],
        },
        Span::default(),
    );
    let err_of_err = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![err3],
        },
        Span::default(),
    );
    let err_of_err_payload = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or"),
            arguments: vec![err_of_err, nine],
        },
        Span::default(),
    );
    carrier_int("err_of_err", err_of_err_payload, &mut package);

    let lib = generate_carrier_lib(&package);
    let user = user_section(&lib);
    p.demand("result_carrier_generated_behaviors/1", lib.contains("Result::<i64, i64>::Ok") && lib.contains("Result::<i64, i64>::Err"), format!("generated carriers must be native Result<i64, i64>, got:\n{lib}"));
    p.demand("result_carrier_generated_behaviors/2", lib.contains("match ") && lib.contains("Option::<i64>::Some"), format!("error_of must compose the error as an Option carrier, got:\n{lib}"));
    p.demand("result_carrier_generated_behaviors/3", !user.contains(".unwrap()") && !user.contains("expect(") && !user.contains("panic!"), format!("generated user code must contain no panicking unwrap / expect / panic, got:\n{user}"));
    demand_run(p, "run", run_generated(&lib, r#"
        assert_eq!(generated::res_ok_carry(), 5i64, "ok payload round trips");
        assert_eq!(generated::res_err_default(), 7i64, "err payload is not the unwrap value; default is");
        assert_eq!(generated::res_is_ok_true(), true, "result_is_ok(ok) is true");
        assert_eq!(generated::res_is_ok_false(), false, "result_is_ok(err) is false");
        assert_eq!(generated::err_of_ok(), 9i64, "error_of(ok) -> None -> default");
        assert_eq!(generated::err_of_err(), 6i64, "error_of(err) -> Some(payload)");
    "#));

}

fn case_wrong_carrier_use_refuses_typed(p: &mut Probe) {
    // option_is_some over a scalar carrier slot.
    let mut package = SemanticPackage::new();
    let scalar = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(5.0_f64.to_bits())),
        Span::default(),
    );
    let bogus = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_is_some"),
            arguments: vec![scalar],
        },
        Span::default(),
    );
    carrier_bool("bogus_opt", bogus, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "bogus".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("option_is_some over a scalar must refuse typed");
    let msg = err.to_string();
    p.demand("wrong_carrier_use_refuses_typed/1", msg.contains("Option carrier") && msg.contains("TypeConfusion"), format!("refusal must name the carrier rule and mirror interp TypeConfusion, got: {msg}"));

    // result_error_of over a scalar carrier slot.
    let mut package = SemanticPackage::new();
    let scalar = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(5.0_f64.to_bits())),
        Span::default(),
    );
    let bogus = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_error_of"),
            arguments: vec![scalar],
        },
        Span::default(),
    );
    carrier_bool("bogus_res", bogus, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "bogus_res".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("result_error_of over a scalar must refuse typed");
    let msg = err.to_string();
    p.demand("wrong_carrier_use_refuses_typed/2", msg.contains("Result carrier") && msg.contains("TypeConfusion"), format!("refusal must name the carrier rule and mirror interp TypeConfusion, got: {msg}"));

}

fn case_unwrap_or_carrier_default_refuses_payload_conflict(p: &mut Probe) {
    // Option: option_unwrap_or(option_some(5), option_some(7)) — the
    // default slot is a genuine scalar, not a carrier.
    let mut package = SemanticPackage::new();
    let five = literal_int(&mut package, "5");
    let seven = literal_int(&mut package, "7");
    let c = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some".to_string()),
            arguments: vec![five],
        },
        Span::default(),
    );
    let def_car = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_some".to_string()),
            arguments: vec![seven],
        },
        Span::default(),
    );
    let y = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("option_unwrap_or".to_string()),
            arguments: vec![c, def_car],
        },
        Span::default(),
    );
    carrier_int("opt_carrier_default", y, &mut package);
    let err = BackendInput {
        package: &package,
        crate_name: "opt_carrier_default".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("unwrap_or carrier default must refuse typed");
    let msg = err.to_string();
    p.demand("unwrap_or_carrier_default_refuses_payload_conflict/1", msg.contains("payload kind conflict") && msg.contains("i64") && msg.contains("Option"), format!("Option carrier-as-default must refuse via payload-kind conflict, got: {msg}"));

    // Result: result_unwrap_or(result_ok(5), result_ok(7)) — same rule.
    let mut rpackage = SemanticPackage::new();
    let rfive = literal_int(&mut rpackage, "5");
    let rseven = literal_int(&mut rpackage, "7");
    let rc = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok".to_string()),
            arguments: vec![rfive],
        },
        Span::default(),
    );
    let rdef_car = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_ok".to_string()),
            arguments: vec![rseven],
        },
        Span::default(),
    );
    let ry = rpackage.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("result_unwrap_or".to_string()),
            arguments: vec![rc, rdef_car],
        },
        Span::default(),
    );
    carrier_int("res_carrier_default", ry, &mut rpackage);
    let rerr = BackendInput {
        package: &rpackage,
        crate_name: "res_carrier_default".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("result unwrap_or carrier default must refuse typed");
    let rmsg = rerr.to_string();
    p.demand("unwrap_or_carrier_default_refuses_payload_conflict/2", rmsg.contains("payload kind conflict") && rmsg.contains("i64") && rmsg.contains("Result"), format!("Result carrier-as-default must refuse via payload-kind conflict, got: {rmsg}"));

}

fn case_field_prime_output_decl_refuses_typed(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let y = literal_int(&mut package, "7");
    carrier_decl(
        "field_out",
        TypeNode::FieldPrime { modulus: 7 },
        y,
        &mut package,
    );
    let err = BackendInput {
        package: &package,
        crate_name: "field_out".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("Field<7> output field must refuse typed, not become f64");
    let msg = err.to_string().to_lowercase();
    p.demand("field_prime_output_decl_refuses_typed/1", msg.contains("field<7>") && msg.contains("unsupported type"), format!("FieldPrime output must refuse naming Field<7> as an unsupported type, got: {err}"));

}

fn case_field_prime_input_decl_refuses_typed(p: &mut Probe) {
    let mut package = SemanticPackage::new();
    let ft = package.push_type(TypeNode::FieldPrime { modulus: 7 });
    let fty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("x".to_string())),
        Span::default(),
    );
    let goal_id = package.push_goal(Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "x".to_string(),
        expression: Some(x),
        requirements: eval_requirements(),
        payload: GoalPayload::default(),
        source: Span::default(),
    });
    let mut definitions = BTreeMap::new();
    definitions.insert("x".to_string(), x);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("field_in"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty: ft,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        outputs: vec![Field {
            name: "x".to_string(),
            ty: fty,
            visibility: Visibility::Public,
            source: Span::default(),
        }],
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: vec![goal_id],
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let err = BackendInput {
        package: &package,
        crate_name: "field_in".to_string(),
        version: "0.1.0".to_string(),
    }
    .generate()
    .expect_err("Field<7> input field must refuse typed");
    let msg = err.to_string().to_lowercase();
    p.demand("field_prime_input_decl_refuses_typed/1", msg.contains("field<7>") && msg.contains("unsupported type"), format!("FieldPrime input must refuse naming Field<7> as an unsupported type, got: {err}"));

}

fn case_carrier_field_decl_refuses_typed(p: &mut Probe) {
    for (spelling, node) in [
        ("Option<Int>", TypeNode::OptionType(Box::new(TypeNode::Int))),
        (
            "Result<Int, Bool>",
            TypeNode::Result {
                ok: Box::new(TypeNode::Int),
                error: Box::new(TypeNode::Bool),
            },
        ),
    ] {
        let mut package = SemanticPackage::new();
        let ft = package.push_type(node);
        let fty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x".to_string())),
            Span::default(),
        );
        let goal_id = package.push_goal(Goal {
            id: GoalId(0),
            kind: GoalKind::Evaluate,
            target: "x".to_string(),
            expression: Some(x),
            requirements: eval_requirements(),
            payload: GoalPayload::default(),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("x".to_string(), x);
        package.declarations.push(Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("carrier_field"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![Field {
                name: "x".to_string(),
                ty: ft,
                visibility: Visibility::Public,
                source: Span::default(),
            }],
            outputs: vec![Field {
                name: "x".to_string(),
                ty: fty,
                visibility: Visibility::Public,
                source: Span::default(),
            }],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: vec![goal_id],
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let err = BackendInput {
            package: &package,
            crate_name: "carrier_field".to_string(),
            version: "0.1.0".to_string(),
        }
        .generate()
        .expect_err("carrier input field must refuse typed");
        let msg = err.to_string().to_lowercase();
        p.demand("carrier_field_decl_refuses_typed/1", msg.contains(&spelling.to_lowercase()) && msg.contains("unsupported type"), format!("`{spelling}` input field must refuse naming its spelling, got: {err}"));
    }

}

fn case_text_option_int_output_carrier_out(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function f:\n    inputs:\n        k: Int\n    outputs:\n        o: Option<Int>\n    definitions:\n        o = option_some(k)\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    p.demand("text_option_int_output_carrier_out/1", lib.contains("Option::<i64>::Some"), format!("generated code must carry Option<i64> natively, got:\n{lib}"));
    demand_run(p, "run", run_generated(&lib, "assert_eq!(generated::f(5), Some(5i64));"));

}

fn case_text_nested_option_option_int(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function n:\n    inputs:\n        k: Int\n    outputs:\n        o: Option<Option<Int>>\n    definitions:\n        o = option_some(option_some(k))\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    p.demand("text_nested_option_option_int/1", lib.contains("Option::<Option<i64>>::Some"), format!("nested carrier must emit Option<Option<i64>>, got:\n{lib}"));
    demand_run(p, "run", run_generated(&lib, "assert_eq!(generated::n(5), Some(Some(5i64)));"));

}

fn case_text_nested_some_none(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function sc:\n    inputs:\n        x: Float64\n    outputs:\n        o: Option<Option<Float64>>\n    definitions:\n        o = option_some(option_none())\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::sc(0.0), Some::<Option<f64>>(None));",
    ));

}

fn case_text_map_by_composition_emits_native_option(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function m:\n    inputs:\n        x: Float64\n    outputs:\n        o: Option<Float64>\n    definitions:\n        o = if x > 0.0 : option_some(2.0 * x) else : option_none()\n    goals:\n        evaluate <o>:\n            produce rust.library\n",
    );
    p.demand("text_map_by_composition_emits_native_option/1", lib.contains("Option::<f64>::Some") && lib.contains("None"), format!("map-by-composition must emit native Option with a None arm, got:\n{lib}"));
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::m(3.0), Some(6.0));\nassert_eq!(generated::m(-1.0), None);",
    ));

}

fn case_text_int_rem_field_add_executes(p: &mut Probe) {
    let src = "emath function field7_add:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a + b, 7)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(p, src);
    p.demand("text_int_rem_field_add_executes/1", lib.contains(".rem_euclid("), format!("int_rem must emit exact Rust `.rem_euclid(`, got:\n{lib}"));
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::field7_add(3, 4), 0);\nassert_eq!(generated::field7_add(6, 5), 4);",
    ));

}

fn case_text_int_rem_field_mul_executes(p: &mut Probe) {
    let src = "emath function field7_mul:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a * b, 7)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(p, src);
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::field7_mul(3, 4), 5);\nassert_eq!(generated::field7_mul(3, 5), 1);",
    ));

}

fn case_text_int_rem_sign_law_generated(p: &mut Probe) {
    let src = "emath function irs:\n    inputs:\n        a: Int\n        m: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, m)\n    goals:\n        evaluate <c>:\n            produce rust.library\n";
    let lib = text_lib(p, src);
    demand_run(p, "run", run_generated(&lib, "assert_eq!(generated::irs(-1, 7), 6);"));

}

fn case_text_fold_guard_vec_index_by_binder_compiles_and_counts(p: &mut Probe) {
    let lib = text_lib(p, fold_guard_repro_source());
    // The generated `idx_by_binder` body must not carry a `?` inside the
    // plain i64 fold closure: that rendering is the raw E0277 defect.
    // (The user functions render AFTER the inlined `mod emath_rt`, so
    // assert on the extracted function, not on a split artifact.)
    let fn_src = extract_fn(&lib, "idx_by_binder");
    p.demand("text_fold_guard_vec_index_by_binder_compiles_and_counts/1", !fn_src.contains(".map_err(|e| e.to_string())?"), format!("checked vec-index inside a plain fold closure must not render `?` (rustc E0277):\n{fn_src}"));
    // The honest fault channel renders instead: panic with the real
    // runtime message (never a silent value).
    p.demand("text_fold_guard_vec_index_by_binder_compiles_and_counts/2", fn_src.contains(".map_err(|e| e.to_string()).unwrap_or_else(|e| panic!(\"{e}\"))"), format!("fold-context checked op must render panic-with-real-runtime-message:\n{fn_src}"));
    // End-to-end: the generated crate compiles and computes the same
    // count the interpreter does (17 and 34 are 0 mod 17).
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::idx_by_binder(17, vec![17.0, 2.0, 34.0, 5.0]), Ok(2i64));",
    ));

}

fn case_text_nested_fold_guards_vec_index_compiles(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function inline_coeff_poly:\n    inputs:\n        p: Int\n        xs: Vector[Int]\n        w: Vector[Int]\n    outputs:\n        c: Int\n    definitions:\n        c = sum a0 in 0..p, a1 in 0..p if (sum i in 0..4 if poly_eval_mod([a0, a1], xs[i], p) == w[i]: 1) >= 3: 1\n    goals:\n        evaluate <c>:\n            produce rust.library\n",
    );
    // Compile + value parity IS the contract here: before the fix the
    // generated crate died with rustc E0277 (`?` inside the plain fold
    // closures). The interpreter value is the oracle.
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::inline_coeff_poly(5, vec![1.0, 2.0, 3.0, 4.0], vec![7.0, 7.0, 7.0, 0.0]), Ok(0i64));",
    ));

}

fn case_text_fault_fold_nested_in_plain_fold_compiles(p: &mut Probe) {
    let lib = text_lib(p, 
        "emath function nested_fault_fold:\n    inputs:\n        p: Int\n        xs: Vector[Int]\n    outputs:\n        c: Int\n    definitions:\n        c = sum a in 0..4 if (sum j in 0..4 if int_rem(xs[j], p) == 0: 1.0) > 2.5: 1\n    goals:\n        evaluate <c>:\n            produce rust.library\n",
    );
    demand_run(p, "run", run_generated(
        &lib,
        "assert_eq!(generated::nested_fault_fold(17, vec![1.0, 2.0, 3.0, 4.0]), Ok(0i64));\nassert_eq!(generated::nested_fault_fold(17, vec![17.0, 34.0, 2.0, 5.0]), Ok(0i64));",
    ));

}
