//! Meaning-lock E2E: portfolio → set → single-world → unset → portfolio, isolation, refusals.
use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};
use std::path::{Path, PathBuf};
fn args(parts: &[&str]) -> Vec<String> { parts.iter().map(|s| s.to_string()).collect() }
fn glyphs() -> &'static str { "emath custom AlienGlyphs:\n    body:\n        ⧖(a ⋈ b) ⊛ ζ\n\n    construct meaning:\n        explore:\n            free_symbolic\n            Boolean_algebra\n            modular_numeric\n\n        protect:\n            total\n            deterministic\n\n        keep:\n            pareto 8\n\n    answer:\n        return interpretation_portfolio\n" }
fn drifted() -> &'static str { "emath custom AlienGlyphs:\n    body:\n        ⧖(b ⋈ a) ⊛ ζ\n\n    construct meaning:\n        explore:\n            free_symbolic\n            Boolean_algebra\n            modular_numeric\n\n        protect:\n            total\n            deterministic\n\n        keep:\n            pareto 8\n\n    answer:\n        return interpretation_portfolio\n" }
fn scratch(name: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("emath-mlock-{name}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0))); std::fs::create_dir_all(&d).expect("dir"); d }
fn write_glyphs(dir: &Path, body: &str) -> PathBuf { let f = dir.join("glyphs.emath"); std::fs::write(&f, body).expect("write"); f }
fn worlds(body: &str) -> usize { body.matches("\"world_id\"").count() }
fn genesis(file: &Path, out: &Path) -> CliExit { run(&args(&["genesis", &file.display().to_string(), "--out", &out.display().to_string()])) }
fn fp(lock: &str) -> String { let m = "\"world_fingerprint\": \""; let s = lock.find(m).expect("fp") + m.len(); lock[s..s + 16].to_string() }
fn lock_world(receipt: &str) -> String { let m = "\"lock_world\": \""; let s = receipt.find(m).expect("lw") + m.len(); receipt[s..s + 16].to_string() }
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("meaning lock round-trips, isolates users, refuses tamper and drift");
    p.case("help", |p| { p.eq("flag", run(&args(&["meaning", "--help"])), EXIT_OK); p.eq("help-cmd", run(&args(&["help", "meaning"])), EXIT_OK); p.eq("bare", run(&args(&["meaning"])), EXIT_USAGE); });
    p.case("roundtrip", |p| {
        let dir = scratch("roundtrip"); let file = write_glyphs(&dir, glyphs());
        p.eq("genesis", genesis(&file, &dir.join("out1")), EXIT_OK);
        let pf1 = std::fs::read_to_string(dir.join("out1/interpretation-portfolio.json")).unwrap();
        p.demand("portfolio", worlds(&pf1) >= 5, "unlocked keeps portfolio");
        p.demand("no-lock", !std::fs::read_to_string(dir.join("out1/answer-receipt.json")).unwrap().contains("user-locked"), "unlocked receipt");
        p.eq("set", run(&args(&["meaning", "set", &file.display().to_string(), "--world", "Boolean_algebra", "--dir", &dir.display().to_string()])), EXIT_OK);
        p.demand("lockfile", dir.join(".emath/meaning.lock").is_file(), "lock written");
        p.eq("genesis-locked", genesis(&file, &dir.join("out2")), EXIT_OK);
        let (pf2, r2) = (std::fs::read_to_string(dir.join("out2/interpretation-portfolio.json")).unwrap(), std::fs::read_to_string(dir.join("out2/answer-receipt.json")).unwrap());
        p.eq("single", worlds(&pf2), 1); p.contains("prov", &r2, "\"meaning_provenance\": \"user-locked\""); p.contains("auth", &r2, "\"authority\": \"structural\"");
        p.eq("unset", run(&args(&["meaning", "unset", "--dir", &dir.display().to_string()])), EXIT_OK);
        p.eq("genesis3", genesis(&file, &dir.join("out3")), EXIT_OK);
        let (pf3, r3) = (std::fs::read_to_string(dir.join("out3/interpretation-portfolio.json")).unwrap(), std::fs::read_to_string(dir.join("out3/answer-receipt.json")).unwrap());
        p.demand("restored", worlds(&pf3) >= 5, "unset restores portfolio"); p.demand("ranked", !r3.contains("user-locked"), "unset receipt ranked");
    });
    p.case("isolation", |p| {
        let (l, r) = (scratch("a"), scratch("b")); let (lf, rf) = (write_glyphs(&l, glyphs()), write_glyphs(&r, glyphs()));
        p.eq("set-a", run(&args(&["meaning", "set", &lf.display().to_string(), "--world", "Boolean_algebra", "--dir", &l.display().to_string()])), EXIT_OK);
        p.eq("set-b", run(&args(&["meaning", "set", &rf.display().to_string(), "--world", "modular_numeric", "--dir", &r.display().to_string()])), EXIT_OK);
        p.eq("gen-a", genesis(&lf, &l.join("out")), EXIT_OK); p.eq("gen-b", genesis(&rf, &r.join("out")), EXIT_OK);
        let (a, b) = (std::fs::read_to_string(l.join("out/answer-receipt.json")).unwrap(), std::fs::read_to_string(r.join("out/answer-receipt.json")).unwrap());
        p.demand("both-locked", a.contains("user-locked") && b.contains("user-locked"), "both receipts locked");
        p.ne("worlds-differ", lock_world(&a), lock_world(&b));
    });
    p.case("shadow", |p| { let dir = scratch("shadow"); let file = write_glyphs(&dir, glyphs()); p.eq("set", run(&args(&["meaning", "set", &file.display().to_string(), "--world", "Boolean_algebra", "--dir", &dir.display().to_string()])), EXIT_OK); let nested = dir.join("nested"); std::fs::create_dir_all(nested.join(".emath")).expect("decoy"); let nf = write_glyphs(&nested, glyphs()); p.eq("gen", genesis(&nf, &nested.join("out")), EXIT_OK); p.eq("single", worlds(&std::fs::read_to_string(nested.join("out/interpretation-portfolio.json")).unwrap()), 1); });
    p.case("negatives", |p| {
        let dir = scratch("neg"); let file = write_glyphs(&dir, glyphs());
        p.eq("set", run(&args(&["meaning", "set", &file.display().to_string(), "--world", "Boolean_algebra", "--dir", &dir.display().to_string()])), EXIT_OK);
        let lock = dir.join(".emath/meaning.lock"); let orig = std::fs::read_to_string(&lock).unwrap();
        std::fs::write(&lock, orig.replacen(&fp(&orig), "aaaaaaaaaaaaaaaa", 1)).unwrap(); p.eq("tamper", genesis(&file, &dir.join("t-out")), EXIT_REFUSED);
        std::fs::write(&lock, &orig).unwrap(); std::fs::write(&file, drifted()).unwrap(); p.eq("drift", genesis(&file, &dir.join("d-out")), EXIT_REFUSED);
        std::fs::write(&lock, "{not json").unwrap(); p.eq("malformed", genesis(&file, &dir.join("m-out")), EXIT_REFUSED);
    });
    p.case("dup-flags", |p| { p.eq("world", run(&args(&["meaning", "set", "a.emath", "--world", "one_point", "--world", "Boolean_algebra"])), EXIT_USAGE); p.eq("dir", run(&args(&["meaning", "list", "--dir", "a", "--dir", "b"])), EXIT_USAGE); p.eq("hole", run(&args(&["meaning", "unset", "--hole", "x", "--hole", "y"])), EXIT_USAGE); p.eq("explain", run(&args(&["meaning", "explain", "a.emath", "--dir", "a", "--dir", "b"])), EXIT_USAGE); });
    p.finish();
}
