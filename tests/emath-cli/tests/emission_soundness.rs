//! Emitter soundness regressions: three lowering defects that made
//! library code pass all VM tests yet emit Rust that faults at runtime
//! or fails to compile (bead emath-k4fmh).
//!
//! 1. `and`/`or` right operands lowered eagerly: a guard like
//!    `k < n and options[k].cost >= 0 and prev[b - cost].ok` read
//!    `prev[b - cost]` unconditionally and faulted ("vector index out
//!    of bounds") on every non-empty input. The engine short-circuits,
//!    so the right arm's exclusively-fed registers now splice into a
//!    block: `__e_left && { let __e_k = ..; ..; __e_right }`.
//! 2. `.numer`/`.denom` off a Rat lowered as struct field access on
//!    the `(i128, i128)` tuple carrier (E0609 no field `numer`). The
//!    projection now reads the tuple part and widens to the exact
//!    integer carrier, mirroring the VM's `CValue::Rat` parts.
//! 3. Mixed Int/Rat arithmetic with ExactInt-backed values
//!    (`int_binom` results) emitted `exact_ratio()` raw tuple ops
//!    (E0369: cannot multiply `(ExactInt, ExactInt)` by `ExactInt`).
//!    Every operand now widens to the one `(i128, i128)` ratio
//!    representation through a checked `to_i128` bridge.
//!
//! Value-level acceptances were proven by compiling and running the
//! emitted crates when the fixes landed: `allocate_best` returns
//! Ok(5/1) on a feasible input, `rat_pow(2/1, 3) == 8/1`, and
//! `fisher_exact([[3,1],[1,3]]) == 17/70` exactly. These source pins
//! keep each lowering shape from regressing without invoking cargo
//! here; each assertion fails against the pre-fix emitter (the
//! miscompiled shapes above).
//!
//! 6pw20 batch (the same soundness class, surfaced by the emission
//! conformance lanes on analysis/powers and probability/distributions):
//! (4) `not` over an order comparison lowered as
//!     `!x.cmp(&y) == Ordering::Less` — unary `!` bound the
//!     `Ordering` (E0600); the negation now parenthesizes the whole
//!     comparison.
//! (5) branch arms in mixed representations (an i64 literal arm
//!     beside an exact machine-root arm) emitted E0308 if/else
//!     pairs; the kind walk joins the pair onto the exact carrier
//!     and the render widens the i64 arm through `ExactInt::from`.
//! (6) Rat == Int-literal equality cast the Int side through f64
//!     (E0308 against the `(i128, i128)` tuple carrier); the Int
//!     side now widens to canonical ratio parts and compares
//!     exactly (equality over tuples, order through `ratio_lt`).
//! Value acceptance for this batch: the emitted powers crate runs
//! 13 pins green (floor_root family incl. the widened `0` arm and
//! the clamped bounds, `pow_int(2, 300)` exact decimal through the
//! Big tail, i64 lanes unchanged); the emitted distributions crate
//! compiles with the exact equality in place.

mod common;
use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn build(path: &std::path::Path, args: &[&str]) -> (String, i32) {
    let output = std::process::Command::new(common::emath_bin())
        .arg("build")
        .arg(path)
        .args(args)
        .output()
        .expect("run emath build");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.code().unwrap_or(-1),
    )
}

fn scratch(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("emath-emission-{}-{}", tag, std::process::id()))
}

const ALLOC_CALL: &str = "use optimization.allocate

emath function smoke:
    inputs:
        budget: Int
    outputs:
        result: Rat
    definitions:
        plan = allocate_best([[AllocOption: {cost: 2, value: 5/1}, AllocOption: {cost: 1, value: 3/1}]], budget)
        result = if plan.ok: plan.value else: 0/1
    tests:
        example <smoke_picks_best>:
            given budget = 2
            expect result == 5/1
";

const RAT_POW_CALL: &str = "use analysis.powers

emath function probe_rat_pow_call:
    inputs:
        p: Rat
        n: Int
    outputs:
        result: Rat
    definitions:
        result = rat_pow(p, n)
    tests:
        example <pow_two_cubed>:
            given p = 2 / 1
            given n = 3
            expect result == 8 / 1
";

const HYPER_CALL: &str = "use probability.distributions

emath function probe_hyper_call:
    inputs:
        k: Int
        n: Int
        big_n: Int
        big_k: Int
    outputs:
        result: Rat
    definitions:
        result = hypergeometric_pmf(k, n, big_n, big_k)
    tests:
        example <two_aces_of_two_draws>:
            given k = 2
            given n = 2
            given big_n = 4
            given big_k = 2
            expect result == 1 / 6
";

#[test]
fn probe() {
    let mut p = Probe::new("emission soundness: short-circuit and/or, Rat part projection, one-representation mixed arithmetic");

    p.case("and-guard-defers-right-arm-index-reads", |p| {
        let src = scratch("alloc-src");
        let out = scratch("alloc-out");
        std::fs::write(&src, ALLOC_CALL).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.demand(
            "right arm deferred behind a block",
            lib.contains("&& { let __"),
            "the and-guard's exclusively-fed right-arm registers move into a block after `&&` (any register namespace: __e, __controlN_e), so `prev[b - cost]` reads only when the left arms decide",
        );
    });

    p.case("rat-parts-project-tuple-not-struct", |p| {
        let src = scratch("ratpow-src");
        let out = scratch("ratpow-out");
        std::fs::write(&src, RAT_POW_CALL).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "numer widens through the exact-integer carrier",
            &lib,
            "emath_rt::ExactInt::from((",
        );
        p.demand(
            "no struct field projection off the tuple",
            !lib.contains(".numer") && !lib.contains(".denom"),
            "`.numer`/`.denom` must read the `(i128, i128)` tuple parts, never struct field access (E0609)",
        );
    });

    p.case("mixed-int-rat-one-representation", |p| {
        let src = scratch("hyper-src");
        let out = scratch("hyper-out");
        std::fs::write(&src, HYPER_CALL).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "exact integers widen through the checked i128 bridge",
            &lib,
            ".to_i128().ok_or_else",
        );
        p.demand(
            "no exact_ratio tuple leak",
            !lib.contains("emath_rt::exact_ratio("),
            "mixed Int/Rat arithmetic meets on the `(i128, i128)` ratio carrier; `exact_ratio` returns `(ExactInt, ExactInt)` and miscompiles into raw tuple ops (E0369)",
        );
    });

    p.finish();
}

#[test]
fn probe_exact_boundaries() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut p = Probe::new("emission soundness: parenthesized negated comparisons, branch arm representation join, exact Rat/Int equality");

    p.case("negated-comparison-parenthesized", |p| {
        let out = scratch("powers-out");
        let (text, code) = build(
            &root.join("language/modules/analysis/powers.emath"),
            &["--out", &out.to_string_lossy()],
        );
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "negated order comparison parenthesizes the whole comparison",
            &lib,
            "Ordering::Less)",
        );
        p.contains(
            "i64 branch arm widens to the exact carrier",
            &lib,
            "emath_rt::ExactInt::from((0i64))",
        );
    });

    p.case("rat-int-equality-never-f64", |p| {
        let out = scratch("dist-out");
        let (text, code) = build(
            &root.join("language/modules/probability/distributions.emath"),
            &["--out", &out.to_string_lossy()],
        );
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "Int side widens to canonical ratio parts",
            &lib,
            "as i128), 1i128",
        );
        p.demand(
            "no equality against a parenthesized f64 cast",
            !lib.contains(" == (__"),
            "Rat == Int-literal equality compares exact ratio parts (the `(i128, i128)` carrier), never the blanket `== (__expr) as f64` cast, which is E0308 against the tuple and silently inexact where it compiled",
        );
    });

    p.finish();
}
