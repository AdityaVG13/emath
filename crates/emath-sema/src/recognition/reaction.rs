//! Structural admission for `emath reaction_network` declarations.
//!
//! Species closure, section shape, and element-balance checks are generic
//! schema application over a labeled multiset transformation. They do not
//! select a FeatureID or own chemistry kernels.

use crate::admit::expr_helpers::measured_digits_uncertainty;
use emath_core::tree::{
    Binder, BinderKind, BinaryOp, Declaration, Expr, ExprKind, ReactionArrow, Stmt, StmtKind,
    TypeKind, UnaryOp,
};
use emath_core::Diagnostics;
use std::collections::{BTreeMap, BTreeSet};

/// Named element catalogs are leftover. The constructor surface has no
/// element FeatureID; the spelling table is empty (no catalog, no parser).
/// The dormant `reaction_network` admission body below uses it only for
/// element-balance checks, which skip on an empty table.
fn element_symbols() -> &'static [&'static str] {
    &[]
}

/// Count atoms per element in one species spelling. `H2O` → {H:2, O:1};
/// `NaCl` → {Na:1, Cl:1}. Longest-symbol-first greedy match; an unknown
/// letter run yields `None` (caller reports E-CHEM-BALANCE rather than
/// guessing a molecular formula).
fn count_atoms(species: &str) -> Option<BTreeMap<&'static str, u64>> {
    let bytes = species.as_bytes();
    let mut atoms: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let rest = &species[index..];
        let symbol = element_symbols()
            .iter()
            .filter(|element| rest.starts_with(**element))
            .max_by_key(|element| element.len())?;
        index += symbol.len();
        // Optional count digits directly after the symbol.
        let digits_end = bytes[index..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map_or(bytes.len(), |offset| index + offset);
        let count: u64 = if digits_end > index {
            species[index..digits_end].parse().ok()?
        } else {
            1
        };
        index = digits_end;
        *atoms.entry(symbol).or_insert(0) += count.max(1);
    }
    Some(atoms)
}

/// Admission for `emath reaction_network Name:` (04 section 3.1, bead
/// emath-r3-reactions-section-92hq): `species:` closes the world (every
/// reaction-line species must be declared — E-CHEM-SPECIES), and element
/// balance is checked statically per reaction line (E-CHEM-BALANCE on
/// imbalance). Admission-only: reaction networks are recognized meaning,
/// not lowered into strict Phase 1 definitions.
/// One admitted equilibrium constant (04 §3.3, ds6x). The numeric value
/// and its uncertainty stay in the tree (admission is not evaluation);
/// only the honesty shape is checked here.
struct ConstantEntry {
    value: f64,
    spread: f64,
    source: emath_core::Span,
}

/// Extract central value and uncertainty from a Measured literal. The
/// tree carries the raw spellings (`value: "1.75(3)e-5"`,
/// `uncertainty_digits: "3"`); admission parses the central value and
/// derives the spread in the literal's own last-digit units (the same
/// convention as the uncertainty beads).
fn parse_measured_literal(value: &Expr) -> Option<(f64, f64)> {
    let ExprKind::Measured {
        value,
        uncertainty,
        uncertainty_digits,
        ..
    } = &value.kind
    else {
        return None;
    };
    let central: f64 = value.trim().parse().ok()?;
    if !uncertainty.is_empty() {
        let spread: f64 = uncertainty.trim().parse().ok()?;
        return Some((central, spread));
    }
    // CODATA parenthesized digits: the shared helper does the
    // exponent/frac last-digit math.
    let spread = measured_digits_uncertainty(value, uncertainty_digits)?;
    Some((central, spread))
}

/// Sum `k1 + k2*10^0 …` style combined uncertainty is overkill at
/// admission; the gate compares relative spread against the K/ratio gap
/// with a generous 2-sigma envelope. Kept as a helper so the mutant has
/// one seam to flip.
fn consistent_within_uncertainty(k: f64, k_spread: f64, ratio: f64, ratio_spread: f64) -> bool {
    let delta = (k - ratio).abs();
    delta <= 2.0 * (k_spread + ratio_spread) + 1e-12 * delta.max(1.0)
}

pub(crate) fn admit_reaction_network(decl: &Declaration, diagnostics: &mut Diagnostics) {
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut reactions: Vec<&emath_core::tree::Stmt> = Vec::new();
    // Equilibrium constants (`Ka: Measured<Real> in M = 1.75(3)e-5 M`):
    // name -> (value-with-uncertainty text, source). 04 §3.3 (ds6x).
    let mut constants: BTreeMap<String, ConstantEntry> = BTreeMap::new();
    // Kinetic rate constants (`kf = 2.0` entries in a `rate:` section):
    // name -> exact numeric value. 04 §3.3 (ds6x) honesty-triangle input.
    let mut rates: BTreeMap<String, f64> = BTreeMap::new();
    // Declared approximation assumptions (`assumptions: quasi_steady_state`
    // — 04 §3.5, i6ri). Declared, never ambient: a non-mass-action rate
    // law without one carries a warning receipt.
    let mut has_assumptions = false;
    // 04 §3.2 (emath-r3-stoich-tables-pqs6): declared extents of reaction
    // (`extents:` → `xi: Real in mol`) — the names the ICE equilibrium
    // identity may reference.
    let mut extents: BTreeSet<String> = BTreeSet::new();
    // ICE tables are collected during the walk and processed AFTER the
    // reaction lines: section order is free, and the derived coefficients
    // (ν) only exist once the `reactions:` lines have been read.
    let mut pending_ice_tables: Vec<&Stmt> = Vec::new();
    // Rate-law warning receipts are collected during the section walk and
    // emitted after it: the `assumptions:` section may follow the
    // `rate:` section in source order (declaration order is free).
    let mut pending_ratelaw_receipts: Vec<(String, emath_core::Span)> = Vec::new();
    for statement in &decl.body {
        match &statement.kind {
            StmtKind::Section(section) => match section.name.as_str() {
                "species" => {
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            // Bare names parse as `FieldDecl` with the
                            // Infer type marker (the same shape `inputs:`
                            // entries carry).
                            StmtKind::FieldDecl { name, .. } => {
                                declared.insert(name.clone());
                            }
                            StmtKind::Expr(expr) => {
                                if let ExprKind::Path { segments, generics: None } = &expr.kind {
                                    if let [name] = segments.as_slice() {
                                        declared.insert(name.clone());
                                        continue;
                                    }
                                }
                                diagnostics.error(
                                    "E-CHEM-SPECIES",
                                    "a `species:` entry must be a bare species name",
                                    nested.source,
                                );
                            }
                            _ => diagnostics.error(
                                "E-CHEM-SPECIES",
                                "`species:` entries are bare names, one per line",
                                nested.source,
                            ),
                        }
                    }
                }
                "reactions" => reactions.extend(section.suite.statements.iter()),
                "assumptions" => {
                    // 04 §3.5 (i6ri): declared approximations — bare
                    // names, one per line (`quasi_steady_state`). Their
                    // hashing into artifacts is the build tier; here the
                    // declaration is recognized and silences the
                    // W-CHEM-RATELAW receipt for this network.
                    has_assumptions = true;
                    for nested in &section.suite.statements {
                        let admitted = match &nested.kind {
                            StmtKind::FieldDecl { ty, .. } => {
                                matches!(&ty.kind, TypeKind::Path { segments, .. } if segments.first().map(String::as_str) == Some("Infer"))
                            }
                            StmtKind::Expr(expr) => matches!(
                                &expr.kind,
                                ExprKind::Path { segments, generics: None } if segments.len() == 1
                            ),
                            _ => false,
                        };
                        if !admitted {
                            diagnostics.error(
                                "E-KIND-027",
                                "an `assumptions:` entry must be a bare name \
                                 (`quasi_steady_state`)",
                                nested.source,
                            );
                        }
                    }
                }
                "rate" => {
                    // A rate entry is a bare numeric assignment
                    // (`kf = 2.0`, ds6x honesty-triangle input) or a
                    // named rate-law call (`v = michaelis_menten(...)`,
                    // 04 §3.5 i6ri). Full rate-law semantics are the
                    // follow-up bead.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::Assign { target, value } => {
                                match &value.kind {
                                    ExprKind::Float(text) => {
                                        let Ok(rate_value) = text.parse::<f64>() else {
                                            diagnostics.error(
                                                "E-KIND-027",
                                                "a `rate:` entry value must be a numeric \
                                                 literal (`kf = 2.0`)",
                                                nested.source,
                                            );
                                            continue;
                                        };
                                        if let Some(rate_name) = target.segments.last() {
                                            rates.insert(rate_name.clone(), rate_value);
                                        }
                                    }
                                    // Named rate-law form (04 §3.5, i6ri): the callee
                                    // names the form (`michaelis_menten` from
                                    // sci::chem::ratelaws; registry membership is the
                                    // import tier). Arguments are bare names or §3.4
                                    // context-scoped concentration brackets: `[S]`
                                    // (a single-element list) reads as
                                    // concentration-of-S when S is a declared species.
                                    ExprKind::Call { function, args } => {
                                        for arg in args {
                                            match &arg.kind {
                                                ExprKind::Path { .. } => {}
                                                ExprKind::List(items) if items.len() == 1 => {
                                                    let concentration = match &items[0].kind {
                                                        ExprKind::Path {
                                                            segments,
                                                            generics: None,
                                                        } if segments.len() == 1
                                                            && declared.contains(&segments[0]) =>
                                                        {
                                                            true
                                                        }
                                                        _ => false,
                                                    };
                                                    if !concentration {
                                                        diagnostics.error(
                                                            "E-NOTATION-AMBIG",
                                                            "a bracketed rate-law argument \
                                                             `[X]` reads as concentration-of-X \
                                                             only when X is a declared species; \
                                                             this spelling has no resolvable \
                                                             reading in the rate context",
                                                            arg.source,
                                                        );
                                                    }
                                                }
                                                _ => diagnostics.error(
                                                    "E-NOTATION-AMBIG",
                                                    "rate-law arguments are bare names or \
                                                     concentration brackets `[S]` of declared \
                                                     species; a list literal here is the \
                                                     ambiguous spelling",
                                                    arg.source,
                                                ),
                                            }
                                        }
                                        if !has_assumptions {
                                            let form = match &function.kind {
                                                ExprKind::Path { segments, .. } => segments
                                                    .last()
                                                    .cloned()
                                                    .unwrap_or_else(|| "rate law".to_string()),
                                                _ => "rate law".to_string(),
                                            };
                                            pending_ratelaw_receipts
                                                .push((form, nested.source));
                                        }
                                    }
                                    _ => diagnostics.error(
                                        "E-KIND-027",
                                        "a `rate:` entry must be `name = <numeric literal>` \
                                         or a named rate-law call",
                                        nested.source,
                                    ),
                                }
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "a `rate:` entry must be `name = <numeric literal>` or a \
                                 named rate-law call",
                                nested.source,
                            ),
                        }
                    }
                }
                "stoichiometry" => {
                    // 04 §3.2 (emath-r3-stoich-tables-pqs6): the
                    // stoichiometric matrix is DERIVED from the reaction
                    // lines. The only admitted right-hand side is exactly
                    // `stoich(reactions)`; the name binds the derived
                    // matrix for the build tier. A re-entered matrix is
                    // precisely the transcription error this section
                    // exists to make impossible.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::Assign { target, value } => {
                                let derived = matches!(
                                    &value.kind,
                                    ExprKind::Call { function, args }
                                        if matches!(
                                            &function.kind,
                                            ExprKind::Path { segments, generics: None }
                                                if segments.len() == 1 && segments[0] == "stoich"
                                        ) && args.len() == 1
                                            && matches!(
                                                &args[0].kind,
                                                ExprKind::Path { segments, generics: None }
                                                    if segments.len() == 1
                                                        && segments[0] == "reactions"
                                            )
                                );
                                if !derived {
                                    let name = target
                                        .segments
                                        .last()
                                        .cloned()
                                        .unwrap_or_else(|| "matrix".to_string());
                                    diagnostics.error(
                                        "E-CHEM-STOICH",
                                        format!(
                                            "stoichiometric matrix `{name}` must be derived: \
                                             the only admitted right-hand side is \
                                             `stoich(reactions)`; re-entering coefficients \
                                             defeats the anti-transcription check"
                                        ),
                                        nested.source,
                                    );
                                }
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "a `stoichiometry:` entry is `name = stoich(reactions)` — \
                                 coefficients are derived from the reaction lines, never \
                                 re-entered",
                                nested.source,
                            ),
                        }
                    }
                }
                "extents" => {
                    // 04 §3.2 (pqs6): typed extents of reaction
                    // (`xi: Real in mol`) — declared symbols the ICE
                    // equilibrium identity references. The bead's bare
                    // `extent xi:` prefix spelling collides with the
                    // two-word section-head grammar (`extent` + name +
                    // `:` reads as a section head, and the trailing type
                    // then trips the definition-row unit fence), so the
                    // section idiom is the admitted spelling.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::FieldDecl { name, .. } => {
                                extents.insert(name.clone());
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "an `extents:` entry is a typed extent of reaction \
                                 (`xi: Real in mol`)",
                                nested.source,
                            ),
                        }
                    }
                }
                "constraints" => {
                    // 04 §3.2 (pqs6): concentration claims over the
                    // species carrier. Admission checks the SHAPE
                    // (forall-over-species); the numeric check itself is
                    // the eval tier. Both binder statement forms are
                    // admitted: the expression form (`forall … : body`
                    // lowering to a binder expression) and the block
                    // `BinderStmt` form the statement parser produces
                    // when the binder body follows the colon.
                    for nested in &section.suite.statements {
                        let admitted = match &nested.kind {
                            StmtKind::Expr(expr) => is_forall_over_species(expr),
                            StmtKind::BinderStmt {
                                kind: BinderKind::ForAll,
                                binders,
                                guard: None,
                                suite,
                            } => {
                                binders.len() == 1
                                    && binder_domain_is_species(&binders[0])
                                    && !suite.statements.is_empty()
                            }
                            _ => false,
                        };
                        if !admitted {
                            diagnostics.error(
                                "E-KIND-027",
                                "a `constraints:` entry is a forall-over-species \
                                 concentration claim (`forall s in species: 0 M <= [s]`)",
                                nested.source,
                            );
                        }
                    }
                }
                "ice_table" => pending_ice_tables.push(statement),
                "conservation" => {
                    // Conservation laws carry their own beads (04 section
                    // 3.1 follow-ups); the reaction-line grammar and
                    // element balance are this bead's slice.
                }
                // 04 §4.1+§4.2 (emath-r3-compartments-e5zq): the
                // compartments/populations vocabulary names its design
                // forks instead of a generic unknown-section error.
                "compartments" => diagnostics.error(
                    "E-KIND-027",
                    "`compartments:` is outside the admitted reaction-network sections — \
                     the compartments design follow-up (emath-r3-compartments-e5zq) must \
                     first settle the C15 `@` collision (species identity `Drug@central` \
                     vs the attribute sigil: an explicit lexer rule or a different \
                     separator like `Drug.at(central)`) and the sink species typing \
                     (`∅` is the declared sink); compartment-qualified identity hashes \
                     into meaning",
                    section.head_source,
                ),
                "populations" => diagnostics.error(
                    "E-KIND-027",
                    "`populations:` is outside the admitted reaction-network sections — \
                     the populations design follow-up (emath-r3-compartments-e5zq) is \
                     WORLD-DEPENDENT: `continuous_ode` (mean-field, N: Real) and \
                     `gillespie_exact` (jump process, N: Nat) are two READINGS of the \
                     same equations, kept as a labeled portfolio pair with the stated \
                     approximation relation `continuous_ode ≈ gillespie_exact for N >> 1`; \
                     the stochastic world for SSA is the named prerequisite",
                    section.head_source,
                ),
                other => diagnostics.error(
                    "E-KIND-027",
                    format!("unknown `reaction_network` section `{other}` (expected `species:`, `reactions:`, `rate:`, `conservation:`, `stoichiometry:`, `extents:`, `ice_table <reaction>:`, or `constraints:`)"),
                    section.head_source,
                ),
            },
            // Equilibrium-constant line: `Ka: Measured<Real> in M = 1.75(3)e-5 M`
            // (ds6x). `Measured` is not a hard type on this tree (admission
            // probe: `E-TYPE-001 unknown type` outside uncertainty lanes),
            // so the honesty shape comes from the VALUE: a Measured
            // literal (± or parenthesized digits) is required — a bare
            // exact literal refuses (E-CHEM-KA-EXACT).
            StmtKind::FieldDecl {
                name,
                default: Some(value),
                ..
            } => {
                if let Some((central, spread)) = parse_measured_literal(value) {
                    constants.insert(name.clone(), ConstantEntry {
                        value: central,
                        spread,
                        source: statement.source,
                    });
                } else if matches!(value.kind, ExprKind::Measured { .. }) {
                    let ExprKind::Measured { value: text, uncertainty, uncertainty_digits, .. } = &value.kind else { unreachable!() };
                    diagnostics.error(
                        "E-CHEM-KA-EXACT",
                        format!(
                            "equilibrium constant `{name}` carries a Measured literal whose \
                             value/uncertainty text does not parse (value=`{text}` \
                             uncertainty=`{uncertainty}` digits=`{uncertainty_digits}`); fix \
                             the spelling"
                        ),
                        statement.source,
                    );
                } else {
                    diagnostics.error(
                        "E-CHEM-KA-EXACT",
                        format!(
                            "equilibrium constant `{name}` must be a Measured value with \
                             uncertainty (`1.75(3)e-5` or `1.75 ± 0.03e-5`); an exact literal \
                             is the dishonest spelling for a measured constant"
                        ),
                        statement.source,
                    );
                }
            }
            other => {
                diagnostics.error(
                    "E-KIND-027",
                    "a `reaction_network` body is `species:` and `reactions:` sections",
                    statement.source,
                );
            }
        }
    }
    // W-CHEM-RATELAW receipts (04 §3.5, i6ri): emitted after the walk so
    // a later `assumptions:` section still silences them.
    if !has_assumptions {
        for (form, source) in &pending_ratelaw_receipts {
            diagnostics.warning(
                "W-CHEM-RATELAW",
                format!(
                    "named rate-law form `{form}` is non-mass-action; declare the approximation \
                     with an `assumptions:` section (e.g. `quasi_steady_state`) so the receipt \
                     is explicit, not ambient"
                ),
                *source,
            );
        }
    }
    if declared.is_empty() {
        diagnostics.error(
            "E-CHEM-SPECIES",
            "`species:` closes the world: a `reaction_network` must declare at least one species",
            decl.head_source,
        );
        return;
    }
    // Honesty triangle (04 §3.3, ds6x): a network declaring BOTH a
    // reversible kinetic pair (`<->`) AND an equilibrium over the same
    // species pair (`<=>`) must carry a constant consistent with
    // K == kf/kr within combined uncertainty. Admission checks the SHAPE:
    // both arrows present + a constant name present. Numeric kf/kr live
    // in `rate:` lines (follow-up bead), so with rates absent the gate
    // records the pair and stays silent (no false positives on plain
    // networks).
    let has_reversible = reactions.iter().any(|reaction| {
        matches!(
            &reaction.kind,
            StmtKind::Reaction { arrow: ReactionArrow::Reversible, .. }
        )
    });
    let has_equilibrium = reactions.iter().any(|reaction| {
        matches!(
            &reaction.kind,
            StmtKind::Reaction { arrow: ReactionArrow::Equilibrium, .. }
        )
    });
    if has_reversible && has_equilibrium {
        let equilibrium = reactions.iter().find(|reaction| {
            matches!(
                &reaction.kind,
                StmtKind::Reaction { arrow: ReactionArrow::Equilibrium, .. }
            )
        });
        if constants.is_empty() {
            if let Some(reaction) = equilibrium {
                diagnostics.error(
                    "E-CHEM-THERMO",
                    "this network declares both a reversible kinetic pair (`<->`) and an \
                     equilibrium (`<=>`) over the same chemistry; thermodynamic consistency \
                     requires the equilibrium constant (K == kf/kr within combined uncertainty) \
                     to be declared — add a `K: Measured<Real> = …` line",
                    reaction.source,
                );
            }
        } else if let (Some(k_entry), Some(kf), Some(kr)) = (
            // `K` by name; a sole constant declared under another name
            // (`Ka`) is read as the equilibrium constant.
            constants
                .get("K")
                .or_else(|| (constants.len() == 1).then(|| constants.values().next())?),
            rates.get("kf"),
            rates.get("kr"),
        ) {
            let ratio = kf / kr;
            // Rates are declared exact; the uncertainty envelope comes
            // from the measured K alone.
            if !consistent_within_uncertainty(k_entry.value, k_entry.spread, ratio, 0.0) {
                if let Some(reaction) = equilibrium {
                    diagnostics.error(
                        "E-CHEM-THERMO",
                        format!(
                            "thermodynamic consistency violated: declared equilibrium constant \
                             K = {} ± {} is inconsistent with kf/kr = {} (K == kf/kr must hold \
                             within combined uncertainty)",
                            k_entry.value, k_entry.spread, ratio
                        ),
                        reaction.source,
                    );
                }
            }
        }
    }
    // 04 §3.2 (pqs6): the derived stoichiometric vector ν per reaction
    // name — lhs terms negative, rhs terms positive. This is the
    // anti-transcription source of truth the ICE tables check against.
    let mut nu_by_reaction: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for reaction in reactions {
        let StmtKind::Reaction {
            name,
            lhs,
            arrow: _,
            rhs,
        } = &reaction.kind
        else {
            diagnostics.error(
                "E-CHEM-SPECIES",
                "only reaction lines are admitted inside `reactions:`",
                reaction.source,
            );
            continue;
        };
        let mut nu: BTreeMap<String, i64> = BTreeMap::new();
        for (sign, side) in [(-1i64, lhs), (1, rhs)] {
            for term in side {
                *nu.entry(term.species.clone()).or_insert(0) +=
                    sign * term.coefficient as i64;
            }
        }
        nu_by_reaction.insert(name.clone(), nu);
        for term in lhs.iter().chain(rhs.iter()) {
            if !declared.contains(&term.species) {
                diagnostics.error(
                    "E-CHEM-SPECIES",
                    format!(
                        "species `{}` is not declared in `species:` (the world is closed; no \
                         implicit species)",
                        term.species
                    ),
                    reaction.source,
                );
            }
        }
        // 04 §4.1 (emath-r3-compartments-e5zq): a sink endpoint is the
        // declared `∅` — the parser admits it as an EMPTY side. A
        // structurally empty side that did not come from a declared
        // sink cannot exist past the parser fence; belt-and-braces
        // refusal here keeps the contract machine-checkable.
        if lhs.is_empty() && rhs.is_empty() {
            diagnostics.error(
                "E-BIO-SINK",
                format!(
                    "reaction `{name}` has no terms on either side; a degradation/elimination \
                     endpoint must be the declared sink `∅` (04 §4.1), never a silently empty \
                     side"
                ),
                reaction.source,
            );
            continue;
        }
        // Static element balance: sum per element over both sides.
        let mut balance: BTreeMap<String, i64> = BTreeMap::new();
        let mut countable = true;
        for (sign, side) in [(-1i64, lhs), (1, rhs)] {
            for term in side {
                // A species that is not an element formula (`A`, `B` in
                // generic networks) is an abstract label: balance cannot
                // be checked statically, so the reaction is skipped
                // rather than refused (04 §3.3, ds6x — kinetic pairs and
                // equilibria over abstract species are first-class).
                let Some(atoms) = count_atoms(&term.species) else {
                    countable = false;
                    continue;
                };
                for (element, count) in atoms {
                    *balance.entry(element.to_string()).or_insert(0) += sign * (count * term.coefficient) as i64;
                }
            }
        }
        if countable {
            let imbalanced: Vec<(String, i64)> = balance
                .iter()
                .filter(|(_, delta)| **delta != 0)
                .map(|(element, delta)| (element.clone(), *delta))
                .collect();
            if !imbalanced.is_empty() {
                let detail = imbalanced
                    .iter()
                    .map(|(element, delta)| format!("{element}{delta:+}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics.error(
                    "E-CHEM-BALANCE",
                    format!(
                        "reaction `{name}` is not element-balanced ({detail}); stoichiometric \
                         coefficients must conserve every element"
                    ),
                    reaction.source,
                );
            }
        }
    }
    // 04 §3.2 (emath-r3-stoich-tables-pqs6): ICE tables are checked
    // against the DERIVED coefficients — the change row is never taken
    // on faith. Section order is free, so the tables were collected
    // during the walk and are processed here, after ν exists.
    for table in pending_ice_tables {
        let StmtKind::Section(section) = &table.kind else {
            unreachable!("pending_ice_tables only collects sections");
        };
        let Some(reaction_name) = section.generic.as_deref() else {
            diagnostics.error(
                "E-CHEM-STOICH",
                "an `ice_table:` must name its reaction (`ice_table r1:`); the table is \
                 checked against that reaction's derived coefficients",
                section.head_source,
            );
            continue;
        };
        let Some(nu) = nu_by_reaction.get(reaction_name) else {
            diagnostics.error(
                "E-CHEM-STOICH",
                format!(
                    "ice_table {reaction_name}: no reaction named `{reaction_name}` is declared \
                     in `reactions:` — an ICE table is checked against a declared reaction, \
                     never a silent guess"
                ),
                section.head_source,
            );
            continue;
        };
        let mut initials: BTreeMap<String, f64> = BTreeMap::new();
        let mut changes: BTreeMap<String, i64> = BTreeMap::new();
        for nested in &section.suite.statements {
            match &nested.kind {
                StmtKind::Section(row) if row.name == "initial" || row.name == "change" => {
                    for entry in &row.suite.statements {
                        let StmtKind::Assign { target, value } = &entry.kind else {
                            diagnostics.error(
                                "E-KIND-027",
                                format!(
                                    "an `ice_table` `{}` row entry is `species = value` \
                                     (`A = 1.0`)",
                                    row.name
                                ),
                                entry.source,
                            );
                            continue;
                        };
                        let Some(species) = target.segments.last() else {
                            continue;
                        };
                        let Some(value) = ice_cell_value(value) else {
                            diagnostics.error(
                                "E-CHEM-STOICH",
                                format!(
                                    "the `ice_table` `{}` value for `{species}` must be a \
                                     numeric literal (`1.0`, `-1`)",
                                    row.name
                                ),
                                entry.source,
                            );
                            continue;
                        };
                        if row.name == "initial" {
                            initials.insert(species.clone(), value);
                        } else if value.fract() == 0.0 {
                            changes.insert(species.clone(), value as i64);
                        } else {
                            diagnostics.error(
                                "E-CHEM-STOICH",
                                format!(
                                    "the ICE change coefficient for `{species}` must be an \
                                     integer (stoichiometric coefficients are integers)"
                                ),
                                entry.source,
                            );
                        }
                    }
                }
                // The equilibrium row is the derived identity; any other
                // formula is a re-entered transcription error.
                StmtKind::Assign { target, value }
                    if target.segments.len() == 1 && target.segments[0] == "equilibrium" =>
                {
                    if !is_equilibrium_identity(value, &extents) {
                        diagnostics.error(
                            "E-CHEM-STOICH",
                            "the equilibrium row is the derived identity \
                             `equilibrium = initial + xi * change` (with `xi` declared in \
                             `extents:`); a re-entered formula defeats the ICE check",
                            nested.source,
                        );
                    }
                }
                _ => diagnostics.error(
                    "E-KIND-027",
                    "an `ice_table` body is `initial:` and `change:` rows plus the optional \
                     `equilibrium = initial + xi * change` identity",
                    nested.source,
                ),
            }
        }
        // Fidelity + coverage: ICE rows cover exactly the reaction's
        // species; every change entry equals the derived ν.
        for species in initials.keys().chain(changes.keys()) {
            if !nu.contains_key(species) {
                diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: `{species}` does not participate in the \
                         reaction; ICE rows cover exactly the reaction's species"
                    ),
                    section.head_source,
                );
            }
        }
        for (species, coefficient) in nu {
            if !initials.contains_key(species.as_str()) {
                diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: species `{species}` participates in the \
                         reaction but has no `initial:` entry"
                    ),
                    section.head_source,
                );
            }
            match changes.get(species.as_str()) {
                None => diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: species `{species}` participates in the \
                         reaction but has no `change:` entry"
                    ),
                    section.head_source,
                ),
                Some(delta) if *delta != *coefficient => diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: the change coefficient for `{species}` is \
                         {delta} but the reaction derives {coefficient} — coefficients are \
                         derived from the reaction line, never re-entered"
                    ),
                    section.head_source,
                ),
                _ => {}
            }
        }
    }
}

/// A forall-over-species binder expression: one binder, domain `species`,
/// no guard — the admitted `constraints:` entry shape (pqs6).
fn is_forall_over_species(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CallableBinder { callee, domain, .. } => {
            matches!(
                &callee.kind,
                ExprKind::Path { segments, .. }
                    if segments.last().map(String::as_str) == Some("forall")
            ) && matches!(
                &domain.kind,
                ExprKind::Path {
                    segments,
                    generics: None
                } if segments.len() == 1 && segments[0] == "species"
            )
        }
        _ => false,
    }
}

/// The binder domain is the species carrier (`in species`).
fn binder_domain_is_species(binder: &Binder) -> bool {
    binder.domain.as_ref().is_some_and(|domain| {
        matches!(
            &domain.kind,
            ExprKind::Path { segments, generics: None }
                if segments.len() == 1 && segments[0] == "species"
        )
    })
}

/// Fold an ICE-table cell (`1.0`, `-2`, `+1`) to its numeric value:
/// Int/Float literals under an optional unary sign. Nothing else is an
/// ICE concentration or coefficient.
fn ice_cell_value(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(text) => text.parse::<f64>().ok(),
        ExprKind::Float(text) => text.parse::<f64>().ok(),
        ExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => Some(-ice_cell_value(value)?),
        ExprKind::Unary {
            op: UnaryOp::Pos,
            value,
        } => ice_cell_value(value),
        _ => None,
    }
}

/// The equilibrium row is the DERIVED identity `initial + xi * change` —
/// exactly `Add(Path("initial"), Mul(Path(extent), Path("change")))` with
/// `extent` declared in `extents:`. A re-entered formula is the classic
/// transcription error the bead refuses (pqs6).
fn is_equilibrium_identity(expr: &Expr, extents: &BTreeSet<String>) -> bool {
    let ExprKind::Binary {
        op: BinaryOp::Add,
        left,
        right,
    } = &expr.kind
    else {
        return false;
    };
    let ExprKind::Path { segments, generics: None } = &left.kind else {
        return false;
    };
    if segments.len() != 1 || segments[0] != "initial" {
        return false;
    }
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left: factor,
        right: rows,
    } = &right.kind
    else {
        return false;
    };
    let ExprKind::Path { segments: extent, generics: None } = &factor.kind else {
        return false;
    };
    if extent.len() != 1 || !extents.contains(&extent[0]) {
        return false;
    }
    matches!(
        &rows.kind,
        ExprKind::Path { segments, generics: None }
            if segments.len() == 1 && segments[0] == "change"
    )
}
