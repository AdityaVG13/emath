//! Meaning-lock / genesis are extracted tokens, not constructor commands.
use emath_cli::{EXIT_USAGE};
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("meaning and genesis refuse; constructor kinds do not lock worlds");
    p.eq("meaning-help", run(&args(&["meaning", "--help"])), EXIT_USAGE);
    p.eq("meaning-bare", run(&args(&["meaning"])), EXIT_USAGE);
    p.eq(
        "meaning-set",
        run(&args(&["meaning", "set", "glyphs.emath", "--world", "Boolean_algebra"])),
        EXIT_USAGE,
    );
    p.eq("genesis", run(&args(&["genesis", "glyphs.emath", "--out", "out"])), EXIT_USAGE);
    p.eq(
        "genesis-help",
        run(&args(&["help", "genesis"])),
        EXIT_USAGE,
    );
    p.finish();
}
