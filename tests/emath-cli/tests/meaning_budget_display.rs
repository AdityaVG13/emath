//! Meaning-budget display and raise refusals (bead
//! emath-meaning-budget-display-zql4b, 04 / V9-06.09).
//!
//! Contracts:
//! - `emath exactness <file>` prints declared/inferred/constructed/open
//!   counts plus the per-dimension ledger, CLI and `--json` alike.
//! - `--raise <dimension>` is propose-only: exit ok, source never rewritten.
//! - A freeze lock pins the frozen meaning: `--raise` on a frozen file is
//!   refused (`E-SYN-155`), while budget display without `--raise` stays
//!   allowed — the budget is a view, not an authority change.

use emath_artifact::JsonValue;
use emath_cli::{exactness_json_document, run, EXIT_OK, EXIT_REFUSED};
use emath_syntax::{exactness_ledger, exactness_ledger_raised, ExactnessDimension, ExactnessStatus};

fn repo_file(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

fn json_arr<'a>(parsed: &'a JsonValue, key: &str) -> &'a [JsonValue] {
    match parsed.field(key).unwrap_or_else(|_| panic!("{key}")) {
        JsonValue::Arr(items) => items,
        other => panic!("{key} must be array, got {other:?}"),
    }
}

/// The guided L1 example produces a full budget: every ledger entry carries
/// the (id, dimension, status, name, rationale) shape, units start open,
/// and the CLI exits ok.
#[test]
fn budget_prints_counts_for_l1_guided() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/l1_guided.emath");
    assert_eq!(run(&["exactness".into(), path.clone()]), EXIT_OK);

    let source = std::fs::read_to_string(&path).expect("l1_guided source");
    let ledger = exactness_ledger(&source);
    assert!(
        !ledger.entries.is_empty(),
        "guided example must produce ledger rows"
    );
    assert_eq!(
        ledger.count(ExactnessStatus::Declared)
            + ledger.count(ExactnessStatus::Inferred)
            + ledger.count(ExactnessStatus::Constructed)
            + ledger.count(ExactnessStatus::Open),
        ledger.entries.len(),
        "every entry has exactly one status"
    );
    assert!(
        ledger.entries.iter().any(|e| e.dimension.as_str() == "unit"
            && e.status.as_str() == "open"),
        "units stay open until declared or raised"
    );
    assert!(
        ledger
            .entries
            .iter()
            .any(|e| e.dimension.as_str() == "syntactic"),
        "the syntactic dimension is always present"
    );
}

/// `--raise units` proposes the units clause in the printed budget but
/// never rewrites the file (propose-only).
#[test]
fn raise_is_propose_only_and_source_stays_untouched() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/l1_guided.emath");
    let before = std::fs::read_to_string(&path).expect("source before");
    assert_eq!(
        run(&[
            "exactness".into(),
            path.clone(),
            "--raise".into(),
            "units".into()
        ]),
        EXIT_OK
    );
    let after = std::fs::read_to_string(&path).expect("source after");
    assert_eq!(before, after, "raise must be propose-only");

    let raised = exactness_ledger_raised(&before, &[ExactnessDimension::Unit]);
    assert!(
        raised.entries.iter().any(|e| e.dimension.as_str() == "unit"
            && e.status.as_str() == "declared"),
        "raised budget must show units declared"
    );
    assert!(
        !before.contains("units:"),
        "fixture must not already carry a units clause"
    );
}

/// Negative control: `--raise` on a frozen file refuses (E-SYN-155);
/// display without `--raise` on the same frozen file stays allowed.
#[test]
fn raise_on_frozen_is_refused_but_display_is_allowed() {
    emath_syntax::install_source_parser();
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/exactness_raise_on_frozen.emath"
    ));
    assert!(
        fixture.contains("expect: E-SYN-155"),
        "fixture must pin E-SYN-155"
    );
    let tmp = std::env::temp_dir().join(format!(
        "emath_zql4b_frozen_{}_raise_on_frozen",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    let src = tmp.join("raise_on_frozen.emath");
    std::fs::write(&src, fixture).expect("fixture copy");
    let frozen = tmp.join("frozen.emath");
    assert_eq!(
        run(&[
            "freeze".into(),
            src.to_string_lossy().into_owned(),
            "--out".into(),
            frozen.to_string_lossy().into_owned(),
        ]),
        EXIT_OK,
        "freeze of the fixture must succeed"
    );
    let lock = tmp.join("frozen.freeze.lock.json");
    assert!(lock.is_file(), "freeze must write the sidecar lock");

    assert_eq!(
        run(&[
            "exactness".into(),
            frozen.to_string_lossy().into_owned(),
            "--raise".into(),
            "units".into()
        ]),
        EXIT_REFUSED,
        "raise on a frozen meaning must refuse"
    );
    assert_eq!(
        run(&["exactness".into(), frozen.to_string_lossy().into_owned()]),
        EXIT_OK,
        "budget display on a frozen file stays allowed"
    );
}

/// JSON schema stability: counts mirror the ledger, meaning_id is carried
/// through, and every entry row has the five contract fields.
#[test]
fn exactness_json_schema_is_stable() {
    emath_syntax::install_source_parser();
    let source = std::fs::read_to_string(repo_file("language/examples/intro/l1_guided.emath"))
        .expect("l1_guided source");
    let ledger = exactness_ledger(&source);
    let text = exactness_json_document(&ledger, Some("emath:meaning:v1:test"));
    let parsed = emath_artifact::parse_json_document(&text).expect("exactness --json");
    assert_eq!(parsed.string_field("command").expect("command"), "exactness");
    assert_eq!(
        parsed.int_field("declared").expect("declared") as usize,
        ledger.count(ExactnessStatus::Declared)
    );
    assert_eq!(
        parsed.int_field("open").expect("open") as usize,
        ledger.count(ExactnessStatus::Open)
    );
    assert_eq!(
        parsed.string_field("meaning_id").expect("meaning_id"),
        "emath:meaning:v1:test"
    );
    let entries = json_arr(&parsed, "entries");
    assert_eq!(entries.len(), ledger.entries.len());
    for (entry, row) in ledger.entries.iter().zip(entries) {
        assert_eq!(row.string_field("id").expect("id"), entry.inference_id);
        assert_eq!(
            row.string_field("dimension").expect("dimension"),
            entry.dimension.as_str()
        );
        assert_eq!(
            row.string_field("status").expect("status"),
            entry.status.as_str()
        );
        assert_eq!(row.string_field("name").expect("name"), entry.name);
        assert_eq!(
            row.string_field("rationale").expect("rationale"),
            entry.rationale
        );
    }
}
