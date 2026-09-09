use emath_core::{Edition, limits::Limits};
use emath_sema::CompilerSession;

const LEGACY_EXAMPLE: &str = "y = x^2 + 4\nexample x = 3\n";

use emath_test_harness::{Probe, boot};

#[test]
fn edition_lifecycle() {
    boot();
    let mut probe = Probe::new("edition_lifecycle syntax surface");
    probe.case("manifest_edition_selects_deprecation_policy_and_home_replay", |p| {
    let f0 = p.failures().len();


    let mut home = CompilerSession::with_edition(Limits::default(), Edition::Ed2026);
    let first = home.check_owned("legacy-example", LEGACY_EXAMPLE);
    p.demand("1",!first.diagnostics.has_errors(), stringify!(!first.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.demand("2",first
            .diagnostics
            .items()
            .iter()
            .any(|diagnostic| diagnostic.code == "W-EDITION-DEPRECATED"), stringify!(first
            .diagnostics
            .items()
            .iter()
            .any(|diagnostic| diagnostic.code == "W-EDITION-DEPRECATED")));
    if p.failures().len() != f0 { return; }

    let replay = home.check_owned("legacy-example-replay", LEGACY_EXAMPLE);
    p.demand("3",!replay.diagnostics.has_errors(), stringify!(!replay.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.eq("4", first.package.meaning_id(&[]).expect("first meaning"), replay.package.meaning_id(&[]).expect("replayed meaning"));

    let mut current = CompilerSession::with_edition(Limits::default(), Edition::Ed2030);
    let hidden = current.check_owned("legacy-example", LEGACY_EXAMPLE);
    p.demand("5",hidden
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-EDITION-HIDDEN"), stringify!(hidden
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-EDITION-HIDDEN")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("deprecated_example_has_verified_migration_and_edition_is_not_meaning", |p| {
    let f0 = p.failures().len();

    let lossless =
        emath_syntax::parse_lossless(LEGACY_EXAMPLE, emath_core::FileId(0), &Limits::default());
    let rewritten = emath_syntax::format_lossless(&lossless);
    p.demand("1", rewritten != LEGACY_EXAMPLE, "canonical rewrite must differ from the legacy spelling");
    let migrated = emath_sema::migrate::migrate_verified_rewrite(
        "legacy-example",
        LEGACY_EXAMPLE,
        &rewritten,
        emath_sema::migrate::RULE_CANONICAL_FORMAT.id,
    );
    p.demand("2", (migrated.receipt.verdict) == ("complete"), format!("expected {:?}, got {:?}", ("complete"), (migrated.receipt.verdict)));
    if p.failures().len() != f0 { return; }
    p.eq("3", migrated.rewritten_source.as_deref(), Some(rewritten.as_str()));

    let modern = "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
    let mut old = CompilerSession::with_edition(Limits::default(), Edition::Ed2026);
    let mut new = CompilerSession::with_edition(Limits::default(), Edition::Ed2030);
    let old = old.check_owned("modern-old", modern);
    let new = new.check_owned("modern-new", modern);
    p.demand("4",!old.diagnostics.has_errors() && !new.diagnostics.has_errors(), stringify!(!old.diagnostics.has_errors() && !new.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.eq("5", old.package.meaning_id(&[]).expect("2026 meaning"), new.package.meaning_id(&[]).expect("2030 meaning"));

    });
    probe.case("package_manifest_drives_the_session_edition_and_unknown_refuses", |p| {
    let f0 = p.failures().len();

    let root = std::env::temp_dir().join(format!(
        "emath-edition-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(&root).expect("create fixture directory");
    let source_path = root.join("main.emath");
    std::fs::write(&source_path, LEGACY_EXAMPLE).expect("write source");

    std::fs::write(root.join("emath.toml"), "edition = \"2030\"\n").expect("write manifest");
    let mut session = CompilerSession::new(Limits::default());
    session.load_package(&source_path).expect("2030 ships");
    p.eq("1", session.edition(), Edition::Ed2030);
    let hidden = session.check_owned("manifest-selected", LEGACY_EXAMPLE);
    p.demand("2",hidden.diagnostics.has_errors(), stringify!(hidden.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    std::fs::write(root.join("emath.toml"), "edition = \"2099\"\n")
        .expect("write unknown manifest");
    let mut unknown = CompilerSession::new(Limits::default());
    let error = unknown
        .load_package(&source_path)
        .expect_err("unknown editions refuse");
    p.demand("3",error.contains("E-PKG-EDITION-UNKNOWN"), format!( "{error}"));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
