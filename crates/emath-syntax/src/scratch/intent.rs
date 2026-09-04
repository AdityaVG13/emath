//! Intent-verb lowering (L1) and L2 rewrites; hole bookkeeping.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_intent(
    verb: IntentVerb,
    payload: &str,
    expr_count: usize,
    intent_index: usize,
    defs: &mut Vec<(String, String)>,
    extra_comments: &mut Vec<String>,
    extra_goals: &mut Vec<(String, String)>,
    compile_target: &mut Option<(String, String)>,
    notes: &mut Vec<ScratchNote>,
) {
    let verb_word = verb.as_str();
    let result_name = if expr_count <= 1 && defs.is_empty() {
        SYNTH_RESULT.to_string()
    } else {
        format!("{verb_word}_result")
    };
    match verb {
        IntentVerb::Plot => {
            let (expr, range) = split_keyword_tail(payload, "on");
            extra_comments.push(format!(
                "# emath expand: intent=plot range={}",
                range.unwrap_or("unspecified")
            ));
            defs.push((result_name, expr.trim().to_string()));
            notes.push(ScratchNote {
                inferred: "goal plot".into(),
                rationale: "plot lowers to evaluating the expression; the range is recorded, not a second parser".into(),
                replacement: "definitions: result = <expr>".into(),
                stability: ExactnessStatus::Inferred,
            });
        }
        IntentVerb::Solve => {
            let (equation, domain) = split_keyword_tail(payload, "over");
            let (lhs, rhs) = split_equation(equation.trim());
            let residual = format!("({lhs}) - ({rhs})");
            let var = first_free_ident(lhs).unwrap_or("x");
            extra_comments.push(format!(
                "# emath expand: intent=solve domain={} candidates={SOLVE_CANDIDATES}",
                domain.unwrap_or("unspecified (candidates labeled, none silently chosen)")
            ));
            defs.push(("residual".into(), residual));
            defs.push((result_name, format!("solve(residual) wrt {var}")));
            notes.push(ScratchNote {
                inferred: format!("solve candidates: {SOLVE_CANDIDATES}"),
                rationale:
                    "intent-completion must label alternatives; `over Real` declares the domain"
                        .into(),
                replacement: "result = solve(residual) wrt x".into(),
                stability: if domain.is_some() {
                    ExactnessStatus::Declared
                } else {
                    ExactnessStatus::Inferred
                },
            });
            let _ = intent_index;
        }
        IntentVerb::Convert => {
            let (qty, unit) = split_keyword_tail(payload, "to");
            let rhs = if let Some(unit) = unit {
                format!("({qty}) / (1 {unit})")
            } else {
                qty.trim().to_string()
            };
            defs.push((result_name, rhs));
            notes.push(ScratchNote {
                inferred: "convert".into(),
                rationale: "convert lowers to a quantity ratio against the target unit".into(),
                replacement: "result = (1 km) / (1 m)".into(),
                stability: ExactnessStatus::Inferred,
            });
        }
        IntentVerb::Differentiate => {
            let (expr, var) = split_keyword_tail(payload, "wrt");
            let var = var.unwrap_or("x");
            defs.push((
                result_name,
                format!("derivative({}) wrt {var}", expr.trim()),
            ));
        }
        IntentVerb::Integrate => {
            if let (expr, Some(range)) = split_keyword_tail(payload, "on") {
                let var = first_free_ident(expr).unwrap_or("x");
                defs.push((
                    result_name,
                    format!("integral {var} in {range}: {}", expr.trim()),
                ));
            } else {
                let (expr, var) = split_keyword_tail(payload, "wrt");
                let var = var.unwrap_or("x");
                defs.push((
                    result_name,
                    format!("integral {var} in a..b: {}", expr.trim()),
                ));
                notes.push(ScratchNote {
                    inferred: "inputs.a, inputs.b".into(),
                    rationale:
                        "indefinite integrate becomes a definite integral over open bounds a..b"
                            .into(),
                    replacement: "integral x in a..b: <expr>".into(),
                    stability: ExactnessStatus::Inferred,
                });
            }
        }
        IntentVerb::Simulate => {
            extra_comments.push(format!("# emath expand: intent=simulate phrase={payload}"));
            extra_comments.push(
                "# emath expand: simulate is a goal; supply an `emath model` to compute a trajectory".into(),
            );
            defs.push((result_name, "0".into()));
            notes.push(ScratchNote {
                inferred: "goal simulate".into(),
                rationale:
                    "English simulate phrases record intent; they do not mint a domain parser"
                        .into(),
                replacement: "emath model Name: with state/equations, then `emath simulate`".into(),
                stability: ExactnessStatus::Inferred,
            });
        }
        IntentVerb::Find
        | IntentVerb::Show
        | IntentVerb::Prove
        | IntentVerb::Compare
        | IntentVerb::Share
        | IntentVerb::Build => {
            extra_comments.push(format!(
                "# emath expand: intent={verb_word} phrase={payload}"
            ));
            let goal_kind = match verb {
                IntentVerb::Find | IntentVerb::Compare => "search",
                IntentVerb::Show => "evaluate",
                IntentVerb::Prove => "prove",
                IntentVerb::Share | IntentVerb::Build => "compile",
                IntentVerb::Plot
                | IntentVerb::Solve
                | IntentVerb::Simulate
                | IntentVerb::Compile
                | IntentVerb::Differentiate
                | IntentVerb::Integrate
                | IntentVerb::Convert => {
                    unreachable!("goal-like intent verbs only")
                }
            };
            let target = goal_target(payload);
            extra_goals.push((goal_kind.to_string(), target));
            if verb == IntentVerb::Build || verb == IntentVerb::Share {
                *compile_target = Some(("rust".into(), "library".into()));
            }
            if defs.is_empty() {
                defs.push((result_name, "Hole".into()));
            }
            notes.push(ScratchNote {
                inferred: format!("goal {verb_word}"),
                rationale: format!(
                    "`{verb_word}` lowers to Goal IR `{goal_kind}`; not a domain parser"
                ),
                replacement: format!("goals: {goal_kind} <result>"),
                stability: ExactnessStatus::Inferred,
            });
        }
        IntentVerb::Compile => {
            let target = payload
                .rsplit_once(" to ")
                .map(|(_, rest)| rest.trim())
                .unwrap_or(payload.trim());
            let (lang, profile) = target.split_once('.').unwrap_or((target, "library"));
            *compile_target = Some((lang.to_string(), profile.to_string()));
            if defs.is_empty() {
                defs.push((result_name, "0".into()));
            }
            extra_comments.push(format!("# emath expand: intent=compile target={target}"));
            notes.push(ScratchNote {
                inferred: format!("compile {lang}.{profile}"),
                rationale: "compile this to rust.library lowers to the compile: section".into(),
                replacement: "compile:\n        target rust\n        profile library".into(),
                stability: ExactnessStatus::Inferred,
            });
        }
    }
}

pub(super) fn rewrite_l2(
    source: &str,
    mut diagnostics: Diagnostics,
    mut notes: Vec<ScratchNote>,
) -> ScratchExpansion {
    let pieces = split_top_level(source);
    let mut out = String::new();
    let mut rewritten_any = false;
    let mut holes: Vec<HoleRecord> = Vec::new();

    for piece in pieces {
        let text = match piece {
            TopPiece::Other(text) => {
                out.push_str(&text);
                continue;
            }
            TopPiece::Declaration(text) => text,
        };
        let Some((header, inline, body)) = split_declaration_text(&text) else {
            out.push_str(&text);
            continue;
        };
        // L2 is shorthand for `emath function` only. Imported custom kinds
        // own their bodies through mounted schemas and must reach the parser
        // unchanged instead of being mistaken for scratch.
        if !header.trim_start().starts_with("emath function ") {
            out.push_str(&text);
            continue;
        }
        let body_lines: Vec<String> = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect();
        let has_section = body_lines.iter().any(|line| is_section_head(line));
        if has_section {
            out.push_str(&text);
            continue;
        }
        let mut classified = Vec::new();
        let mut empty = inline.as_deref().is_none_or(|s| s.trim().is_empty())
            && body_lines.iter().all(|l| is_comment(l));
        if let Some(inline) = inline {
            let trimmed = inline.trim();
            if !trimmed.is_empty() {
                classified.push(classify_line(trimmed));
                empty = false;
            }
        }
        for line in &body_lines {
            if is_comment(line) {
                continue;
            }
            classified.push(classify_line(line));
            empty = false;
        }
        if empty {
            refuse(
                &mut diagnostics,
                "E-SYN-143",
                "L2 named declaration needs a body (`y = x^2`) or L3 sections; a name without a body is not L0 scratch",
                span_of_source(&text),
                Pedagogy::teacher(
                    "an `emath function Name:` header is present",
                    "a body of assignments/examples/intent verbs, or L3 sections",
                    "a name without a body is not L0 scratch",
                    "write `y = x^2` under the name, or use full `inputs:`/`definitions:` sections",
                    "language/examples/intro/hello-square.emath",
                ),
            );
            out.push_str(&text);
            continue;
        }
        if classified.iter().any(|k| matches!(k, LineKind::Invalid)) {
            refuse(
                &mut diagnostics,
                "E-SYN-143",
                "L2 body must be assignments, examples, or intent verbs; unknown kind/body is a typed refuse, not a scratch grab",
                span_of_source(&text),
                Pedagogy::teacher(
                    "an L2 named declaration body is present",
                    "assignments, examples, or intent verbs",
                    "unknown kind/body is not a scratch grab",
                    "write `y = x^2` or `example x = 3`",
                    "language/examples/intro/hello-square.emath",
                ),
            );
            out.push_str(&text);
            continue;
        }

        let mut examples = Vec::new();
        let mut defs = Vec::new();
        let mut extra_comments = Vec::new();
        let mut extra_goals = Vec::new();
        let mut compile_target = None;
        let expr_count = classified
            .iter()
            .filter(|k| matches!(k, LineKind::Expr(_) | LineKind::Intent { .. }))
            .count();
        let mut piece_holes: Vec<HoleRecord> = Vec::new();
        apply_scratch_kinds(
            classified,
            expr_count,
            &mut examples,
            &mut defs,
            &mut extra_comments,
            &mut extra_goals,
            &mut compile_target,
            &mut notes,
            &mut piece_holes,
            &mut diagnostics,
            &text,
            false,
        );
        finalize_holes(&mut piece_holes);
        emit_hole_comments(&piece_holes, &mut extra_comments);
        holes.extend(piece_holes);

        let assigned: Vec<&str> = defs.iter().map(|(n, _)| n.as_str()).collect();
        let body_free = free_names(&defs, &[], &assigned);
        let head = header_args(&header);
        let head_names: Vec<&str> = head.iter().map(|(name, _)| name.as_str()).collect();
        let mut l2_ok = true;
        if !head.is_empty() {
            for free_name in &body_free {
                if !head_names.contains(&free_name.as_str()) {
                    refuse(
                        &mut diagnostics,
                        "E-SYN-149",
                        format!(
                            "L2 header names `{free_name}` in the body but not in the explicit signature; that is a typed refusal, not a guessed coercion"
                        ),
                        span_of_source(&text),
                        Pedagogy::teacher(
                            "an explicit head-arg signature is present",
                            format!(
                                "body free names to be a subset of the head-args ({})",
                                head_names.join(", ")
                            ),
                            "L2 must not coerce a body name onto a different header name",
                            format!(
                                "rename the header argument to `{free_name}`, or use `{free_name}` in the body"
                            ),
                            "language/examples/intro/hello-square.emath",
                        ),
                    );
                    l2_ok = false;
                }
            }
        }
        for (_, rhs) in &defs {
            for callee in call_position_names(rhs) {
                if is_builtin(&callee) || assigned.contains(&callee.as_str()) {
                    continue;
                }
                refuse(
                    &mut diagnostics,
                    "E-SYN-150",
                    format!(
                        "L2 cannot infer a domain for `{callee}` without a hole; unknown callees are refusals, not silent inputs"
                    ),
                    span_of_source(&text),
                    Pedagogy::teacher(
                        format!("`{callee}(...)` appears in an L2 body"),
                        format!("a hole `{callee} = ?` or a known builtin/definition"),
                        "an unknown callee is not a guessed Float64 input",
                        format!(
                            "write `{callee} = ?` under the name, or bind `{callee}` in `definitions:`"
                        ),
                        "language/examples/intro/hello-square.emath",
                    ),
                );
                l2_ok = false;
            }
        }
        if !l2_ok {
            out.push_str(&text);
            continue;
        }
        let free: Vec<String> = if head.is_empty() {
            body_free
        } else {
            Vec::new()
        };
        let name = declaration_name(&header).unwrap_or(SYNTH_DECL);
        for free_name in &free {
            notes.push(ScratchNote {
                inferred: format!("inputs.{free_name}"),
                rationale:
                    "L2 body inferred a free name; declare `inputs:` or head-args to pin the domain"
                        .into(),
                replacement: format!("    inputs:\n        {free_name}: Float64"),
                stability: ExactnessStatus::Inferred,
            });
        }
        notes.push(ScratchNote {
            inferred: format!("declaration {name}"),
            rationale: "L2 named shorthand; evidence remains open until an L3 evidence: section"
                .into(),
            replacement: header.trim().to_string(),
            stability: ExactnessStatus::Declared,
        });
        diagnostics.note(
            "N-SCRATCH-001",
            format!(
                "L2 `{name}` expanded to a contracted component with open evidence; run `emath expand`"
            ),
            span_of_source(&text),
        );

        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&render_from_header(
            header.trim(),
            if head.is_empty() {
                &free
            } else {
                &[] as &[String]
            },
            &defs,
            &examples,
            compile_target.as_ref(),
            &extra_comments,
            &extra_goals,
        ));
        rewritten_any = true;
    }

    if diagnostics.has_errors() {
        return expansion(
            source,
            ExpansionOutcome::Refused {
                level: ScratchLevel::L2,
            },
            notes,
            diagnostics,
            holes,
        );
    }
    if rewritten_any {
        expansion(
            out,
            ExpansionOutcome::Rewritten {
                level: ScratchRewriteLevel::L2,
            },
            notes,
            diagnostics,
            holes,
        )
    } else {
        expansion(
            source,
            ExpansionOutcome::Identity,
            notes,
            diagnostics,
            holes,
        )
    }
}

pub(super) fn refuse(
    diagnostics: &mut Diagnostics,
    code: &'static str,
    message: impl Into<String>,
    span: Span,
    pedagogy: Pedagogy,
) {
    diagnostics.error(code, message, span);
    diagnostics.attach_pedagogy(pedagogy);
}

pub(super) fn record_hole(holes: &mut Vec<HoleRecord>, name: String) {
    holes.push(HoleRecord::open(name));
}

pub(super) fn constrain_last_hole(holes: &mut [HoleRecord], expr: String) {
    if let Some(hole) = holes.last_mut() {
        hole.constraints.push(expr);
    }
}

pub(super) fn attach_find_continuation(holes: &mut [HoleRecord], verb: IntentVerb, payload: &str) {
    if verb != IntentVerb::Find {
        return;
    }
    let needle = payload
        .split_whitespace()
        .next()
        .unwrap_or(payload)
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    let index = holes
        .iter()
        .position(|hole| hole.name == needle)
        .or_else(|| holes.len().checked_sub(1));
    if let Some(index) = index {
        holes[index].continuation = HoleContinuation::Search {
            goal: payload.trim().to_string(),
        };
    }
}

pub(super) fn finalize_holes(holes: &mut [HoleRecord]) {
    for hole in holes {
        if hole.constraints.is_empty() {
            hole.candidates.clear();
            hole.rejections.push(HoleRejection {
                attempt: "invented closed form".into(),
                reason: "unconstrained hole stays open; no solution is invented".into(),
            });
            continue;
        }
        hole.candidates = labeled_hole_candidates(&hole.constraints);
        if matches!(hole.continuation, HoleContinuation::Open) {
            hole.rejections.push(HoleRejection {
                attempt: "silent fill".into(),
                reason: "constraints are recorded; continuation stays open until `find`".into(),
            });
        }
    }
}

pub(super) fn emit_hole_comments(holes: &[HoleRecord], extra_comments: &mut Vec<String>) {
    for hole in holes {
        extra_comments.push(format!("# emath hole object: {}", hole.summary()));
        for candidate in &hole.candidates {
            extra_comments.push(format!(
                "# emath hole candidate: {} ({}) status=labeled",
                candidate.label,
                candidate.kind.as_str()
            ));
        }
        for rejection in &hole.rejections {
            extra_comments.push(format!(
                "# emath hole rejection: {} — {}",
                rejection.attempt, rejection.reason
            ));
        }
    }
}

pub(super) fn labeled_hole_candidates(constraints: &[String]) -> Vec<HoleCandidate> {
    let joined = constraints.join(" ");
    let mut candidates = Vec::new();
    if joined.contains("derivative") {
        candidates.push(HoleCandidate {
            label: "exponential family (satisfies f'=f)".into(),
            kind: HoleKind::Symbolic,
        });
    }
    if joined.contains("(0)") || joined.contains("f(0)") {
        candidates.push(HoleCandidate {
            label: "initial-value family".into(),
            kind: HoleKind::Symbolic,
        });
    }
    candidates.push(HoleCandidate {
        label: "numeric search".into(),
        kind: HoleKind::Numeric,
    });
    candidates
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IntentVerb {
    Plot,
    Solve,
    Simulate,
    Compile,
    Differentiate,
    Integrate,
    Convert,
    Find,
    Show,
    Prove,
    Compare,
    Share,
    Build,
}

impl IntentVerb {
    #[must_use]
    pub(super) fn parse_word(word: &str) -> Option<Self> {
        match word {
            "plot" => Some(Self::Plot),
            "solve" => Some(Self::Solve),
            "simulate" => Some(Self::Simulate),
            "compile" => Some(Self::Compile),
            "differentiate" => Some(Self::Differentiate),
            "integrate" => Some(Self::Integrate),
            "convert" => Some(Self::Convert),
            "find" => Some(Self::Find),
            "show" => Some(Self::Show),
            "prove" => Some(Self::Prove),
            "compare" => Some(Self::Compare),
            "share" => Some(Self::Share),
            "build" => Some(Self::Build),
            _ => None,
        }
    }

    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Self::Plot => "plot",
            Self::Solve => "solve",
            Self::Simulate => "simulate",
            Self::Compile => "compile",
            Self::Differentiate => "differentiate",
            Self::Integrate => "integrate",
            Self::Convert => "convert",
            Self::Find => "find",
            Self::Show => "show",
            Self::Prove => "prove",
            Self::Compare => "compare",
            Self::Share => "share",
            Self::Build => "build",
        }
    }
}
