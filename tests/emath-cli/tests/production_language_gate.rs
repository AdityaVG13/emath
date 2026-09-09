//! Production semantic commands require the verified checked-in language distribution.
mod common;
use emath_test_harness::Probe;
use std::path::{Path, PathBuf};
fn copy_tree(source: &Path, dest: &Path) { std::fs::create_dir_all(dest).expect("dest"); for e in std::fs::read_dir(source).expect("read") { let e = e.expect("entry"); let (s, d) = (e.path(), dest.join(e.file_name())); if s.is_dir() { if e.file_name() == "target" { continue; } copy_tree(&s, &d); } else { std::fs::copy(&s, &d).expect("copy"); } } }
fn fixture() -> PathBuf { let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_nanos(); let root = std::env::temp_dir().join(format!("emath-prod-gate-{}-{n}", std::process::id())); copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language"), &root.join("language")); root }
fn check(root: &Path) -> std::process::Output { std::process::Command::new(common::emath_bin()).args(["check", "tests/fixtures/language/intro/clamp-distance-builtins.emath", "--json"]).current_dir(root).output().expect("check") }
fn text(o: &std::process::Output) -> String { format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)) }
#[test]
fn probe() {
    let mut p = Probe::new("tampered language image, missing source map, or seeded hole refuses with E-LANG-IMAGE");
    let root = fixture();
    p.case("clean", |p| { let c = check(&root); p.demand("ok", c.status.success(), text(&c)); p.contains("admitted", &String::from_utf8_lossy(&c.stdout).into_owned(), "\"admitted\": true"); });
    p.case("tampered-lock", |p| { let lock = root.join("language/language.lock"); let clean = std::fs::read_to_string(&lock).expect("lock"); std::fs::write(&lock, format!("{clean}tampered=true\n")).expect("tamper"); let t = check(&root); p.eq("exit", t.status.code(), Some(1)); p.contains("code", &text(&t), "E-LANG-IMAGE"); std::fs::write(&lock, clean).expect("restore"); });
    p.case("missing-map", |p| { let (m, h) = (root.join("language/generated/source-map.lock"), root.join("language/generated/source-map.lock.missing")); std::fs::rename(&m, &h).expect("hide"); let t = check(&root); p.eq("exit", t.status.code(), Some(1)); p.contains("code", &text(&t), "E-LANG-IMAGE"); std::fs::rename(&h, &m).expect("restore"); });
    p.case("seeded-hole", |p| { let cap = root.join("language/spec/capabilities/core/add.emath"); let clean = std::fs::read_to_string(&cap).expect("cap"); let holed = clean.replace("projection: \"semantics -> provided\"", "projection: \"semantics -> hole(hidden-active-hole | seeded refusal)\""); p.ne("seeded", holed.clone(), clean.clone()); std::fs::write(&cap, &holed).expect("seed"); let t = check(&root); p.eq("exit", t.status.code(), Some(1)); p.contains("code", &text(&t), "E-LANG-IMAGE"); std::fs::write(&cap, &clean).expect("restore"); });
    p.case("restored", |p| { let r = check(&root); p.demand("ok", r.status.success(), text(&r)); });
    p.finish();
}
