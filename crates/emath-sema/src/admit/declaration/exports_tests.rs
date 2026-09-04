use super::*;
use emath_ir::goal::CompileSpec;

/// Admit the `compile:`, `exports:`, and `tests:` sections
/// (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_exports_tests(
    mut admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    decl: &emath_core::tree::Declaration,
    is_policy: bool,
    is_model: bool,
    inputs: &[Field],
    outputs_raw: &[Field],
    state: &[Field],
    definitions: &BTreeMap<String, ExprId>,
    constructors: &[Constructor],
) -> (CompileSpec, Vec<emath_ir::goal::Export>, Vec<TestCase>) {
    // Compile spec.
    let compile_spec = admit_compile_spec(&mut admitter, by_name.get("compile").copied());

    // Exports.
    let mut exports = Vec::new();
    if let Some(section) = by_name.get("exports") {
        for stmt in &section.suite.statements {
            let StmtKind::Command { head, .. } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "exports must be `public <kind> <name>` commands",
                    stmt.source,
                );
                continue;
            };
            let mut words = head.iter().map(String::as_str);
            let visibility_word = words.next().unwrap_or("");
            let kind = words.next().unwrap_or("");
            let name = words.next().unwrap_or("");
            let public = visibility_word == "public";
            if !public {
                admitter.error(
                    "E-NAME-021",
                    "Phase 1 exports must be `public`",
                    stmt.source,
                );
                continue;
            }
            match kind {
                "constructor" => {
                    if name != "new" || constructors.is_empty() {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported constructor `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "constructor".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "function" => {
                    let from_diff = name.strip_prefix("gradient_").is_some_and(|target| {
                        by_name.get("goals").is_some_and(|section| {
                            section.suite.statements.iter().any(|stmt| {
                                matches!(
                                    &stmt.kind,
                                    StmtKind::Section(goal)
                                        if goal.name == "differentiate"
                                            && goal.generic.as_deref() == Some(target)
                                )
                            })
                        })
                    });
                    if !definitions.contains_key(name)
                        && !outputs_raw.iter().any(|o| o.name == *name)
                        && !from_diff
                    {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported function `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "function".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "type" => {
                    if name != decl.name {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported type `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "type".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                other => {
                    admitter.error(
                        "E-NAME-021",
                        format!("unsupported export kind `{other}`"),
                        stmt.source,
                    );
                }
            }
        }
    }

    // Tests.
    let mut tests: Vec<TestCase> = Vec::new();
    if let Some(section) = by_name.get("tests") {
        // Test blocks are named surface: each resolves to one generated
        // test fn `<function>_<name>`, so two blocks with the same name
        // would collide in generated Rust. Refuse the second (E-NAME-022,
        // the duplicate-declaration lane) instead of emitting a broken
        // crate the compiler only rejects downstream as a raw rustc
        // E0428. Scope is per declaration: two functions may each carry
        // an `example <eval>:` block.
        let mut seen_tests: std::collections::BTreeSet<String> = Default::default();
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `example <name>:` blocks inside `tests:`",
                    stmt.source,
                );
                continue;
            };
            if example.name != "example" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown test block `{}`", example.name),
                    example.source,
                );
                continue;
            }
            let mut given: BTreeMap<String, ExprId> = BTreeMap::new();
            let mut expect: Option<ExprId> = None;
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        if !admitter.inputs.contains_key(name)
                            && !admitter.params.contains_key(name)
                            && !(is_model && admitter.states.contains_key(name))
                        {
                            admitter.error(
                                "E-NAME-026",
                                format!(
                                    "`given` name `{name}` is not an input, constructor parameter, or model state field"
                                ),
                                inner.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((
                                id,
                                Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Rat
                                | Infer::BigInt
                                | Infer::Complex
                                | Infer::Unit { .. }
                                | Infer::HostDeferred
                                | Infer::Vector { .. }
                                | Infer::Matrix { .. }
                                | Infer::Tensor { .. }
                                | Infer::OptionCarrier
                                | Infer::ResultCarrier,
                            )) => {
                                given.insert(name.clone(), id);
                            }
                            Some((
                                _,
                                Infer::Bool | Infer::Text | Infer::Set(_) | Infer::Record(_),
                            )) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric or tensor"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric; opaque host values are not scalars"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Series | Infer::Sequence)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric or tensor; a series is admitted data, not a scalar input"),
                                    inner.source,
                                );
                            }
                            None => {}
                        }
                    }
                    StmtKind::Expect(expr) => match admitter.lower_expr(expr) {
                        Some((id, Infer::Bool)) => {
                            // Multiple `expect` lines are a conjunction; keeping
                            // only the last one silently dropped earlier checks.
                            expect = Some(match expect {
                                Some(prev) => admitter.push_expr(
                                    ExprNode::Binary {
                                        operation: BinaryOp::And,
                                        left: prev,
                                        right: id,
                                    },
                                    inner.source,
                                ),
                                None => id,
                            });
                        }
                        Some((
                            _,
                            Infer::F64
                            | Infer::Nat
                            | Infer::Int
                            | Infer::Rat
                            | Infer::BigInt
                            | Infer::Complex
                            | Infer::Vector { .. }
                            | Infer::Matrix { .. }
                            | Infer::Tensor { .. }
                            | Infer::Unit { .. }
                            | Infer::HostDeferred
                            | Infer::Series
                            | Infer::Sequence
                            | Infer::Text
                            | Infer::Set(_)
                            | Infer::Record(_)
                            | Infer::OptionCarrier
                            | Infer::ResultCarrier
                            | Infer::Opaque,
                        )) => {
                            admitter.error(
                                "E-TYPE-012",
                                "`expect` must be a Boolean comparison",
                                inner.source,
                            );
                        }
                        None => {}
                    },
                    other => {
                        let _ = other;
                        admitter.error(
                            "E-SYN-101",
                            "only `given x = ...` and `expect ...` are allowed in example blocks",
                            inner.source,
                        );
                    }
                }
            }
            if is_policy || (is_model && !constructors.is_empty()) {
                // constructor parameters must be supplied by `given` values
                let constructor_params: Vec<String> = constructors
                    .first()
                    .map(|c| c.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();
                for param in &constructor_params {
                    if !given.contains_key(param) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "policy example `{}` must supply constructor parameter `{param}` via `given`",
                                example.generic.clone().unwrap_or_default()
                            ),
                            example.source,
                        );
                    }
                }
            }
            if is_model && constructors.is_empty() {
                for field in state {
                    if !given.contains_key(&field.name) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "model example `{}` must supply state `{name}` via `given`",
                                example.generic.clone().unwrap_or_default(),
                                name = field.name
                            ),
                            example.source,
                        );
                    }
                }
            }
            let test_name = example
                .generic
                .clone()
                .unwrap_or_else(|| format!("test_{}", tests.len()));
            if !seen_tests.insert(test_name.clone()) {
                admitter.error(
                    "E-NAME-022",
                    format!("duplicate test name `{test_name}`"),
                    example.source,
                );
                continue;
            }
            tests.push(TestCase {
                name: test_name,
                given,
                expect,
                source: example.source,
            });
        }
    }

    // Rebuild inputs/outputs/state as neutral fields.
    (compile_spec, exports, tests)
}
