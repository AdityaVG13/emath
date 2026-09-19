//! Goal-section extraction: `elaborate_requests` and its parsing helpers.

use super::*;

/// Extract the `goals:` section into request specs and validate targets
/// against the admitted declaration (`E-GOAL-041`/`E-GOAL-042`/`E-GOAL-043`).
pub fn elaborate_requests(
    package: &SemanticPackage,
    declaration_name: &str,
    sections: &[Section],
    diagnostics: &mut Diagnostics,
) -> Vec<RequestSpec> {
    let mut requests = Vec::new();
    let Some(section) = sections.iter().find(|s| s.name == "goals") else {
        // Ergonomics default: with no `goals:` section, every definition is
        // an evaluate goal (`produce rust.library`). Declaring `goals:`
        // selects the subset you want; definitions stay queryable either
        // way. The request carries the declaration head as its source so
        // goal ownership attaches to the declaration.
        let Some(declaration) = package
            .declarations
            .iter()
            .find(|d| d.name.leaf() == declaration_name)
        else {
            return requests;
        };
        // The default only mints goals that can exist. Carrier-typed
        // INPUTS (`Option<T>`, `Result<T, E>`) have no runtime binding
        // story — the input boundary refuses them
        // (`carrier_field_decl_refuses_typed`) — so a declaration whose
        // inputs are all/any carriers is a check-only surface: minting
        // the implicit `evaluate` goal would force its codegen to refuse
        // and make every such admission unbuildable. The E-SEC-133
        // warning (raised at setup) stays the visible trace of the
        // default; an explicit `goals:` section still pins real intent
        // and still refuses at the input boundary, as pinned.
        let inputs_carry_typed = declaration.inputs.iter().any(|field| {
            matches!(
                package.ty(field.ty),
                Some(emath_ir::TypeNode::OptionType(_))
                    | Some(emath_ir::TypeNode::Result { .. })
            )
        });
        if inputs_carry_typed {
            return requests;
        }
        for target in declaration.definitions.keys() {
            requests.push(RequestSpec {
                kind: "evaluate".into(),
                target: target.clone(),
                produce: "rust.library".into(),
                payload: GoalPayload::default(),
                source: declaration.source,
            });
        }
        return requests;
    };
    for stmt in &section.suite.statements {
        let StmtKind::Section(request) = &stmt.kind else {
            diagnostics.error(
                "E-SYN-101",
                "unexpected statement inside `goals:`",
                stmt.source,
            );
            continue;
        };
        match request.name.as_str() {
            "evaluate" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`evaluate` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                // the flat-goal sugar — a
                // heading line with no indented payload — is permanently
                // refused. A goal is never guessed from a heading; write
                // the command form (`produce rust.library`).
                if request.suite.statements.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        format!(
                            "flat goal `evaluate <{target}>:` is permanently refused: a goal heading with no payload is never guessed; write the command form (`evaluate <{target}>:` with `produce rust.library` inside)"
                        ),
                        request.source,
                    );
                    continue;
                }
                let produce = read_produce(&request.suite);
                if produce.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        "`evaluate` requires `produce rust.library` today",
                        request.source,
                    );
                    continue;
                }
                if produce != "rust.library" {
                    // Accepting an arbitrary produce target would silently
                    // admit an unimplemented export surface; refuse.
                    diagnostics.error(
                        "E-GOAL-042",
                        format!(
                            "produce target `{produce}` is outside the current subset (`rust.library` only)"
                        ),
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "evaluate".into(),
                    target,
                    produce,
                    payload: GoalPayload::default(),
                    source: request.source,
                });
            }
            "differentiate" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`differentiate` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                // same flat-goal rule as `evaluate`.
                if request.suite.statements.is_empty() {
                    diagnostics.error(
                        "E-GOAL-044",
                        format!(
                            "flat goal `differentiate <{target}>:` is permanently refused: write the command form (`differentiate <{target}>:` with `wrt [names]` inside)"
                        ),
                        request.source,
                    );
                    continue;
                }
                let payload = read_payload(&request.suite);
                if payload.wrt.is_empty() {
                    diagnostics.error(
                        "E-GOAL-044",
                        "`differentiate` requires `wrt [names]`",
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "differentiate".into(),
                    target,
                    produce: String::new(),
                    payload,
                    source: request.source,
                });
            }
            "simplify" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`simplify` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "simplify".into(),
                    target,
                    produce: String::new(),
                    payload: GoalPayload::default(),
                    source: request.source,
                });
            }
            "benchmark" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`benchmark` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                let payload = read_payload(&request.suite);
                if payload.against.is_none() {
                    diagnostics.error(
                        "E-GOAL-045",
                        "`benchmark` requires `against <path>`",
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "benchmark".into(),
                    target,
                    produce: String::new(),
                    payload,
                    source: request.source,
                });
            }
            other => {
                diagnostics.error(
                    "E-GOAL-043",
                    format!(
                        "request kind `{other}` is outside the current subset (supported: evaluate, differentiate, benchmark)"
                    ),
                    request.source,
                );
            }
        }
    }
    // targets must be outputs or definitions
    let declared: Vec<&String> = package
        .declarations
        .iter()
        .find(|d| d.name.leaf() == declaration_name)
        .map(|d| {
            d.outputs
                .iter()
                .map(|f| &f.name)
                .chain(d.definitions.keys())
                .collect()
        })
        .unwrap_or_default();
    for request in &requests {
        if !declared.contains(&&request.target) {
            diagnostics.error(
                "E-GOAL-041",
                format!(
                    "request target `{}` is not an output or definition",
                    request.target
                ),
                request.source,
            );
        }
    }
    requests
}

pub(super) fn read_produce(suite: &emath_core::tree::Suite) -> String {
    for stmt in &suite.statements {
        if let StmtKind::Command { head, argument } = &stmt.kind {
            if head.first().is_some_and(|h| h == "produce") {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Path { segments, .. } = &expr.kind {
                        return segments.join(".");
                    }
                }
                if head.len() > 1 {
                    return head[1..].join(".");
                }
            }
        }
    }
    String::new()
}

pub(super) fn read_payload(suite: &emath_core::tree::Suite) -> GoalPayload {
    let mut payload = GoalPayload::default();
    for stmt in &suite.statements {
        let StmtKind::Command { head, argument } = &stmt.kind else {
            continue;
        };
        let Some(word) = head.first() else {
            continue;
        };
        match word.as_str() {
            "wrt" => payload.wrt = command_names(head, argument.as_ref()),
            "order" => {
                payload.order = command_u32(head, argument.as_ref());
            }
            "against" => {
                let path = command_path(head, argument.as_ref());
                if !path.is_empty() {
                    payload.against = Some(path);
                }
            }
            "measure" => payload.measure = command_names(head, argument.as_ref()),
            _ => {}
        }
    }
    payload
}

pub(super) fn command_names(head: &[String], argument: Option<&CommandArgument>) -> Vec<String> {
    match argument {
        Some(CommandArgument::List(items)) => items
            .iter()
            .filter_map(|item| match &item.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                _ => None,
            })
            .collect(),
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => vec![segments.join(".")],
            ExprKind::List(items) => items
                .iter()
                .filter_map(|item| match &item.kind {
                    ExprKind::Path { segments, .. } => Some(segments.join(".")),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        },
        None if head.len() > 1 => head[1..].to_vec(),
        _ => Vec::new(),
    }
}

pub(super) fn command_path(head: &[String], argument: Option<&CommandArgument>) -> String {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => segments.join("::"),
            _ => String::new(),
        },
        None if head.len() > 1 => head[1..].join("::"),
        _ => String::new(),
    }
}

pub(super) fn command_u32(head: &[String], argument: Option<&CommandArgument>) -> Option<u32> {
    let text = match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => text.as_str(),
            _ => return None,
        },
        None => head.get(1).map(String::as_str)?,
        _ => return None,
    };
    text.parse().ok()
}

/// Parse through the installed source-parser backend; `E-SYN-120` when none
/// is installed (wire `emath_syntax::install_source_parser` at startup).
pub(super) fn parse_through(
    text: &str,
    limits: &Limits,
    edition: emath_core::Edition,
) -> Result<(emath_core::tree::SyntaxTree, Diagnostics), Diagnostics> {
    let Some(parser) = source_parser() else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-SYN-120",
            "source parser backend not installed: call emath_syntax::install_source_parser once per process before parsing",
            Span::default(),
        );
        return Err(diagnostics);
    };
    Ok(parser.parse(text, FileId(0), limits, edition))
}

pub(super) fn nearest_manifest(source: &Path) -> Option<std::path::PathBuf> {
    let mut directory = source.parent();
    while let Some(current) = directory {
        let candidate = current.join("emath.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

pub(super) fn edition_from_manifest(manifest: &str) -> Result<emath_core::Edition, String> {
    for line in manifest.lines() {
        let content = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = content.split_once('=') else {
            continue;
        };
        if key.trim() != "edition" {
            continue;
        }
        let value = value.trim();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            return emath_core::Edition::from_manifest_str(&value[1..value.len() - 1])
                .map_err(|error| error.to_string());
        }
        return Err(format!(
            "{}: edition must be a quoted string",
            emath_core::E_PKG_EDITION_UNKNOWN
        ));
    }
    Err("E-PKG-EDITION-MISSING: emath.toml requires `edition = \"2026\"`".to_string())
}

/// The module name of an in-package file import: `use <package>.<module>`
/// where `<package>` matches the importing file's own `package` path
/// (one extra segment; the dot is a path separator to the lexer).
/// Library paths (`std.numeric.Real`), curated law packages
/// (`physics::classical`), and unprefixed paths do not match.
pub(super) fn file_import_module<'a>(
    path: &'a [String],
    package_path: Option<&[String]>,
) -> Option<&'a str> {
    let package_path = package_path?;
    if path.len() == package_path.len() + 1 && path.starts_with(package_path) {
        path.last().map(String::as_str)
    } else {
        None
    }
}
