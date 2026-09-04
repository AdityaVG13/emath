//! The `--function-spec` evaluation path.

use super::*;

/// The function-spec lane: admit the source, select one `emath function`
/// entrypoint, bind every declared input from `--set`, lower through
/// EMIR, evaluate on the reference VM, and emit the `emath.eval-function`
/// receipt. Failures are typed E-EVAL-* refusals; no partial authority.
pub(super) fn eval_function_spec(args: &EvalArgs) -> CliExit {
    let source = match std::fs::read_to_string(&args.path) {
        Ok(source) if has_declaration_content(&source) => source,
        Ok(_) => {
            return refuse_eval_coded(
                "E-PKG-081",
                &format!("source has no declarations ({})", args.path.display()),
                args.json,
            );
        }
        Err(_) => {
            return refuse_eval_coded(
                "E-PKG-080",
                &format!("cannot read source file ({})", args.path.display()),
                args.json,
            );
        }
    };
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(&args.path.display().to_string(), &source);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        if args.json {
            print_json_diagnostics(
                "eval",
                false,
                &json_diagnostics_entries(&result.diagnostics),
            );
        }
        return EXIT_REFUSED;
    }
    let package = result.package;
    let declaration = match select_entrypoint(args, &package) {
        Ok(declaration) => declaration,
        Err((code, message)) => return refuse_eval_coded(code, &message, args.json),
    };
    if !declaration.state.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-001",
            &format!(
                "entrypoint `{}` is stateful; `emath eval` executes only stateless function declarations",
                declaration.name.leaf()
            ),
            args.json,
        );
    }
    // Inputs must be Float64, Int/Nat, Vector[Float64], or
    // Vector[Int]/Vector[Nat]: the value vocabulary this surface binds
    // is deliberately narrow (scalars and flat vectors). Anything else
    // is E-EVAL-006.
    for field in &declaration.inputs {
        let supported = match package.ty(field.ty) {
            Some(TypeNode::Float64) => true,
            Some(TypeNode::Int | TypeNode::Nat) => true,
            Some(TypeNode::Vector { element, .. }) => {
                matches!(
                    &**element,
                    TypeNode::Float64 | TypeNode::Int | TypeNode::Nat
                )
            }
            _ => false,
        };
        if !supported {
            return refuse_eval_coded(
                "E-EVAL-006",
                &format!(
                    "input `{}` has a type `emath eval` cannot bind (Float64, Int, Nat, Vector[Float64], Vector[Int], and Vector[Nat] only)",
                    field.name
                ),
                args.json,
            );
        }
    }
    // Binding source: explicit `--set` when present, otherwise the
    // spec's own oracle — the single worked example's `given` bindings
    // (deterministic, nothing invented; a function with zero inputs
    // evaluates with an empty binding map).
    let mut inputs_from = "set".to_string();
    let mut bindings: BTreeMap<String, Value> = BTreeMap::new();
    if args.set.is_empty() {
        (bindings, inputs_from) = match oracle_bindings(&package, declaration) {
            Ok((bindings, source)) => (bindings, source),
            Err((code, message)) => return refuse_eval_coded(code, &message, args.json),
        };
    } else {
        // `--set` closure: duplicates and undeclared names are
        // E-EVAL-005, malformed values are E-EVAL-005, values that
        // mismatch the declared slot shape are E-EVAL-006.
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in &args.set {
            if !seen.insert(name) {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!("duplicate `--set` binding for input `{name}`"),
                    args.json,
                );
            }
        }
        for (name, raw) in &args.set {
            if !declaration.inputs.iter().any(|field| field.name == *name) {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!(
                        "`--set` names `{name}`, which is not a declared input of `{}`",
                        declaration.name.leaf()
                    ),
                    args.json,
                );
            }
            let declared = declaration
                .inputs
                .iter()
                .find(|field| field.name == *name)
                .and_then(|field| package.ty(field.ty).cloned());
            let Some(value) = parse_set_value_for(declared.as_ref(), raw) else {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!(
                        "cannot parse `--set {name}={raw}` as a value of the declared input type"
                    ),
                    args.json,
                );
            };
            bindings.insert(name.clone(), value);
        }
    }
    for field in &declaration.inputs {
        if let Some(value) = bindings.get(&field.name) {
            let mismatch = match package.ty(field.ty) {
                Some(TypeNode::Float64) => !matches!(value, Value::F64(_)),
                Some(TypeNode::Int) => !matches!(value, Value::I64(_)),
                Some(TypeNode::Nat) => !matches!(value, Value::I64(v) if *v >= 0),
                Some(TypeNode::Vector { element, .. }) => {
                    // Element-type strictness mirrors the scalar Int/Nat
                    // contract: exact whole elements (Nat: non-negative),
                    // bound-consistent with the scalar i64 range.
                    let exact = |e: &f64| e.fract() == 0.0 && e.abs() <= 9.3e18;
                    !match (value, &**element) {
                        (Value::Vector(elements), TypeNode::Float64) => true,
                        (Value::Vector(elements), TypeNode::Int) => elements.iter().all(exact),
                        (Value::Vector(elements), TypeNode::Nat) => {
                            elements.iter().all(|e| exact(e) && *e >= 0.0)
                        }
                        _ => false,
                    }
                }
                _ => true,
            };
            if mismatch {
                return refuse_eval_coded(
                    "E-EVAL-006",
                    &format!(
                        "`--set {}` value does not match the declared input type",
                        field.name
                    ),
                    args.json,
                );
            }
        }
    }
    let missing: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .filter(|name| !bindings.contains_key(name))
        .collect();
    if !missing.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-004",
            &format!(
                "missing input binding(s): {} (bind every declared input with `--set name=value`, or give the spec a single worked example to evaluate)",
                missing.join(", ")
            ),
            args.json,
        );
    }
    let meaning_id = match package.meaning_id(&[]) {
        Ok(id) => id,
        Err(error) => {
            return refuse_eval_coded(
                "E-EVAL-007",
                &format!("meaning identity refused: {error:?}"),
                args.json,
            );
        }
    };
    let empty_state = BTreeMap::new();
    let definitions = match eval_definitions_values(&package, declaration, &bindings, &empty_state)
    {
        Ok(definitions) => definitions,
        Err(verdict) => {
            return refuse_eval_coded(
                "E-EVAL-007",
                &format!(
                    "evaluation refused: {}",
                    verdict.reason_text().unwrap_or_else(|| verdict.to_string())
                ),
                args.json,
            );
        }
    };
    // Declared outputs, in declaration order, each with a computed
    // definition; a declared output with no computed definition is
    // simply absent from the receipt (nothing computes it).
    let outputs: Vec<(String, String)> = declaration
        .outputs
        .iter()
        .filter_map(|field| {
            definitions
                .get(&field.name)
                .map(|value| (field.name.clone(), value.to_string()))
        })
        .collect();
    let inputs: Vec<(String, String)> = bindings
        .iter()
        .map(|(name, value)| (name.clone(), value.to_string()))
        .collect();
    let entrypoint = if args.function.is_some() {
        "named"
    } else {
        "sole"
    };
    emit_function_receipt(
        declaration.name.leaf(),
        entrypoint,
        &inputs_from,
        &inputs,
        &outputs,
        &meaning_id.to_string(),
        args.json,
    );
    EXIT_OK
}

/// Spec-oracle bindings for a plain eval (no `--set`): the file's own
/// single worked example supplies the inputs — its `given` values run
/// through the same generic lowering, and the example's expect verdict
/// must pass. Deterministic and never invented; a spec with several
/// examples (E-EVAL-003) or with inputs but none (E-EVAL-004) must
/// bind explicitly instead.
pub(super) fn oracle_bindings(
    package: &emath_ir::SemanticPackage,
    declaration: &emath_ir::Declaration,
) -> Result<(BTreeMap<String, Value>, String), (&'static str, String)> {
    if declaration.tests.len() == 1 {
        let report = run_declaration(package, declaration);
        let run = report.tests.first().ok_or((
            "E-EVAL-007",
            "the spec's single example did not produce a run".to_string(),
        ))?;
        match &run.verdict {
            TestVerdict::Passed | TestVerdict::Computed => {
                Ok((run.given.clone(), format!("example:{}", run.name)))
            }
            verdict => Err((
                "E-EVAL-007",
                format!(
                    "the spec's example `{}` did not pass: {}",
                    run.name,
                    verdict.reason_text().unwrap_or_else(|| verdict.to_string())
                ),
            )),
        }
    } else if declaration.tests.is_empty() {
        if declaration.inputs.is_empty() {
            Ok((BTreeMap::new(), "none".to_string()))
        } else {
            Err((
                "E-EVAL-004",
                "this function declares inputs but has no worked example to supply them; bind them with `--set name=value`"
                    .to_string(),
            ))
        }
    } else {
        Err((
            "E-EVAL-003",
            format!(
                "the file carries {} worked examples; select the entrypoint inputs explicitly with `--set name=value` (each example may bind different values)",
                declaration.tests.len()
            ),
        ))
    }
}

/// Select the evaluable entrypoint: exactly one function declaration, or
/// `--function <name>` naming a declared function (E-EVAL-002 unknown,
/// E-EVAL-001 non-function, E-EVAL-003 ambiguous).
pub(super) fn select_entrypoint<'p>(
    args: &EvalArgs,
    package: &'p emath_ir::SemanticPackage,
) -> Result<&'p emath_ir::Declaration, (&'static str, String)> {
    let functions: Vec<&emath_ir::Declaration> = package
        .declarations
        .iter()
        .filter(|declaration| declaration.kind_label == "function")
        .collect();
    match &args.function {
        Some(name) => match package.declarations.iter().find(|declaration| declaration.name.leaf() == name)
        {
            Some(declaration) if declaration.kind_label == "function" => Ok(declaration),
            Some(declaration) => Err((
                "E-EVAL-001",
                format!(
                    "entrypoint `{name}` is a `{}` declaration, not an evaluable `emath function`",
                    declaration.kind_label
                ),
            )),
            None => Err((
                "E-EVAL-002",
                format!(
                    "no declaration named `{name}` in the admitted package; `--function` must name a declared `emath function`"
                ),
            )),
        },
        None => match functions.len() {
            0 => Err((
                "E-EVAL-001",
                "no `emath function` entrypoint in this file; `emath eval` executes only stateless function declarations"
                    .to_string(),
            )),
            1 => Ok(functions[0]),
            _ => Err((
                "E-EVAL-003",
                format!(
                    "{} function declarations share this file; select the entrypoint with `--function <name>` ({})",
                    functions.len(),
                    functions
                        .iter()
                        .map(|declaration| declaration.name.leaf())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        },
    }
}

/// Parse one `--set name=value` payload against the DECLARED input type:
/// a finite decimal scalar, an integer scalar for `Int`/`Nat` inputs, or
/// a `[a, b, c]` vector of finite decimals. Integer literals keep parsing
/// as Float64 for Float64 inputs (prior behavior), so `--set x=7` works
/// for both float and integer parameters.
pub(super) fn parse_set_value_for(declared: Option<&TypeNode>, raw: &str) -> Option<Value> {
    // Stage-2 (emath-t63iz): `BigInt` inputs bind the exact decimal —
    // never an f64 round trip, which would lose digits above 2^53.
    if matches!(declared, Some(TypeNode::BigInt)) {
        return Value::parse_bigint(raw);
    }
    let integer_scalar = matches!(declared, Some(TypeNode::Int) | Some(TypeNode::Nat));
    let parsed = parse_set_value(raw)?;
    match (integer_scalar, &parsed) {
        // Integer literal for an integer input: re-derive the exact i64.
        (true, Value::F64(value)) if value.fract() == 0.0 && value.abs() <= 9.3e18 => {
            Some(Value::I64(*value as i64))
        }
        // Non-integer (or out-of-range) literal for an integer input.
        (true, _) => None,
        (_, other) => Some(other.clone()),
    }
}

/// Untyped parse: a finite decimal scalar or a `[a, b, c]` vector of
/// finite decimals. Nothing else binds.
pub(super) fn parse_set_value(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner.is_empty() {
            return None;
        }
        let mut elements = Vec::new();
        for part in inner.split(',') {
            let value: f64 = part.trim().parse().ok()?;
            if !value.is_finite() {
                return None;
            }
            elements.push(value);
        }
        Some(Value::Vector(elements))
    } else {
        let value: f64 = trimmed.parse().ok()?;
        if value.is_finite() {
            Some(Value::F64(value))
        } else {
            None
        }
    }
}

/// Deterministic `emath.eval-function` receipt. Inputs are rendered in
/// sorted name order (`BTreeMap`), outputs in declared order, so the
/// byte stream is stable across runs and `--set` arrangement.
pub(super) fn emit_function_receipt(
    function: &str,
    entrypoint: &str,
    inputs_from: &str,
    inputs: &[(String, String)],
    outputs: &[(String, String)],
    meaning_id: &str,
    json: bool,
) {
    if json {
        println!(
            "{}",
            render_function_receipt_json(
                function,
                entrypoint,
                inputs_from,
                inputs,
                outputs,
                meaning_id
            )
        );
    } else {
        println!("function {function}");
        println!("entrypoint {entrypoint}");
        println!("inputs_from {inputs_from}");
        println!("meaning_id {meaning_id}");
        for (name, value) in inputs {
            println!("input {name} = {value}");
        }
        for (name, value) in outputs {
            println!("output {name} = {value}");
        }
    }
}

pub(super) fn render_function_receipt_json(
    function: &str,
    entrypoint: &str,
    inputs_from: &str,
    inputs: &[(String, String)],
    outputs: &[(String, String)],
    meaning_id: &str,
) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.eval-function");
    object.int("schema_version", 1);
    object.string("function", function);
    object.string("entrypoint", entrypoint);
    object.string("inputs_from", inputs_from);
    object.string("meaning_id", meaning_id);
    object.object_field("inputs", &value_map_json(inputs));
    object.object_field("outputs", &value_map_json(outputs));
    object.finish()
}

/// One JSON object body from an ordered `(name, rendered value)` list.
pub(super) fn value_map_json(entries: &[(String, String)]) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in entries {
        object.string(name, value);
    }
    object.finish().trim_end().to_string()
}
