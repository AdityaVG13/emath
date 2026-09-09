//! CLI: expand / exactness / freeze / why / assumptions.
use emath_cli::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, plan_json_document};
use emath_cli_lab::{agent_check_json_document, agent_plan_json_document, agent_triage_json_document, exactness_json_document, expand_json_document, run};
use emath_syntax::{exactness_ledger, expand_scratch};
use emath_test_harness::{Probe, boot};
fn repo(rel: &str) -> String { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).to_string_lossy().into_owned() }
fn arr<'a>(v: &'a emath_artifact::JsonValue, k: &str) -> &'a [emath_artifact::JsonValue] { match v.field(k).unwrap_or_else(|_| panic!("{k}")) { emath_artifact::JsonValue::Arr(i) => i, o => panic!("{k} must be array, got {o:?}") } }
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("expand, exactness, freeze, plan, and agent envelopes keep schema and exits");
    let scratch = repo("tests/fixtures/language/intro/scratch.emath");
    p.case("expand-exactness", |p| {
        for a in [vec!["expand", scratch.as_str()], vec!["expand", scratch.as_str(), "--json"], vec!["exactness", scratch.as_str()], vec!["exactness", scratch.as_str(), "--json"], vec!["exactness", scratch.as_str(), "--raise", "units"], vec!["assumptions", scratch.as_str()], vec!["freeze", scratch.as_str()]] { p.eq(a.join(" "), run(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>()), EXIT_OK); }
        p.eq("why", run(&["why".into(), scratch.clone(), "inference:1".into()]), EXIT_OK);
        let src = std::fs::read_to_string(&scratch).expect("src"); let exp = expand_scratch(&src);
        let parsed = emath_artifact::parse_json_document(&expand_json_document(&src, &exp, None)).expect("expand json");
        p.eq("command", parsed.string_field("command").expect("c"), "expand".to_string()); p.eq("level", parsed.string_field("level").expect("l"), exp.level().as_str().to_string());
        p.demand("rewritten-bool", matches!(parsed.field("rewritten"), Ok(emath_artifact::JsonValue::Bool(_))), "rewritten bool"); p.demand("ok-bool", matches!(parsed.field("ok"), Ok(emath_artifact::JsonValue::Bool(_))), "ok bool");
        p.eq("notes", arr(&parsed, "notes").len(), exp.notes.len()); p.eq("holes", arr(&parsed, "holes").len(), exp.holes.len()); p.eq("menu", arr(&parsed, "solve_candidates").len(), exp.solve.menu().len());
        p.demand("diag-shape", arr(&parsed, "diagnostics").iter().all(|d| d.string_field("code").is_ok() && matches!(d.string_field("severity").ok().as_deref(), Some("error" | "warning" | "note")) && d.string_field("message").is_ok()), "diagnostics carry code/severity/message");
        let ledger = exactness_ledger(&src); let ep = emath_artifact::parse_json_document(&exactness_json_document(&ledger, None)).expect("exactness json");
        p.eq("e-command", ep.string_field("command").expect("c"), "exactness".to_string()); p.eq("entries", arr(&ep, "entries").len(), ledger.entries.len());
        p.demand("entry-shape", arr(&ep, "entries").iter().zip(&ledger.entries).all(|(r, e)| r.string_field("id").ok().as_deref() == Some(e.inference_id.as_str()) && r.string_field("dimension").ok().as_deref() == Some(e.dimension.as_str()) && r.string_field("status").ok().as_deref() == Some(e.status.as_str()) && r.string_field("name").ok().as_deref() == Some(e.name.as_str())), "ledger rows mirror (id, dimension, status, name)");
    });
    p.case("freeze-lock", |p| {
        let tmp = std::env::temp_dir().join("emath-freeze-check.emath");
        p.eq("exit", run(&["freeze".into(), scratch.clone(), "--out".into(), tmp.display().to_string(), "--json".into()]), EXIT_OK);
        let lock = std::fs::read_to_string(tmp.with_extension("freeze.lock.json")).expect("lock");
        for needle in ["emath.freeze.lock.v1", "emath:meaning:v1:", "\"schema\": \"emath.freeze.lock.v1\"", "\"authority_raised\": false", "\"source_content_id\"", "\"frozen_content_id\"", "strict-f64", "native.rust"] { p.contains(needle, &lock, needle); }
        p.demand("no-raise", !lock.contains("\"authority_raised\": true"), "never raises authority");
        let parsed = emath_artifact::parse_json_document(&lock).expect("lock json"); let (orig, frozen) = (std::fs::read_to_string(&scratch).expect("orig"), std::fs::read_to_string(&tmp).expect("frozen"));
        p.eq("schema", parsed.string_field("schema").expect("s"), "emath.freeze.lock.v1".to_string()); p.demand("no-command", parsed.field("command").is_err(), "lock is not the envelope");
        p.eq("src-id", parsed.string_field("source_content_id").expect("s"), emath_core::content_id_of_str(&orig).0); p.eq("frozen-id", parsed.string_field("frozen_content_id").expect("f"), emath_core::content_id_of_str(&frozen).0);
        p.ne("ids-differ", parsed.string_field("source_content_id").expect("s"), parsed.string_field("frozen_content_id").expect("f"));
        p.demand("meaning", parsed.string_field("meaning_id").expect("m").starts_with("emath:meaning:v1:"), "meaning carried"); p.eq("prelude", parsed.string_field("prelude").expect("p"), "scratch-v1".to_string()); p.eq("numeric", parsed.string_field("numeric_policy").expect("n"), "strict-f64".to_string());
        p.demand("arrays", ["packages", "methods", "providers", "open", "ledger"].iter().all(|k| matches!(parsed.field(k), Ok(emath_artifact::JsonValue::Arr(_)))), "array sections");
        p.eq("ledger", arr(&parsed, "ledger").len(), exactness_ledger(&orig).entries.len());
        p.demand("header", frozen.starts_with("# emath freeze: does not raise evidence authority\n"), "frozen header");
    });
    p.case("plan", |p| {
        let hello = repo("tests/fixtures/language/intro/hello-square.emath"); p.eq("exit", run(&["plan".into(), hello.clone(), "--json".into()]), EXIT_OK);
        let mut s = emath_sema::CompilerSession::new(emath_core::limits::Limits::default()); let pkg = s.load_package(std::path::Path::new(&hello)).expect("load"); let r = s.plan(pkg.file);
        p.demand("goals", !r.package.goals.is_empty(), "hello-square has goals");
        let parsed = emath_artifact::parse_json_document(&plan_json_document(!r.diagnostics.has_errors(), &r.package.goals, r.plans.len() as u64)).expect("plan json");
        p.eq("command", parsed.string_field("command").expect("c"), "plan".to_string()); p.demand("no-count", parsed.int_field("goals").is_err(), "goals is an object array");
        p.eq("len", arr(&parsed, "goals").len(), r.package.goals.len());
        p.demand("kinds", arr(&parsed, "goals").iter().zip(&r.package.goals).all(|(row, g)| row.string_field("kind").ok().as_deref() == Some(g.kind.as_str()) && row.string_field("target").ok().as_deref() == Some(g.target.as_str())), "goal (kind, target) rows");
    });
    p.case("agent", |p| {
        let hello = repo("tests/fixtures/language/intro/hello-square.emath"); p.eq("plan", run(&["agent".into(), "plan".into(), hello.clone()]), EXIT_OK);
        let mut s = emath_sema::CompilerSession::new(emath_core::limits::Limits::default()); let pkg = s.load_package(std::path::Path::new(&hello)).expect("load"); let r = s.plan(pkg.file);
        let ap = emath_artifact::parse_json_document(&agent_plan_json_document(!r.diagnostics.has_errors(), &r.package.goals, r.plans.len() as u64)).expect("agent plan");
        p.eq("schema", ap.string_field("schema").expect("s"), "emath.agent".to_string()); p.demand("no-count", ap.int_field("goals").is_err(), "object array"); p.eq("len", arr(&ap, "goals").len(), r.package.goals.len());
        let at = emath_artifact::parse_json_document(&agent_triage_json_document(&hello, true, &[], !r.diagnostics.has_errors(), &r.package.content_id().0, &r.diagnostics, true, None, &r.package.goals, r.plans.len() as u64)).expect("triage");
        p.demand("triage-count", at.int_field("goals").is_err(), "triage goals array"); p.eq("triage-len", arr(&at, "goals").len(), r.package.goals.len());
        let empty = std::env::temp_dir().join(format!("emath-agent-empty-{}.emath", std::process::id())); std::fs::write(&empty, "").expect("empty");
        p.eq("refused", run(&["agent".into(), "check".into(), empty.to_string_lossy().into_owned()]), EXIT_REFUSED);
        let mut s2 = emath_sema::CompilerSession::new(emath_core::limits::Limits::default()); let pkg2 = s2.load_package(&empty).expect("load empty"); let res2 = s2.check(pkg2.file);
        let ac = emath_artifact::parse_json_document(&agent_check_json_document(false, &res2.package.content_id().0, &res2.diagnostics)).expect("agent check");
        p.demand("diag-array", ac.int_field("diagnostics").is_err(), "diagnostics array"); p.demand("e-pkg-081", arr(&ac, "diagnostics").iter().any(|d| d.string_field("code").ok().as_deref() == Some("E-PKG-081")), "surfaces E-PKG-081");
        let _ = std::fs::remove_file(&empty);
    });
    p.case("freeze-negatives", |p| { p.eq("hole", run(&["freeze".into(), repo("tests/invalid/exactness_introspection.emath")]), EXIT_REFUSED); let dir = std::env::temp_dir().join("emath-partial-freeze-check"); let _ = std::fs::create_dir_all(&dir); let out = dir.join("frozen.emath"); let lock = out.with_extension("freeze.lock.json"); let _ = std::fs::remove_file(&out); let _ = std::fs::remove_dir_all(&lock); std::fs::create_dir_all(&lock).expect("lock as dir"); p.eq("usage", run(&["freeze".into(), scratch.clone(), "--out".into(), out.display().to_string()]), EXIT_USAGE); p.demand("removed", !out.exists(), "partial source removed"); let _ = std::fs::remove_dir_all(&lock); let _ = std::fs::remove_dir_all(&dir); });
    p.finish();
}
