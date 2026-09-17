//! Sequence-passage wall band (bead emath-g9rpo): a 1000-element
//! sequence flowing through ~13k recursive calls must stay an
//! O(1)-per-step argument cost. Failure-first: with the deep-copy
//! carrier this exact shape measured 17.4s (PE P8); the band is 6s.

mod common;
use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("big-sequence passage stays cheap");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/constructor/sequence_passage.emath"
    );
    let start = std::time::Instant::now();
    let (text, code) = common::cli(&["test", fixture, "--work", "5000000"]);
    let elapsed = start.elapsed().as_secs_f64();
    p.eq("exit", code, EXIT_OK as i32);
    p.contains("rows passed", &text, "2 authored tests passed");
    p.demand(
        "wall band",
        elapsed < 6.0,
        format!(
            "sequence passage took {elapsed:.1}s; the COW carrier must keep \
             big-sequence calls O(1) per step (deep-copy baseline: 17.4s)"
        ),
    );
    p.finish();
}
