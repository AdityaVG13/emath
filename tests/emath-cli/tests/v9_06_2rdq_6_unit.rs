//! CLI: expand / exactness / freeze / why / assumptions.

use emath_cli::{
    agent_check_json_document, agent_plan_json_document, agent_triage_json_document,
    exactness_json_document, expand_json_document, plan_json_document, run, EXIT_OK, EXIT_REFUSED,
    EXIT_USAGE,
};
use emath_syntax::{exactness_ledger, expand_scratch};

fn repo_file(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

fn json_arr<'a>(
    parsed: &'a emath_artifact::JsonValue,
    key: &str,
) -> &'a [emath_artifact::JsonValue] {
    match parsed.field(key).unwrap_or_else(|_| panic!("{key}")) {
        emath_artifact::JsonValue::Arr(items) => items,
        other => panic!("{key} must be array, got {other:?}"),
    }
}

#[test]
fn expand_and_exactness_and_assumptions_succeed() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/scratch.emath");
    assert_eq!(run(&["expand".into(), path.clone()]), EXIT_OK);
    assert_eq!(
        run(&["expand".into(), path.clone(), "--json".into()]),
        EXIT_OK
    );
    let source = std::fs::read_to_string(&path).expect("scratch source");
    let expansion = expand_scratch(&source);
    let parsed =
        emath_artifact::parse_json_document(&expand_json_document(&source, &expansion, None))
            .expect("expand --json");
    assert_eq!(parsed.string_field("command").expect("command"), "expand");
    match parsed.field("rewritten").expect("rewritten") {
        emath_artifact::JsonValue::Bool(_) => {}
        other => panic!("rewritten must be bool, got {other:?}"),
    }
    assert_eq!(
        parsed.string_field("level").expect("level"),
        expansion.level().as_str()
    );
    match parsed.field("ok").expect("ok") {
        emath_artifact::JsonValue::Bool(_) => {}
        other => panic!("ok must be bool, got {other:?}"),
    }
    assert_eq!(parsed.string_field("source").expect("source"), source);
    let _ = parsed.string_field("expanded").expect("expanded");
    let notes = json_arr(&parsed, "notes");
    assert_eq!(notes.len(), expansion.notes.len());
    for (note, row) in expansion.notes.iter().zip(notes) {
        assert_eq!(
            row.string_field("stability").expect("stability"),
            note.stability.as_str()
        );
    }
    let holes = json_arr(&parsed, "holes");
    assert_eq!(holes.len(), expansion.holes.len());
    for (hole, row) in expansion.holes.iter().zip(holes) {
        assert_eq!(
            row.string_field("continuation").expect("continuation"),
            hole.continuation.as_str()
        );
        let candidates = json_arr(row, "candidates");
        assert_eq!(candidates.len(), hole.candidates.len());
        for (candidate, cand) in hole.candidates.iter().zip(candidates) {
            assert_eq!(cand.string_field("status").expect("status"), "labeled");
            assert_eq!(
                cand.string_field("kind").expect("kind"),
                candidate.kind.as_str()
            );
            assert_eq!(cand.string_field("label").expect("label"), candidate.label);
        }
    }
    let solve_candidates = json_arr(&parsed, "solve_candidates");
    assert_eq!(solve_candidates.len(), expansion.solve.menu().len());
    for (world, cand) in expansion.solve.menu().iter().zip(solve_candidates) {
        assert_eq!(cand.string_field("label").expect("label"), world.as_str());
    }
    let diagnostics = json_arr(&parsed, "diagnostics");
    for item in diagnostics {
        let _ = item.string_field("code").expect("diagnostics[].code");
        let severity = item
            .string_field("severity")
            .expect("diagnostics[].severity");
        assert!(
            matches!(severity.as_str(), "error" | "warning" | "note"),
            "severity token {severity}"
        );
        let _ = item.string_field("message").expect("diagnostics[].message");
    }
    assert_eq!(run(&["exactness".into(), path.clone()]), EXIT_OK);
    assert_eq!(
        run(&["exactness".into(), path.clone(), "--json".into()]),
        EXIT_OK
    );
    let ledger = exactness_ledger(&source);
    let parsed = emath_artifact::parse_json_document(&exactness_json_document(&ledger, None))
        .expect("exactness --json");
    assert_eq!(
        parsed.string_field("command").expect("command"),
        "exactness"
    );
    for key in ["declared", "inferred", "constructed", "open"] {
        let _ = parsed.int_field(key).unwrap_or_else(|_| panic!("{key}"));
    }
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
        let _ = row.string_field("rationale").expect("rationale");
    }
    assert_eq!(
        run(&[
            "exactness".into(),
            path.clone(),
            "--raise".into(),
            "units".into()
        ]),
        EXIT_OK
    );
    assert_eq!(run(&["assumptions".into(), path.clone()]), EXIT_OK);
    assert_eq!(
        run(&["why".into(), path.clone(), "inference:1".into()]),
        EXIT_OK
    );
    assert_eq!(run(&["freeze".into(), path.clone()]), EXIT_OK);
}

#[test]
fn freeze_emits_versioned_lock_without_raising_authority() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/scratch.emath");
    let tmp = std::env::temp_dir().join("emath-v9-06-2rdq-6-freeze.emath");
    assert_eq!(
        run(&[
            "freeze".into(),
            path.clone(),
            "--out".into(),
            tmp.display().to_string(),
            "--json".into(),
        ]),
        EXIT_OK
    );
    let lock_path = tmp.with_extension("freeze.lock.json");
    let lock = std::fs::read_to_string(&lock_path).expect("sidecar lock");
    assert!(lock.contains("emath.freeze.lock.v1"), "{lock}");
    assert!(lock.contains("emath:meaning:v1:"), "{lock}");
    assert!(
        lock.contains("\"schema\": \"emath.freeze.lock.v1\""),
        "{lock}"
    );
    assert!(lock.contains("\"authority_raised\": false"), "{lock}");
    assert!(!lock.contains("\"authority_raised\": true"), "{lock}");
    assert!(lock.contains("\"source_content_id\""), "{lock}");
    assert!(lock.contains("\"frozen_content_id\""), "{lock}");
    assert!(lock.contains("\"prelude\""), "{lock}");
    assert!(lock.contains("\"numeric_policy\""), "{lock}");
    assert!(lock.contains("\"open\""), "{lock}");
    assert!(lock.contains("\"ledger\""), "{lock}");
    assert!(lock.contains("strict-f64"), "{lock}");
    assert!(lock.contains("native.rust"), "{lock}");
    let parsed = emath_artifact::parse_json_document(&lock).expect("lock must parse as JSON");
    let original = std::fs::read_to_string(&path).expect("original source");
    let frozen = std::fs::read_to_string(&tmp).expect("frozen sidecar");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        "emath.freeze.lock.v1"
    );
    assert!(
        parsed.field("command").is_err(),
        "lock is not the freeze envelope"
    );
    let source_id = parsed
        .string_field("source_content_id")
        .expect("source_content_id");
    let frozen_id = parsed
        .string_field("frozen_content_id")
        .expect("frozen_content_id");
    assert_eq!(source_id, emath_core::content_id_of_str(&original).0);
    assert_eq!(frozen_id, emath_core::content_id_of_str(&frozen).0);
    assert_ne!(source_id, frozen_id);
    assert!(parsed
        .string_field("meaning_id")
        .expect("meaning_id")
        .starts_with("emath:meaning:v1:"));
    assert_eq!(
        parsed.field("authority_raised").expect("authority_raised"),
        &emath_artifact::JsonValue::Bool(false)
    );
    assert_eq!(
        parsed.string_field("prelude").expect("prelude"),
        "scratch-v1"
    );
    assert_eq!(
        parsed
            .string_field("numeric_policy")
            .expect("numeric_policy"),
        "strict-f64"
    );
    for key in ["packages", "methods", "providers", "open", "ledger"] {
        match parsed.field(key).unwrap_or_else(|_| panic!("{key}")) {
            emath_artifact::JsonValue::Arr(_) => {}
            other => panic!("{key} must be array, got {other:?}"),
        }
    }
    match parsed.field("providers").expect("providers") {
        emath_artifact::JsonValue::Arr(items) => {
            assert!(
                items.iter().any(|item| {
                    matches!(item, emath_artifact::JsonValue::Str(s) if s == "native.rust")
                }),
                "{items:?}"
            );
        }
        other => panic!("providers must be array, got {other:?}"),
    }
    let exactness = exactness_ledger(&original);
    let ledger_rows = json_arr(&parsed, "ledger");
    assert_eq!(ledger_rows.len(), exactness.entries.len());
    for (entry, row) in exactness.entries.iter().zip(ledger_rows) {
        match row {
            emath_artifact::JsonValue::Str(concatenated) => {
                panic!("ledger items must be objects with as_str tokens, got {concatenated:?}")
            }
            _ => {}
        }
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
    }
    assert!(frozen.starts_with("# emath freeze: does not raise evidence authority\n"));
}

#[test]
fn plan_json_goals_are_kind_as_str_objects() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/hello-square.emath");
    assert_eq!(
        run(&["plan".into(), path.clone(), "--json".into()]),
        EXIT_OK
    );
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let package = session
        .load_package(std::path::Path::new(&path))
        .expect("load hello-square");
    let result = session.plan(package.file);
    assert!(
        !result.package.goals.is_empty(),
        "hello-square must have goals"
    );
    let parsed = emath_artifact::parse_json_document(&plan_json_document(
        !result.diagnostics.has_errors(),
        &result.package.goals,
        result.plans.len() as u64,
    ))
    .expect("plan --json");
    assert_eq!(parsed.string_field("command").expect("command"), "plan");
    match parsed.field("admitted").expect("admitted") {
        emath_artifact::JsonValue::Bool(_) => {}
        other => panic!("admitted must be bool, got {other:?}"),
    }
    let _ = parsed.int_field("plans").expect("plans");
    assert!(
        parsed.int_field("goals").is_err(),
        "goals must be an object array, not a duplicate count key"
    );
    let goals = json_arr(&parsed, "goals");
    assert_eq!(goals.len(), result.package.goals.len());
    for (goal, row) in result.package.goals.iter().zip(goals) {
        match row {
            emath_artifact::JsonValue::Str(concatenated) => {
                panic!("goals items must be objects with as_str kind, got {concatenated:?}")
            }
            _ => {}
        }
        assert_eq!(row.string_field("kind").expect("kind"), goal.kind.as_str());
        assert_eq!(row.string_field("target").expect("target"), goal.target);
    }
}

#[test]
fn agent_plan_json_goals_are_kind_as_str_objects() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/hello-square.emath");
    assert_eq!(run(&["agent".into(), "plan".into(), path.clone()]), EXIT_OK);
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let package = session
        .load_package(std::path::Path::new(&path))
        .expect("load hello-square");
    let result = session.plan(package.file);
    let parsed = emath_artifact::parse_json_document(&agent_plan_json_document(
        !result.diagnostics.has_errors(),
        &result.package.goals,
        result.plans.len() as u64,
    ))
    .expect("agent plan json");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        "emath.agent"
    );
    assert!(
        parsed.int_field("goals").is_err(),
        "agent plan goals must be an object array, not a duplicate count key"
    );
    let goals = json_arr(&parsed, "goals");
    assert_eq!(goals.len(), result.package.goals.len());
    for (goal, row) in result.package.goals.iter().zip(goals) {
        assert_eq!(row.string_field("kind").expect("kind"), goal.kind.as_str());
        assert_eq!(row.string_field("target").expect("target"), goal.target);
    }
    let parsed = emath_artifact::parse_json_document(&agent_triage_json_document(
        &path,
        true,
        &[],
        !result.diagnostics.has_errors(),
        &result.package.content_id().0,
        &result.diagnostics,
        true,
        None,
        &result.package.goals,
        result.plans.len() as u64,
    ))
    .expect("agent triage json");
    assert!(
        parsed.int_field("goals").is_err(),
        "agent triage goals must be an object array, not a count"
    );
    let goals = json_arr(&parsed, "goals");
    assert_eq!(goals.len(), result.package.goals.len());
    for (goal, row) in result.package.goals.iter().zip(goals) {
        assert_eq!(row.string_field("kind").expect("kind"), goal.kind.as_str());
        assert_eq!(row.string_field("target").expect("target"), goal.target);
    }
    let empty =
        std::env::temp_dir().join(format!("emath-agent-empty-{}.emath", std::process::id()));
    std::fs::write(&empty, "").expect("empty pane");
    assert_eq!(
        run(&[
            "agent".into(),
            "check".into(),
            empty.to_string_lossy().into_owned(),
        ]),
        EXIT_REFUSED
    );
    let (diagnostics, package_id) = {
        let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
        match session.load_package(&empty) {
            Ok(package) => {
                let result = session.check(package.file);
                (result.diagnostics, result.package.content_id().0)
            }
            Err(_) => panic!("empty file must load as empty source"),
        }
    };
    let parsed = emath_artifact::parse_json_document(&agent_check_json_document(
        false,
        &package_id,
        &diagnostics,
    ))
    .expect("agent check json");
    assert!(
        parsed.int_field("diagnostics").is_err(),
        "agent check diagnostics must be objects, not a count"
    );
    assert!(parsed.string_field("diagnostics_text").is_err());
    let items = json_arr(&parsed, "diagnostics");
    assert!(
        items
            .iter()
            .any(|row| row.string_field("code").ok().as_deref() == Some("E-PKG-081")),
        "agent check must surface E-PKG-081, got {items:?}"
    );
    let _ = std::fs::remove_file(&empty);
}

#[test]
fn freeze_refuses_claimed_exact_hole() {
    emath_syntax::install_source_parser();
    let path = repo_file("tests/invalid/v9_06_2rdq_6.emath");
    assert_eq!(run(&["freeze".into(), path]), EXIT_REFUSED);
}

#[test]
fn freeze_sidecar_lock_write_failure_removes_partial_source() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/scratch.emath");
    let dir = std::env::temp_dir().join("emath-v9-06-2rdq-6-partial-freeze");
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join("frozen.emath");
    let lock_path = out.with_extension("freeze.lock.json");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir_all(&lock_path);
    std::fs::create_dir_all(&lock_path).expect("lock path as directory");
    assert_eq!(
        run(&[
            "freeze".into(),
            path,
            "--out".into(),
            out.display().to_string(),
        ]),
        EXIT_USAGE
    );
    assert!(!out.exists(), "partial frozen source must be removed");
    let _ = std::fs::remove_dir_all(&lock_path);
    let _ = std::fs::remove_dir_all(&dir);
}
