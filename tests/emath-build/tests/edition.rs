//! edition tests migrated from the in-crate `#[cfg(test)]` module.
use emath_build::edition::*;
use emath_core::Edition;
use emath_core::E_PKG_EDITION_UNKNOWN;
use emath_test_harness::{Case, Probe, check_all};
use std::path::Path;

#[test]
fn probe() {
    let mut p = Probe::new("edition field parses 2026, first wins, and refuses unknown/missing with codes");
    if let Err(m) = check_all(
        &[
            Case::new("bare", "edition = \"2026\"\n", Ok(Edition::Ed2026)),
            Case::new("table", "[package]\n  edition=\"2026\"  # founding epoch\n", Ok(Edition::Ed2026)),
            Case::new("first-wins", "edition = \"2026\"\nedition = \"2030\"\n", Ok(Edition::Ed2026)),
        ],
        |s| parse_edition_field(*s),
    ) {
        p.fail("parse", m);
    } else {
        p.demand("parse", true, "ok");
    }
    p.case("refusals", |p| {
        p.eq("unknown-code", parse_edition_field("edition = \"2099\"\n").expect_err("2099").code(), E_PKG_EDITION_UNKNOWN);
        p.eq("missing", parse_edition_field("[package]\nname = \"x\"\n"), Err(ManifestEditionError::Missing));
        p.eq("unquoted-code", parse_edition_field("edition = 2026\n").expect_err("unquoted").code(), E_PKG_EDITION_UNKNOWN);
        p.eq("unreadable", manifest_edition(Path::new("/nonexistent/emath.toml")).expect_err("missing").code(), "E-PKG-MANIFEST-UNREADABLE");
    });
    p.finish();
}
