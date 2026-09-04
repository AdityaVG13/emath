//! WorldResultBundle envelope: every execution labels world,
//! method, inputs, assumptions, answer-or-disposition, evidence, and
//! cost. The World ABI ([`crate::evaluate_bounded`]) is the
//! producer; this module is the envelope — a bare scalar never escapes a
//! public path, dispositions are first-class, and the bundle id is a
//! deterministic content id (replay from IDs reconstructs the labeled
//! result).

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;

use emath_term::Term;
use emath_world_ir::fnv1a64;

use crate::{Environment, EvalError, FirstOrderWorld, WorldBudget};

/// Canonical schema id for the world-result envelope.
pub const WORLD_RESULT_SCHEMA: &str = "emath.world-result";
/// Envelope schema version. Bump on any change to the canonical encoding.
pub const WORLD_RESULT_VERSION: u32 = 1;

/// First-class outcome of one labeled execution. An execution that did
/// not produce an answer is still a complete, bundleable result — open,
/// refused, and faulted runs are recorded, never dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// The world produced a value; `canonical` is its labeled form.
    Answer {
        /// Canonical value text (producer-supplied labeler).
        canonical: String,
    },
    /// The term is open under the given valuation; the missing free
    /// variables are named.
    Open {
        /// Free variables without a valuation.
        missing: Vec<String>,
    },
    /// The world (or budget) refused the execution; the reason is named.
    Refused {
        /// Human-readable refusal reason.
        reason: String,
    },
    /// A custom world faulted; the fault detail is labeled, never
    /// silently dropped.
    Fault {
        /// Fault detail text.
        detail: String,
    },
}

impl Disposition {
    /// Stable disposition token.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Answer { .. } => "answer",
            Self::Open { .. } => "open",
            Self::Refused { .. } => "refused",
            Self::Fault { .. } => "fault",
        }
    }
}

/// One labeled execution: no field may be empty where a label is required
/// (`validate` refuses naked results).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldResult {
    /// World identity token (`ModularAlienWorld.evidence().world`).
    pub world: String,
    /// Origin class (`seed` / `user-defined` / `synthesized`).
    pub origin: String,
    /// Producer method (`evaluate-bounded`).
    pub method: String,
    /// Canonical form of the executed term.
    pub term_canonical: String,
    /// Free-variable valuation, labeled.
    pub inputs: BTreeMap<String, String>,
    /// Declared world effects (assumptions the answer relies on).
    pub assumptions: Vec<String>,
    /// First-class outcome.
    pub disposition: Disposition,
    /// Laws the world claims (its evidence record).
    pub evidence_laws: Vec<String>,
    /// Node visits spent (ABI budget meter).
    pub cost_steps: u32,
}

impl WorldResult {
    /// Typed refusal of a naked result: a required label is missing.
    #[must_use]
    pub fn validate(&self) -> Result<(), NakedResultRefusal> {
        if self.world.is_empty() {
            return Err(NakedResultRefusal::MissingWorld);
        }
        if self.method.is_empty() {
            return Err(NakedResultRefusal::MissingMethod);
        }
        Ok(())
    }

    fn canonical(&self) -> String {
        fn field(out: &mut String, name: &str, value: &str) {
            let _ =
                std::fmt::Write::write_fmt(out, format_args!("{name}:{}:{value}\n", value.len()));
        }
        let mut out = String::new();
        field(&mut out, "schema", WORLD_RESULT_SCHEMA);
        field(&mut out, "version", &WORLD_RESULT_VERSION.to_string());
        field(&mut out, "world", &self.world);
        field(&mut out, "origin", &self.origin);
        field(&mut out, "method", &self.method);
        field(&mut out, "term", &self.term_canonical);
        for (name, value) in &self.inputs {
            field(&mut out, "input", &format!("{name}={value}"));
        }
        for assumption in &self.assumptions {
            field(&mut out, "assumption", assumption);
        }
        field(&mut out, "disposition", self.disposition.kind());
        match &self.disposition {
            Disposition::Answer { canonical } => field(&mut out, "value", canonical),
            Disposition::Open { missing } => {
                for name in missing {
                    field(&mut out, "missing", name);
                }
            }
            Disposition::Refused { reason } => field(&mut out, "reason", reason),
            Disposition::Fault { detail } => field(&mut out, "detail", detail),
        }
        for law in &self.evidence_laws {
            field(&mut out, "evidence", law);
        }
        field(&mut out, "cost", &self.cost_steps.to_string());
        out
    }

    /// BTreeMap-ordered JSON (genesis receipt convention).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut object = BTreeMap::new();
        object.insert("assumptions", json_array(&self.assumptions));
        object.insert("cost_steps", Json::Number(self.cost_steps.to_string()));
        let mut disposition = BTreeMap::new();
        disposition.insert("kind", Json::Str(self.disposition.kind().to_string()));
        match &self.disposition {
            Disposition::Answer { canonical } => {
                disposition.insert("canonical", Json::Str(canonical.clone()));
            }
            Disposition::Open { missing } => {
                disposition.insert("missing", json_array(missing));
            }
            Disposition::Refused { reason } => {
                disposition.insert("reason", Json::Str(reason.clone()));
            }
            Disposition::Fault { detail } => {
                disposition.insert("detail", Json::Str(detail.clone()));
            }
        }
        object.insert("disposition", Json::Object(disposition));
        object.insert("evidence", json_array(&self.evidence_laws));
        let inputs = self
            .inputs
            .iter()
            .map(|(name, value)| {
                let mut entry = BTreeMap::new();
                entry.insert("name", Json::Str(name.clone()));
                entry.insert("value", Json::Str(value.clone()));
                Json::Object(entry)
            })
            .collect();
        object.insert("inputs", Json::Array(inputs));
        object.insert("method", Json::Str(self.method.clone()));
        object.insert("origin", Json::Str(self.origin.clone()));
        object.insert("schema", Json::Str(WORLD_RESULT_SCHEMA.to_string()));
        object.insert("term", Json::Str(self.term_canonical.clone()));
        object.insert("version", Json::Number(WORLD_RESULT_VERSION.to_string()));
        object.insert("world", Json::Str(self.world.clone()));
        emit_object(&object)
    }
}

/// Typed refusal of a naked result. Closed set: every refusal names the
/// missing label; nothing silent passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NakedResultRefusal {
    /// `E-WORLD-001` — the result does not name the world that produced
    /// it: a naked answer.
    MissingWorld,
    /// `E-WORLD-002` — the result does not name the producer method.
    MissingMethod,
}

impl NakedResultRefusal {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::MissingWorld => "E-WORLD-001",
            Self::MissingMethod => "E-WORLD-002",
        }
    }
}

impl fmt::Display for NakedResultRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorld => write!(
                formatter,
                "result carries no world label: a bare scalar is a naked \
                 answer (E-WORLD-001)"
            ),
            Self::MissingMethod => write!(
                formatter,
                "result carries no producer method label (E-WORLD-002)"
            ),
        }
    }
}

impl std::error::Error for NakedResultRefusal {}

/// A labeled bundle: one or more `WorldResult`s under a deterministic
/// content id. Replay from the id reconstructs the labeled result (same
/// producer + inputs + budget rebuild the same id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultBundle {
    /// The labeled results (every one validated at construction).
    pub results: Vec<WorldResult>,
    /// Content id: `fnv1a64:<hex>` over the canonical encoding.
    pub bundle_id: String,
}

impl ResultBundle {
    /// Bundles labeled results; a naked result refuses typed and never
    /// enters a bundle.
    pub fn new(results: Vec<WorldResult>) -> Result<Self, NakedResultRefusal> {
        for result in &results {
            result.validate()?;
        }
        let mut canonical = String::new();
        canonical.push_str(WORLD_RESULT_SCHEMA);
        canonical.push('\n');
        canonical.push_str(&WORLD_RESULT_VERSION.to_string());
        canonical.push('\n');
        for result in &results {
            canonical.push_str(&result.canonical());
        }
        Ok(Self {
            bundle_id: format!("fnv1a64:{:016x}", fnv1a64(canonical.as_bytes())),
            results,
        })
    }

    /// BTreeMap-ordered JSON (genesis receipt convention).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut object = BTreeMap::new();
        object.insert("bundle_id", Json::Str(self.bundle_id.clone()));
        let results = self.results.iter().map(|r| {
            let mut entry = BTreeMap::new();
            entry.insert("result", Json::Raw(r.to_json()));
            Json::Object(entry)
        });
        object.insert("results", Json::Array(results.collect()));
        object.insert("schema", Json::Str(WORLD_RESULT_SCHEMA.to_string()));
        object.insert("version", Json::Number(WORLD_RESULT_VERSION.to_string()));
        emit_object(&object)
    }
}

/// Runs the World ABI producer and wraps the outcome in the envelope:
/// answers, open terms, refusals, and faults all become labeled results
/// (dispositions are first-class, never missing answers). Worlds with
/// custom error types map their errors into [`EvalError`] first (the
/// `From` impl the ABI already requires).
pub fn evaluate_labeled<W, F>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: WorldBudget,
    label_value: F,
) -> WorldResult
where
    W: FirstOrderWorld<Error = EvalError>,
    F: Fn(&W::Value) -> String,
{
    let evidence = world.evidence();
    let inputs = environment
        .iter()
        .map(|(variable, value)| (variable.0.clone(), label_value(value)))
        .collect();
    let (disposition, cost_steps) = match crate::evaluate_counted(term, world, environment, budget)
    {
        Ok((value, steps)) => (
            Disposition::Answer {
                canonical: label_value(&value),
            },
            steps,
        ),
        Err(EvalError::MissingVariable(variable)) => (
            Disposition::Open {
                missing: vec![variable.0],
            },
            0,
        ),
        Err(EvalError::UnknownSymbol(symbol)) => (
            Disposition::Open {
                missing: vec![format!("symbol:{}", symbol.0)],
            },
            0,
        ),
        Err(EvalError::Arity {
            symbol,
            expected,
            actual,
        }) => (
            Disposition::Fault {
                detail: format!(
                    "Arity: symbol `{}` expects {expected} argument(s), got {actual}",
                    symbol.0
                ),
            },
            0,
        ),
        Err(EvalError::BudgetExhausted { steps }) => (
            Disposition::Refused {
                reason: format!("budget exhausted after {steps} step(s)"),
            },
            steps,
        ),
    };
    WorldResult {
        world: evidence.world.clone(),
        origin: evidence.origin.clone(),
        method: "evaluate-bounded".to_string(),
        term_canonical: term.canonical(),
        inputs,
        assumptions: world
            .effects()
            .iter()
            .map(|effect| (*effect).to_string())
            .collect(),
        disposition,
        evidence_laws: evidence.laws.clone(),
        cost_steps,
    }
}

// ── JSON emission (genesis receipt convention) ─────────────────────────

#[derive(Clone, Debug)]
enum Json {
    Str(String),
    Number(String),
    Array(Vec<Json>),
    Object(BTreeMap<&'static str, Json>),
    /// Pre-rendered JSON (used to nest a receipt verbatim).
    Raw(String),
}

fn json_array(items: &[String]) -> Json {
    Json::Array(items.iter().cloned().map(Json::Str).collect())
}

fn emit_object(fields: &BTreeMap<&'static str, Json>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\"{key}\":"));
        emit_json(value, &mut out);
    }
    out.push('}');
    out
}

fn emit_json(value: &Json, out: &mut String) {
    match value {
        Json::Str(text) => {
            let _ = std::fmt::Write::write_fmt(out, format_args!("\"{}\"", json_escape(text)));
        }
        Json::Number(text) => out.push_str(text),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                emit_json(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push_str(&emit_object(fields));
        }
        Json::Raw(text) => out.push_str(text),
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out
}
