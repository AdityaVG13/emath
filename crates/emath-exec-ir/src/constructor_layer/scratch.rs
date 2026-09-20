//! The `emath.scratch.v1` loop-session checkpoint contract.
//!
//! A scratch is the host-side checkpoint of one research-loop session:
//! the module identity, the target Step function, the explicit loop
//! state, and the immutable batch ledger. It is a sibling of the
//! continuation checkpoint (`checkpoint.rs`) with a different job:
//! the continuation checkpoint suspends an engine mid-reduction, the
//! scratch commits a session at a batch boundary. All loop state is
//! explicit data (X1), so no continuation state is needed here.
//!
//! Identity is carried, never computed: the host passes the module's
//! meaning id, language image id, and target name in, and loading
//! against different values refuses by name. The module-semantic law
//! that the revision must equal the loop state's own batch count
//! belongs to the host (it knows LoopState's shape); this contract
//! enforces the file-internal version of it - the ledger length and
//! the 1-based entry ordinals must match the declared revision.
//!
//! Determinism class: the encoding is byte-deterministic for equal
//! inputs (ordered maps, fixed field order, exact decimal integers,
//! float bits as hex). Values are preserved verbatim: non-canonical
//! rationals stay non-canonical; the scratch is storage, not a
//! normalizer.
//!
//! No-claim boundaries: closures, code, receipts, and buffers refuse
//! (`scratch_unserializable`) - closures are re-instantiated by the
//! host from authored declarations, buffers are mutable state, and
//! code/receipts are engine artifacts, none of them checkpoint cargo.
//! Ledger projections are machine-scale (`i128`); a loop whose keys or
//! scores exceed that scale cannot keep a scratch ledger.

use super::*;
use super::prelude::*;

use emath_artifact::{parse_json_document, JsonValue};
use emath_core::{json_quote, JsonWriter};

/// The schema line every scratch file declares and every load demands.
pub const SCRATCH_SCHEMA: &str = "emath.scratch.v1";

/// What a scratch belongs to. Carried by the host, compared on load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScratchIdentity {
    /// The module's meaning id (admitted mathematics, presentation-independent).
    pub meaning_id: String,
    /// The language image id the session ran against.
    pub language_id: String,
    /// The target Step function the host drives batch by batch.
    pub target: String,
}

/// One committed batch as the host observed it. Host projection, not
/// loop interpretation: the verdict vocabulary and the frozen case
/// identity are data, and case semantics stay target-side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchLedgerEntry {
    /// 1-based committed batch ordinal; must match its ledger position.
    pub batch: u64,
    /// The loop's verdict vocabulary (0 running, 1 goal_attained,
    /// 2 domain_exhausted, 3 plateau, 4 budget_exhausted).
    pub verdict: i128,
    /// Cumulative logical units charged after the batch (X4).
    pub used: i128,
    /// The final incumbent's key.
    pub incumbent_key: i128,
    /// The final incumbent's score, numerator.
    pub incumbent_score_num: i128,
    /// The final incumbent's score, denominator.
    pub incumbent_score_den: i128,
    /// Archive length after the batch.
    pub archive_len: u64,
    /// Keys newly accepted this batch, in archive order.
    pub promoted: Vec<i128>,
    /// Keys whose acceptance withdrew this batch (quarantine).
    pub quarantined: Vec<i128>,
    /// The frozen case identity after the batch.
    pub case_ids: Vec<i128>,
    /// Whether the batch ended on the budget halt (verdict 4).
    pub halted: bool,
}

/// A decoded scratch: what the host gets back from a load.
#[derive(Clone, Debug, PartialEq)]
pub struct ScratchCheckpoint {
    pub identity: ScratchIdentity,
    pub revision: u64,
    pub state: CValue,
    pub ledger: Vec<BatchLedgerEntry>,
}

// ---- value interchange ------------------------------------------------

fn render_value(value: &CValue, out: &mut String) -> Result<(), ConstructorError> {
    match value {
        CValue::Int(n) => {
            out.push_str(&n.to_string());
        }
        CValue::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
        }
        CValue::Rat { num, den } => {
            if den.is_zero() {
                return Err(fault(
                    "scratch_unserializable",
                    "a rational with zero denominator is not a value",
                ));
            }
            out.push_str(&format!("{{\"rat\": [{}, {}]}}", num, den));
        }
        CValue::Float64(f) => {
            out.push_str(&format!("{{\"f64\": \"{:016x}\"}}", f.to_bits()));
        }
        CValue::Sequence(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                render_value(item, out)?;
            }
            out.push(']');
        }
        CValue::Tuple(items) => {
            out.push_str("{\"tuple\": [");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                render_value(item, out)?;
            }
            out.push_str("]}");
        }
        CValue::Record { type_name, fields } => {
            out.push_str(&format!("{{\"record\": {{\"type\": {},\"fields\": {{", json_quote(type_name)));
            for (index, (name, field)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&json_quote(name));
                out.push_str(": ");
                render_value(field, out)?;
            }
            out.push_str("}}}");
        }
        CValue::Variant {
            type_name,
            tag,
            fields,
        } => {
            out.push_str(&format!(
                "{{\"variant\": {{\"type\": {},\"tag\": {},\"fields\": [",
                json_quote(type_name),
                json_quote(tag)
            ));
            for (index, item) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                render_value(item, out)?;
            }
            out.push_str("]}}");
        }
        CValue::Unit => out.push_str("{\"unit\": true}"),
        CValue::Absent => out.push_str("{\"absent\": true}"),
        CValue::Closure(_) | CValue::Code(_) | CValue::Receipt(_) | CValue::Buffer(_) => {
            return Err(fault(
                "scratch_unserializable",
                format!(
                    "{kind} is not checkpoint cargo: closures are re-instantiated by the host \
                     from authored declarations; code, receipts, and buffers are engine artifacts",
                    kind = match value {
                        CValue::Closure(_) => "a closure",
                        CValue::Code(_) => "code",
                        CValue::Receipt(_) => "a receipt",
                        _ => "a buffer",
                    }
                ),
            ));
        }
    }
    Ok(())
}

/// A single-key tagged object, strictly: exactly one entry whose key
/// is `tag`. Anything else is a malformed value, not a slow path.
fn tagged<'a>(json: &'a JsonValue, tag: &str) -> Result<&'a JsonValue, ConstructorError> {
    let JsonValue::Obj(entries) = json else {
        return Err(fault(
            "scratch_value",
            format!("expected a tagged `{tag}` object, found {json:?}"),
        ));
    };
    if entries.len() != 1 || entries[0].0 != tag {
        return Err(fault(
            "scratch_value",
            format!("expected exactly one `{tag}` entry, found {json:?}"),
        ));
    }
    Ok(&entries[0].1)
}

fn parse_i128(json: &JsonValue, what: &str) -> Result<i128, ConstructorError> {
    match json {
        JsonValue::Num(text) => text.parse::<i128>().map_err(|_| {
            fault(
                "scratch_value",
                format!("{what} is not a machine-scale integer: {text}"),
            )
        }),
        other => Err(fault(
            "scratch_value",
            format!("{what} is not a number: {other:?}"),
        )),
    }
}

/// Unique-key object view: duplicate keys are malformed (they would
/// silently drop state on decode).
fn unique_fields<'a>(
    json: &'a JsonValue,
) -> Result<&'a [(String, JsonValue)], ConstructorError> {
    let JsonValue::Obj(entries) = json else {
        return Err(fault("scratch_value", format!("expected an object, found {json:?}")));
    };
    let mut seen = BTreeSet::new();
    for (key, _) in entries {
        if !seen.insert(key.as_str()) {
            return Err(fault(
                "scratch_value",
                format!("duplicate object key `{key}`"),
            ));
        }
    }
    Ok(entries)
}

fn entry_field<'a>(
    entries: &'a [(String, JsonValue)],
    name: &str,
) -> Result<&'a JsonValue, ConstructorError> {
    entries
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| fault("scratch_value", format!("missing `{name}` field")))
}

fn parse_value(json: &JsonValue) -> Result<CValue, ConstructorError> {
    match json {
        JsonValue::Num(text) => {
            let n = ExactInt::parse(text).map_err(|err| {
                fault("scratch_value", format!("integer `{text}` does not parse: {err}"))
            })?;
            Ok(CValue::Int(n))
        }
        JsonValue::Bool(b) => Ok(CValue::Bool(*b)),
        JsonValue::Str(text) => Err(fault(
            "scratch_value",
            format!("a bare string `{text}` is not a constructor value"),
        )),
        JsonValue::Arr(items) => {
            let mut parsed = Vec::with_capacity(items.len());
            for item in items {
                parsed.push(parse_value(item)?);
            }
            Ok(CValue::Sequence(Arc::new(parsed)))
        }
        JsonValue::Null => Err(fault(
            "scratch_value",
            "null is not a constructor value (use {\"absent\": true} or {\"unit\": true})",
        )),
        JsonValue::Obj(_) => {
            if let Ok(rat) = tagged(json, "rat") {
                let JsonValue::Arr(parts) = rat else {
                    return Err(fault(
                        "scratch_value",
                        format!("`rat` payload must be [num, den], found {rat:?}"),
                    ));
                };
                if parts.len() != 2 {
                    return Err(fault(
                        "scratch_value",
                        format!("`rat` payload must be [num, den], found {rat:?}"),
                    ));
                }
                let num = ExactInt::parse(&match &parts[0] {
                    JsonValue::Num(text) => text.clone(),
                    other => {
                        return Err(fault(
                            "scratch_value",
                            format!("`rat` numerator is not a number: {other:?}"),
                        ))
                    }
                })
                .map_err(|err| fault("scratch_value", format!("`rat` numerator: {err}")))?;
                let den = ExactInt::parse(&match &parts[1] {
                    JsonValue::Num(text) => text.clone(),
                    other => {
                        return Err(fault(
                            "scratch_value",
                            format!("`rat` denominator is not a number: {other:?}"),
                        ))
                    }
                })
                .map_err(|err| fault("scratch_value", format!("`rat` denominator: {err}")))?;
                if den.is_zero() {
                    return Err(fault(
                        "scratch_value",
                        "a rational with zero denominator is not a value",
                    ));
                }
                return Ok(CValue::Rat { num, den });
            }
            if let Ok(f64_json) = tagged(json, "f64") {
                let JsonValue::Str(hex) = f64_json else {
                    return Err(fault(
                        "scratch_value",
                        format!("`f64` payload must be a 16-digit hex string, found {f64_json:?}"),
                    ));
                };
                if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(fault(
                        "scratch_value",
                        format!("`f64` payload must be a 16-digit hex string, found {hex}"),
                    ));
                }
                let bits = u64::from_str_radix(hex, 16)
                    .map_err(|err| fault("scratch_value", format!("`f64` bits: {err}")))?;
                return Ok(CValue::Float64(f64::from_bits(bits)));
            }
            if let Ok(tuple) = tagged(json, "tuple") {
                let JsonValue::Arr(items) = tuple else {
                    return Err(fault(
                        "scratch_value",
                        format!("`tuple` payload must be an array, found {tuple:?}"),
                    ));
                };
                let mut parsed = Vec::with_capacity(items.len());
                for item in items {
                    parsed.push(parse_value(item)?);
                }
                return Ok(CValue::Tuple(parsed));
            }
            if let Ok(record) = tagged(json, "record") {
                let entries = unique_fields(record)?;
                let type_name = match entry_field(entries, "type")? {
                    JsonValue::Str(name) => name.clone(),
                    other => {
                        return Err(fault(
                            "scratch_value",
                            format!("record `type` is not a string: {other:?}"),
                        ))
                    }
                };
                let fields_json = entry_field(entries, "fields")?;
                let field_entries = unique_fields(fields_json)?;
                let mut fields = BTreeMap::new();
                for (name, value) in field_entries {
                    fields.insert(name.clone(), parse_value(value)?);
                }
                return Ok(CValue::Record {
                    type_name,
                    fields: Arc::new(fields),
                });
            }
            if let Ok(variant) = tagged(json, "variant") {
                let entries = unique_fields(variant)?;
                let type_name = match entry_field(entries, "type")? {
                    JsonValue::Str(name) => name.clone(),
                    other => {
                        return Err(fault(
                            "scratch_value",
                            format!("variant `type` is not a string: {other:?}"),
                        ))
                    }
                };
                let tag = match entry_field(entries, "tag")? {
                    JsonValue::Str(tag) => tag.clone(),
                    other => {
                        return Err(fault(
                            "scratch_value",
                            format!("variant `tag` is not a string: {other:?}"),
                        ))
                    }
                };
                let fields_json = entry_field(entries, "fields")?;
                let JsonValue::Arr(items) = fields_json else {
                    return Err(fault(
                        "scratch_value",
                        format!("variant `fields` must be an array, found {fields_json:?}"),
                    ));
                };
                let mut parsed = Vec::with_capacity(items.len());
                for item in items {
                    parsed.push(parse_value(item)?);
                }
                return Ok(CValue::Variant {
                    type_name,
                    tag,
                    fields: parsed,
                });
            }
            if let Ok(unit) = tagged(json, "unit") {
                return match unit {
                    JsonValue::Bool(true) => Ok(CValue::Unit),
                    other => Err(fault(
                        "scratch_value",
                        format!("`unit` payload must be true, found {other:?}"),
                    )),
                };
            }
            if let Ok(absent) = tagged(json, "absent") {
                return match absent {
                    JsonValue::Bool(true) => Ok(CValue::Absent),
                    other => Err(fault(
                        "scratch_value",
                        format!("`absent` payload must be true, found {other:?}"),
                    )),
                };
            }
            Err(fault(
                "scratch_value",
                format!("unknown value shape: {json:?}"),
            ))
        }
    }
}

// ---- ledger entries ---------------------------------------------------

fn render_i128_array(items: &[i128]) -> String {
    let parts: Vec<String> = items.iter().map(|n| n.to_string()).collect();
    format!("[{}]", parts.join(", "))
}

fn render_entry(entry: &BatchLedgerEntry) -> String {
    let mut out = JsonWriter::object();
    out.int("batch", entry.batch);
    out.field("verdict", &entry.verdict.to_string());
    out.field("used", &entry.used.to_string());
    out.field("incumbent_key", &entry.incumbent_key.to_string());
    out.object_field(
        "incumbent_score",
        &format!(
            "{{\"rat\": [{}, {}]}}",
            entry.incumbent_score_num, entry.incumbent_score_den
        ),
    );
    out.int("archive_len", entry.archive_len);
    out.field("promoted", &render_i128_array(&entry.promoted));
    out.field("quarantined", &render_i128_array(&entry.quarantined));
    out.field("case_ids", &render_i128_array(&entry.case_ids));
    out.bool("halted", entry.halted);
    out.finish()
}

fn parse_i128_array(json: &JsonValue, what: &str) -> Result<Vec<i128>, ConstructorError> {
    let JsonValue::Arr(items) = json else {
        return Err(fault(
            "scratch_value",
            format!("`{what}` must be an array, found {json:?}"),
        ));
    };
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        parsed.push(parse_i128(item, what)?);
    }
    Ok(parsed)
}

fn parse_u64(entries: &[(String, JsonValue)], name: &str) -> Result<u64, ConstructorError> {
    match entry_field(entries, name)? {
        JsonValue::Num(text) => text.parse::<u64>().map_err(|_| {
            fault("scratch_value", format!("`{name}` is not a u64: {text}"))
        }),
        other => Err(fault(
            "scratch_value",
            format!("`{name}` is not a number: {other:?}")),
        ),
    }
}

fn parse_entry(json: &JsonValue) -> Result<BatchLedgerEntry, ConstructorError> {
    let entries = unique_fields(json)?;
    let verdict = parse_i128(entry_field(entries, "verdict")?, "verdict")?;
    let used = parse_i128(entry_field(entries, "used")?, "used")?;
    let incumbent_key = parse_i128(entry_field(entries, "incumbent_key")?, "incumbent_key")?;
    let score = parse_value(entry_field(entries, "incumbent_score")?)?;
    let (incumbent_score_num, incumbent_score_den) = match &score {
        CValue::Rat { num, den } => (
            num.to_i128().ok_or_else(|| {
                fault(
                    "scratch_value",
                    "ledger score numerator exceeds the machine-scale projection",
                )
            })?,
            den.to_i128().ok_or_else(|| {
                fault(
                    "scratch_value",
                    "ledger score denominator exceeds the machine-scale projection",
                )
            })?,
        ),
        other => {
            return Err(fault(
                "scratch_value",
                format!("`incumbent_score` is not a rational: {other:?}"),
            ))
        }
    };
    let halted = match entry_field(entries, "halted")? {
        JsonValue::Bool(halted) => *halted,
        other => {
            return Err(fault(
                "scratch_value",
                format!("`halted` is not a bool: {other:?}"),
            ))
        }
    };
    Ok(BatchLedgerEntry {
        batch: parse_u64(entries, "batch")?,
        verdict,
        used,
        incumbent_key,
        incumbent_score_num,
        incumbent_score_den,
        archive_len: parse_u64(entries, "archive_len")?,
        promoted: parse_i128_array(entry_field(entries, "promoted")?, "promoted")?,
        quarantined: parse_i128_array(entry_field(entries, "quarantined")?, "quarantined")?,
        case_ids: parse_i128_array(entry_field(entries, "case_ids")?, "case_ids")?,
        halted,
    })
}

/// File-internal ledger consistency: the entries are 1-based, ordered,
/// and their count is the declared revision. The host additionally
/// demands the revision equal the loop state's own batch count.
fn check_ledger(revision: u64, ledger: &[BatchLedgerEntry]) -> Result<(), ConstructorError> {
    if ledger.len() as u64 != revision {
        return Err(fault(
            "scratch_ledger",
            format!(
                "revision {revision} does not match the {len}-entry ledger",
                len = ledger.len()
            ),
        ));
    }
    for (index, entry) in ledger.iter().enumerate() {
        let expected = index as u64 + 1;
        if entry.batch != expected {
            return Err(fault(
                "scratch_ledger",
                format!(
                    "ledger entry {} declares batch {} (entries are 1-based and ordered)",
                    expected, entry.batch
                ),
            ));
        }
    }
    Ok(())
}

// ---- envelope ---------------------------------------------------------

/// Encode a session as a deterministic `emath.scratch.v1` document.
pub fn encode_scratch(
    identity: &ScratchIdentity,
    revision: u64,
    state: &CValue,
    ledger: &[BatchLedgerEntry],
) -> Result<String, ConstructorError> {
    check_ledger(revision, ledger)?;
    let mut state_text = String::new();
    render_value(state, &mut state_text)?;
    let entries: Vec<String> = ledger.iter().map(render_entry).collect();
    let mut out = JsonWriter::object();
    out.string("schema_version", SCRATCH_SCHEMA);
    out.string("meaning_id", &identity.meaning_id);
    out.string("language_id", &identity.language_id);
    out.string("target", &identity.target);
    out.int("revision", revision);
    out.object_field("state", &state_text);
    out.objects("ledger", &entries);
    Ok(out.finish())
}

/// Write a session to `path`. Refuses an inconsistent session the same
/// way [`decode_scratch`] would.
pub fn save_scratch(
    path: &Path,
    identity: &ScratchIdentity,
    revision: u64,
    state: &CValue,
    ledger: &[BatchLedgerEntry],
) -> Result<(), ConstructorError> {
    let text = encode_scratch(identity, revision, state, ledger)?;
    std::fs::write(path, text).map_err(|err| {
        fault(
            "scratch_io",
            format!("cannot write scratch {}: {err}", path.display()),
        )
    })
}

fn string_field_of(
    entries: &[(String, JsonValue)],
    name: &str,
) -> Result<String, ConstructorError> {
    match entry_field(entries, name)? {
        JsonValue::Str(text) => {
            if text.is_empty() {
                return Err(fault(
                    "scratch_value",
                    format!("`{name}` must be non-empty"),
                ));
            }
            Ok(text.clone())
        }
        other => Err(fault(
            "scratch_value",
            format!("`{name}` is not a string: {other:?}"),
        )),
    }
}

/// Decode and authenticate a scratch document. The identity must match
/// `expected` exactly; a mismatch refuses by name.
pub fn decode_scratch(
    text: &str,
    expected: &ScratchIdentity,
) -> Result<ScratchCheckpoint, ConstructorError> {
    let json = parse_json_document(text).map_err(|err| {
        fault(
            "scratch_torn",
            format!("scratch is not a writer-JSON document: {err}"),
        )
    })?;
    let entries = unique_fields(&json)?;
    let schema = string_field_of(entries, "schema_version")?;
    if schema != SCRATCH_SCHEMA {
        return Err(fault(
            "scratch_schema",
            format!("scratch declares schema `{schema}`, this contract is `{SCRATCH_SCHEMA}`"),
        ));
    }
    let identity = ScratchIdentity {
        meaning_id: string_field_of(entries, "meaning_id")?,
        language_id: string_field_of(entries, "language_id")?,
        target: string_field_of(entries, "target")?,
    };
    for (name, actual, expected_value) in [
        ("meaning_id", &identity.meaning_id, &expected.meaning_id),
        ("language_id", &identity.language_id, &expected.language_id),
        ("target", &identity.target, &expected.target),
    ] {
        if actual != expected_value {
            return Err(fault(
                "scratch_identity",
                format!(
                    "scratch {name} `{actual}` does not match the expected `{expected_value}`"
                ),
            ));
        }
    }
    let revision = parse_u64(entries, "revision")?;
    let state = parse_value(entry_field(entries, "state")?)?;
    let ledger_json = entry_field(entries, "ledger")?;
    let JsonValue::Arr(ledger_entries) = ledger_json else {
        return Err(fault(
            "scratch_value",
            format!("`ledger` must be an array, found {ledger_json:?}"),
        ));
    };
    let mut ledger = Vec::with_capacity(ledger_entries.len());
    for entry in ledger_entries {
        ledger.push(parse_entry(entry)?);
    }
    check_ledger(revision, &ledger)?;
    Ok(ScratchCheckpoint {
        identity,
        revision,
        state,
        ledger,
    })
}

/// Load and authenticate a scratch file.
pub fn load_scratch(
    path: &Path,
    expected: &ScratchIdentity,
) -> Result<ScratchCheckpoint, ConstructorError> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        fault(
            "scratch_io",
            format!("cannot read scratch {}: {err}", path.display()),
        )
    })?;
    decode_scratch(&text, expected)
}
