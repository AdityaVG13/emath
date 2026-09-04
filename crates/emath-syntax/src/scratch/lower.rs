//! Expansion entry points: `expand_scratch`, `apply_solve_candidate`, wrapping and refusals.

use super::*;

pub(super) fn expansion(
    expanded: impl Into<String>,
    outcome: ExpansionOutcome,
    notes: Vec<ScratchNote>,
    diagnostics: Diagnostics,
    holes: Vec<HoleRecord>,
) -> ScratchExpansion {
    ScratchExpansion {
        expanded: expanded.into(),
        outcome,
        notes,
        holes,
        solve: SolveIntent::Absent,
        diagnostics,
    }
}

pub(super) fn solve_intent_payload(line: &str) -> Option<String> {
    let LineKind::Intent { verb, payload } = classify_line(line) else {
        return None;
    };
    if verb == IntentVerb::Solve {
        Some(payload)
    } else {
        None
    }
}

pub(super) fn claims_unlabeled_unique_solve(source: &str) -> bool {
    source.lines().any(|line| {
        let Some(payload) = solve_intent_payload(line) else {
            return false;
        };
        let lower = payload.to_ascii_lowercase();
        lower.contains("uniquely")
            || lower.contains("unique")
            || payload.contains("=>")
            || payload.contains(" uniquely ")
    })
}

pub(super) fn extract_solve_intent(source: &str) -> SolveIntent {
    let Some(payload) = source.lines().find_map(solve_intent_payload) else {
        return SolveIntent::Absent;
    };
    let (_, domain) = split_keyword_tail(&payload, "over");
    labeled_solve_menu(domain.as_deref())
}

pub(super) fn labeled_solve_menu(domain: Option<&str>) -> SolveIntent {
    match domain.map(str::trim).filter(|d| !d.is_empty()) {
        None => SolveIntent::Unlabeled,
        Some(d) => SolveWorld::parse_label(&d.to_ascii_lowercase())
            .map(SolveIntent::Over)
            .unwrap_or(SolveIntent::Unlabeled),
    }
}

/// Pin a labeled `solve` candidate into source (domain/method/holes).
///
/// `world` is a closed [`SolveWorld`]. Parse labels at the argv / API boundary
/// with [`SolveWorld::parse_label`]. There is no `&str` overload and no
/// `SolveCandidate` wrapper.
///
/// Returns `(rewritten source, meaning delta)`. Never a silent numeric root.
///
/// # Errors
///
/// Returns `Err` if the source has no `solve` intent to pin.
pub fn apply_solve_candidate(source: &str, world: SolveWorld) -> Result<(String, String), String> {
    let pin = world.pin_phrase();
    let mut found = false;
    let mut out = String::new();
    let required_parameter = match world {
        SolveWorld::Modular => Some("modulus"),
        SolveWorld::Numeric => Some("tolerance"),
        SolveWorld::RealPm | SolveWorld::Complex | SolveWorld::Symbolic => None,
    };
    if let Some(parameter) = required_parameter
        && !source
            .lines()
            .any(|line| line.trim().starts_with(parameter))
    {
        out.push_str(parameter);
        out.push_str(" = ?\n");
    }
    for line in source.lines() {
        if !found {
            if let Some(payload) = solve_intent_payload(line) {
                found = true;
                let (equation, _) = split_keyword_tail(&payload, "over");
                out.push_str("solve ");
                out.push_str(equation.trim());
                out.push(' ');
                out.push_str(pin);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !found {
        return Err("source has no `solve` intent to pin".into());
    }
    let delta = format!(
        "meaning: selected `{}` ({pin}); beginner_default={}; constructed worlds stay labeled",
        world.as_str(),
        world.beginner_default()
    );
    Ok((out, delta))
}

/// Expand L0/L1 scratch and L2 named shorthand to contracted emath.
#[must_use]
pub fn expand_scratch(source: &str) -> ScratchExpansion {
    let mut diagnostics = Diagnostics::new();
    let notes = Vec::new();
    refuse_hidden_desugar(source, &mut diagnostics);
    if crate::exactness::claims_exactness_with_open_holes(source) {
        refuse(
            &mut diagnostics,
            "E-SYN-147",
            "claiming exactness while holes remain open is refused; freeze and `--raise` must not upgrade open meaning",
            span_of_source(source),
            Pedagogy::teacher(
                "the file contains a typed hole (`=` `?`)",
                "a claimed exact closed form",
                "open meaning is a continuation; freeze does not invent a solution",
                "drop `claim exact` / `exact` until the hole is filled, or keep the hole labeled open",
                "language/examples/intro/scratch.emath",
            ),
        );
    }

    if first_content_line(source).is_none() {
        return expansion(
            source,
            ExpansionOutcome::Identity,
            notes,
            diagnostics,
            Vec::new(),
        );
    }

    if mix_scratch_and_declaration(source) {
        refuse(
            &mut diagnostics,
            "E-SYN-141",
            "scratch lines cannot mix with an explicit `emath` declaration; wrap the scratch in `emath function Name:` or drop the declaration",
            span_of_source(source),
            Pedagogy::teacher(
                "the file contains both an `emath` header and margin scratch",
                "which surface is official",
                "scratch and contracted declarations are the same IR; mixing hides the desugar",
                "wrap the scratch in `emath function Name:` or drop the header",
                "language/examples/intro/scratch.emath",
            ),
        );
        return expansion(
            source,
            ExpansionOutcome::Identity,
            notes,
            diagnostics,
            Vec::new(),
        );
    }

    if claims_unlabeled_unique_solve(source) {
        refuse(
            &mut diagnostics,
            "E-SYN-151",
            "an unlabeled numeric root is not the unique solution of `solve`; name a candidate (`over Real`) or inspect the labeled set",
            span_of_source(source),
            Pedagogy::teacher(
                "a solve goal with a naked unique numeric claim",
                "a labeled domain (`over Real`) or the candidate menu from `emath expand` / `emath solve --check`",
                "x^2 = 2 is many problems; 1.414… is not the intended meaning",
                "write `solve x^2 = 2 over Real`, or run `emath solve --check` and pin a candidate",
                "language/examples/intro/scratch.emath",
            ),
        );
    }

    let mut expansion = if needs_scratch_wrap(source) {
        wrap_scratch(source, diagnostics, notes)
    } else {
        rewrite_l2(source, diagnostics, notes)
    };
    expansion.solve = extract_solve_intent(source);
    expansion
}

pub(super) fn refuse_hidden_desugar(source: &str, diagnostics: &mut Diagnostics) {
    for (offset, line) in line_offsets(source) {
        let trimmed = line.trim();
        refuse_if_contains_pair(
            diagnostics,
            trimmed,
            offset,
            line.len(),
            "emath:hide-desugar",
            "@hide_desugar",
            "E-SYN-144",
            "hidden desugaring is refused; every shorthand must expand through `emath expand`",
            Pedagogy::teacher(
                "a hide-desugar marker is present",
                "the contracted form of this shorthand",
                "hidden defaults are the learnability failure mode",
                "delete the hide marker and run `emath expand`",
                "language/examples/intro/scratch.emath",
            ),
        );
        refuse_if_contains_pair(
            diagnostics,
            trimmed,
            offset,
            line.len(),
            "hide alternatives",
            "@silent_default",
            "E-SYN-146",
            "unlabeled defaults that hide solve/plot candidates are refused; name the domain (`over Real`) or inspect candidates via `emath expand`",
            Pedagogy::teacher(
                "a silent-default marker is present",
                "labeled candidates (Real, Complex, modular, symbolic, numeric)",
                "intent-completion must name alternatives, not pick one",
                "write `over Real` or inspect candidates with `emath expand`",
                "language/examples/intro/scratch.emath",
            ),
        );
    }
}

pub(super) fn refuse_if_contains_pair(
    diagnostics: &mut Diagnostics,
    trimmed: &str,
    offset: usize,
    line_len: usize,
    a: &str,
    b: &str,
    code: &'static str,
    message: &'static str,
    pedagogy: Pedagogy,
) {
    if trimmed.contains(a) || trimmed.contains(b) {
        refuse(
            diagnostics,
            code,
            message,
            span_bytes(offset, line_len),
            pedagogy,
        );
    }
}

pub(super) fn needs_scratch_wrap(source: &str) -> bool {
    first_content_line(source).is_some_and(|line| !is_item_header(line))
}

pub(super) fn mix_scratch_and_declaration(source: &str) -> bool {
    let mut saw_scratch_at_margin = false;
    let mut saw_decl = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if !is_content_line(trimmed) {
            continue;
        }
        let at_margin = is_unindented(line);
        if is_item_header(trimmed) {
            saw_decl = true;
        } else if at_margin {
            saw_scratch_at_margin = true;
        }
    }
    saw_scratch_at_margin && saw_decl
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_scratch_kinds(
    kinds: impl IntoIterator<Item = LineKind>,
    expr_count: usize,
    examples: &mut Vec<(String, String)>,
    defs: &mut Vec<(String, String)>,
    extra_comments: &mut Vec<String>,
    extra_goals: &mut Vec<(String, String)>,
    compile_target: &mut Option<(String, String)>,
    notes: &mut Vec<ScratchNote>,
    holes: &mut Vec<HoleRecord>,
    diagnostics: &mut Diagnostics,
    span_source: &str,
    example_conflict_verbose: bool,
) {
    let mut intent_index = 0usize;
    for kind in kinds {
        match kind {
            LineKind::Assign { name, rhs } => {
                defs.push((name, rhs));
            }
            LineKind::Example { name, value } => {
                if let Some((_, previous)) = examples.iter().find(|(n, _)| n == &name) {
                    if literal_class(previous) != literal_class(&value)
                        && literal_class(previous) != "other"
                        && literal_class(&value) != "other"
                    {
                        let message = if example_conflict_verbose {
                            format!(
                                "example `{name}` has conflicting types ({previous} vs {value}); refuse rather than pick a silent default"
                            )
                        } else {
                            format!("example `{name}` has conflicting types")
                        };
                        refuse(
                            diagnostics,
                            "E-SYN-142",
                            message,
                            span_of_source(span_source),
                            Pedagogy::teacher(
                                format!("example `{name}` appears more than once"),
                                "a single type for that example",
                                "conflicting examples must refuse rather than pick a silent default",
                                "keep one `example` binding, or make both the same type",
                                "language/examples/intro/scratch.emath",
                            ),
                        );
                    }
                }
                examples.push((name, value));
            }
            LineKind::Hole { name } => {
                defs.push((name.clone(), "Hole".into()));
                extra_comments.push(format!("# emath exactness: open hole {name}"));
                notes.push(ScratchNote {
                    inferred: format!("hole {name}"),
                    rationale: "typed hole remains open meaning; freeze must not claim it exact"
                        .into(),
                    replacement: format!("{name} = ?"),
                    stability: ExactnessStatus::Open,
                });
                record_hole(holes, name);
            }
            LineKind::Require { expr } => {
                extra_comments.push(format!("# emath hole constraint: {expr}"));
                constrain_last_hole(holes, expr);
            }
            LineKind::Expr(expr) => {
                intent_index += 1;
                let name = if expr_count <= 1 && defs.is_empty() {
                    SYNTH_RESULT.to_string()
                } else {
                    format!("{SYNTH_RESULT}_{intent_index}")
                };
                defs.push((name, expr));
            }
            LineKind::Intent { verb, payload } => {
                intent_index += 1;
                lower_intent(
                    verb,
                    &payload,
                    expr_count,
                    intent_index,
                    defs,
                    extra_comments,
                    extra_goals,
                    compile_target,
                    notes,
                );
                attach_find_continuation(holes, verb, &payload);
            }
            LineKind::Invalid => {}
        }
    }
}

pub(super) fn wrap_scratch(
    source: &str,
    mut diagnostics: Diagnostics,
    mut notes: Vec<ScratchNote>,
) -> ScratchExpansion {
    let mut lines = Vec::new();
    for (offset, raw) in line_offsets(source) {
        let trimmed = raw.trim();
        if !is_content_line(trimmed) {
            continue;
        }
        match classify_line(trimmed) {
            LineKind::Invalid => {
                let first = first_word(trimmed);
                let word_count = trimmed.split_whitespace().count();
                if !first.is_empty()
                    && first.chars().all(|ch| ch.is_ascii_alphabetic())
                    && word_count <= 4
                    && trimmed[first.len()..]
                        .trim_start()
                        .chars()
                        .any(|ch| !ch.is_whitespace())
                {
                    refuse(
                        &mut diagnostics,
                        "E-SYN-148",
                        format!(
                            "unknown intent verb `{first}`; known verbs are plot, solve, simulate, compile, differentiate, integrate, convert, find, show, prove, compare, share, build"
                        ),
                        span_bytes(offset, raw.len()),
                        Pedagogy::teacher(
                            "the line starts with a verb-like word",
                            "a known intent verb or an assignment/expression",
                            "unknown verbs must be typed diagnostics, not a crash or silent skip",
                            "use `solve`/`plot`/`find` or write `name = expr`",
                            "language/examples/intro/scratch.emath",
                        ),
                    );
                } else {
                    refuse(
                        &mut diagnostics,
                        "E-SYN-145",
                        "scratch line is not an expression, assignment, example, or intent verb",
                        span_bytes(offset, raw.len()),
                        Pedagogy::teacher(
                            "this file is L0/L1 scratch",
                            "an expression, `name = expr`, `example x = 3`, or an intent verb",
                            "junk text is not a hidden beginner dialect",
                            "start with `2+2` or `y = x^2`",
                            "language/examples/intro/scratch.emath",
                        ),
                    );
                }
            }
            kind => lines.push((offset, raw.to_string(), kind)),
        }
    }
    if diagnostics.has_errors() {
        return expansion(
            source,
            ExpansionOutcome::Refused {
                level: ScratchLevel::L0,
            },
            notes,
            diagnostics,
            Vec::new(),
        );
    }

    let mut examples: Vec<(String, String)> = Vec::new();
    let mut defs: Vec<(String, String)> = Vec::new();
    let mut extra_comments: Vec<String> = Vec::new();
    let mut extra_goals: Vec<(String, String)> = Vec::new();
    let mut compile_target: Option<(String, String)> = None;
    let mut has_example = false;
    let mut expr_count = 0usize;

    for (_, _, kind) in &lines {
        if let LineKind::Example { .. } = kind {
            has_example = true;
        }
        if matches!(kind, LineKind::Expr(_) | LineKind::Intent { .. }) {
            expr_count += 1;
        }
    }

    let mut holes: Vec<HoleRecord> = Vec::new();
    apply_scratch_kinds(
        lines.into_iter().map(|(_, _, kind)| kind),
        expr_count,
        &mut examples,
        &mut defs,
        &mut extra_comments,
        &mut extra_goals,
        &mut compile_target,
        &mut notes,
        &mut holes,
        &mut diagnostics,
        source,
        true,
    );
    finalize_holes(&mut holes);
    emit_hole_comments(&holes, &mut extra_comments);

    if diagnostics.has_errors() {
        return expansion(
            source,
            ExpansionOutcome::Refused {
                level: if has_example {
                    ScratchLevel::L1
                } else {
                    ScratchLevel::L0
                },
            },
            notes,
            diagnostics,
            holes,
        );
    }

    let assigned: Vec<&str> = defs.iter().map(|(n, _)| n.as_str()).collect();
    let free = free_names(&defs, &examples, &assigned);

    for name in &free {
        notes.push(ScratchNote {
            inferred: format!("inputs.{name}"),
            rationale:
                "free name in scratch; admission defaults untyped inputs to Float64 (N-TYPE-001)"
                    .into(),
            replacement: format!("    inputs:\n        {name}: Float64"),
            stability: ExactnessStatus::Inferred,
        });
    }
    notes.push(ScratchNote {
        inferred: format!("declaration {SYNTH_DECL}"),
        rationale:
            "L0/L1 scratch desugars to an implicit emath function; run `emath expand` to inspect"
                .into(),
        replacement: format!("emath function {SYNTH_DECL}:"),
        stability: ExactnessStatus::Inferred,
    });
    diagnostics.note(
        "N-SCRATCH-001",
        format!(
            "scratch expanded to `emath function {SYNTH_DECL}:`; run `emath expand` to inspect the contracted form"
        ),
        span_of_source(source),
    );

    let expanded = render_function(
        SYNTH_DECL,
        None,
        &free,
        &defs,
        &examples,
        compile_target.as_ref(),
        &extra_comments,
        &extra_goals,
    );
    let level = if has_example {
        ScratchRewriteLevel::L1
    } else {
        ScratchRewriteLevel::L0
    };
    expansion(
        expanded,
        ExpansionOutcome::Rewritten { level },
        notes,
        diagnostics,
        holes,
    )
}
