//! Checking reports the source loaded into the session, not a later disk edit.

use emath_core::parse::{SourceParser, register_source_parser};
use emath_core::{Diagnostics, Edition, FileId, limits::Limits, tree::SyntaxTree};
use std::path::PathBuf;
use std::sync::OnceLock;

const ORIGINAL: &str = "emath function Snapshot:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n
";
const EDITED: &str = "emath function Snapshot:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = missing
";

static SOURCE: OnceLock<PathBuf> = OnceLock::new();
struct EditDuringParse;

#[test]
fn session_reload_keeps_source_revisions() {
    register_source_parser(&EditDuringParse);
    let valid = ORIGINAL.replace("Snapshot", "SessionSnapshot");
    let invalid = EDITED.replace("Snapshot", "SessionSnapshot");
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let first = session.load_text("session.emath", &valid);
    assert!(!session.check(first).diagnostics.has_errors());
    let rejected = session.check_owned("session.emath", &invalid);
    assert!(
        rejected
            .diagnostics
            .errors()
            .any(|error| error.code == "E-TYPE-002"),
        "checking a changed display name must use its new bytes"
    );
    assert_eq!(session.store.get(first).unwrap().text, valid);
    assert!(!session.check(first).diagnostics.has_errors());
    let second = session.load_text("session.emath", &invalid);
    assert_ne!(
        first, second,
        "different bytes require different source revisions"
    );
    assert_eq!(session.store.get(second).unwrap().text, invalid);
    assert_eq!(session.load_text("session.emath", &invalid), second);
}

#[test]
fn disk_reload_keeps_file_ids_bound_to_bytes() {
    register_source_parser(&EditDuringParse);
    let dir = std::env::temp_dir().join(format!("emath-session-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reload.emath");
    let valid = ORIGINAL.replace("Snapshot", "DiskSnapshot");
    let invalid = EDITED.replace("Snapshot", "DiskSnapshot");
    std::fs::write(&path, &valid).unwrap();
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let first = session.load_package(&path).unwrap();
    let mut store = emath_core::SourceStore::new();
    let stored_first = store.load_path(&path).unwrap();
    std::fs::write(&path, &invalid).unwrap();
    let second = session.load_package(&path).unwrap();
    assert_eq!(
        session.store.get(second.file).unwrap().text,
        second.text,
        "a loaded package's FileId must identify its returned source bytes"
    );
    assert_eq!(session.store.get(first.file).unwrap().text, first.text);
    assert!(!session.check(first.file).diagnostics.has_errors());
    assert!(session.check(second.file).diagnostics.has_errors());
    let stored_second = store.load_path(&path).unwrap();
    assert_eq!(store.get(stored_second).unwrap().text, invalid);
    assert_eq!(store.get(stored_first).unwrap().text, valid);
}

#[test]
fn session_diagnostics_use_loaded_file_ids() {
    register_source_parser(&EditDuringParse);
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    session.load_text("unrelated.emath", "# unrelated source\n");
    let invalid = EDITED.replace("Snapshot", "DiagnosticSnapshot");
    let file = session.load_text("diagnostic.emath", &invalid);
    let result = session.check(file);
    assert_eq!(result.syntax.as_ref().unwrap().source.file, file);
    let unknown = result
        .diagnostics
        .errors()
        .find(|error| error.code == "E-TYPE-002")
        .unwrap();
    assert_eq!(unknown.primary.file, file);
    let rendered = session.store.render(unknown);
    assert!(rendered.contains("diagnostic.emath:7:"), "{rendered}");
    assert!(rendered.contains("result = missing"), "{rendered}");

    let broken = session.load_text(
        "broken.emath",
        "emath function Broken:\n    definitions:\n        result = [1\n",
    );
    let result = session.check(broken);
    assert!(result.parse_diagnostics.has_errors());
    assert!(
        result
            .parse_diagnostics
            .errors()
            .all(|error| error.primary.file == broken),
        "{:?}",
        result.parse_diagnostics
    );
}

#[test]
fn package_check_uses_current_sibling_revision_and_spans() {
    register_source_parser(&EditDuringParse);
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    session.load_text("unrelated.emath", "# unrelated source\n");
    let main_text = format!(
        "package snapshot\nuse snapshot.helper\n{}",
        ORIGINAL.replace("Snapshot", "MainSnapshot")
    );
    let main = session.load_text("main.emath", &main_text);
    let helper_text = ORIGINAL.replace("Snapshot", "HelperSnapshot");
    let first = session.load_text("helper.emath", &helper_text);
    let result = session.check_package(main);
    assert!(!result.diagnostics.has_errors(), "{:?}", result.diagnostics);
    assert_eq!(result.syntax.as_ref().unwrap().source.file, main);
    let helper = result
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.name.leaf() == "HelperSnapshot")
        .unwrap();
    assert_eq!(helper.source.file, first);

    let invalid = EDITED.replace("Snapshot", "HelperSnapshot");
    let second = session.load_text("helper.emath", &invalid);
    let result = session.check_package(main);
    let unknown = result
        .diagnostics
        .errors()
        .find(|error| error.code == "E-TYPE-002")
        .expect("the latest sibling revision, not the previous valid definition, must be checked");
    assert_eq!(unknown.primary.file, second);
    assert!(session.store.render(unknown).contains("result = missing"));
    assert!(result.prepared.is_none());
    assert_eq!(session.store.get(first).unwrap().text, helper_text);

    let broken = session.load_text(
        "helper.emath",
        "emath function Broken:\n    definitions:\n        result = [1\n",
    );
    let result = session.check_package(main);
    assert!(result.parse_diagnostics.has_errors());
    assert!(
        result
            .parse_diagnostics
            .errors()
            .all(|error| error.primary.file == broken),
        "{:?}",
        result.parse_diagnostics
    );
    assert!(result.prepared.is_none());
}

impl SourceParser for EditDuringParse {
    fn parse(
        &self,
        text: &str,
        file: FileId,
        limits: &Limits,
        edition: Edition,
    ) -> (SyntaxTree, Diagnostics) {
        let parsed = emath_syntax::parse_with_edition(text, file, limits, edition);
        if text == ORIGINAL {
            std::fs::write(SOURCE.get().unwrap(), EDITED).unwrap();
        } else if text == EDITED
            && std::env::var("EMATH_SNAPSHOT_CHILD").as_deref() == Ok("invalid")
        {
            std::fs::write(SOURCE.get().unwrap(), ORIGINAL).unwrap();
        }
        parsed
    }
}

#[test]
fn check_uses_loaded_source_snapshot() {
    let dir = std::env::temp_dir().join(format!("emath-check-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snapshot.emath");
    std::fs::write(&path, ORIGINAL).unwrap();
    SOURCE.set(path.clone()).unwrap();
    register_source_parser(&EditDuringParse);

    if let Ok(mode) = std::env::var("EMATH_SNAPSHOT_CHILD") {
        let invalid = mode == "invalid" || mode == "run-invalid";
        std::fs::write(&path, if invalid { EDITED } else { ORIGINAL }).unwrap();
        let exit = if mode == "run-invalid" {
            emath_cli::run(&[
                "run".into(),
                path.display().to_string(),
                "--function".into(),
                "Snapshot".into(),
                "--set".into(),
                "n=7".into(),
                "--json".into(),
            ])
        } else {
            emath_cli::check(&path, true, false)
        };
        assert_eq!(
            exit,
            if invalid {
                emath_cli::EXIT_ADMISSION
            } else {
                emath_cli::EXIT_OK
            }
        );
        return;
    }

    let (diagnostics, original_id, _) = emath_cli::run_check(&path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), EDITED);
    assert!(
        !diagnostics.has_errors(),
        "the loaded valid source must not be rejected by a later edit: {diagnostics:?}"
    );
    assert!(!original_id.is_empty());

    let (diagnostics, _, _) = emath_cli::run_check(&path);
    assert!(diagnostics.errors().any(|error| error.code == "E-TYPE-002"));

    // Compare identities only for admitted snapshots, not rejected packages.
    let (diagnostics, memory_id, _) = emath_cli::run_check_source("", ORIGINAL);
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    assert_eq!(original_id, memory_id);

    // Run the actual command in this executable so it uses the same parser
    // seam, while its stdout/stderr remain observable to this parent.
    for mode in ["valid", "invalid", "run-invalid"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "check_uses_loaded_source_snapshot",
                "--nocapture",
            ])
            .env("EMATH_SNAPSHOT_CHILD", mode)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        if mode == "valid" {
            let expected = emath_cli::admitted_meaning_id(&path, ORIGINAL).unwrap();
            assert!(
                stdout.contains(expected.as_str()),
                "successful check must identify its loaded snapshot: {stdout}"
            );
        } else if mode == "invalid" {
            assert!(
                stderr.contains("result = missing"),
                "diagnostics must render the rejected snapshot, not the disk replacement: {stderr}"
            );
        } else {
            assert!(
                stdout.contains("E-TYPE-002") && stdout.contains("\"admission\": \"refused\""),
                "run admission refusals must retain the structured JSON response: {stdout}"
            );
        }
    }
}

#[test]
fn query_body_fault_is_not_a_missing_entry() {
    register_source_parser(&EditDuringParse);
    let path = std::env::temp_dir().join(format!("emath-query-fault-{}.emath", std::process::id()));
    std::fs::write(
        &path,
        "emath query Broken:
    inputs:
        n: Int
    definitions:
        result = [n][1]
    question:
        target = quote(result)
    answer:
        form = value
",
    )
    .unwrap();
    assert_eq!(
        emath_cli::run(&[
            "run".into(),
            path.display().to_string(),
            "--function".into(),
            "Broken".into(),
            "--set".into(),
            "n=7".into(),
        ]),
        emath_cli::EXIT_FAULT,
        "an admitted query's out-of-bounds definition must report its execution fault"
    );
}

#[test]
fn execution_preserves_session_admission() {
    register_source_parser(&EditDuringParse);
    let dir = std::env::temp_dir().join(format!("emath-admission-reuse-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reserved.emath");
    std::fs::write(&path, ORIGINAL.replace("Snapshot", "_")).unwrap();
    // The constructor type checker alone accepts this declaration. The
    // session's retained name checks refuse it before any command executes.
    for command in ["check", "run", "test"] {
        let mut args = vec![command.to_string(), path.display().to_string()];
        if command == "run" {
            args.extend(["--function", "_", "--set", "n=7"].map(str::to_string));
        }
        assert_eq!(
            emath_cli::run(&args),
            emath_cli::EXIT_ADMISSION,
            "{command} must preserve the semantic session's reserved-name refusal"
        );
    }
}

#[test]
fn checked_module_retains_imports_and_isolates_runs() {
    use emath_exec_ir::constructor_layer::{CValue, DEFAULT_WORK};
    use std::collections::BTreeMap;
    register_source_parser(&EditDuringParse);
    let dir = std::env::temp_dir().join(format!("emath-prepared-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.emath");
    let helper = dir.join("helper.emath");
    let dependency = "package snapshot
exports: increment
emath function increment:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n + 40
";
    std::fs::write(&helper, dependency).unwrap();
    let source = "package snapshot
use snapshot.helper.{increment}
emath function Snapshot:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        cell = buffer(1, n)
        wrote = buffer_set(cell, 0, increment(n))
        result = if n < 0: cell[1] else: cell[0]
    tests:
        example <snapshot>:
            given n = 1
            expect result == 41
";
    let (tree, diagnostics) = emath_syntax::parse_str(source);
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let checked = emath_sema::admit::check_tree_at(&tree, Some(&path));
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics
    );
    let module = checked
        .prepared
        .expect("admission retains its resolved module");
    std::fs::write(&helper, dependency.replace("n + 40", "n + 900")).unwrap();
    let input = |n: i64| BTreeMap::from([("n".to_string(), CValue::Int(n.into()))]);
    for n in [1, 2, 1] {
        assert_eq!(
            module
                .evaluate_function("Snapshot", &input(n), DEFAULT_WORK)
                .unwrap()
                .to_string(),
            (n + 40).to_string(),
            "a later import edit and previous buffer writes must not change this module"
        );
    }
    assert_eq!(
        module
            .evaluate_function("Snapshot", &input(-1), DEFAULT_WORK)
            .unwrap_err()
            .code,
        "invalid_index"
    );
    assert_eq!(
        module
            .evaluate_function("Snapshot", &input(1), 0)
            .unwrap_err()
            .code,
        "budget_exhausted"
    );
    assert_eq!(
        module
            .evaluate_function("Snapshot", &input(1), DEFAULT_WORK)
            .unwrap()
            .to_string(),
        "41",
        "faults and work exhaustion must not poison the next run"
    );
    for _ in 0..2 {
        let report = module.evaluate_tests(DEFAULT_WORK).unwrap();
        assert_eq!(report.tests.len(), 1);
        assert!(report.tests[0].passed, "{report:?}");
    }
    let (fault, checkpoint) = module
        .evaluate_function_budgeted_reported("Snapshot", &input(1), 0, None, "snapshot", source)
        .unwrap_err();
    assert_eq!(fault.code, "budget_exhausted");
    let (value, _) = module
        .evaluate_function_budgeted_reported(
            "Snapshot",
            &input(1),
            DEFAULT_WORK,
            Some(&checkpoint),
            "snapshot",
            source,
        )
        .unwrap();
    assert_eq!(
        value.to_string(),
        "41",
        "resume must use this same imported snapshot"
    );
    let changed = emath_sema::admit::check_tree_at(&tree, Some(&path));
    assert!(
        !changed.diagnostics.has_errors(),
        "{:?}",
        changed.diagnostics
    );
    assert_eq!(
        changed
            .prepared
            .unwrap()
            .evaluate_function("Snapshot", &input(1), DEFAULT_WORK)
            .unwrap()
            .to_string(),
        "901",
        "a new check must see the new imported definition"
    );
}
