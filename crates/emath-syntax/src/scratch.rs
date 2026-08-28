//! Official scratch expansion for progressive exactness L0–L2.
//!
//! Bare expressions, guided relationships, intent verbs, and named-declaration
//! shorthand lower to the same declaration IR as contracted components.
//! Inspect the expansion with `emath expand`.

use crate::exactness::ExactnessStatus;
use emath_core::{Diagnostics, FileId, Pedagogy, Span};

const SYNTH_DECL: &str = "Scratch";
const SYNTH_RESULT: &str = "result";

const SECTION_HEADS: &[&str] = &[
    "about",
    "algebraic",
    "compile",
    "constraints",
    "constructors",
    "definitions",
    "equation",
    "equations",
    "events",
    "evidence",
    "exports",
    "goals",
    "host",
    "inputs",
    "invariant",
    "outputs",
    "state",
    "tests",
    "transitions",
];

const SOLVE_CANDIDATES: &str = "Real, Complex, modular, symbolic, numeric";

const BUILTINS: &[&str] = &[
    "abs",
    "and",
    "at",
    "atan2",
    "Bool",
    "ceil",
    "Complex",
    "cos",
    "derivative",
    "else",
    "ensure",
    "example",
    "exists",
    "exp",
    "false",
    "Float64",
    "floor",
    "for",
    "forall",
    "Hole",
    "if",
    "in",
    "Int",
    "integral",
    "is_finite",
    "let",
    "ln",
    "log",
    "match",
    "max",
    "min",
    "Nat",
    "not",
    "on",
    "or",
    "over",
    "pi",
    "plot",
    "pow",
    "product",
    "Real",
    "require",
    "return",
    "self",
    "sin",
    "solve",
    "sqrt",
    "sum",
    "tan",
    "tanh",
    "then",
    "this",
    "to",
    "true",
    "while",
    "with",
    "wrt",
    "m",
    "km",
    "s",
    "kg",
    "g",
];

/// Progressive-exactness level of the surface that was expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchLevel {
    /// Bare expression or intent verb.
    L0,
    /// Named relationship plus optional `example` bindings.
    L1,
    /// `emath function Name:` (or model/policy) without required L3 sections.
    L2,
    /// Already a contracted declaration; expansion is identity.
    Canonical,
}

impl ScratchLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::Canonical => "canonical",
        }
    }
}

/// Level a successful rewrite may occupy. Canonical is identity, not a rewrite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchRewriteLevel {
    L0,
    L1,
    L2,
}

impl ScratchRewriteLevel {
    #[must_use]
    pub fn as_scratch_level(self) -> ScratchLevel {
        match self {
            Self::L0 => ScratchLevel::L0,
            Self::L1 => ScratchLevel::L1,
            Self::L2 => ScratchLevel::L2,
        }
    }
}

/// How scratch expansion concluded. A rewrite cannot be Canonical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionOutcome {
    Identity,
    Rewritten { level: ScratchRewriteLevel },
    Refused { level: ScratchLevel },
}

impl ExpansionOutcome {
    #[must_use]
    pub fn rewritten(self) -> bool {
        matches!(self, Self::Rewritten { .. })
    }

    #[must_use]
    pub fn level(self) -> ScratchLevel {
        match self {
            Self::Identity => ScratchLevel::Canonical,
            Self::Rewritten { level } => level.as_scratch_level(),
            Self::Refused { level } => level,
        }
    }
}

/// One inferred default recorded so the expansion is inspectable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScratchNote {
    pub inferred: String,
    pub rationale: String,
    pub replacement: String,
    pub stability: ExactnessStatus,
}

/// Symbolic or numeric labeled hole candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleKind {
    Symbolic,
    Numeric,
}

impl HoleKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }
}

/// Labeled candidate for a typed hole. Labels alternatives; never a filled-in solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleCandidate {
    pub label: String,
    pub kind: HoleKind,
}

/// An attempt that was considered and refused for a hole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleRejection {
    pub attempt: String,
    pub reason: String,
}

/// What happens next for an open hole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleContinuation {
    /// Meaning stays open; freeze must not claim exactness.
    Open,
    /// `find <name>` recorded a search goal over the hole.
    Search { goal: String },
}

impl HoleContinuation {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Search { .. } => "search",
        }
    }
}

/// Durable typed-hole object: constraints, labeled candidates, rejections, continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleRecord {
    pub name: String,
    pub constraints: Vec<String>,
    pub candidates: Vec<HoleCandidate>,
    pub rejections: Vec<HoleRejection>,
    pub continuation: HoleContinuation,
}

impl HoleRecord {
    #[must_use]
    pub fn open(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            constraints: Vec::new(),
            candidates: Vec::new(),
            rejections: Vec::new(),
            continuation: HoleContinuation::Open,
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "hole {} constraints={} candidates={} rejections={} continuation={}",
            self.name,
            self.constraints.len(),
            self.candidates.len(),
            self.rejections.len(),
            self.continuation.as_str()
        )
    }
}

/// Closed set of labeled `solve` worlds. The menu is these five rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveWorld {
    RealPm,
    Complex,
    Modular,
    Symbolic,
    Numeric,
}

impl SolveWorld {
    pub const ALL: [Self; 5] = [
        Self::RealPm,
        Self::Complex,
        Self::Modular,
        Self::Symbolic,
        Self::Numeric,
    ];

    #[must_use]
    pub fn parse_label(label: &str) -> Option<Self> {
        match label {
            "real" | "real-pm" | "ℝ" => Some(Self::RealPm),
            "complex" => Some(Self::Complex),
            "modular" => Some(Self::Modular),
            "symbolic" => Some(Self::Symbolic),
            "numeric" => Some(Self::Numeric),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RealPm => "real-pm",
            Self::Complex => "complex",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn result_type(self) -> &'static str {
        match self {
            Self::RealPm => "Real",
            Self::Complex => "Complex",
            Self::Modular => "Int",
            Self::Symbolic => "expression",
            Self::Numeric => "Float64",
        }
    }

    #[must_use]
    pub fn domain(self) -> &'static str {
        match self {
            Self::RealPm => "Real",
            Self::Complex => "Complex",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn exactness(self) -> &'static str {
        match self {
            Self::RealPm | Self::Complex => "exact-algebraic",
            Self::Modular => "exact",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric-tolerance",
        }
    }

    #[must_use]
    pub fn method(self) -> &'static str {
        match self {
            Self::RealPm | Self::Complex => "algebraic",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn evidence_class(self) -> &'static str {
        match self {
            Self::Symbolic => "identity",
            Self::RealPm | Self::Complex | Self::Modular | Self::Numeric => "residual",
        }
    }

    #[must_use]
    pub fn holes(self) -> &'static [&'static str] {
        match self {
            Self::Modular => &["modulus"],
            Self::Numeric => &["tolerance"],
            Self::RealPm | Self::Complex | Self::Symbolic => &[],
        }
    }

    #[must_use]
    pub fn beginner_default(self) -> bool {
        matches!(self, Self::RealPm)
    }

    #[must_use]
    pub fn pin_phrase(self) -> &'static str {
        match self {
            Self::RealPm => "over Real",
            Self::Complex => "over Complex",
            Self::Modular => "over modular",
            Self::Symbolic => "over symbolic",
            Self::Numeric => "over numeric",
        }
    }
}

/// At most one labeled `solve` world. Two worlds selected is unrepresentable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SolveIntent {
    #[default]
    Absent,
    Unlabeled,
    Over(SolveWorld),
}

impl SolveIntent {
    #[must_use]
    pub fn selected(self, world: SolveWorld) -> bool {
        matches!(self, Self::Over(w) if w == world)
    }

    #[must_use]
    pub fn menu(self) -> &'static [SolveWorld] {
        match self {
            Self::Absent => &[],
            Self::Unlabeled | Self::Over(_) => &SolveWorld::ALL,
        }
    }
}

/// Result of official scratch / L2 expansion.
#[derive(Debug)]
pub struct ScratchExpansion {
    pub expanded: String,
    pub outcome: ExpansionOutcome,
    pub notes: Vec<ScratchNote>,
    pub holes: Vec<HoleRecord>,
    pub solve: SolveIntent,
    pub diagnostics: Diagnostics,
}

impl ScratchExpansion {
    /// Display/JSON: true iff [`ExpansionOutcome::Rewritten`]. Never Canonical.
    #[must_use]
    pub fn rewritten(&self) -> bool {
        self.outcome.rewritten()
    }

    #[must_use]
    pub fn level(&self) -> ScratchLevel {
        self.outcome.level()
    }

    /// Source the parser should read: expanded text when the rewrite is clean.
    #[must_use]
    pub fn parse_source<'a>(&'a self, original: &'a str) -> &'a str {
        match self.outcome {
            ExpansionOutcome::Rewritten { .. } if !self.diagnostics.has_errors() => {
                self.expanded.as_str()
            }
            _ => original,
        }
    }
}

fn expansion(
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

fn solve_intent_payload(line: &str) -> Option<String> {
    let LineKind::Intent { verb, payload } = classify_line(line) else {
        return None;
    };
    if verb == IntentVerb::Solve {
        Some(payload)
    } else {
        None
    }
}

fn claims_unlabeled_unique_solve(source: &str) -> bool {
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

fn extract_solve_intent(source: &str) -> SolveIntent {
    let Some(payload) = source.lines().find_map(solve_intent_payload) else {
        return SolveIntent::Absent;
    };
    let (_, domain) = split_keyword_tail(&payload, "over");
    labeled_solve_menu(domain.as_deref())
}

fn labeled_solve_menu(domain: Option<&str>) -> SolveIntent {
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
    if world == SolveWorld::Modular
        && !source
            .lines()
            .any(|line| line.trim().starts_with("modulus"))
    {
        out.push_str("modulus = ?\n");
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

fn refuse_hidden_desugar(source: &str, diagnostics: &mut Diagnostics) {
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

fn refuse_if_contains_pair(
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

fn needs_scratch_wrap(source: &str) -> bool {
    first_content_line(source).is_some_and(|line| !is_item_header(line))
}

fn mix_scratch_and_declaration(source: &str) -> bool {
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
fn apply_scratch_kinds(
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

fn wrap_scratch(
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

#[allow(clippy::too_many_arguments)]
fn lower_intent(
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

fn rewrite_l2(
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

fn refuse(
    diagnostics: &mut Diagnostics,
    code: &'static str,
    message: impl Into<String>,
    span: Span,
    pedagogy: Pedagogy,
) {
    diagnostics.error(code, message, span);
    diagnostics.attach_pedagogy(pedagogy);
}

fn record_hole(holes: &mut Vec<HoleRecord>, name: String) {
    holes.push(HoleRecord::open(name));
}

fn constrain_last_hole(holes: &mut [HoleRecord], expr: String) {
    if let Some(hole) = holes.last_mut() {
        hole.constraints.push(expr);
    }
}

fn attach_find_continuation(holes: &mut [HoleRecord], verb: IntentVerb, payload: &str) {
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

fn finalize_holes(holes: &mut [HoleRecord]) {
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

fn emit_hole_comments(holes: &[HoleRecord], extra_comments: &mut Vec<String>) {
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

fn labeled_hole_candidates(constraints: &[String]) -> Vec<HoleCandidate> {
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
enum IntentVerb {
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
    fn parse_word(word: &str) -> Option<Self> {
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

#[derive(Clone, Debug)]
enum LineKind {
    Assign { name: String, rhs: String },
    Example { name: String, value: String },
    Expr(String),
    Intent { verb: IntentVerb, payload: String },
    Hole { name: String },
    Require { expr: String },
    Invalid,
}

fn classify_line(line: &str) -> LineKind {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("require ") {
        return LineKind::Require {
            expr: rest.trim().to_string(),
        };
    }
    if let Some((lhs, rhs)) = split_assignment(trimmed) {
        if rhs.trim() == "?" {
            let head = lhs.trim();
            let name = head.split('(').next().unwrap_or(head).trim().to_string();
            if is_ident(&name) {
                return LineKind::Hole { name };
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("example ") {
        if let Some((name, value)) = split_assignment(rest) {
            let name = name.trim();
            if is_ident(name) {
                return LineKind::Example {
                    name: name.to_string(),
                    value: value.trim().to_string(),
                };
            }
        }
        return LineKind::Invalid;
    }
    let first = first_word(trimmed);
    if let Some(verb) = IntentVerb::parse_word(first) {
        let payload = trimmed[first.len()..].trim_start();
        return LineKind::Intent {
            verb,
            payload: payload.to_string(),
        };
    }
    if let Some((lhs, rhs)) = split_assignment(trimmed) {
        let name = lhs.trim();
        if is_ident(name) {
            return LineKind::Assign {
                name: name.to_string(),
                rhs: rhs.trim().to_string(),
            };
        }
    }
    if looks_like_expression(trimmed) {
        return LineKind::Expr(trimmed.to_string());
    }
    LineKind::Invalid
}

fn looks_like_expression(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_ident(trimmed) || is_number(trimmed) {
        return true;
    }
    trimmed.chars().any(|ch| {
        matches!(
            ch,
            '+' | '-' | '*' | '/' | '^' | '(' | ')' | '[' | ']' | ',' | '.' | '<' | '>' | '=' | '!'
        )
    })
}

fn is_number(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_digit()
        || (first == '.' && chars.next().is_some_and(|c| c.is_ascii_digit()))
        || (first == '-'
            && text[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.'))
}

fn goal_target(payload: &str) -> String {
    let mut out = String::new();
    for ch in payload.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let slug = out.trim_matches('_').to_string();
    if slug.is_empty() {
        "result".to_string()
    } else {
        slug
    }
}

fn first_word(line: &str) -> &str {
    line.split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or(line)
}

fn split_keyword_tail<'a>(payload: &'a str, keyword: &str) -> (&'a str, Option<&'a str>) {
    let needle = format!(" {keyword} ");
    if let Some(index) = payload.rfind(&needle) {
        let expr = &payload[..index];
        let tail = payload[index + needle.len()..].trim();
        (expr, Some(tail))
    } else if let Some(rest) = payload.strip_suffix(&format!(" {keyword}")) {
        (rest, None)
    } else {
        (payload, None)
    }
}

fn split_equation(equation: &str) -> (&str, &str) {
    if let Some(index) = equation.find("==") {
        return (equation[..index].trim(), equation[index + 2..].trim());
    }
    if let Some((lhs, rhs)) = split_assignment(equation) {
        return (lhs.trim(), rhs.trim());
    }
    (equation, "0")
}

fn literal_class(value: &str) -> &'static str {
    let value = value.trim();
    if value == "true" || value == "false" {
        "bool"
    } else if is_number(value) {
        "number"
    } else if value.starts_with('"') || value.starts_with('\'') {
        "string"
    } else {
        "other"
    }
}

fn is_item_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('@')
        || is_emath_keyword_prefix(trimmed)
        || trimmed.starts_with("package ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("notation ")
        || trimmed.starts_with("extern ")
}

fn is_section_head(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(name) = trimmed.split(':').next() else {
        return false;
    };
    let name = name.trim();
    SECTION_HEADS.contains(&name) && trimmed[name.len()..].trim_start().starts_with(':')
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//")
}

fn is_content_line(s: &str) -> bool {
    !s.is_empty() && !is_comment(s)
}

fn first_content_line(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|line| is_content_line(line))
}

fn line_offsets(source: &str) -> Vec<(usize, &str)> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for line in source.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        out.push((offset, text));
        offset += line.len();
    }
    out
}

fn span_of_source(source: &str) -> Span {
    span_bytes(0, source.len())
}

fn span_bytes(start: usize, len: usize) -> Span {
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    let end = start.saturating_add(u32::try_from(len).unwrap_or(u32::MAX));
    Span::new(FileId(0), start, end)
}

enum TopPiece {
    Declaration(String),
    Other(String),
}

fn is_unindented(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t')
}

fn is_emath_keyword_prefix(s: &str) -> bool {
    s.starts_with("emath ") || s.starts_with("emath\t")
}

fn split_top_level(source: &str) -> Vec<TopPiece> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_decl = false;
    let mut started = false;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let at_margin =
            is_emath_keyword_prefix(line) || trimmed.starts_with("emath ") && is_unindented(line);
        let margin_header =
            is_unindented(line) && (is_emath_keyword_prefix(trimmed) || trimmed.starts_with('@'));
        if started && margin_header && at_margin {
            pieces.push(if current_decl {
                TopPiece::Declaration(std::mem::take(&mut current))
            } else {
                TopPiece::Other(std::mem::take(&mut current))
            });
            current_decl = true;
            started = true;
            current.push_str(line);
            continue;
        }
        if !started {
            current_decl = margin_header && is_emath_keyword_prefix(trimmed);
            started = true;
        }
        current.push_str(line);
    }
    if started || !current.is_empty() {
        pieces.push(if current_decl {
            TopPiece::Declaration(current)
        } else {
            TopPiece::Other(current)
        });
    }
    pieces
}

fn split_declaration_text(text: &str) -> Option<(String, Option<String>, String)> {
    let mut header = String::new();
    let mut rest_start = None;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start();
        if index == 0
            || (header.trim_end().ends_with(',')
                || !header.contains(':') && trimmed.starts_with('@'))
        {
            header.push_str(line);
            continue;
        }
        rest_start = Some(index);
        break;
    }
    let header_line = header.lines().last()?.trim();
    let (head, inline) = split_header_colon(header_line)?;
    let body = if rest_start.is_some() {
        text.split_inclusive('\n').skip(1).collect()
    } else {
        String::new()
    };
    let lines: Vec<&str> = header.lines().collect();
    let mut prefix = lines[..lines.len().saturating_sub(1)].join("\n");
    if lines.len() > 1 {
        prefix.push('\n');
    }
    prefix.push_str(&head);
    prefix.push(':');
    Some((prefix, inline, body))
}

fn split_header_colon(header: &str) -> Option<(String, Option<String>)> {
    let bytes = header.as_bytes();
    let mut i = 0;
    // skip `emath`
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i);
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i); // kind
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i); // name
    i = skip_ws(bytes, i);
    if i < bytes.len() && bytes[i] == b'<' {
        i = skip_balanced(bytes, i, b'<', b'>')?;
    }
    i = skip_ws(bytes, i);
    if i < bytes.len() && bytes[i] == b'(' {
        i = skip_balanced(bytes, i, b'(', b')')?;
    }
    i = skip_ws(bytes, i);
    if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'>' {
        i += 2;
        i = skip_ws(bytes, i);
        i = skip_word(bytes, i);
    }
    i = skip_ws(bytes, i);
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    let head = header[..i].trim_end().to_string();
    let inline = header[i + 1..].trim();
    let inline = if inline.is_empty() {
        None
    } else {
        Some(inline.to_string())
    };
    Some((head, inline))
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_word(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

fn skip_balanced(bytes: &[u8], mut i: usize, open: u8, close: u8) -> Option<usize> {
    if i >= bytes.len() || bytes[i] != open {
        return None;
    }
    let mut depth = 0i32;
    while i < bytes.len() {
        if bytes[i] == open {
            depth += 1;
        } else if bytes[i] == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

fn header_args(header: &str) -> Vec<(String, Option<String>)> {
    let bytes = header.as_bytes();
    let Some(start) = header.find('(') else {
        return Vec::new();
    };
    let Some(end) = skip_balanced(bytes, start, b'(', b')') else {
        return Vec::new();
    };
    let inner = header[start + 1..end - 1].trim();
    if inner.is_empty() {
        return Vec::new();
    }
    let mut args = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, ty) = match part.split_once(':') {
            Some((name, ty)) => (name.trim(), Some(ty.trim().to_string())),
            None => (part, None),
        };
        if !name.is_empty() {
            args.push((name.to_string(), ty));
        }
    }
    args
}

fn call_position_names(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index = skip_word(bytes, index);
            let name = &text[start..index];
            let next = skip_ws(bytes, index);
            if next < bytes.len()
                && bytes[next] == b'('
                && is_ident(name)
                && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_string());
            }
            continue;
        }
        index += 1;
    }
    names
}

fn declaration_name(header: &str) -> Option<&str> {
    let rest = header.trim().strip_prefix("emath")?.trim_start();
    let rest = rest.split_whitespace().nth(1)?;
    let name = rest
        .split('(')
        .next()?
        .split('<')
        .next()?
        .trim_end_matches(':');
    Some(name)
}

fn render_function(
    name: &str,
    header: Option<&str>,
    inputs: &[String],
    defs: &[(String, String)],
    examples: &[(String, String)],
    compile: Option<&(String, String)>,
    comments: &[String],
    goals: &[(String, String)],
) -> String {
    let header = header.map_or_else(|| format!("emath function {name}:"), ToString::to_string);
    render_from_header(&header, inputs, defs, examples, compile, comments, goals)
}

fn render_from_header(
    header: &str,
    inputs: &[String],
    defs: &[(String, String)],
    examples: &[(String, String)],
    compile: Option<&(String, String)>,
    comments: &[String],
    goals: &[(String, String)],
) -> String {
    let mut out = String::new();
    for comment in comments {
        out.push_str(comment);
        out.push('\n');
    }
    out.push_str(header);
    if !header.ends_with(':') {
        out.push(':');
    }
    out.push('\n');
    if !inputs.is_empty() {
        out.push_str("    inputs:\n");
        for name in inputs {
            out.push_str("        ");
            out.push_str(name);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("    definitions:\n");
    if defs.is_empty() {
        out.push_str("        result = 0\n");
    } else {
        for (name, rhs) in defs {
            out.push_str("        ");
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(rhs);
            out.push('\n');
        }
    }
    if let Some((lang, profile)) = compile {
        out.push('\n');
        out.push_str("    compile:\n");
        out.push_str("        target ");
        out.push_str(lang);
        out.push('\n');
        out.push_str("        profile ");
        out.push_str(profile);
        out.push('\n');
        out.push_str("        numeric strict-f64\n");
    }
    if !goals.is_empty() {
        out.push('\n');
        out.push_str("    goals:\n");
        for (kind, target) in goals {
            out.push_str("        ");
            out.push_str(kind);
            out.push_str(" <");
            out.push_str(target);
            out.push_str(">:\n");
            out.push_str("            produce rust.library\n");
        }
    }
    if !examples.is_empty() {
        out.push('\n');
        out.push_str("    tests:\n");
        for (name, value) in examples {
            out.push_str("        example <");
            out.push_str(name);
            out.push_str("_example>:\n");
            out.push_str("            given ");
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(value);
            out.push('\n');
        }
    }
    out
}

fn free_names(
    defs: &[(String, String)],
    examples: &[(String, String)],
    assigned: &[&str],
) -> Vec<String> {
    let mut free = Vec::new();
    let mut scan = |text: &str| {
        let mut used = Vec::new();
        let mut bound = Vec::new();
        collect_names(text, &mut used, &mut bound);
        for ident in used {
            if !assigned.contains(&ident.as_str())
                && !bound.iter().any(|b| b == &ident)
                && !is_builtin(&ident)
                && !free.iter().any(|f| f == &ident)
            {
                free.push(ident);
            }
        }
    };
    for (_, rhs) in defs {
        scan(rhs);
    }
    for (_, value) in examples {
        scan(value);
    }
    free
}

fn collect_names(text: &str, used: &mut Vec<String>, bound: &mut Vec<String>) {
    let idents = scan_idents(text);
    for name in binder_names(&idents) {
        if !bound.iter().any(|b| b == name) {
            bound.push(name.to_string());
        }
    }
    for ident in idents {
        if !used.iter().any(|u| u == ident) {
            used.push(ident.to_string());
        }
    }
}

fn first_free_ident(text: &str) -> Option<&str> {
    scan_idents(text)
        .into_iter()
        .find(|ident| !is_builtin(ident))
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            let prev = index.checked_sub(1).and_then(|pos| bytes.get(pos)).copied();
            let next = bytes.get(index + 1).copied();
            if matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) || next == Some(b'=') {
                index += 1;
                continue;
            }
            return Some((&line[..index], &line[index + 1..]));
        }
        index += 1;
    }
    None
}

fn is_ident(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn is_binder_head(name: &str) -> bool {
    matches!(name, "sum" | "product" | "integral" | "forall" | "exists")
}

fn binder_names<'a>(idents: &[&'a str]) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut index = 0;
    while index + 2 < idents.len() {
        if is_binder_head(idents[index]) && idents[index + 2] == "in" {
            names.push(idents[index + 1]);
            index += 3;
            continue;
        }
        index += 1;
    }
    names
}

fn scan_idents(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut idents = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let b = bytes[index];
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = index;
            index = skip_word(bytes, index);
            idents.push(&text[start..index]);
            continue;
        }
        if b.is_ascii_digit() {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_digit()
                    || bytes[index] == b'.'
                    || bytes[index] == b'e'
                    || bytes[index] == b'E'
                    || bytes[index] == b'+'
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    idents
}
