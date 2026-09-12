//! Restricted Feature Capsule source and class validation.
//!
//! The decoder intentionally accepts a small line-oriented data language. It is
//! the mounted schema behind the generic `emath feature` shell; class rules are
//! table data and never feature-name branches.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use crate::capsule_record::{
    CapsuleEdge, CapsuleProjection, CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule,
    FeatureClass, Maturity, ProjectionDisposition,
};
use emath_core::{CanonicalField, FeatureId, SemanticHash};
use emath_term::{Signature as TermSignature, SymbolId as TermSymbol, Term, TermError};

const COMMON_REQUIRED: [&str; 15] = [
    "surface",
    "semantics",
    "exactness",
    "effects",
    "worlds",
    "providers",
    "artifacts",
    "reference",
    "conformance",
    "migration",
    "authority_target",
    "presentation",
    "agent",
    "source",
    "summary",
];

/// The known Meaning Spine edge vocabulary is completed in the next slice. The
/// capsule validator refuses unknown synonyms now so authored data cannot grow
/// an untyped parallel graph.
pub const CAPSULE_EDGE_KINDS: [&str; 12] = [
    "depends_on",
    "implements",
    "defines",
    "uses",
    "requires_world",
    "provided_by",
    "emits",
    "documents",
    "conforms_to",
    "migrates_from",
    "replaces",
    "projects_to",
];

#[derive(Clone, Copy, Debug)]
pub struct ClassRule {
    pub class: FeatureClass,
    pub required: &'static [&'static str],
    pub reference_modes: &'static [&'static str],
}

const REFERENCE_NONE: &[&str] = &["none", "generated", "authored"];
const REFERENCE_EXEC: &[&str] = &["authored", "generated", "provider"];
const REFERENCE_PROVIDER: &[&str] = &["provider", "authored"];

/// Closed twenty-row class table. Each family adds only its genuinely
/// class-specific obligation; universal obligations remain one shared rule.
pub const CLASS_RULES: [ClassRule; 20] = [
    ClassRule {
        class: FeatureClass::Constitution,
        required: &["semantics", "authority_target"],
        reference_modes: REFERENCE_NONE,
    },
    ClassRule {
        class: FeatureClass::Syntax,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Kind,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Section,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Surface,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Symbol,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Type,
        required: &["semantics", "exactness"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Binder,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Capability,
        required: &["semantics", "exactness"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Theory,
        required: &["semantics", "conformance"],
        reference_modes: REFERENCE_NONE,
    },
    ClassRule {
        class: FeatureClass::Instance,
        required: &["semantics", "worlds"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Goal,
        required: &["semantics", "artifacts"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Method,
        required: &["semantics", "conformance"],
        reference_modes: REFERENCE_PROVIDER,
    },
    ClassRule {
        class: FeatureClass::World,
        required: &["semantics", "effects"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Provider,
        required: &["providers", "conformance"],
        reference_modes: REFERENCE_PROVIDER,
    },
    ClassRule {
        class: FeatureClass::Effect,
        required: &["effects", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Artifact,
        required: &["artifacts", "semantics"],
        reference_modes: REFERENCE_EXEC,
    },
    ClassRule {
        class: FeatureClass::Diagnostic,
        required: &["surface", "semantics"],
        reference_modes: REFERENCE_NONE,
    },
    ClassRule {
        class: FeatureClass::Migration,
        required: &["migration", "authority_target"],
        reference_modes: REFERENCE_NONE,
    },
    ClassRule {
        class: FeatureClass::FieldPack,
        required: &["artifacts", "conformance"],
        reference_modes: REFERENCE_EXEC,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapsuleIssue {
    pub code: &'static str,
    pub detail: String,
    pub line: usize,
}

#[derive(Default)]
struct RawCapsule {
    scalars: BTreeMap<String, (String, usize)>,
    edges: Vec<(String, String, usize)>,
    projections: Vec<(String, String, usize)>,
}

/// Parse and validate one restricted Feature Capsule document.
///
/// Grammar: `key: value` lines plus repeatable `edge:` and `projection:` rows.
/// Values are UTF-8 text; `n/a(rule | reason)` and `hole(gate | reason)` are
/// typed. The canonical hash is checked against the meaning-bearing fields.
#[must_use]
pub fn parse_feature_capsule(text: &str) -> (Option<FeatureCapsule>, Vec<CapsuleIssue>) {
    let mut issues = Vec::new();
    let mut raw = RawCapsule::default();
    let lines: Vec<(usize, &str)> = text.lines().enumerate().collect();
    let mut index = 0;
    while index < lines.len() {
        let (line_index, source) = lines[index];
        let line_number = line_index + 1;
        let indent = source.len() - source.trim_start().len();
        let line = source.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("emath feature ") {
            index += 1;
            continue;
        }
        if let Some(key) = empty_field_key(line) {
            if key == "reference_body" {
                let (body, consumed) = collect_indented_block(&lines, index, indent);
                insert_scalar(
                    &mut raw,
                    key,
                    body,
                    line_number,
                    &mut issues,
                );
                index += consumed;
                continue;
            }
        }
        let line = line.strip_suffix(':').unwrap_or(line);
        let Some((key, value)) = line.split_once(':') else {
            issues.push(issue(
                "E-CAPSULE-001",
                "expected `field: value`",
                line_number,
            ));
            index += 1;
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if forbidden_name(key) {
            issues.push(issue(
                "E-CAPSULE-002",
                format!("forbidden revision field `{key}`"),
                line_number,
            ));
            index += 1;
            continue;
        }
        match key {
            "edge" => raw
                .edges
                .push(parse_pair(value, line_number, "edge", &mut issues)),
            "projection" => {
                raw.projections
                    .push(parse_pair(value, line_number, "projection", &mut issues))
            }
            _ if raw
                .scalars
                .insert(key.to_string(), (value.to_string(), line_number))
                .is_some() =>
            {
                issues.push(issue(
                    "E-CAPSULE-003",
                    format!("duplicate field `{key}`"),
                    line_number,
                ));
            }
            _ => {}
        }
        index += 1;
    }

    if raw.scalars.contains_key("reference_body") && !raw.scalars.contains_key("reference")
    {
        raw.scalars
            .insert("reference".to_string(), ("authored".to_string(), 0));
    }

    let required = |name: &str, issues: &mut Vec<CapsuleIssue>| -> Option<(String, usize)> {
        raw.scalars.get(name).cloned().or_else(|| {
            issues.push(issue(
                "E-CAPSULE-004",
                format!("missing required field `{name}`"),
                0,
            ));
            None
        })
    };
    let Some((schema, schema_line)) = required("schema", &mut issues) else {
        return (None, issues);
    };
    if schema != FEATURE_CAPSULE_SCHEMA {
        issues.push(issue(
            "E-CAPSULE-005",
            format!("schema must be `{FEATURE_CAPSULE_SCHEMA}`"),
            schema_line,
        ));
    }
    let feature_id = required("feature_id", &mut issues).and_then(|(value, line)| {
        FeatureId::from_str(&value)
            .map_err(|error| issues.push(issue("E-CAPSULE-006", error.to_string(), line)))
            .ok()
    });
    let class = required("class", &mut issues).and_then(|(value, line)| {
        FeatureClass::from_str(&value)
            .map_err(|error| issues.push(issue("E-CAPSULE-007", error.to_string(), line)))
            .ok()
    });
    let maturity = required("maturity", &mut issues).and_then(|(value, line)| {
        Maturity::from_str(&value)
            .map_err(|error| issues.push(issue("E-CAPSULE-008", error.to_string(), line)))
            .ok()
    });
    let semantic_hash = match raw.scalars.get("semantic_hash") {
        Some((value, line)) => SemanticHash::from_str(value)
            .map_err(|error| issues.push(issue("E-CAPSULE-009", error.to_string(), *line)))
            .ok(),
        None => match capsule_semantic_hash(text) {
            Ok(hash) => Some(hash),
            Err(error) => {
                issues.push(error);
                None
            }
        },
    };
    let summary = required("summary", &mut issues)
        .map(|pair| pair.0)
        .unwrap_or_default();
    let source = required("source", &mut issues)
        .map(|pair| pair.0)
        .unwrap_or_default();

    let mut slots = BTreeMap::new();
    for name in COMMON_REQUIRED {
        if matches!(name, "summary" | "source") {
            continue;
        }
        let Some((value, line)) = required(name, &mut issues) else {
            continue;
        };
        match parse_capsule_slot(&value) {
            Ok(slot) => {
                slots.insert(name.to_string(), slot);
            }
            Err(detail) => issues.push(issue("E-CAPSULE-010", format!("{name}: {detail}"), line)),
        }
    }

    if let Some((value, line)) = raw.scalars.remove("params") {
        if raw
            .scalars
            .insert("reference_params".to_string(), (value, line))
            .is_some()
        {
            issues.push(issue(
                "E-CAPSULE-003",
                "duplicate field `reference_params`",
                line,
            ));
        }
    }
    let has_body = raw.scalars.contains_key("reference_body");
    let has_params = raw.scalars.contains_key("reference_params");
    let has_signature = raw.scalars.contains_key("reference_signature");
    if has_body {
        if !has_params {
            issues.push(issue(
                "E-CAPSULE-023",
                "executable reference requires `reference_params` (argument order)",
                0,
            ));
        } else {
            for name in ["reference_body", "reference_params"] {
                let (value, _) = raw.scalars[name].clone();
                slots.insert(name.to_string(), CapsuleSlot::Value(value));
            }
            if has_signature {
                let (value, _) = raw.scalars["reference_signature"].clone();
                slots.insert("reference_signature".to_string(), CapsuleSlot::Value(value));
            } else if let Ok(term) = Term::parse_body(&raw.scalars["reference_body"].0) {
                match term.inferred_signature_text() {
                    Ok(signature) => {
                        slots.insert(
                            "reference_signature".to_string(),
                            CapsuleSlot::Value(signature),
                        );
                    }
                    Err(error) => issues.push(issue(
                        "E-CAPSULE-024",
                        reference_term_error(&error),
                        0,
                    )),
                }
            }
        }
    } else if has_params || has_signature {
        issues.push(issue(
            "E-CAPSULE-023",
            "executable reference requires `reference_body` with `reference_params`",
            0,
        ));
    }

    for name in [
        "reference_defaults",
        "record_fields",
        "method_step",
        "method_check",
        "method_complete",
        "method_bindings",
    ] {
        if let Some((value, _)) = raw.scalars.get(name) {
            slots.insert(name.to_string(), CapsuleSlot::Value(value.clone()));
        }
    }

    let mut edges = Vec::new();
    for (kind, target, line) in raw.edges {
        if !CAPSULE_EDGE_KINDS.contains(&kind.as_str()) {
            issues.push(issue(
                "E-CAPSULE-011",
                format!("unknown edge kind `{kind}`"),
                line,
            ));
            continue;
        }
        match FeatureId::from_str(&target) {
            Ok(target) => edges.push(CapsuleEdge { kind, target }),
            Err(error) => issues.push(issue("E-CAPSULE-006", error.to_string(), line)),
        }
    }

    let mut projections = Vec::new();
    let mut projection_names = BTreeSet::new();
    for (name, disposition, line) in raw.projections {
        if !projection_names.insert(name.clone()) {
            issues.push(issue(
                "E-CAPSULE-012",
                format!("duplicate projection `{name}`"),
                line,
            ));
            continue;
        }
        match parse_projection_disposition(&disposition) {
            Ok(disposition) => projections.push(CapsuleProjection { name, disposition }),
            Err(detail) => issues.push(issue("E-CAPSULE-013", detail, line)),
        }
    }

    let (Some(feature_id), Some(class), Some(maturity), Some(semantic_hash)) =
        (feature_id, class, maturity, semantic_hash)
    else {
        return (None, issues);
    };
    if let Err(error) = feature_id.require_class(class.as_str()) {
        issues.push(issue("E-CAPSULE-014", error.to_string(), 0));
    }
    let capsule = FeatureCapsule {
        schema,
        feature_id,
        semantic_hash,
        class,
        maturity,
        summary,
        source,
        edges,
        slots,
        projections,
    };
    validate_capsule(&capsule, &mut issues);
    if let Ok(computed) = capsule_semantic_hash(text) {
        if computed != capsule.semantic_hash {
            issues.push(issue(
                "E-CAPSULE-022",
                format!(
                    "semantic_hash mismatch: declared {}, computed {computed}",
                    capsule.semantic_hash
                ),
                0,
            ));
        }
    }
    if issues.is_empty() {
        (Some(capsule), issues)
    } else {
        (None, issues)
    }
}

pub fn validate_capsule(capsule: &FeatureCapsule, issues: &mut Vec<CapsuleIssue>) {
    let rule = CLASS_RULES
        .iter()
        .find(|rule| rule.class == capsule.class)
        .expect("closed class table");
    for name in COMMON_REQUIRED
        .into_iter()
        .chain(rule.required.iter().copied())
    {
        if matches!(name, "summary" | "source") {
            continue;
        }
        if !capsule.slots.contains_key(name) {
            issues.push(issue(
                "E-CAPSULE-015",
                format!("class `{}` requires `{name}`", capsule.class),
                0,
            ));
        }
    }
    for edge in &capsule.edges {
        if !CAPSULE_EDGE_KINDS.contains(&edge.kind.as_str()) {
            issues.push(issue(
                "E-CAPSULE-011",
                format!("unknown edge kind `{}`", edge.kind),
                0,
            ));
        }
    }
    let mut projection_names = BTreeSet::new();
    for projection in &capsule.projections {
        if !projection_names.insert(projection.name.as_str()) {
            issues.push(issue(
                "E-CAPSULE-012",
                format!("duplicate projection `{}`", projection.name),
                0,
            ));
        }
    }
    if capsule.maturity == Maturity::Cataloged {
        if capsule.projections.iter().any(|projection| {
            matches!(
                projection.disposition,
                ProjectionDisposition::Provided
                    | ProjectionDisposition::Generated
                    | ProjectionDisposition::Provider(_)
            )
        }) {
            issues.push(issue(
                "E-CAPSULE-016",
                "cataloged capsules cannot represent live projections",
                0,
            ));
        }
    }
    if matches!(capsule.maturity, Maturity::Accepted | Maturity::Stable)
        && capsule.has_blocking_hole()
    {
        issues.push(issue(
            "E-CAPSULE-017",
            "blocking Spec Hole prevents accepted/stable publication",
            0,
        ));
    }
    if capsule.slots.contains_key("reference_body") {
        validate_executable_reference(capsule, issues);
    }
    let Some(CapsuleSlot::Value(reference)) = capsule.slots.get("reference") else {
        return;
    };
    if !rule.reference_modes.contains(&reference.as_str()) {
        issues.push(issue(
            "E-CAPSULE-018",
            format!(
                "reference mode `{reference}` is illegal for `{}`",
                capsule.class
            ),
            0,
        ));
    }
    if !capsule
        .slots
        .get("conformance")
        .is_some_and(|slot| !matches!(slot, CapsuleSlot::Value(value) if value.trim().is_empty()))
    {
        issues.push(issue("E-CAPSULE-019", "missing conformance declaration", 0));
    }
}

/// Verify a legal direct maturity transition.
pub fn validate_maturity_transition(from: Maturity, to: Maturity) -> Result<(), CapsuleIssue> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(issue(
            "E-CAPSULE-020",
            format!(
                "illegal maturity transition {} -> {}",
                from.as_str(),
                to.as_str()
            ),
            0,
        ))
    }
}

/// Validate an executable pure reference body: a canonical first-order term
/// with a declared operator signature and parameter list. The body is data
/// for the generic term compiler — never a Rust feature-name dispatch.
fn validate_executable_reference(capsule: &FeatureCapsule, issues: &mut Vec<CapsuleIssue>) {
    let slot_text = |name: &str| match capsule.slots.get(name) {
        Some(CapsuleSlot::Value(value)) => Some(value.as_str()),
        _ => None,
    };
    let Some(term_text) = slot_text("reference_body") else {
        issues.push(issue(
            "E-CAPSULE-023",
            "executable reference body must be concrete data",
            0,
        ));
        return;
    };
    let Some(params_text) = slot_text("reference_params") else {
        issues.push(issue(
            "E-CAPSULE-023",
            "executable reference requires `reference_params` (argument order)",
            0,
        ));
        return;
    };
    if slot_text("reference") != Some("authored") {
        issues.push(issue(
            "E-CAPSULE-026",
            "executable reference body requires `reference: authored`",
            0,
        ));
    }
    let mut params = Vec::new();
    for token in params_text.split(',') {
        let name = token.trim();
        if !is_param_name(name) || params.contains(&name.to_string()) {
            issues.push(issue(
                "E-CAPSULE-023",
                format!("invalid reference parameter `{name}`"),
                0,
            ));
            return;
        }
        params.push(name.to_string());
    }
    let Ok(term) = Term::parse_body(term_text) else {
        issues.push(issue(
            "E-CAPSULE-023",
            "reference body is not an emath-term (canonical apply(...) or surface expression)",
            0,
        ));
        return;
    };
    let signature = match slot_text("reference_signature") {
        Some(signature_text) => {
            let mut signature = TermSignature::default();
            for entry in signature_text.split(',') {
                let Some((symbol, arity)) = entry.trim().rsplit_once('=') else {
                    issues.push(issue(
                        "E-CAPSULE-023",
                        format!("reference signature entry `{entry}` requires `symbol=arity`"),
                        0,
                    ));
                    return;
                };
                let symbol = symbol.trim();
                let Ok(arity) = arity.trim().parse::<usize>() else {
                    issues.push(issue(
                        "E-CAPSULE-023",
                        format!("reference signature entry `{entry}` has a non-integer arity"),
                        0,
                    ));
                    return;
                };
                if symbol.is_empty()
                    || signature
                        .insert(TermSymbol(symbol.to_string()), arity)
                        .is_err()
                {
                    issues.push(issue(
                        "E-CAPSULE-024",
                        format!("reference symbol `{symbol}` declared with conflicting arities"),
                        0,
                    ));
                    return;
                }
            }
            signature
        }
        None => match term.inferred_signature() {
            Ok(signature) => signature,
            Err(error) => {
                issues.push(issue("E-CAPSULE-024", reference_term_error(&error), 0));
                return;
            }
        },
    };
    if let Err(error) = signature.validate(&term) {
        issues.push(issue("E-CAPSULE-024", reference_term_error(&error), 0));
        return;
    }
    let mut free = BTreeSet::new();
    collect_variables(&term, &mut free);
    if let Some(name) = free.into_iter().find(|name| !params.contains(name)) {
        issues.push(issue(
            "E-CAPSULE-025",
            format!("reference term uses variable `{name}` outside `reference_params`"),
            0,
        ));
    }
}

/// Render an emath-term structural error as a stable capsule detail.
fn reference_term_error(error: &TermError) -> String {
    match error {
        TermError::UnknownSymbol(symbol) => format!(
            "reference term uses operator `{}` outside `reference_signature`",
            symbol.0
        ),
        TermError::ArityMismatch {
            symbol,
            expected,
            actual,
        } => format!(
            "reference operator `{}` applied to {actual} argument(s), \
             signature declares {expected}",
            symbol.0
        ),
        TermError::ConflictingArity {
            symbol,
            first,
            second,
        } => format!(
            "reference symbol `{}` declared with conflicting arities {first} and {second}",
            symbol.0
        ),
    }
}

/// A parameter name is a non-empty identifier: alphanumeric or `_`, never
/// starting with a digit.
fn is_param_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
        && name.chars().next().is_some_and(|ch| !ch.is_ascii_digit())
}

/// Collect the free variables of a term (constants are nullary symbols).
fn collect_variables(term: &Term, out: &mut BTreeSet<String>) {
    match term {
        Term::Variable(variable) => {
            out.insert(variable.0.clone());
        }
        Term::Constant(_) => {}
        Term::Apply {
            operator,
            arguments,
        } => {
            let binding = match (operator.0.as_str(), arguments.as_slice()) {
                ("let", [Term::Variable(name), init, body]) => {
                    Some((vec![&name.0], vec![init], body, None))
                }
                ("collect", [Term::Variable(index), count, body]) => {
                    Some((vec![&index.0], vec![count], body, None))
                }
                (
                    "iterate",
                    [
                        Term::Variable(index),
                        Term::Variable(state),
                        count,
                        init,
                        body,
                    ],
                ) => Some((vec![&index.0, &state.0], vec![count, init], body, None)),
                (
                    "iterate_until",
                    [
                        Term::Variable(index),
                        Term::Variable(state),
                        count,
                        init,
                        stop,
                        body,
                    ],
                ) => Some((
                    vec![&index.0, &state.0],
                    vec![count, init],
                    body,
                    Some(stop),
                )),
                _ => None,
            };
            if let Some((names, initializers, body, stop)) = binding {
                for initializer in initializers {
                    collect_variables(initializer, out);
                }
                let mut body_free = BTreeSet::new();
                collect_variables(body, &mut body_free);
                if let Some(stop) = stop {
                    collect_variables(stop, &mut body_free);
                }
                for name in names {
                    body_free.remove(name);
                }
                out.extend(body_free);
            } else {
                for argument in arguments {
                    collect_variables(argument, out);
                }
            }
        }
    }
}

pub fn parse_capsule_slot(value: &str) -> Result<CapsuleSlot, String> {
    if let Some(body) = value
        .strip_prefix("n/a(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (rule, reason) = body
            .split_once('|')
            .ok_or_else(|| "typed N/A requires `rule | reason`".to_string())?;
        if rule.trim().is_empty() || reason.trim().is_empty() {
            return Err("typed N/A rule and reason must be non-empty".to_string());
        }
        return Ok(CapsuleSlot::NotApplicable {
            rule: rule.trim().to_string(),
            reason: reason.trim().to_string(),
        });
    }
    if let Some(body) = value
        .strip_prefix("hole(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (gate, reason) = body
            .split_once('|')
            .ok_or_else(|| "Spec Hole requires `gate | reason`".to_string())?;
        if gate.trim().is_empty() || reason.trim().is_empty() {
            return Err("Spec Hole gate and reason must be non-empty".to_string());
        }
        return Ok(CapsuleSlot::Hole {
            gate: gate.trim().to_string(),
            reason: reason.trim().to_string(),
        });
    }
    if value == "n/a" || value == "hole" {
        return Err("untyped N/A/Spec Hole is forbidden".to_string());
    }
    if value.trim().is_empty() {
        return Err("missing work cannot be encoded as an empty field".to_string());
    }
    Ok(CapsuleSlot::Value(value.to_string()))
}

pub fn parse_projection_disposition(value: &str) -> Result<ProjectionDisposition, String> {
    match value {
        "required" => Ok(ProjectionDisposition::Required),
        "provided" => Ok(ProjectionDisposition::Provided),
        "generated" => Ok(ProjectionDisposition::Generated),
        _ if value.starts_with("provider(") => value
            .strip_prefix("provider(")
            .and_then(|value| value.strip_suffix(')'))
            .filter(|value| !value.is_empty())
            .map(|value| ProjectionDisposition::Provider(value.to_string()))
            .ok_or_else(|| "provider projection requires an identifier".to_string()),
        _ => match parse_capsule_slot(value)? {
            CapsuleSlot::NotApplicable { rule, reason } => {
                Ok(ProjectionDisposition::NotApplicable { rule, reason })
            }
            CapsuleSlot::Hole { gate, reason } => Ok(ProjectionDisposition::Hole { gate, reason }),
            CapsuleSlot::Value(_) => Err(format!("unknown projection disposition `{value}`")),
        },
    }
}

fn parse_pair<'a>(
    value: &'a str,
    line: usize,
    label: &str,
    issues: &mut Vec<CapsuleIssue>,
) -> (String, String, usize) {
    match value.split_once(" -> ").or_else(|| value.split_once(" = ")) {
        Some((left, right)) if !left.trim().is_empty() && !right.trim().is_empty() => {
            (left.trim().to_string(), right.trim().to_string(), line)
        }
        _ => {
            issues.push(issue(
                "E-CAPSULE-001",
                format!("{label} requires `name -> value`"),
                line,
            ));
            (String::new(), String::new(), line)
        }
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn empty_field_key(line: &str) -> Option<&str> {
    let key = line.strip_suffix(':')?.trim();
    if key.is_empty() || key.contains(':') {
        None
    } else {
        Some(key)
    }
}

fn field_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if let Some(key) = empty_field_key(line) {
        return Some((key, ""));
    }
    let (name, value) = line.split_once(':')?;
    Some((name.trim(), value.trim()))
}

fn insert_scalar(
    raw: &mut RawCapsule,
    key: &str,
    value: String,
    line_number: usize,
    issues: &mut Vec<CapsuleIssue>,
) {
    if forbidden_name(key) {
        issues.push(issue(
            "E-CAPSULE-002",
            format!("forbidden revision field `{key}`"),
            line_number,
        ));
        return;
    }
    if raw
        .scalars
        .insert(key.to_string(), (value, line_number))
        .is_some()
    {
        issues.push(issue(
            "E-CAPSULE-003",
            format!("duplicate field `{key}`"),
            line_number,
        ));
    }
}

fn collect_indented_block(
    lines: &[(usize, &str)],
    start: usize,
    base_indent: usize,
) -> (String, usize) {
    let mut body = String::new();
    let mut consumed = 1;
    for (_, source) in lines.iter().skip(start + 1) {
        let indent = source.len() - source.trim_start().len();
        let trimmed = source.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            consumed += 1;
            continue;
        }
        if indent <= base_indent {
            break;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(trimmed);
        consumed += 1;
    }
    (body, consumed)
}

fn hash_field_value(name: &str, value: &str) -> String {
    if name == "reference_body" {
        if let Ok(term) = Term::parse_body(value) {
            return term.canonical();
        }
    }
    value.to_string()
}

fn capsule_document_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    while let Some(relative) = text[pos..].find("emath feature ") {
        let start = pos + relative;
        let search = start + "emath feature ".len();
        let end = text[search..]
            .find("\nemath feature ")
            .map(|index| search + index)
            .unwrap_or(text.len());
        ranges.push(start..end);
        pos = end;
    }
    ranges
}

fn rewrite_one_document(document: &str) -> Result<(String, SemanticHashRewrite), CapsuleIssue> {
    let computed = capsule_semantic_hash(document)?;
    let feature_id = field_value(document, "feature_id").unwrap_or_default();
    let previous = field_value(document, "semantic_hash").unwrap_or_default();
    let computed_text = computed.to_string();
    if previous == computed_text {
        return Ok((
            document.to_string(),
            SemanticHashRewrite {
                feature_id,
                previous,
                computed,
                wrote: false,
            },
        ));
    }
    Ok((
        replace_or_insert_hash(document, &computed_text),
        SemanticHashRewrite {
            feature_id,
            previous,
            computed,
            wrote: true,
        },
    ))
}

fn field_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some((name, value)) = field_line(line) else {
            continue;
        };
        if name == key {
            return Some(unquote(value).to_string());
        }
    }
    None
}

fn replace_or_insert_hash(document: &str, hash: &str) -> String {
    let mut output = String::with_capacity(document.len().saturating_add(hash.len()));
    let mut replaced = false;
    for line in document.lines() {
        let trimmed = line.trim();
        if !replaced && trimmed.starts_with("semantic_hash:") {
            let indent_len = line.len() - line.trim_start().len();
            let indent = &line[..indent_len];
            if trimmed.contains('"') {
                output.push_str(indent);
                output.push_str("semantic_hash: \"");
                output.push_str(hash);
                output.push_str("\"\n");
            } else {
                output.push_str(indent);
                output.push_str("semantic_hash: ");
                output.push_str(hash);
                output.push('\n');
            }
            replaced = true;
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    if !replaced {
        let mut inserted = String::with_capacity(output.len().saturating_add(hash.len()));
        let mut did = false;
        for line in output.lines() {
            inserted.push_str(line);
            inserted.push('\n');
            if !did && line.trim().starts_with("feature_id:") {
                let indent_len = line.len() - line.trim_start().len();
                let indent = &line[..indent_len];
                inserted.push_str(indent);
                inserted.push_str("semantic_hash: \"");
                inserted.push_str(hash);
                inserted.push_str("\"\n");
                did = true;
            }
        }
        output = inserted;
    }
    if !document.ends_with('\n') && output.ends_with('\n') {
        output.pop();
    }
    output
}

fn forbidden_name(name: &str) -> bool {
    matches!(
        name,
        "version"
            | "edition"
            | "major"
            | "minor"
            | "patch"
            | "semver"
            | "compatibility_range"
            | "schema_version"
    )
}

fn issue(code: &'static str, detail: impl Into<String>, line: usize) -> CapsuleIssue {
    CapsuleIssue {
        code,
        detail: detail.into(),
        line,
    }
}

/// Compute the semantic hash for a capsule source before filling its
/// `semantic_hash` row. Presentation, agent guidance, summary/source location,
/// and projections are intentionally excluded. `reference_body` contributes
/// its canonical term, not the pretty spelling an author typed.
pub fn capsule_semantic_hash(text: &str) -> Result<SemanticHash, CapsuleIssue> {
    let mut raw_fields = Vec::new();
    let mut field_counts = BTreeMap::new();
    let lines: Vec<(usize, &str)> = text.lines().enumerate().collect();
    let mut index = 0;
    while index < lines.len() {
        let (line_index, source) = lines[index];
        let indent = source.len() - source.trim_start().len();
        let line = source.trim();
        let Some((name, value)) = field_line(line) else {
            index += 1;
            continue;
        };
        if matches!(
            name,
            "semantic_hash" | "summary" | "source" | "presentation" | "agent" | "projection"
        ) || line.starts_with("emath feature ")
        {
            index += 1;
            continue;
        }
        if forbidden_name(name) {
            return Err(issue(
                "E-CAPSULE-002",
                format!("forbidden revision field `{name}`"),
                line_index + 1,
            ));
        }
        let (value, consumed) = if name == "reference_body" && value.is_empty() {
            collect_indented_block(&lines, index, indent)
        } else {
            (unquote(value).to_string(), 1)
        };
        let value = hash_field_value(name, &value);
        let name = if name == "params" {
            "reference_params"
        } else {
            name
        };
        let field = CanonicalField::new(name, value.as_bytes())
            .map_err(|error| issue("E-CAPSULE-021", error.to_string(), line_index + 1))?;
        SemanticHash::new(&[field])
            .map_err(|error| issue("E-CAPSULE-021", error.to_string(), line_index + 1))?;
        raw_fields.push((name.to_string(), value));
        *field_counts.entry(name.to_string()).or_insert(0_usize) += 1;
        index += consumed;
    }
    if !raw_fields
        .iter()
        .any(|(name, _)| name == "reference_signature")
    {
        if let Some((_, body)) = raw_fields
            .iter()
            .find(|(name, _)| name == "reference_body")
        {
            if let Ok(term) = Term::parse_body(body) {
                if let Ok(signature) = term.inferred_signature_text() {
                    raw_fields.push(("reference_signature".to_string(), signature));
                    *field_counts
                        .entry("reference_signature".to_string())
                        .or_insert(0_usize) += 1;
                }
            }
        }
    }

    let mut occurrences = BTreeMap::new();
    let field_names = raw_fields
        .iter()
        .map(|(name, _)| {
            if field_counts[name] == 1 {
                return name.clone();
            }
            let occurrence = occurrences.entry(name.clone()).or_insert(0_usize);
            let identified = format!("{name}_{occurrence}");
            *occurrence += 1;
            identified
        })
        .collect::<Vec<_>>();
    let fields = raw_fields
        .iter()
        .zip(&field_names)
        .map(|((_, value), name)| {
            CanonicalField::new(name, value.as_bytes())
                .map_err(|error| issue("E-CAPSULE-021", error.to_string(), 0))
        })
        .collect::<Result<Vec<_>, _>>()?;
    SemanticHash::new(&fields).map_err(|error| issue("E-CAPSULE-021", error.to_string(), 0))
}

/// One hash write-back for a capsule document. The body text is never
/// rewritten; only the `semantic_hash` line moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticHashRewrite {
    pub feature_id: String,
    pub previous: String,
    pub computed: SemanticHash,
    pub wrote: bool,
}

/// Split a file into `emath feature` documents. Comments before the first
/// feature are not included.
#[must_use]
pub fn capsule_documents(text: &str) -> Vec<String> {
    text.split("\nemath feature ")
        .enumerate()
        .filter_map(|(index, part)| {
            if index == 0 {
                part.find("emath feature ")
                    .map(|start| part[start..].to_string())
            } else {
                Some(format!("emath feature {part}"))
            }
        })
        .collect()
}

/// Rewrite each document's `semantic_hash` to the hash of its current
/// meaning-bearing fields. Never rewrites `reference_body`.
pub fn rewrite_declared_semantic_hashes(
    text: &str,
) -> Result<(String, Vec<SemanticHashRewrite>), CapsuleIssue> {
    let ranges = capsule_document_ranges(text);
    let mut output = String::with_capacity(text.len().saturating_add(64));
    let mut last = 0;
    let mut reports = Vec::new();
    for range in ranges {
        output.push_str(&text[last..range.start]);
        let document = &text[range.start..range.end];
        let (rewritten, report) = rewrite_one_document(document)?;
        output.push_str(&rewritten);
        reports.push(report);
        last = range.end;
    }
    output.push_str(&text[last..]);
    Ok((output, reports))
}
