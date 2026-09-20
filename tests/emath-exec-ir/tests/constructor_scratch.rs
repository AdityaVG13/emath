//! emath.scratch.v1 loop-session checkpoint contract (bead emath-8aqvm).
//!
//! The scratch is the host-side checkpoint of a research-loop session:
//! the module identity (carried, never computed here), the target Step
//! function, the explicit LoopState, and the immutable batch ledger.
//! The cases pin the laws:
//!   - round-trip: save -> load returns the same state and the
//!     encoding is byte-deterministic;
//!   - resume: continuing a loaded checkpoint matches the straight
//!     run (the X1 replay discipline through the file boundary);
//!   - mutation: a one-field edit of the checkpoint text diverges the
//!     resumed session (the file is load-bearing, not decorative);
//!   - identity: loading against a different module or target refuses
//!     by name;
//!   - ledger/schema: torn or internally inconsistent files refuse by
//!     name, and so does a save that would write one;
//!   - interchange: every serializable CValue shape round-trips
//!     verbatim; closures and buffers refuse - closures are
//!     re-instantiated by the host from authored declarations, buffers
//!     are mutable state, not checkpoint data.
//!
//! The valley session-surface fixture (`research_step_valley.emath`)
//! is the driving seam: StepValley is exactly the authored Step
//! function a loop host steps batch by batch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use emath_exec_ir::constructor_layer::{
    decode_scratch, encode_scratch, evaluate_function_at, save_scratch, load_scratch,
    BatchLedgerEntry, CValue, ScratchIdentity, SCRATCH_SCHEMA,
};
use emath_sema::session::CompilerSession;
use emath_syntax::parse_str;
use emath_test_harness::Probe;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn module_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/modules")
        .join(name)
}

fn parse_fixture(name: &str) -> (emath_core::tree::SyntaxTree, PathBuf) {
    let path = fixture_path(name);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {name}: {err}"));
    let (tree, diagnostics) = parse_str(&source);
    assert!(!diagnostics.has_errors(), "{name} parse errors: {diagnostics:?}");
    (tree, path)
}

/// Real module identity, computed the way the CLI's check lane does.
/// The scratch module itself never computes identity - it carries and
/// compares - so this helper stands in for the host's identity source.
fn meaning_id_of(path: &Path) -> String {
    emath_syntax::install_source_parser();
    let source = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read: {err}"));
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(&path.display().to_string(), &source);
    assert!(
        !result.diagnostics.has_errors(),
        "fixture {} admits: {:?}",
        path.display(),
        result.diagnostics
    );
    result
        .package
        .meaning_id(&[])
        .expect("meaning id")
        .as_str()
        .to_string()
}

fn valley_identity() -> ScratchIdentity {
    ScratchIdentity {
        meaning_id: meaning_id_of(&fixture_path("research_step_valley.emath")),
        language_id: "test-image".into(),
        target: "StepValley".into(),
    }
}

fn unused() -> BTreeMap<String, CValue> {
    BTreeMap::from([("unused".to_string(), CValue::Int(0.into()))])
}

fn seed_valley(tree: &emath_core::tree::SyntaxTree, path: &Path) -> CValue {
    evaluate_function_at(tree, "SeedValley", &unused(), Some(path))
        .unwrap_or_else(|err| panic!("SeedValley: {err}"))
}

fn step_valley(
    tree: &emath_core::tree::SyntaxTree,
    path: &Path,
    state: &CValue,
    budget: i128,
) -> CValue {
    let inputs = BTreeMap::from([
        ("state".to_string(), state.clone()),
        ("budget".to_string(), CValue::Int(budget.into())),
    ]);
    evaluate_function_at(tree, "StepValley", &inputs, Some(path))
        .unwrap_or_else(|err| panic!("StepValley: {err}"))
}

fn record_fields(value: &CValue) -> &BTreeMap<String, CValue> {
    match value {
        CValue::Record { fields, .. } => fields,
        other => panic!("not a record: {other:?}"),
    }
}

fn field_int(fields: &BTreeMap<String, CValue>, name: &str) -> i128 {
    match fields.get(name) {
        Some(CValue::Int(value)) => value.to_i128().unwrap_or_else(|| {
            panic!("field {name} exceeds the i128 ledger projection")
        }),
        other => panic!("field {name} is not an Int: {other:?}"),
    }
}

fn field_bool(fields: &BTreeMap<String, CValue>, name: &str) -> bool {
    match fields.get(name) {
        Some(CValue::Bool(value)) => *value,
        other => panic!("field {name} is not a Bool: {other:?}"),
    }
}

fn field_seq<'a>(fields: &'a BTreeMap<String, CValue>, name: &str) -> &'a [CValue] {
    match fields.get(name) {
        Some(CValue::Sequence(items)) => items,
        other => panic!("field {name} is not a Sequence: {other:?}"),
    }
}

fn state_int(state: &CValue, name: &str) -> i128 {
    field_int(record_fields(state), name)
}

/// The mini-host's ledger derivation: one entry per committed batch,
/// projected from the previous and next states (archive ordinals are
/// append-only, so accepted-flips diff by ordinal). This is the same
/// projection the B2 loop host will perform; the scratch contract only
/// requires the entries to be internally consistent.
fn ledger_entry(previous: &CValue, next: &CValue) -> BatchLedgerEntry {
    let prev_fields = record_fields(previous);
    let next_fields = record_fields(next);
    let prev_archive = field_seq(prev_fields, "archive");
    let next_archive = field_seq(next_fields, "archive");
    assert!(
        next_archive.len() >= prev_archive.len(),
        "archive shrank across a batch"
    );
    let mut promoted = Vec::new();
    let mut quarantined = Vec::new();
    for (ordinal, record) in next_archive.iter().enumerate() {
        let was_accepted = match prev_archive.get(ordinal) {
            Some(CValue::Record { fields, .. }) => field_bool(fields, "accepted"),
            _ => false,
        };
        let is_accepted = field_bool(record_fields(record), "accepted");
        let key = field_int(record_fields(record), "key");
        if is_accepted && !was_accepted {
            promoted.push(key);
        }
        if !is_accepted && was_accepted {
            quarantined.push(key);
        }
    }
    let incumbent = field_int(next_fields, "incumbent") as usize;
    let incumbent_record = record_fields(&next_archive[incumbent]);
    let case_set_fields = record_fields(next_fields.get("case_set").expect("case_set field"));
    let case_ids = field_seq(case_set_fields, "ids")
        .iter()
        .map(|value| match value {
            CValue::Int(id) => id
                .to_i128()
                .unwrap_or_else(|| panic!("case id exceeds the i128 ledger projection")),
            other => panic!("case id is not an Int: {other:?}"),
        })
        .collect();
    let incumbent_score = match incumbent_record.get("score") {
        Some(CValue::Rat { num, den }) => (
            num.to_i128()
                .unwrap_or_else(|| panic!("score exceeds the i128 ledger projection")),
            den.to_i128()
                .unwrap_or_else(|| panic!("score exceeds the i128 ledger projection")),
        ),
        other => panic!("score is not a Rat: {other:?}"),
    };
    BatchLedgerEntry {
        batch: field_int(next_fields, "batch") as u64,
        verdict: field_int(next_fields, "verdict"),
        used: field_int(next_fields, "used"),
        incumbent_key: field_int(incumbent_record, "key"),
        incumbent_score_num: incumbent_score.0,
        incumbent_score_den: incumbent_score.1,
        archive_len: next_archive.len() as u64,
        promoted,
        quarantined,
        case_ids,
        halted: field_int(next_fields, "verdict") == 4,
    }
}

/// One-field text surgery on the deterministic encoding. The rendering
/// is pinned by the scratch module (`"name": value` fields), so a
/// targeted replacement is a surgical mutation, not a rewrite.
fn replace_field(text: &str, field: &str, from: &str, to: &str) -> String {
    let needle = format!("\"{field}\": {from}");
    let replacement = format!("\"{field}\": {to}");
    assert!(
        text.contains(&needle),
        "the encoding does not render `{field}: {from}` as the test expected"
    );
    text.replacen(&needle, &replacement, 1)
}

fn temp_path(label: &str) -> PathBuf {
    let unique = format!(
        "emath_scratch_{label}_{}.json",
        std::process::id()
    );
    std::env::temp_dir().join(unique)
}

fn scratch_file(label: &str, identity: &ScratchIdentity, state: &CValue, ledger: &[BatchLedgerEntry]) -> PathBuf {
    let path = temp_path(label);
    save_scratch(
        &path,
        identity,
        ledger.len() as u64,
        state,
        ledger,
    )
    .unwrap_or_else(|err| panic!("save {label}: {err}"));
    path
}

/// Run the valley session batch by batch, checkpointing after each
/// batch, and return every intermediate state (seed first).
fn valley_session(
    tree: &emath_core::tree::SyntaxTree,
    path: &Path,
    batches: usize,
    budget: i128,
) -> Vec<CValue> {
    let mut states = vec![seed_valley(tree, path)];
    for _ in 0..batches {
        let next = step_valley(tree, path, states.last().expect("nonempty"), budget);
        states.push(next);
    }
    states
}

fn valley_ledger(states: &[CValue]) -> Vec<BatchLedgerEntry> {
    states
        .windows(2)
        .map(|pair| ledger_entry(&pair[0], &pair[1]))
        .collect()
}

#[test]
fn scratch_contract() {
    let mut probe = Probe::new(
        "emath.scratch.v1: loop-session checkpoint laws over the valley session surface",
    );

    probe.case("scratch-roundtrip-and-byte-determinism", |p| {
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let states = valley_session(&tree, &path, 2, 60);
        let ledger = valley_ledger(&states);
        let identity = valley_identity();
        let a2 = &states[2];

        let first = encode_scratch(&identity, 2, a2, &ledger)
            .unwrap_or_else(|err| panic!("encode: {err}"));
        let again = encode_scratch(&identity, 2, a2, &ledger)
            .unwrap_or_else(|err| panic!("encode: {err}"));
        p.demand("byte-deterministic", first == again, "second encode differed");

        let file_a = scratch_file("rt_a", &identity, a2, &ledger);
        let file_b = scratch_file("rt_b", &identity, a2, &ledger);
        let text_a = std::fs::read_to_string(&file_a).unwrap();
        let text_b = std::fs::read_to_string(&file_b).unwrap();
        p.demand("file-equals-encode", text_a == first, "save wrote different bytes than encode");
        p.demand("two-saves-identical", text_a == text_b, "two saves of the same session differ");

        let loaded = load_scratch(&file_a, &identity)
            .unwrap_or_else(|err| panic!("load: {err}"));
        p.eq("state-roundtrips", loaded.state, a2.clone());
        p.demand("revision-roundtrips", loaded.revision == 2, format!("{}", loaded.revision));
        p.demand("ledger-roundtrips", loaded.ledger == ledger, format!("{:?}", loaded.ledger));
        p.demand(
            "schema-line",
            text_a.contains(&format!("\"schema_version\": \"{SCRATCH_SCHEMA}\"")),
            "schema_version field missing",
        );
    });

    probe.case("scratch-resume-through-file-matches-straight", |p| {
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let states = valley_session(&tree, &path, 3, 60);
        let identity = valley_identity();

        // Resume from the batch-1 checkpoint: the next batch through
        // the file boundary equals the straight run's batch 2.
        let ledger1 = valley_ledger(&states[..2]);
        let file1 = scratch_file("resume1", &identity, &states[1], &ledger1);
        let resumed = load_scratch(&file1, &identity)
            .unwrap_or_else(|err| panic!("load: {err}"));
        let continued = step_valley(&tree, &path, &resumed.state, 60);
        p.eq("batch2-through-file", continued, states[2].clone());

        // Resume from the batch-2 checkpoint: the final batch matches
        // the straight run's goal state.
        let ledger2 = valley_ledger(&states[..3]);
        let file2 = scratch_file("resume2", &identity, &states[2], &ledger2);
        let resumed2 = load_scratch(&file2, &identity)
            .unwrap_or_else(|err| panic!("load: {err}"));
        let continued2 = step_valley(&tree, &path, &resumed2.state, 60);
        p.eq("batch3-through-file", continued2, states[3].clone());
        p.demand(
            "goal-reached",
            state_int(&states[3], "verdict") == 1,
            format!("verdict {}", state_int(&states[3], "verdict")),
        );
    });

    probe.case("scratch-mutation-diverges", |p| {
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let states = valley_session(&tree, &path, 3, 60);
        let identity = valley_identity();
        let ledger2 = valley_ledger(&states[..3]);
        let text = encode_scratch(&identity, 2, &states[2], &ledger2)
            .unwrap_or_else(|err| panic!("encode: {err}"));
        let clean_next = &states[3];

        // Mutation 1: a stepping-stone's `retained` flag. The freshness
        // law re-probes and re-charges every retained record each
        // batch; a record the checkpoint marks not-retained is skipped
        // by the next batch's refresh, and the divergence surfaces in
        // the charged units (and anywhere else the flag gates).
        let mutated = replace_field(&text, "retained", "true", "false");
        let loaded = decode_scratch(&mutated, &identity)
            .unwrap_or_else(|err| panic!("mutated file still decodes: {err}"));
        let mutated_next = step_valley(&tree, &path, &loaded.state, 60);
        p.demand(
            "retained-mutation-diverges",
            mutated_next != *clean_next,
            "the next batch was identical despite the mutated retained flag",
        );
        let used_clean = state_int(clean_next, "used");
        let used_mutated = state_int(&mutated_next, "used");
        p.demand(
            "retained-mutation-skips-refresh",
            used_mutated + 2 == used_clean,
            format!(
                "expected the skipped record to save its two refresh units \
                 (mutated {used_mutated}, clean {used_clean})"
            ),
        );

        // Mutation 2: the charged-units counter. A wrong `used` moves
        // the next batch across its budget gate.
        let used = state_int(&states[2], "used");
        let mutated_used = replace_field(
            &text,
            "used",
            &used.to_string(),
            &(used + 10_000).to_string(),
        );
        let loaded_used = decode_scratch(&mutated_used, &identity)
            .unwrap_or_else(|err| panic!("mutated file still decodes: {err}"));
        let mutated_used_next = step_valley(&tree, &path, &loaded_used.state, 60);
        p.demand(
            "used-mutation-diverges",
            mutated_used_next != *clean_next,
            "the next batch was identical despite the mutated used counter",
        );
        p.demand(
            "used-mutation-halts",
            state_int(&mutated_used_next, "verdict") == 4,
            format!(
                "expected the budget-halt verdict 4, got {}",
                state_int(&mutated_used_next, "verdict")
            ),
        );
    });

    probe.case("scratch-identity-refusal", |p| {
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let states = valley_session(&tree, &path, 2, 60);
        let ledger = valley_ledger(&states[..3]);
        let identity = valley_identity();
        let file = scratch_file("identity", &identity, &states[2], &ledger);

        // A different module's identity refuses by name.
        let other_module = ScratchIdentity {
            meaning_id: meaning_id_of(&module_fixture_path("research_loop_targets.emath")),
            language_id: identity.language_id.clone(),
            target: identity.target.clone(),
        };
        match load_scratch(&file, &other_module) {
            Err(err) => p.demand(
                "module-identity-refused",
                err.code == "scratch_identity",
                err.to_string(),
            ),
            Ok(_) => p.fail("module-identity-refused", "loaded against a foreign module"),
        };

        // A wrong target refuses by name.
        let wrong_target = ScratchIdentity {
            target: "StepTargets".into(),
            ..identity.clone()
        };
        match load_scratch(&file, &wrong_target) {
            Err(err) => p.demand(
                "target-refused",
                err.code == "scratch_identity",
                err.to_string(),
            ),
            Ok(_) => p.fail("target-refused", "loaded against a wrong target"),
        };

        // A wrong language image refuses by name.
        let wrong_image = ScratchIdentity {
            language_id: "other-image".into(),
            ..identity.clone()
        };
        match load_scratch(&file, &wrong_image) {
            Err(err) => p.demand(
                "language-refused",
                err.code == "scratch_identity",
                err.to_string(),
            ),
            Ok(_) => p.fail("language-refused", "loaded against a wrong image"),
        };
    });

    probe.case("scratch-ledger-and-schema-laws", |p| {
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let states = valley_session(&tree, &path, 2, 60);
        let ledger = valley_ledger(&states[..3]);
        let identity = valley_identity();
        let text = encode_scratch(&identity, 2, &states[2], &ledger)
            .unwrap_or_else(|err| panic!("encode: {err}"));

        // The revision must equal the ledger length: a file that
        // claims more batches than it logged is torn.
        let torn = replace_field(&text, "revision", "2", "3");
        match decode_scratch(&torn, &identity) {
            Err(err) => p.demand(
                "revision-ledger-refused",
                err.code == "scratch_ledger",
                err.to_string(),
            ),
            Ok(_) => p.fail("revision-ledger-refused", "torn revision accepted"),
        };

        // Ledger entries are 1-based and ordered: a batch ordinal that
        // does not match its position refuses.
        let reordered = replace_field(&text, "batch", "1", "5");
        match decode_scratch(&reordered, &identity) {
            Err(err) => p.demand(
                "entry-ordinal-refused",
                err.code == "scratch_ledger",
                err.to_string(),
            ),
            Ok(_) => p.fail("entry-ordinal-refused", "wrong entry ordinal accepted"),
        };

        // The schema version is the file's self-declaration: anything
        // else refuses before any other interpretation.
        let wrong_schema = replace_field(
            &text,
            "schema_version",
            &format!("\"{SCRATCH_SCHEMA}\""),
            "\"emath.scratch.v0\"",
        );
        match decode_scratch(&wrong_schema, &identity) {
            Err(err) => p.demand(
                "schema-refused",
                err.code == "scratch_schema",
                err.to_string(),
            ),
            Ok(_) => p.fail("schema-refused", "foreign schema accepted"),
        };

        // Non-JSON text is a torn file, not a panic.
        match decode_scratch("not json {", &identity) {
            Err(err) => p.demand(
                "torn-refused",
                err.code == "scratch_torn",
                err.to_string(),
            ),
            Ok(_) => p.fail("torn-refused", "garbage decoded"),
        };

        // The save side refuses to write an inconsistent session: the
        // ledger must match the revision it is saved under.
        let short_ledger = &ledger[..1];
        match encode_scratch(&identity, 2, &states[2], short_ledger) {
            Err(err) => p.demand(
                "save-consistency-refused",
                err.code == "scratch_ledger",
                err.to_string(),
            ),
            Ok(_) => p.fail("save-consistency-refused", "inconsistent save accepted"),
        };
    });

    probe.case("scratch-value-interchange-laws", |p| {
        // Every serializable shape, verbatim (non-canonical rats stay
        // non-canonical: the scratch preserves values, it does not
        // normalize them).
        let value = CValue::Record {
            type_name: "Cover".into(),
            fields: Arc::new(BTreeMap::from([
                (
                    "neg_int".into(),
                    CValue::Int((-5).into()),
                ),
                (
                    "rat_noncanonical".into(),
                    CValue::Rat {
                        num: 2.into(),
                        den: 4.into(),
                    },
                ),
                (
                    "float".into(),
                    CValue::Float64(1.5e300),
                ),
                (
                    "neg_zero_float".into(),
                    CValue::Float64(-0.0),
                ),
                (
                    "nested".into(),
                    CValue::Sequence(Arc::new(vec![
                        CValue::Int(7.into()),
                        CValue::Sequence(Arc::new(Vec::new())),
                        CValue::Tuple(vec![CValue::Bool(true), CValue::Int(0.into())]),
                    ])),
                ),
                (
                    "variant".into(),
                    CValue::Variant {
                        type_name: "Verdict".into(),
                        tag: "Halted".into(),
                        fields: vec![CValue::Int(4.into())],
                    },
                ),
                ("unit".into(), CValue::Unit),
                ("absent".into(), CValue::Absent),
            ])),
        };
        let identity = valley_identity();
        let text = encode_scratch(&identity, 0, &value, &[])
            .unwrap_or_else(|err| panic!("encode: {err}"));
        let loaded = decode_scratch(&text, &identity)
            .unwrap_or_else(|err| panic!("decode: {err}"));
        p.eq("all-shapes-roundtrip", loaded.state.clone(), value.clone());

        // Float bits survive exactly (structural equality alone would
        // not distinguish -0.0 from 0.0).
        let neg_zero = record_fields(&loaded.state)["neg_zero_float"].clone();
        match neg_zero {
            CValue::Float64(bits) => p.demand(
                "float-bits-preserved",
                bits.to_bits() == (-0.0f64).to_bits(),
                format!("{bits:?}"),
            ),
            other => p.fail("float-bits-preserved", format!("{other:?}")),
        };

        // Closures refuse: the host re-instantiates them from authored
        // declarations; they are never checkpoint cargo.
        let (tree, path) = parse_fixture("research_step_valley.emath");
        let closure = evaluate_function_at(&tree, "MakeValueOf", &unused(), Some(&path))
            .unwrap_or_else(|err| panic!("MakeValueOf: {err}"));
        p.demand(
            "closure-value-produced",
            matches!(closure, CValue::Closure(_)),
            format!("{closure:?}"),
        );
        match encode_scratch(&identity, 0, &closure, &[]) {
            Err(err) => p.demand(
                "closure-refused",
                err.code == "scratch_unserializable",
                err.to_string(),
            ),
            Ok(_) => p.fail("closure-refused", "a closure was serialized"),
        };

        // Buffers refuse: mutable state is not checkpoint data (the
        // constructor layer's own memo-key precedent).
        let buffer = CValue::Buffer(Arc::new(Mutex::new(vec![CValue::Int(1.into())])));
        match encode_scratch(&identity, 0, &buffer, &[]) {
            Err(err) => p.demand(
                "buffer-refused",
                err.code == "scratch_unserializable",
                err.to_string(),
            ),
            Ok(_) => p.fail("buffer-refused", "a buffer was serialized"),
        };
    });

    probe.finish();
}
