//! Emission conformance harness (emath-9sw3v): authored module tests re-run
//! against `emath build` output.
//!
//! Per module lane:
//! 1. VM truth: `emath test <module>` must pass (the authored rows are the
//!    ground truth, already pinned in the VM).
//! 2. Emit: `emath build <module> --out <scratch> --json` must report
//!    `runnable: true` with no unresolved entries.
//! 3. Compile the emitted crate offline (generated Cargo.toml has no
//!    dependencies; the runtime is embedded as `mod emath_rt`).
//! 4. Drive the EMITTED entry functions with the authored givens and
//!    authored expectations; every row diffs its emitted result against
//!    the VM value and prints a named diff on mismatch:
//!    `EMITTED f(args) = X; VM = Y`.
//!
//! The drivers are transcribed from the authored `tests:` blocks of
//! `language/modules/optimization/allocate.emath`,
//! `language/modules/probability/inference.emath`,
//! `language/modules/probability/distributions.emath`,
//! `language/modules/analysis/powers.emath`,
//! `language/modules/exact/residues.emath`, `language/domains/cryptology/roundtrip.emath`,
//! `language/modules/exact/quadratic.emath`,
//! `language/modules/numerics/bounds.emath`,
//! `language/modules/geometry/surfaces.emath`,
//! `language/domains/mechanics/oscillations.emath`,
//! `language/modules/discrete/order.emath` (the mutual-recursion
//! cycle pins), `language/modules/discrete/matching.emath` (the
//! mixed-kind join pins), `language/domains/research/
//! structural_spike.emath` (the code-template-carrier pins),
//! `language/modules/analysis/evolution/composition.emath` (the
//! shared-state record + curried-closure pins), and
//! `language/modules/analysis/evolution/stiff.emath` (the implicit-solve +
//! adaptive-walk pins), and `language/modules/algebra/linalg.emath`
//! (the substitute-seam + sparse/dense/hessian pins), and
//! `language/modules/analysis/evolution/events.emath` (the event-data
//! + closure-field pins), `language/modules/search/rewriting.emath`
//! (the scalar-make + substitute-carrier pins), and
//! `language/modules/numerics/levenberg.emath` (the curried-closure
//! model-seam pins), `language/modules/quote/walk.emath` (the
//! Fragment + dynamic-sequence node-family pins),
//! `language/modules/search/discovery.emath` (the applied-evaluate
//! pins), and `language/modules/analysis/derivative.emath` (the
//! derived-derivative-into-Newton pins) — the same givens,
//! the same expectations, executed against emitted code instead of the VM.
//! Refusal rows (`expect diagnostic.code == <atom>`) pin the emitted
//! `Err(<atom>)` strings exactly. Labeled transcription boundaries live
//! in the driver headers (cross-module bracket rows, i64 ABI limits).
//! The witness lane drives `tests/fixtures/constructor/`
//! `operator_tag_witness.emath` — its quoted forms are built as
//! distilled trees (the same shapes `quote(...)` distills), one row
//! per authored witness example.

use std::path::PathBuf;
use std::process::Command;

use common::cli;

mod common;

/// Workspace root (tests/emath-cli/tests -> tests/emath-cli -> repo).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Full conformance lane for one module. Returns the named diff text for
/// every failing row (empty when emitted == VM on all pins).
///
/// The emitted crate lives in the module's stable dir (shared with the
/// emission census, see `common::emitted_crate_dir`): an unchanged emission
/// and driver re-run as a cargo no-op, a changed one recompiles
/// incrementally.
fn run_lane(module_rel: &str, driver_src: &str) -> Vec<String> {
    let module = repo_root().join(module_rel);

    // 1. VM truth: the authored rows pass in the VM.
    let (text, code) = cli(&["test", module.to_str().expect("utf8 module path")]);
    assert_eq!(
        code, 0,
        "authored module must pass its own tests in the VM first\n{text}"
    );

    // 2. Emit: runnable, nothing unresolved.
    let out = common::emitted_crate_dir(&module);
    let _lock = common::lock_emitted_crate(&out);
    let (text, code) = cli(&[
        "build",
        module.to_str().expect("utf8 module path"),
        "--out",
        out.to_str().expect("utf8 scratch path"),
        "--json",
    ]);
    assert_eq!(code, 0, "emath build failed\n{text}");
    assert!(
        text.contains("\"runnable\": true"),
        "emitted crate must be runnable\n{text}"
    );
    assert!(
        text.contains("\"unresolved\": []"),
        "emitted crate must have no unresolved entries\n{text}"
    );

    // 3. Conformance driver: a src/bin that links the emitted lib and runs
    //    the authored pins against the emitted entries.
    let bin_dir = out.join("src").join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create src/bin");
    common::write_if_changed(&bin_dir.join("conf_driver.rs"), driver_src);

    // 4. Compile the emitted crate offline (no deps by construction).
    let status = common::cargo_build_emitted(&out, &["--bin", "conf_driver"]);
    assert!(
        status.status.success(),
        "emitted crate must compile: {}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );

    // 5. Run the driver; collect named diffs.
    let run = Command::new(out.join("target").join("debug").join("conf_driver"))
        .output()
        .expect("run conformance driver");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "conformance driver must run cleanly\n{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    stdout
        .lines()
        .filter(|line| line.starts_with("DIFF "))
        .map(|line| line.trim_start_matches("DIFF ").to_string())
        .collect()
}

const ALLOCATE_DRIVER: &str = r#"
//! Authored pins of optimization/allocate.emath against the emitted crate.

fn q_eq(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 == b.0 * a.1
}

use allocate::{allocate_best, dominates, pareto_frontier};
use allocate::EmathRecord_AllocOption as Opt;
use allocate::EmathRecord_ParetoPoint as Pt;

fn main() {
    let mut failures = 0usize;

    // example <alloc_two_groups_under_budget_takes_best_joint_plan>
    let got = allocate_best(
        vec![
            vec![Opt { cost: 2, value: (5, 1) }, Opt { cost: 1, value: (3, 1) }],
            vec![Opt { cost: 1, value: (2, 1) }, Opt { cost: 3, value: (9, 1) }],
        ],
        3,
    );
    // VM: ok == true, picks == [0, 0], value == 7/1
    match &got {
        Ok(o) if o.ok && o.picks == vec![0, 0] && q_eq(o.value, (7, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_two_groups_under_budget_takes_best_joint_plan EMITTED {:?}; VM = ok:true picks:[0,0] value:7/1", got);
        }
    }

    // example <alloc_leftover_budget_prefers_cheaper_better_option>
    let got = allocate_best(vec![vec![Opt { cost: 3, value: (1, 1) }, Opt { cost: 2, value: (5, 1) }]], 3);
    match &got {
        Ok(o) if o.ok && o.picks == vec![1] && q_eq(o.value, (5, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_leftover_budget_prefers_cheaper_better_option EMITTED {:?}; VM = ok:true picks:[1] value:5/1", got);
        }
    }

    // example <alloc_first_option_wins_exact_value_ties>
    let got = allocate_best(vec![vec![Opt { cost: 2, value: (5, 1) }, Opt { cost: 2, value: (5, 1) }]], 2);
    match &got {
        Ok(o) if o.ok && o.picks == vec![0] => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_first_option_wins_exact_value_ties EMITTED {:?}; VM = ok:true picks:[0]", got);
        }
    }

    // example <alloc_infeasible_budget_refuses>
    let got = allocate_best(vec![vec![Opt { cost: 5, value: (10, 1) }]], 3);
    match &got {
        Ok(o) if !o.ok && o.picks.is_empty() => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_infeasible_budget_refuses EMITTED {:?}; VM = ok:false picks:[]", got);
        }
    }

    // example <alloc_negative_budget_refuses>
    let got = allocate_best(vec![vec![Opt { cost: 1, value: (1, 1) }]], -1);
    match &got {
        Ok(o) if !o.ok => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_negative_budget_refuses EMITTED {:?}; VM = ok:false", got);
        }
    }

    // example <alloc_zero_groups_is_the_empty_ok_plan>
    let got = allocate_best(vec![], 4);
    match &got {
        Ok(o) if o.ok && o.picks.is_empty() && q_eq(o.value, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_zero_groups_is_the_empty_ok_plan EMITTED {:?}; VM = ok:true picks:[] value:0/1", got);
        }
    }

    // example <alloc_empty_group_refuses>
    let got = allocate_best(vec![vec![]], 4);
    match &got {
        Ok(o) if !o.ok => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_empty_group_refuses EMITTED {:?}; VM = ok:false", got);
        }
    }

    // example <alloc_exact_rat_values_not_floats>: 1/3 + 1/3 == 2/3 exactly
    let got = allocate_best(
        vec![
            vec![Opt { cost: 1, value: (1, 3) }],
            vec![Opt { cost: 1, value: (1, 3) }],
        ],
        2,
    );
    match &got {
        Ok(o) if o.ok && q_eq(o.value, (2, 3)) => {}
        _ => {
            failures += 1;
            println!("DIFF alloc_exact_rat_values_not_floats EMITTED {:?}; VM = ok:true value:2/3", got);
        }
    }

    // example <frontier_drops_dominated_points>
    let got = pareto_frontier(vec![
        Pt { cost: 1, value: (5, 1) },
        Pt { cost: 2, value: (5, 1) },
        Pt { cost: 3, value: (10, 1) },
        Pt { cost: 2, value: (3, 1) },
    ]);
    match &got {
        Ok(p) if p.len() == 2 && p[0].cost == 1 && q_eq(p[0].value, (5, 1)) && p[1].cost == 3 && q_eq(p[1].value, (10, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF frontier_drops_dominated_points EMITTED {:?}; VM = [(1,5/1),(3,10/1)]", got);
        }
    }

    // example <frontier_keeps_exact_duplicates>
    let got = pareto_frontier(vec![Pt { cost: 1, value: (5, 1) }, Pt { cost: 1, value: (5, 1) }]);
    match &got {
        Ok(p) if p.len() == 2 => {}
        _ => {
            failures += 1;
            println!("DIFF frontier_keeps_exact_duplicates EMITTED {:?}; VM = len 2", got);
        }
    }

    // example <frontier_empty_points_yield_empty>
    let got = pareto_frontier(vec![]);
    match &got {
        Ok(p) if p.is_empty() => {}
        _ => {
            failures += 1;
            println!("DIFF frontier_empty_points_yield_empty EMITTED {:?}; VM = len 0", got);
        }
    }

    // example <frontier_single_point_survives>
    let got = pareto_frontier(vec![Pt { cost: 7, value: (1, 2) }]);
    match &got {
        Ok(p) if p.len() == 1 => {}
        _ => {
            failures += 1;
            println!("DIFF frontier_single_point_survives EMITTED {:?}; VM = len 1", got);
        }
    }

    // example <dominates_strictly_cheaper_same_value>
    let got = dominates(Pt { cost: 1, value: (5, 1) }, Pt { cost: 2, value: (5, 1) });
    if got != Ok(true) {
        failures += 1;
        println!("DIFF dominates_strictly_cheaper_same_value EMITTED {:?}; VM = true", got);
    }

    // example <dominates_rejects_duplicates>
    let got = dominates(Pt { cost: 1, value: (5, 1) }, Pt { cost: 1, value: (5, 1) });
    if got != Ok(false) {
        failures += 1;
        println!("DIFF dominates_rejects_duplicates EMITTED {:?}; VM = false", got);
    }

    // example <dominates_rejects_split_tradeoffs>
    let got = dominates(Pt { cost: 1, value: (3, 1) }, Pt { cost: 2, value: (5, 1) });
    if got != Ok(false) {
        failures += 1;
        println!("DIFF dominates_rejects_split_tradeoffs EMITTED {:?}; VM = false", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all allocate pins conform");
}
"#;

const INFERENCE_DRIVER: &str = r#"
//! Authored pins of probability/inference.emath against the emitted crate.

fn q_eq(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 == b.0 * a.1
}

use inference::EmathRecord_Stratum as Stratum;
use inference::{binom_upper_tail, ci_lower, ci_upper, fisher_exact, mcnemar_exact, stratified_rate};

fn main() {
    let mut failures = 0usize;

    // example <tail_two_of_two>: result == 1/4
    let got = binom_upper_tail(2, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (1, 4)) => {}
        _ => {
            failures += 1;
            println!("DIFF tail_two_of_two EMITTED {:?}; VM = 1/4", got);
        }
    }

    // example <tail_at_least_one_of_two>: result == 3/4
    let got = binom_upper_tail(1, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (3, 4)) => {}
        _ => {
            failures += 1;
            println!("DIFF tail_at_least_one_of_two EMITTED {:?}; VM = 3/4", got);
        }
    }

    // example <tail_impossible_k_is_zero>: result == 0/1
    let got = binom_upper_tail(3, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF tail_impossible_k_is_zero EMITTED {:?}; VM = 0/1", got);
        }
    }

    // example <tail_refuses_bad_probability>: diagnostic.code == bad_probability
    let got = binom_upper_tail(1, 2, (5, 4));
    match &got {
        Err(text) if text == "bad_probability" => {}
        _ => {
            failures += 1;
            println!("DIFF tail_refuses_bad_probability EMITTED {:?}; VM = bad_probability", got);
        }
    }

    // example <mcnemar_one_nine>: 2 * (1 + 10) / 1024 = 11/512
    let got = mcnemar_exact(1, 9);
    match &got {
        Ok(q) if q_eq(*q, (11, 512)) => {}
        _ => {
            failures += 1;
            println!("DIFF mcnemar_one_nine EMITTED {:?}; VM = 11/512", got);
        }
    }

    // example <mcnemar_nine_one_symmetric>: min(b, c) is the statistic
    let got = mcnemar_exact(9, 1);
    match &got {
        Ok(q) if q_eq(*q, (11, 512)) => {}
        _ => {
            failures += 1;
            println!("DIFF mcnemar_nine_one_symmetric EMITTED {:?}; VM = 11/512", got);
        }
    }

    // example <mcnemar_no_discordance_is_one>
    let got = mcnemar_exact(0, 0);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF mcnemar_no_discordance_is_one EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <mcnemar_refuses_negative>: diagnostic.code == bad_count
    let got = mcnemar_exact(-1, 3);
    match &got {
        Err(text) if text == "bad_count" => {}
        _ => {
            failures += 1;
            println!("DIFF mcnemar_refuses_negative EMITTED {:?}; VM = bad_count", got);
        }
    }

    // example <fisher_tea_table>: 16/70 + 1/70 = 17/70
    let got = fisher_exact(3, 1, 1, 3);
    match &got {
        Ok(q) if q_eq(*q, (17, 70)) => {}
        _ => {
            failures += 1;
            println!("DIFF fisher_tea_table EMITTED {:?}; VM = 17/70", got);
        }
    }

    // example <fisher_all_successes>
    let got = fisher_exact(4, 0, 0, 4);
    match &got {
        Ok(q) if q_eq(*q, (1, 70)) => {}
        _ => {
            failures += 1;
            println!("DIFF fisher_all_successes EMITTED {:?}; VM = 1/70", got);
        }
    }

    // example <fisher_refuses_negative_entry>: diagnostic.code == bad_table
    let got = fisher_exact(-1, 1, 1, 3);
    match &got {
        Err(text) if text == "bad_table" => {}
        _ => {
            failures += 1;
            println!("DIFF fisher_refuses_negative_entry EMITTED {:?}; VM = bad_table", got);
        }
    }

    // example <stratified_pools_exact>: result == 7/20
    let got = stratified_rate(vec![Stratum { hits: 3, denom: 10 }, Stratum { hits: 4, denom: 10 }]);
    match &got {
        Ok(q) if q_eq(*q, (7, 20)) => {}
        _ => {
            failures += 1;
            println!("DIFF stratified_pools_exact EMITTED {:?}; VM = 7/20", got);
        }
    }

    // example <stratified_refuses_empty>: diagnostic.code == empty_strata
    let got = stratified_rate(vec![]);
    match &got {
        Err(text) if text == "empty_strata" => {}
        _ => {
            failures += 1;
            println!("DIFF stratified_refuses_empty EMITTED {:?}; VM = empty_strata", got);
        }
    }

    // example <stratified_refuses_bad_stratum>: diagnostic.code == bad_stratum
    let got = stratified_rate(vec![Stratum { hits: 5, denom: 3 }]);
    match &got {
        Err(text) if text == "bad_stratum" => {}
        _ => {
            failures += 1;
            println!("DIFF stratified_refuses_bad_stratum EMITTED {:?}; VM = bad_stratum", got);
        }
    }

    // example <lower_k_zero_is_point_zero>: Itv {lo: 0/1, hi: 0/1}
    let got = ci_lower(0, 10, (1, 20), 6);
    match &got {
        Ok(itv) if q_eq(itv.lo, (0, 1)) && q_eq(itv.hi, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF lower_k_zero_is_point_zero EMITTED {:?}; VM = Itv 0/1..0/1", got);
        }
    }

    // example <upper_k_n_is_point_one>: Itv {lo: 1/1, hi: 1/1}
    let got = ci_upper(10, 10, (1, 20), 6);
    match &got {
        Ok(itv) if q_eq(itv.lo, (1, 1)) && q_eq(itv.hi, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF upper_k_n_is_point_one EMITTED {:?}; VM = Itv 1/1..1/1", got);
        }
    }

    // example <lower_brackets_the_quantile>: the crossing invariant, exact —
    // itv_width(result) <= 1/64, tail(8, 10, lo) <= 1/40 <= tail(8, 10, hi).
    // The width and both tails go through the EMITTED binom_upper_tail.
    let got = ci_lower(8, 10, (1, 20), 6);
    match &got {
        Ok(itv) => {
            // width = hi - lo <= 1/64 with positive canonical denominators
            let w_num = itv.hi.0 * itv.lo.1 - itv.lo.0 * itv.hi.1;
            let w_den = itv.lo.1 * itv.hi.1;
            let t_lo = binom_upper_tail(8, 10, itv.lo);
            let t_hi = binom_upper_tail(8, 10, itv.hi);
            let narrow = w_num * 64 <= w_den;
            let t_lo_ok = matches!(&t_lo, Ok(q) if q.0 * 40 <= q.1);
            let t_hi_ok = matches!(&t_hi, Ok(q) if q.0 * 40 >= q.1);
            if !(narrow && t_lo_ok && t_hi_ok) {
                failures += 1;
                println!("DIFF lower_brackets_the_quantile EMITTED itv={:?} tail(lo)={:?} tail(hi)={:?}; VM = width<=1/64, tail(lo)<=1/40<=tail(hi)", got, t_lo, t_hi);
            }
        }
        _ => {
            failures += 1;
            println!("DIFF lower_brackets_the_quantile EMITTED {:?}; VM = certified Itv bracket", got);
        }
    }

    // example <upper_brackets_the_quantile>: cdf(2, 10, lo) >= 1/40 and
    // cdf(2, 10, hi) <= 1/40, with cdf(2, 10, p) = 1 - tail(3, 10, p):
    // tail(3, 10, lo) <= 39/40 and tail(3, 10, hi) >= 39/40, plus width.
    let got = ci_upper(2, 10, (1, 20), 6);
    match &got {
        Ok(itv) => {
            let w_num = itv.hi.0 * itv.lo.1 - itv.lo.0 * itv.hi.1;
            let w_den = itv.lo.1 * itv.hi.1;
            let t_lo = binom_upper_tail(3, 10, itv.lo);
            let t_hi = binom_upper_tail(3, 10, itv.hi);
            let narrow = w_num * 64 <= w_den;
            let t_lo_ok = matches!(&t_lo, Ok(q) if q.0 * 40 <= q.1 * 39);
            let t_hi_ok = matches!(&t_hi, Ok(q) if q.0 * 40 >= q.1 * 39);
            if !(narrow && t_lo_ok && t_hi_ok) {
                failures += 1;
                println!("DIFF upper_brackets_the_quantile EMITTED itv={:?} tail(lo)={:?} tail(hi)={:?}; VM = width<=1/64, cdf(lo)>=1/40>=cdf(hi)", got, t_lo, t_hi);
            }
        }
        _ => {
            failures += 1;
            println!("DIFF upper_brackets_the_quantile EMITTED {:?}; VM = certified Itv bracket", got);
        }
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all inference pins conform");
}
"#;

#[test]
fn emission_conformance_allocate() {
    let diffs = run_lane(
        "language/modules/optimization/allocate.emath",
        ALLOCATE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted allocate disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

#[test]
fn emission_conformance_inference() {
    let diffs = run_lane(
        "language/modules/probability/inference.emath",
        INFERENCE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted inference disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const POWERS_DRIVER: &str = r#"
//! Authored pins of analysis/powers.emath against the emitted crate.
//!
//! Carrier boundary (labeled, from the module header): VM Int is
//! arbitrary precision; the emitted crate carries ExactInt (checked
//! machine arithmetic with a Big tail). The sqrt_two_to_three_hundred
//! row passes 2^300 as an Int INPUT, which the emitted `n: i64` ABI
//! cannot even name — that row stays VM-only. The two_to_three_hundred
//! row (result too big for i64 but the INPUTS fit) is pinned here via
//! exact decimal Display equality against the authored expectation.

fn ei(n: i64) -> powers::emath_rt::ExactInt {
    powers::emath_rt::ExactInt::from(n)
}

fn q_eq(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 == b.0 * a.1
}

use powers::{
    ceil_root, composite_lower, composite_upper, floor_root, floor_root_between, is_exact_root,
    pow_int, power_lower, power_upper, rat_pow, square_int, upper_root_at, weighted_prod,
};

fn main() {
    let mut failures = 0usize;

    // example <pythag_three>: result + 4*4 == 25, i.e. square_int(3) == 9
    let got = square_int(3);
    if got.as_ref() != Ok(&9) {
        failures += 1;
        println!("DIFF pythag_three EMITTED {:?}; VM = 9", got);
    }

    // example <two_to_three_hundred>: result == 2^300 exactly (the
    // authored decimal). Display equality over the emitted ExactInt.
    let want = "2037035976334486086268445688409378161051468393665936250636140449354381299763336706183397376";
    let got = pow_int(2, 300);
    match &got {
        Ok(v) => {
            let text = format!("{v}");
            if text != want {
                failures += 1;
                println!("DIFF two_to_three_hundred EMITTED {}; VM = {}", text, want);
            }
        }
        Err(text) => {
            failures += 1;
            println!("DIFF two_to_three_hundred EMITTED Err({}); VM = 2^300 exact", text);
        }
    }

    // machine-range exponent law: pow_int(2, 62) == 2^62
    let got = pow_int(2, 62);
    if !matches!(&got, Ok(v) if *v == ei(4611686018427387904)) {
        failures += 1;
        println!("DIFF pow_int_machine_range EMITTED {:?}; VM = 2^62", got);
    }

    // example <sqrt_nine_unclamped>
    let got = floor_root_between(9, 2, 0, 100);
    if got.as_ref() != Ok(&3) {
        failures += 1;
        println!("DIFF sqrt_nine_unclamped EMITTED {:?}; VM = 3", got);
    }

    // example <clamped_low>
    let got = floor_root_between(9, 2, 4, 100);
    if got.as_ref() != Ok(&4) {
        failures += 1;
        println!("DIFF clamped_low EMITTED {:?}; VM = 4", got);
    }

    // example <sqrt_nine>
    let got = floor_root(9, 2);
    if got.as_ref() != Ok(&3) {
        failures += 1;
        println!("DIFF sqrt_nine EMITTED {:?}; VM = 3", got);
    }

    // example <zeroth_root>: q <= 0 is not a root
    let got = floor_root(9, 0);
    if got.as_ref() != Ok(&0) {
        failures += 1;
        println!("DIFF zeroth_root EMITTED {:?}; VM = 0", got);
    }

    // example <cbrt_eight>
    let got = floor_root(8, 3);
    if got.as_ref() != Ok(&2) {
        failures += 1;
        println!("DIFF cbrt_eight EMITTED {:?}; VM = 2", got);
    }

    // example <sqrt_eight>
    let got = floor_root(8, 2);
    if got.as_ref() != Ok(&2) {
        failures += 1;
        println!("DIFF sqrt_eight EMITTED {:?}; VM = 2", got);
    }

    // example <zero>
    let got = floor_root(0, 2);
    if got.as_ref() != Ok(&0) {
        failures += 1;
        println!("DIFF zero EMITTED {:?}; VM = 0", got);
    }

    // example <first_root>
    let got = floor_root(11, 1);
    if got.as_ref() != Ok(&11) {
        failures += 1;
        println!("DIFF first_root EMITTED {:?}; VM = 11", got);
    }

    // example <next_after_sqrt_nine>
    let got = upper_root_at(9, 2, 0);
    if got.as_ref() != Ok(&4) {
        failures += 1;
        println!("DIFF next_after_sqrt_nine EMITTED {:?}; VM = 4", got);
    }

    // example <already_past>
    let got = upper_root_at(9, 2, 5);
    if got.as_ref() != Ok(&5) {
        failures += 1;
        println!("DIFF already_past EMITTED {:?}; VM = 5", got);
    }

    // example <ceil_sqrt_ten>
    let got = ceil_root(10, 2);
    if got.as_ref() != Ok(&4) {
        failures += 1;
        println!("DIFF ceil_sqrt_ten EMITTED {:?}; VM = 4", got);
    }

    // example <exact_cbrt>
    let got = ceil_root(8, 3);
    if got.as_ref() != Ok(&2) {
        failures += 1;
        println!("DIFF exact_cbrt EMITTED {:?}; VM = 2", got);
    }

    // example <eight_is_cube>
    let got = is_exact_root(8, 3);
    if got.as_ref() != Ok(&1) {
        failures += 1;
        println!("DIFF eight_is_cube EMITTED {:?}; VM = 1", got);
    }

    // example <nine_is_not_cube>
    let got = is_exact_root(9, 3);
    if got.as_ref() != Ok(&0) {
        failures += 1;
        println!("DIFF nine_is_not_cube EMITTED {:?}; VM = 0", got);
    }

    // example <sqrt_sixteen> (power_lower): result == 4/1
    let got = power_lower(16, 1, 1, 2);
    match &got {
        Ok(q) if q_eq(*q, (4, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF power_lower_sqrt_sixteen EMITTED {:?}; VM = 4/1", got);
        }
    }

    // example <sqrt_sixteen> (power_upper): result == 4/1
    let got = power_upper(16, 1, 1, 2);
    match &got {
        Ok(q) if q_eq(*q, (4, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF power_upper_sqrt_sixteen EMITTED {:?}; VM = 4/1", got);
        }
    }

    // example <three_twos>: 2*2*2 == 8
    let got = weighted_prod(vec![2, 2, 2], vec![1, 1, 1], 0, 1);
    if got.as_ref() != Ok(&8) {
        failures += 1;
        println!("DIFF three_twos EMITTED {:?}; VM = 8", got);
    }

    // example <sqrt_of_product> (composite_lower): result == 4/1
    let got = composite_lower(vec![4, 4], vec![1, 1], vec![1, 1], 2);
    match &got {
        Ok(q) if q_eq(*q, (4, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF composite_lower_sqrt_of_product EMITTED {:?}; VM = 4/1", got);
        }
    }

    // example <sqrt_of_product> (composite_upper): result == 4/1
    let got = composite_upper(vec![4, 4], vec![1, 1], vec![1, 1], 2);
    match &got {
        Ok(q) if q_eq(*q, (4, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF composite_upper_sqrt_of_product EMITTED {:?}; VM = 4/1", got);
        }
    }

    // example <half_cubed>: (1/2)^3 == 1/8
    let got = rat_pow((1, 2), 3);
    match &got {
        Ok(q) if q_eq(*q, (1, 8)) => {}
        _ => {
            failures += 1;
            println!("DIFF half_cubed EMITTED {:?}; VM = 1/8", got);
        }
    }

    // example <neg_exponent_inverts>: (2/3)^-2 == 9/4
    let got = rat_pow((2, 3), -2);
    match &got {
        Ok(q) if q_eq(*q, (9, 4)) => {}
        _ => {
            failures += 1;
            println!("DIFF neg_exponent_inverts EMITTED {:?}; VM = 9/4", got);
        }
    }

    // example <zeroth_power>: (5/7)^0 == 1/1
    let got = rat_pow((5, 7), 0);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF zeroth_power EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <zero_base_negative_is_zero>: (0/1)^-1 == 0/1
    let got = rat_pow((0, 1), -1);
    match &got {
        Ok(q) if q_eq(*q, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF zero_base_negative_is_zero EMITTED {:?}; VM = 0/1", got);
        }
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all powers pins conform");
}
"#;

const DISTRIBUTIONS_DRIVER: &str = r#"
//! Authored pins of probability/distributions.emath against the emitted
//! crate.
//!
//! Transcription notes (honest, labeled): the fair-coin entropy row in
//! the authored module overlaps its enclosure with `log2_bracket(40)`
//! from analysis.series — that function is not an entry of THIS crate,
//! so the pin fences the enclosure at log 2 within 1e-6 instead
//! (log 2 = 0.6931471805...). Row-level identity rows (sum == 1,
//! dist_ok) go through the crate's own emitted dist_total / dist_ok.

fn q_eq(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 == b.0 * a.1
}

use distributions::{
    binomial_cdf, binomial_mean, binomial_moments_match, binomial_pmf, binomial_row,
    binomial_variance, dist_cdf, dist_mean, dist_nonneg, dist_ok, dist_total, dist_variance,
    entropy_bracket, entropy_sum, hypergeometric_mean, hypergeometric_moments_match,
    hypergeometric_pmf, hypergeometric_row, poisson_cdf_bracket, poisson_cdf_sum,
    poisson_pmf_bracket,
};
use distributions::EmathRecord_Itv as Itv;

fn main() {
    let mut failures = 0usize;

    // example <two_halves>: total mass == 1/1
    let got = dist_total(vec![(1, 2), (1, 2)]);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF two_halves EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <fair_coin>: dist_ok == true
    let got = dist_ok(vec![(1, 2), (1, 2)]);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF fair_coin EMITTED {:?}; VM = true", got);
    }

    // example <missing_mass>: dist_ok == false
    let got = dist_ok(vec![(1, 2), (1, 3)]);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF missing_mass EMITTED {:?}; VM = false", got);
    }

    // example <negative_entry>: dist_ok == false
    let got = dist_ok(vec![(3, 2), (-1, 2)]);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF negative_entry EMITTED {:?}; VM = false", got);
    }

    // example <empty_is_not>: dist_ok == false
    let got = dist_ok(vec![]);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF empty_is_not EMITTED {:?}; VM = false", got);
    }

    // example <all_positive>: dist_nonneg == true
    let got = dist_nonneg(vec![(1, 2), (1, 2)], 0);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF all_positive EMITTED {:?}; VM = true", got);
    }

    // example <one_negative>: dist_nonneg == false
    let got = dist_nonneg(vec![(1, 2), (-1, 4)], 0);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF one_negative EMITTED {:?}; VM = false", got);
    }

    // example <uniform_six_indices_mean>: mean == 5/2
    let six = vec![(1, 6); 6];
    let got = dist_mean(six.clone());
    match &got {
        Ok(q) if q_eq(*q, (5, 2)) => {}
        _ => {
            failures += 1;
            println!("DIFF uniform_six_indices_mean EMITTED {:?}; VM = 5/2", got);
        }
    }

    // example <refuses_bad_mass_mean>: diagnostic.code == not_a_distribution
    let got = dist_mean(vec![(1, 2), (1, 3)]);
    match &got {
        Err(text) if text == "not_a_distribution" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_bad_mass_mean EMITTED {:?}; VM = not_a_distribution", got);
        }
    }

    // example <fair_die_variance>: variance == 35/12
    let got = dist_variance(six.clone());
    match &got {
        Ok(q) if q_eq(*q, (35, 12)) => {}
        _ => {
            failures += 1;
            println!("DIFF fair_die_variance EMITTED {:?}; VM = 35/12", got);
        }
    }

    // example <refuses_bad_mass_variance>
    let got = dist_variance(vec![(1, 2), (1, 3)]);
    match &got {
        Err(text) if text == "not_a_distribution" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_bad_mass_variance EMITTED {:?}; VM = not_a_distribution", got);
        }
    }

    // example <coin_first_half>: cdf(0) == 1/2
    let coin = vec![(1, 2), (1, 2)];
    let got = dist_cdf(coin.clone(), 0);
    match &got {
        Ok(q) if q_eq(*q, (1, 2)) => {}
        _ => {
            failures += 1;
            println!("DIFF coin_first_half EMITTED {:?}; VM = 1/2", got);
        }
    }

    // example <coin_full>: cdf(1) == 1/1
    let got = dist_cdf(coin.clone(), 1);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF coin_full EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <past_end_is_one>: cdf(5) == 1/1
    let got = dist_cdf(coin.clone(), 5);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF past_end_is_one EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <below_zero_is_zero>: cdf(-1) == 0/1
    let got = dist_cdf(coin.clone(), -1);
    match &got {
        Ok(q) if q_eq(*q, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF below_zero_is_zero EMITTED {:?}; VM = 0/1", got);
        }
    }

    // example <fair_coin_two_flips>: pmf(1; 2, 1/2) == 1/2
    let got = binomial_pmf(1, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (1, 2)) => {}
        _ => {
            failures += 1;
            println!("DIFF fair_coin_two_flips EMITTED {:?}; VM = 1/2", got);
        }
    }

    // example <all_successes>: pmf(3; 3, 1/3) == 1/27
    let got = binomial_pmf(3, 3, (1, 3));
    match &got {
        Ok(q) if q_eq(*q, (1, 27)) => {}
        _ => {
            failures += 1;
            println!("DIFF all_successes EMITTED {:?}; VM = 1/27", got);
        }
    }

    // example <refuses_probability_above_one>
    let got = binomial_pmf(1, 2, (3, 2));
    match &got {
        Err(text) if text == "bad_probability" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_probability_above_one EMITTED {:?}; VM = bad_probability", got);
        }
    }

    // example <refuses_index_past_n>
    let got = binomial_pmf(3, 2, (1, 2));
    match &got {
        Err(text) if text == "bad_trial_index" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_index_past_n EMITTED {:?}; VM = bad_trial_index", got);
        }
    }

    // example <row_sums_to_one_exactly>: sum(binomial_row(4, 1/3)) == 1/1,
    // through the crate's own emitted dist_total.
    let row = binomial_row(4, (1, 3));
    let total = match &row {
        Ok(r) => dist_total(r.clone()),
        Err(_) => Err("row refused".to_string()),
    };
    match &total {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF row_sums_to_one_exactly EMITTED row={:?} total={:?}; VM = 1/1", row, total);
        }
    }

    // example <is_a_distribution>: dist_ok(binomial_row(4, 1/3)) == true
    let ok = match &row {
        Ok(r) => dist_ok(r.clone()),
        Err(_) => Err("row refused".to_string()),
    };
    if ok != Ok(true) {
        failures += 1;
        println!("DIFF is_a_distribution EMITTED row={:?} ok={:?}; VM = true", row, ok);
    }

    // example <at_most_one_of_two>: cdf(1; 2, 1/2) == 3/4
    let got = binomial_cdf(1, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (3, 4)) => {}
        _ => {
            failures += 1;
            println!("DIFF at_most_one_of_two EMITTED {:?}; VM = 3/4", got);
        }
    }

    // example <zero_is_zero>: cdf(-1; 2, 1/2) == 0/1
    let got = binomial_cdf(-1, 2, (1, 2));
    match &got {
        Ok(q) if q_eq(*q, (0, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF zero_is_zero EMITTED {:?}; VM = 0/1", got);
        }
    }

    // example <four_thirds>: np == 4/3
    let got = binomial_mean(4, (1, 3));
    match &got {
        Ok(q) if q_eq(*q, (4, 3)) => {}
        _ => {
            failures += 1;
            println!("DIFF four_thirds EMITTED {:?}; VM = 4/3", got);
        }
    }

    // example <four_thirds_two_thirds>: np(1-p) == 8/9
    let got = binomial_variance(4, (1, 3));
    match &got {
        Ok(q) if q_eq(*q, (8, 9)) => {}
        _ => {
            failures += 1;
            println!("DIFF four_thirds_two_thirds EMITTED {:?}; VM = 8/9", got);
        }
    }

    // example <n_four_p_third>: the moments certificate == true
    let got = binomial_moments_match(4, (1, 3));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF n_four_p_third EMITTED {:?}; VM = true", got);
    }

    // example <n_five_p_two_fifths>
    let got = binomial_moments_match(5, (2, 5));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF n_five_p_two_fifths EMITTED {:?}; VM = true", got);
    }

    // example <n_seven_p_one_seventh>
    let got = binomial_moments_match(7, (1, 7));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF n_seven_p_one_seventh EMITTED {:?}; VM = true", got);
    }

    // example <two_aces_of_two_draws>: 1/6
    let got = hypergeometric_pmf(2, 2, 4, 2);
    match &got {
        Ok(q) if q_eq(*q, (1, 6)) => {}
        _ => {
            failures += 1;
            println!("DIFF two_aces_of_two_draws EMITTED {:?}; VM = 1/6", got);
        }
    }

    // example <zero_successes>: 1/6
    let got = hypergeometric_pmf(0, 2, 4, 2);
    match &got {
        Ok(q) if q_eq(*q, (1, 6)) => {}
        _ => {
            failures += 1;
            println!("DIFF zero_successes EMITTED {:?}; VM = 1/6", got);
        }
    }

    // example <refuses_too_many_successes>
    let got = hypergeometric_pmf(3, 2, 4, 2);
    match &got {
        Err(text) if text == "bad_trial_index" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_too_many_successes EMITTED {:?}; VM = bad_trial_index", got);
        }
    }

    // example <hyper_row_sums_to_one_exactly>
    let row = hypergeometric_row(3, 6, 2);
    let total = match &row {
        Ok(r) => dist_total(r.clone()),
        Err(_) => Err("row refused".to_string()),
    };
    match &total {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF hyper_row_sums_to_one_exactly EMITTED row={:?} total={:?}; VM = 1/1", row, total);
        }
    }

    // example <hyper_is_a_distribution>
    let ok = match &row {
        Ok(r) => dist_ok(r.clone()),
        Err(_) => Err("row refused".to_string()),
    };
    if ok != Ok(true) {
        failures += 1;
        println!("DIFF hyper_is_a_distribution EMITTED row={:?} ok={:?}; VM = true", row, ok);
    }

    // example <three_draws_two_of_six>: mean == 1/1
    let got = hypergeometric_mean(3, 6, 2);
    match &got {
        Ok(q) if q_eq(*q, (1, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF three_draws_two_of_six EMITTED {:?}; VM = 1/1", got);
        }
    }

    // example <matches_exactly>: the hypergeometric moments certificate
    let got = hypergeometric_moments_match(3, 6, 2);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF matches_exactly EMITTED {:?}; VM = true", got);
    }

    // example <one_encloses_exp_minus_one>: pmf(1; lambda=1) encloses e^-1.
    // q=20 stays inside the emitted i128 Rat carrier (the authored q=40
    // makes 40! overflow it — pinned as the boundary row below; the
    // 1e-6 fences are far coarser than the q=20 bracket width).
    let got = poisson_pmf_bracket(1, (1, 1), 20);
    match &got {
        Ok(itv) if q_gt(itv.lo, (367879, 1000000)) && q_lt(itv.hi, (367880, 1000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF one_encloses_exp_minus_one EMITTED {:?}; VM = e^-1 fence (0.367879, 0.367880)", got);
        }
    }

    // example <zero_is_exp_minus_lambda>: pmf(0; lambda=1) encloses e^-1
    let got = poisson_pmf_bracket(0, (1, 1), 20);
    match &got {
        Ok(itv) if q_gt(itv.lo, (3678794, 10000000)) && q_lt(itv.hi, (3678795, 10000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF zero_is_exp_minus_lambda EMITTED {:?}; VM = e^-1 fence (0.3678794, 0.3678795)", got);
        }
    }

    // carrier boundary (labeled): the authored q=40 budget needs 40! in
    // the exp series — past the emitted checked-i128 Rat carrier, which
    // refuses by name (VM computes it in arbitrary precision).
    let got = poisson_pmf_bracket(1, (1, 1), 40);
    match &got {
        Err(text) if text == "E-RAT-002: exact rational part exceeds i128" => {}
        _ => {
            failures += 1;
            println!("DIFF poisson_q40_carrier_boundary EMITTED {:?}; VM = e^-1 enclosure (i128 Rat carrier refuses at 40!)", got);
        }
    }

    // example <refuses_negative_rate>
    let got = poisson_pmf_bracket(1, (-1, 2), 40);
    match &got {
        Err(text) if text == "negative_rate" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_negative_rate EMITTED {:?}; VM = negative_rate", got);
        }
    }

    // example <refuses_negative_index>
    let got = poisson_pmf_bracket(-1, (1, 1), 40);
    match &got {
        Err(text) if text == "bad_trial_index" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_negative_index EMITTED {:?}; VM = bad_trial_index", got);
        }
    }

    // example <at_most_one_lambda_one>: cdf(1; lambda=1) encloses 2e^-1
    let got = poisson_cdf_bracket(1, (1, 1), 20);
    match &got {
        Ok(itv) if q_gt(itv.lo, (735758, 1000000)) && q_lt(itv.hi, (735760, 1000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF at_most_one_lambda_one EMITTED {:?}; VM = 2e^-1 fence (0.735758, 0.735760)", got);
        }
    }

    // example <at_most_zero_is_pmf_zero>
    let got = poisson_cdf_bracket(0, (1, 1), 20);
    match &got {
        Ok(itv) if q_gt(itv.lo, (3678794, 10000000)) && q_lt(itv.hi, (3678795, 10000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF at_most_zero_is_pmf_zero EMITTED {:?}; VM = e^-1 fence", got);
        }
    }

    // example <refuses_negative_rate_cdf>
    let got = poisson_cdf_bracket(1, (-2, 1), 40);
    match &got {
        Err(text) if text == "negative_rate" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_negative_rate_cdf EMITTED {:?}; VM = negative_rate", got);
        }
    }

    // example <two_terms_lambda_one>: accumulator row == cdf(1)
    let got = poisson_cdf_sum(1, (1, 1), 20, Itv { lo: (0, 1), hi: (0, 1) });
    match &got {
        Ok(itv) if q_gt(itv.lo, (735758, 1000000)) && q_lt(itv.hi, (735760, 1000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF two_terms_lambda_one EMITTED {:?}; VM = 2e^-1 fence (0.735758, 0.735760)", got);
        }
    }

    // example <fair_coin_is_log_two>: entropy([1/2,1/2]) overlaps log 2.
    // Fenced at log 2 within 1e-6 (see module doc note above).
    let got = entropy_bracket(vec![(1, 2), (1, 2)], 40);
    match &got {
        Ok(itv) if q_gt(itv.lo, (693147, 1000000)) && q_lt(itv.hi, (693148, 1000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF fair_coin_is_log_two EMITTED {:?}; VM = enclosure overlapping log 2 = 0.6931471805...", got);
        }
    }

    // example <deterministic_is_zero_width>: contains 0, width < 1e-6
    let got = entropy_bracket(vec![(1, 1)], 40);
    match &got {
        Ok(itv) => {
            let w_num = itv.hi.0 * itv.lo.1 - itv.lo.0 * itv.hi.1;
            let w_den = itv.lo.1 * itv.hi.1;
            let contains_zero = itv.lo.0 <= 0 && itv.hi.0 >= 0;
            if !(contains_zero && w_num < w_den * 1000000) {
                failures += 1;
                println!("DIFF deterministic_is_zero_width EMITTED {:?}; VM = contains 0 with width < 1/1000000", got);
            }
        }
        _ => {
            failures += 1;
            println!("DIFF deterministic_is_zero_width EMITTED {:?}; VM = certified ~zero enclosure", got);
        }
    }

    // example <refuses_bad_mass_entropy>
    let got = entropy_bracket(vec![(1, 2), (1, 3)], 40);
    match &got {
        Err(text) if text == "not_a_distribution" => {}
        _ => {
            failures += 1;
            println!("DIFF refuses_bad_mass_entropy EMITTED {:?}; VM = not_a_distribution", got);
        }
    }

    // example <single_half_outcome>: -1/2 log(1/2) encloses (1/2) log 2
    let got = entropy_sum(vec![(1, 2), (1, 2)], 0, 40, Itv { lo: (0, 1), hi: (0, 1) });
    match &got {
        Ok(itv) if q_gt(itv.lo, (346573, 1000000)) && q_lt(itv.hi, (346575, 1000000)) => {}
        _ => {
            failures += 1;
            println!("DIFF single_half_outcome EMITTED {:?}; VM = 0.5*log2 fence (0.346573, 0.346575)", got);
        }
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all distributions pins conform");
}

fn q_gt(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 > b.0 * a.1
}

fn q_lt(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 < b.0 * a.1
}
"#;

#[test]
fn emission_conformance_powers() {
    let diffs = run_lane("language/modules/analysis/powers.emath", POWERS_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted powers disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const INVARIANTS_DRIVER: &str = r#"
//! Authored pins of search/invariants.emath against the emitted crate.
//!
//! inv_coprime is the exact-Int lane: Rat inputs scale into ExactInt,
//! compare against zero, and divide by a signed gcd. The Code-valued
//! inv_term_code rows stay VM-only (they pin by quote.evaluate).

use invariants::emath_rt::ExactInt;
use invariants::{inv_basis, inv_coprime};

fn ints(xs: &[i64]) -> Vec<ExactInt> {
    xs.iter().map(|&x| ExactInt::from(x)).collect()
}

fn main() {
    let mut failures = 0usize;

    // example <degree_two_over_q_p>
    let got = inv_basis(vec![1, 1], 2, 3);
    let ok = matches!(&got, Ok(b) if b.len() == 5 && b[0] == vec![0, 1] && b[4] == vec![2, 0]);
    if !ok {
        failures += 1;
        println!("DIFF degree_two_over_q_p EMITTED {:?}; VM = 5 monomials, [0,1] .. [2,0]", got);
    }

    // example <parameters_alone_are_not_monomials>
    let got = inv_basis(vec![0, 1], 2, 3);
    if !matches!(&got, Ok(b) if b.len() == 3) {
        failures += 1;
        println!("DIFF parameters_alone_are_not_monomials EMITTED {:?}; VM = 3 monomials", got);
    }

    // example <negative_lead_flips_sign>
    let got = inv_coprime(vec![(0, 1), (-2, 3), (4, 3)]);
    if got.as_ref() != Ok(&ints(&[0, 1, -2])) {
        failures += 1;
        println!("DIFF negative_lead_flips_sign EMITTED {:?}; VM = [0, 1, -2]", got);
    }

    // example <denominators_cleared>
    let got = inv_coprime(vec![(1, 2), (1, 3)]);
    if got.as_ref() != Ok(&ints(&[3, 2])) {
        failures += 1;
        println!("DIFF denominators_cleared EMITTED {:?}; VM = [3, 2]", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all invariants pins conform");
}
"#;

#[test]
fn emission_conformance_invariants() {
    let diffs = run_lane(
        "language/modules/search/invariants.emath",
        INVARIANTS_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted invariants disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

#[test]
fn emission_conformance_distributions() {
    let diffs = run_lane(
        "language/modules/probability/distributions.emath",
        DISTRIBUTIONS_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted distributions disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const LONG_WALK_DRIVER: &str = r#"
//! Tail-recursive walks over 200000 elements: linear in emitted code
//! (well under a second even unoptimized), quadratic if each step
//! copies the walked sequence (about 2e10 element copies).

use long_walk::{dot_self, fold_sum, walk_sum};

fn main() {
    let n = 200_000i64;
    let xs: Vec<f64> = (0..n).map(|k| k as f64).collect();
    let want = (n * (n - 1) / 2) as f64;
    let mut failures = 0usize;

    let start = std::time::Instant::now();
    let got = walk_sum(xs, 0, 0.0);
    let secs = start.elapsed().as_secs_f64();
    if !matches!(&got, Ok(value) if *value == want) {
        failures += 1;
        println!("DIFF walk_sum EMITTED {:?}; VM = {want}", got);
    }
    if secs > 5.0 {
        failures += 1;
        println!("DIFF walk_sum took {secs:.1} s over {n} elements; linear walks finish well under 5 s");
    }

    let start = std::time::Instant::now();
    let got = fold_sum(n);
    let secs = start.elapsed().as_secs_f64();
    if !matches!(&got, Ok(value) if *value == n * (n - 1) / 2) {
        failures += 1;
        println!("DIFF fold_sum EMITTED {:?}; VM = {}", got, n * (n - 1) / 2);
    }
    if secs > 5.0 {
        failures += 1;
        println!("DIFF fold_sum took {secs:.1} s over {n} elements; linear walks finish well under 5 s");
    }
    // Captured sequences are shared, not copied, each time a step closure
    // is rebuilt; copying makes this 1.6e11 element copies.
    let m = 400_000i64;
    let ys: Vec<f64> = (0..m).map(|k| (k % 7) as f64).collect();
    let want = (0..m).map(|k| (k % 7) * (k % 7)).sum::<i64>() as f64;
    let start = std::time::Instant::now();
    let got = dot_self(ys);
    let secs = start.elapsed().as_secs_f64();
    if !matches!(&got, Ok(value) if *value == want) {
        failures += 1;
        println!("DIFF dot_self EMITTED {:?}; VM = {want}", got);
    }
    if secs > 5.0 {
        failures += 1;
        println!("DIFF dot_self took {secs:.1} s over {m} elements; linear walks finish well under 5 s");
    }
    if failures == 0 {
        println!("long_walk: all pins hold");
    }
}
"#;

#[test]
fn emission_conformance_long_walk() {
    let diffs = run_lane(
        "tests/fixtures/constructor/long_walk.emath",
        LONG_WALK_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted long_walk disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const NAMED_FN_DRIVER: &str = r#"
//! A named function used as a value: passed to an arrow input, stored
//! in a record field, mapped by a generic fold, and - with two inputs -
//! coerced curried as a fold step or a local binding.
use named_fn_closure_arg::{
    run_composed, run_direct, run_field, run_fold, run_local, run_map,
};
fn main() {
    let mut failures = 0usize;
    let got = run_direct(0);
    if got != Ok((6, 1)) {
        failures += 1;
        println!("DIFF coerces_and_applies EMITTED {:?}; VM = 6/1", got);
    }
    let got = run_composed(0);
    if got != Ok((12, 1)) {
        failures += 1;
        println!("DIFF applies_through_two_calls EMITTED {:?}; VM = 12/1", got);
    }
    let got = run_field(0);
    if got != Ok((10, 1)) {
        failures += 1;
        println!("DIFF field_holds_named EMITTED {:?}; VM = 10/1", got);
    }
    let got = run_map(vec![(1, 1), (1, 2)]);
    if got != Ok(vec![(2, 1), (1, 1)]) {
        failures += 1;
        println!("DIFF maps_named EMITTED {:?}; VM = [2/1, 1/1]", got);
    }
    let got = run_fold(vec![1, 3]);
    if got != Ok((7, 1)) {
        failures += 1;
        println!("DIFF folds_named_pair EMITTED {:?}; VM = 7/1", got);
    }
    let got = run_local(0);
    if got != Ok((8, 3)) {
        failures += 1;
        println!("DIFF local_holds_named_pair EMITTED {:?}; VM = 8/3", got);
    }
    if failures == 0 {
        println!("named_fn: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_named_fn() {
    let diffs = run_lane(
        "tests/fixtures/constructor/named_fn_closure_arg.emath",
        NAMED_FN_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted named_fn_closure_arg disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const CODE_IDENTITY_DRIVER: &str = r#"
//! quote.identity in the emitted crate: the VM's exact numbers (FNV-1a
//! over the canonical bytes) and the equalities the search modules use.
use code_identity::{
    body_identity, child_matches_template, different_code_differs, exact_substitute,
    index_identity,
    index_round_trip, joined_name, joined_tag, recur_identity,
    recur_over_sequence, recur_renamed_agree, recur_round_trip, recur_shadows,
    renamed_binders_agree, substituted_identity, template_identity,
};
fn main() {
    let mut failures = 0usize;
    let pins: [(&str, Result<i64, String>, i64); 13] = [
        ("exact_three", exact_substitute(3), 7734061378284101075),
        ("exact_minus_four", exact_substitute(-4), 3757166404498068685),
        ("index_two", index_identity(2), -789755635920280717),
        ("index_minus_one", index_identity(-1), -8317634363300468623),
        ("joined_twelve", joined_name(12), -4213184102754636739),
        ("joined_zero", joined_name(0), -2904622620705799990),
        ("template", template_identity(1), 1083551230141008259),
        ("substituted_half", substituted_identity((1, 2)), 6799677611696251689),
        ("sequence_binder_body", body_identity(1), -3712620093599836444),
        ("recur_three", recur_identity(3), 8921994034227166034),
        ("recur_minus_two", recur_identity(-2), -4154231795667992624),
        ("recur_over_two", recur_over_sequence(vec![(1, 2), (3, 1)]), -3980976677755716721),
        ("recur_over_empty", recur_over_sequence(vec![]), -3818502551705519958),
    ];
    for (name, got, vm) in pins {
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF {name} EMITTED {:?}; VM = {vm}", got);
        }
    }
    let bools = [
        ("alpha_equivalent", renamed_binders_agree(3)),
        ("distinct", different_code_differs(1)),
        ("child_is_x", child_matches_template(1)),
        ("alpha_equivalent_recur", recur_renamed_agree(2)),
        ("recur_name_bound", recur_shadows(2)),
        ("recur_view_make", recur_round_trip(2)),
        ("tag_joins", joined_tag(1)),
        ("tag_joins_other", joined_tag(0)),
        ("index_round_trip", index_round_trip(2)),
        ("index_round_trip_negative", index_round_trip(-1)),
    ];
    for (name, got) in bools {
        if got != Ok(true) {
            failures += 1;
            println!("DIFF {name} EMITTED {:?}; VM = true", got);
        }
    }
    // A negative number is not a name suffix: the VM refuses `type`.
    let got = joined_name(-1);
    if !matches!(&got, Err(e) if e.starts_with("type")) {
        failures += 1;
        println!("DIFF joined_negative EMITTED {:?}; VM = type refusal", got);
    }
    if failures == 0 {
        println!("code_identity: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_code_identity() {
    let diffs = run_lane(
        "tests/fixtures/constructor/code_identity.emath",
        CODE_IDENTITY_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted code_identity disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const CLOSURE_SELF_CALL_DRIVER: &str = r#"
//! A function calling itself from inside a closure it hands to a fold.
use closure_self_call::emath_rt::ExactInt;
use closure_self_call::{
    applied_recur, call_shapes, callee_body_applies, counts_big, digits, digits_are, finds_x, leaves, misses_z, paired, plugged,
    rewrite_total, shelf_shape, twice_literal, x_first,
};
fn main() {
    let mut failures = 0usize;
    let got = leaves(10);
    if got != Ok(ExactInt::from(89)) {
        failures += 1;
        println!("DIFF leaves_ten EMITTED {:?}; VM = 89", got);
    }
    let got = finds_x(2);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF deep_local EMITTED {:?}; VM = true", got);
    }
    let got = misses_z(2);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF absent EMITTED {:?}; VM = false", got);
    }
    for (k, vm) in [(3, 2), (2, 1)] {
        let got = counts_big(k);
        if got != Ok(ExactInt::from(vm)) {
            failures += 1;
            println!("DIFF counts_big({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    for (k, vm) in [(1, true), (0, false)] {
        let got = x_first(k);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF x_first({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    let got = call_shapes(5);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF call_shapes EMITTED {:?}; VM = true", got);
    }
    let got = twice_literal(4);
    if got != Ok(8) {
        failures += 1;
        println!("DIFF twice_literal EMITTED {:?}; VM = 8", got);
    }
    // The emitted signature is `impl Debug` (an applied recur's carrier
    // is not inferred), so the pin compares the printed value.
    let got = format!("{:?}", applied_recur(12, 2));
    if got != "Ok(55)" {
        failures += 1;
        println!("DIFF fib_ten EMITTED {got}; VM = 55");
    }
    for (n, vm) in [(11, vec![2, 0, 1]), (0, vec![])] {
        let got = digits(n);
        if got != Ok(vm.iter().map(|&d: &i64| ExactInt::from(d)).collect::<Vec<_>>()) {
            failures += 1;
            println!("DIFF digits({n}) EMITTED {:?}; VM = {vm:?}", got);
        }
    }
    let got = plugged(3);
    if got != Ok(10) {
        failures += 1;
        println!("DIFF plugged_three EMITTED {:?}; VM = 10", got);
    }
    for (k, vm) in [(2, 79), (5, 178), (-3, -86)] {
        let got = paired(k);
        if got != Ok(ExactInt::from(vm)) {
            failures += 1;
            println!("DIFF paired({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    for (k, vm) in [(3, 208), (-2, 104), (0, 102)] {
        let got = rewrite_total(k);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF rewrite_total({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    for (k, vm) in [(4, 234), (-7, 223)] {
        let got = shelf_shape(k);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF shelf_shape({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    for (n, vm) in [(11, true), (10, false)] {
        let got = digits_are(n);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF digits_are({n}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    for (k, vm) in [(4, 26), (-2, -3)] {
        let got = callee_body_applies(k);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF callee_body_applies({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    if failures == 0 {
        println!("closure_self_call: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_closure_self_call() {
    let diffs = run_lane(
        "tests/fixtures/constructor/closure_self_call.emath",
        CLOSURE_SELF_CALL_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted closure_self_call disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const REWRITE_CLIMB_DRIVER: &str = r#"
//! One search.optimize rewrite step, emitted against the VM.
use rewrite_climb::factor_step;
fn main() {
    let mut failures = 0usize;
    // best_work * 10000 + scored * 1000 + verdict * 100 + best at x = 2, y = k
    for (k, vm) in [(3, 93020), (-1, 93004), (0, 93008)] {
        let got = factor_step(k);
        if got != Ok(vm) {
            failures += 1;
            println!("DIFF factor_step({k}) EMITTED {:?}; VM = {vm}", got);
        }
    }
    if failures == 0 {
        println!("rewrite_climb: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_rewrite_climb() {
    let diffs = run_lane(
        "tests/fixtures/constructor/rewrite_climb.emath",
        REWRITE_CLIMB_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted rewrite_climb disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const UNION_EXACT_COMPARE_DRIVER: &str = r#"
//! A run-time program's union value compared with exact Int
//! arithmetic: the emitted comparison must agree with the VM.
use union_exact_compare::divisible_values;
use union_exact_compare::emath_rt::code::{open_expr, CodeValue, ExprCode};
use union_exact_compare::emath_rt::code_tree::{CodeTree, TreeBinary, TreeDomain};

// quote n in Int: n * 6
fn six_n() -> ExprCode {
    open_expr(
        Vec::new(),
        CodeTree::Closure {
            param: "n".to_string(),
            domain: TreeDomain::Scalar("Int".to_string()),
            body: Box::new(CodeTree::Binary {
                op: TreeBinary::Mul,
                left: Box::new(CodeTree::Path(vec!["n".to_string()])),
                right: Box::new(CodeTree::Literal(CodeValue::Int(6))),
            }),
        },
        None,
        std::collections::BTreeMap::new(),
    )
}

fn main() {
    let mut failures = 0usize;
    let got = divisible_values(six_n(), 3);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF three_divides_six_n EMITTED {:?}; VM = true", got);
    }
    let got = divisible_values(six_n(), 4);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF four_misses_six_n EMITTED {:?}; VM = false", got);
    }
    if failures == 0 {
        println!("union_exact_compare: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_union_exact_compare() {
    let diffs = run_lane(
        "tests/fixtures/constructor/union_exact_compare.emath",
        UNION_EXACT_COMPARE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted union_exact_compare disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
const GENERIC_RECORD_DRIVER: &str = r#"
//! One generic object at two carriers: each instantiation has its own
//! native layout, and the generic fold reads it at the bound carrier.
use generic_record::{adding, maxing, peak, total};
fn main() {
    let mut failures = 0usize;
    let got = total(vec![3, 4, 5]);
    if !matches!(&got, Ok(12)) {
        failures += 1;
        println!("DIFF sums EMITTED {:?}; VM = 12", got);
    }
    let got = peak(vec![0.5, 2.5, 1.0]);
    if !matches!(&got, Ok(value) if *value == 2.5) {
        failures += 1;
        println!("DIFF largest EMITTED {:?}; VM = 2.5", got);
    }
    let got = peak(vec![]);
    if !matches!(&got, Ok(value) if *value == -1.0) {
        failures += 1;
        println!("DIFF empty_is_floor EMITTED {:?}; VM = -1.0", got);
    }
    let unit: Result<i64, String> = adding(7).map(|m| m.unit);
    if !matches!(&unit, Ok(0)) {
        failures += 1;
        println!("DIFF adding.unit EMITTED {:?}; VM = 0", unit);
    }
    let unit: Result<f64, String> = maxing(-3.0).map(|m| m.unit);
    if !matches!(&unit, Ok(value) if *value == -3.0) {
        failures += 1;
        println!("DIFF maxing.unit EMITTED {:?}; VM = -3.0", unit);
    }
    if failures == 0 {
        println!("generic_record: all pins hold");
    }
}
"#;
#[test]
fn emission_conformance_generic_record() {
    let diffs = run_lane(
        "tests/fixtures/constructor/generic_record.emath",
        GENERIC_RECORD_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted generic_record disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const FLOAT64_QUOTE_DRIVER: &str = r#"
//! Authored pins of the Float64 quote lane against the emitted crate:
//! a unary Float64 function template (one open constant) through
//! substitute, evaluate, and the application seam, plus the
//! open-code guard. The compiled factory is `emath_rt::code::open`
//! monomorphized over f64; no interpreter runs.

use float64_quote::{open_refuses, scaled_shift};

fn main() {
    let mut failures = 0usize;

    // example <two_x_plus_one>: scaled_shift(2.0, 3.0) == 7.0
    let got = scaled_shift(2.0, 3.0);
    if !matches!(&got, Ok(value) if *value == 7.0) {
        failures += 1;
        println!("DIFF two_x_plus_one EMITTED {:?}; VM = 7.0", got);
    }

    // example <zero_scale_shifts_to_one>: scaled_shift(0.0, 5.0) == 1.0
    let got = scaled_shift(0.0, 5.0);
    if !matches!(&got, Ok(value) if *value == 1.0) {
        failures += 1;
        println!("DIFF zero_scale_shifts_to_one EMITTED {:?}; VM = 1.0", got);
    }

    // example <negative_scale>: scaled_shift(-1.5, 2.0) == -2.0
    let got = scaled_shift(-1.5, 2.0);
    if !matches!(&got, Ok(value) if *value == -2.0) {
        failures += 1;
        println!("DIFF negative_scale EMITTED {:?}; VM = -2.0", got);
    }

    // example <unbound_open_code_refuses>: evaluate of open code
    // refuses `unbound_code`, naming the remaining open constant.
    let got = open_refuses(0.0);
    match &got {
        Err(text) if text.contains("unbound_code") => {}
        other => {
            failures += 1;
            println!(
                "DIFF unbound_open_code_refuses EMITTED {:?}; VM = unbound_code refusal",
                other
            );
        }
    }

    if failures == 0 {
        println!("float64_quote: all pins hold");
    }
}
"#;

#[test]
fn emission_conformance_float64_quote() {
    let diffs = run_lane(
        "tests/fixtures/constructor/float64_quote.emath",
        FLOAT64_QUOTE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted float64_quote disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const RESIDUES_DRIVER: &str = r#"
//! Authored pins of exact/residues.emath against the emitted crate.
//!
//! Transcription boundaries (labeled, from the module header):
//! - The numeric lane (mod_pos .. mod_pow) emits
//!   `Result<ExactInt, String>`; pins compare via `ExactInt::from`.
//! - Bezout rows pin the IDENTITY `a*x + b*y == g`, not just g.
//! - The Carmichael rows are the module's honesty pins: 561 passes
//!   Fermat for every coprime base (a probable-prime LIE, pinned).

use residues::{
    bezout, bezout_lift, fermat_all_pass, fermat_witness, is_fermat_prime, mod_add, mod_inv,
    mod_mul, mod_pos, mod_pow, shares_factor, EmathRecord_Bezout,
};

fn ei(n: i64) -> residues::emath_rt::ExactInt {
    residues::emath_rt::ExactInt::from(n)
}

fn exact_ok(got: &Result<residues::emath_rt::ExactInt, String>, want: i64) -> bool {
    matches!(got, Ok(v) if *v == ei(want))
}

fn refused<T: std::fmt::Debug>(got: &Result<T, String>, code: &str) -> bool {
    matches!(got, Err(text) if text == code)
}

/// mod_mul(a, inv, m) == 1 — the authored composed inverse check.
fn inverse_roundtrip(a: i64, m: i64) -> Result<residues::emath_rt::ExactInt, String> {
    mod_inv(a, m).and_then(|v| match v.to_i64() {
        Some(i) => mod_mul(a, i, m),
        None => Err(String::from("E-INT-002")),
    })
}

fn main() {
    let mut failures = 0usize;

    // --- mod_pos ---------------------------------------------------------
    // example <positive_unchanged>: mod_pos(7, 5) == 2
    let got = mod_pos(7, 5);
    if !exact_ok(&got, 2) {
        failures += 1;
        println!("DIFF positive_unchanged EMITTED {:?}; VM = 2", got);
    }

    // example <negative_wraps_once>: mod_pos(-7, 5) == 3
    let got = mod_pos(-7, 5);
    if !exact_ok(&got, 3) {
        failures += 1;
        println!("DIFF negative_wraps_once EMITTED {:?}; VM = 3", got);
    }

    // example <exact_multiple_is_zero>: mod_pos(10, 5) == 0
    let got = mod_pos(10, 5);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF exact_multiple_is_zero EMITTED {:?}; VM = 0", got);
    }

    // example <refuses_zero_modulus>
    let got = mod_pos(3, 0);
    if !refused(&got, "non_positive_modulus") {
        failures += 1;
        println!("DIFF refuses_zero_modulus EMITTED {:?}; VM = non_positive_modulus", got);
    }

    // --- mod_add / mod_mul ------------------------------------------------
    // example <wraps>: mod_add(3, 4, 5) == 2
    let got = mod_add(3, 4, 5);
    if !exact_ok(&got, 2) {
        failures += 1;
        println!("DIFF wraps EMITTED {:?}; VM = 2", got);
    }

    // example <three_times_four_mod_five>: mod_mul(3, 4, 5) == 2
    let got = mod_mul(3, 4, 5);
    if !exact_ok(&got, 2) {
        failures += 1;
        println!("DIFF three_times_four_mod_five EMITTED {:?}; VM = 2", got);
    }

    // example <negative_factor>: mod_mul(-3, 4, 5) == 3
    let got = mod_mul(-3, 4, 5);
    if !exact_ok(&got, 3) {
        failures += 1;
        println!("DIFF negative_factor EMITTED {:?}; VM = 3", got);
    }

    // --- bezout: the certificate is the identity, not just g -------------
    // example <bezout_twelve_eight>: g == 4 and 12x + 8y == g
    let got = bezout(12, 8);
    match &got {
        Ok(r) if r.g == 4 && 12 * r.x + 8 * r.y == r.g => {}
        _ => {
            failures += 1;
            println!("DIFF bezout_twelve_eight EMITTED {:?}; VM = g 4 with 12x + 8y == g", got);
        }
    }

    // example <bezout_seven_five>: g == 1 and 7x + 5y == g
    let got = bezout(7, 5);
    match &got {
        Ok(r) if r.g == 1 && 7 * r.x + 5 * r.y == r.g => {}
        _ => {
            failures += 1;
            println!("DIFF bezout_seven_five EMITTED {:?}; VM = g 1 with 7x + 5y == g", got);
        }
    }

    // example <bezout_240_46>: g == 2 and 240x + 46y == g
    let got = bezout(240, 46);
    match &got {
        Ok(r) if r.g == 2 && 240 * r.x + 46 * r.y == r.g => {}
        _ => {
            failures += 1;
            println!("DIFF bezout_240_46 EMITTED {:?}; VM = g 2 with 240x + 46y == g", got);
        }
    }

    // example <refuses_negative>
    let got = bezout(-12, 8);
    if !refused(&got, "negative_operand") {
        failures += 1;
        println!("DIFF refuses_negative EMITTED {:?}; VM = negative_operand", got);
    }

    // example <one_lift>: bezout_lift({g 4, x 0, y 1}, 12, 8) has
    // x == 1, y == -1, and 12x + 8y == g == 4.
    let got = bezout_lift(EmathRecord_Bezout { g: 4, x: 0, y: 1 }, 12, 8);
    match &got {
        Ok(r) if r.x == 1 && r.y == -1 && 12 * r.x + 8 * r.y == r.g => {}
        _ => {
            failures += 1;
            println!("DIFF one_lift EMITTED {:?}; VM = x 1, y -1, 12x + 8y == 4", got);
        }
    }

    // --- mod_inv ----------------------------------------------------------
    // example <three_times_inverse_is_one>: mod_mul(3, inv, 5) == 1
    let got = inverse_roundtrip(3, 5);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF three_times_inverse_is_one EMITTED {:?}; VM = mod_mul(3, inv, 5) == 1", got);
    }

    // example <seven_mod_twenty_six>: inv == 15 and 7*15 == 1 (mod 26)
    let got = mod_inv(7, 26);
    let composed = inverse_roundtrip(7, 26);
    if !exact_ok(&got, 15) || !exact_ok(&composed, 1) {
        failures += 1;
        println!("DIFF seven_mod_twenty_six EMITTED {:?} (composed {:?}); VM = 15 with 7*15 == 1 mod 26", got, composed);
    }

    // example <refuses_even_key_mod_twenty_six>
    let got = mod_inv(2, 26);
    if !refused(&got, "non_invertible_key") {
        failures += 1;
        println!("DIFF refuses_even_key_mod_twenty_six EMITTED {:?}; VM = non_invertible_key", got);
    }

    // --- mod_pow ----------------------------------------------------------
    // example <two_to_ten_mod_seven>: 2^10 == 2 (mod 7)
    let got = mod_pow(2, 10, 7);
    if !exact_ok(&got, 2) {
        failures += 1;
        println!("DIFF two_to_ten_mod_seven EMITTED {:?}; VM = 2", got);
    }

    // example <three_to_hundred_mod_one_oh_one>: 3^100 == 1 (mod 101)
    let got = mod_pow(3, 100, 101);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF three_to_hundred_mod_one_oh_one EMITTED {:?}; VM = 1", got);
    }

    // example <zero_to_positive_is_zero>: 0^5 == 0 (mod 7)
    let got = mod_pow(0, 5, 7);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF zero_to_positive_is_zero EMITTED {:?}; VM = 0", got);
    }

    // example <refuses_negative_exponent>
    let got = mod_pow(2, -1, 7);
    if !refused(&got, "negative_exponent") {
        failures += 1;
        println!("DIFF refuses_negative_exponent EMITTED {:?}; VM = negative_exponent", got);
    }

    // --- Fermat lane and the Carmichael honesty pins ----------------------
    // example <three_shares_with_five_sixty_one>: gcd(3, 561) = 3 > 1
    let got = shares_factor(3, 561);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF three_shares_with_five_sixty_one EMITTED {:?}; VM = true", got);
    }

    // example <coprime_pair>: gcd(5, 561) = 1
    let got = shares_factor(5, 561);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF coprime_pair EMITTED {:?}; VM = false", got);
    }

    // example <two_witnesses_fifteen>: 2^14 == 4 (mod 15), witnessed
    let got = fermat_witness(2, 15);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF two_witnesses_fifteen EMITTED {:?}; VM = true", got);
    }

    // example <no_witness_for_eleven>: 11 is prime
    let got = fermat_witness(2, 11);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF no_witness_for_eleven EMITTED {:?}; VM = false", got);
    }

    // example <base_sharing_factor_is_not_fermat_witness>: gcd(3, 15) = 3
    let got = fermat_witness(3, 15);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF base_sharing_factor_is_not_fermat_witness EMITTED {:?}; VM = false", got);
    }

    // example <eleven_passes>: is_fermat_prime(11, [2, 3, 5])
    let got = is_fermat_prime(11, vec![2, 3, 5]);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF eleven_passes EMITTED {:?}; VM = true", got);
    }

    // example <fifteen_fails_on_two>: is_fermat_prime(15, [2])
    let got = is_fermat_prime(15, vec![2]);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF fifteen_fails_on_two EMITTED {:?}; VM = false", got);
    }

    // example <two_passes_fifteen_fails>: fermat_all_pass(15, [2, 7], 0)
    let got = fermat_all_pass(15, vec![2, 7], 0);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF two_passes_fifteen_fails EMITTED {:?}; VM = false", got);
    }

    // example <carmichael_passes_falsely>: THE honesty pin — 561 =
    // 3*11*17 passes Fermat for every coprime base: probable-prime,
    // NOT prime.
    let got = is_fermat_prime(561, vec![2, 7, 13]);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF carmichael_passes_falsely EMITTED {:?}; VM = true (the Carmichael lie)", got);
    }
    // pinned per-base so the lie cannot hide behind the set.
    for base in [2, 7, 13] {
        let got = fermat_witness(base, 561);
        if got != Ok(false) {
            failures += 1;
            println!("DIFF carmichael_passes_falsely EMITTED {:?}; VM = false (base {base})", got);
        }
    }

    // example <carmichael_certified_composite_by_factor>: the SAME
    // number is certified composite by the factor proof.
    let got = is_fermat_prime(561, vec![3]);
    let factor = shares_factor(3, 561);
    if got != Ok(false) || factor != Ok(true) {
        failures += 1;
        println!("DIFF carmichael_certified_composite_by_factor EMITTED {:?} / {:?}; VM = false / true", got, factor);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all residues pins conform");
}
"#;

#[test]
fn emission_conformance_residues() {
    let diffs = run_lane("language/modules/exact/residues.emath", RESIDUES_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted residues disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const ROUNDTRIP_DRIVER: &str = r#"
//! Authored pins of the classical ciphers in
//! domains/cryptology/roundtrip.emath against the emitted crate.
//!
//! Transcription boundaries (labeled, from the module header):
//! - The affine lane emits `Result<ExactInt, String>`; pins compare
//!   via `ExactInt::from`.
//! - The affine roundtrip rows are authored as composed calls
//!   `decrypt(encrypt(p))`; the driver composes through the i64 ABI
//!   seam with a `to_i64` crossing (E-INT-002 named if it ever fires).
//! - Vigenere sequence results carry ExactRatio `(i128, i128)`
//!   elements; integer expectations compare by cross-multiplication.

use roundtrip::{
    affine_decrypt, affine_encrypt, vigenere_decrypt, vigenere_encrypt, vigenere_key_at,
};

fn ei(n: i64) -> roundtrip::emath_rt::ExactInt {
    roundtrip::emath_rt::ExactInt::from(n)
}

fn exact_ok(got: &Result<roundtrip::emath_rt::ExactInt, String>, want: i64) -> bool {
    matches!(got, Ok(v) if *v == ei(want))
}

fn refused<T: std::fmt::Debug>(got: &Result<T, String>, code: &str) -> bool {
    matches!(got, Err(text) if text == code)
}

fn q_eq(a: (i128, i128), b: (i128, i128)) -> bool {
    a.0 * b.1 == b.0 * a.1
}

/// decrypt(encrypt(p)) through the i64 ABI seam (authored composition).
fn affine_roundtrip(
    p: i64,
    a: i64,
    b: i64,
    m: i64,
) -> Result<roundtrip::emath_rt::ExactInt, String> {
    affine_encrypt(p, a, b, m).and_then(|v| match v.to_i64() {
        Some(c) => affine_decrypt(c, a, b, m),
        None => Err(String::from("E-INT-002")),
    })
}

fn main() {
    let mut failures = 0usize;

    // --- affine cipher ----------------------------------------------------
    // example <classic_three_five>: affine_encrypt(4, 3, 5, 26) == 17
    let got = affine_encrypt(4, 3, 5, 26);
    if !exact_ok(&got, 17) {
        failures += 1;
        println!("DIFF classic_three_five EMITTED {:?}; VM = 17", got);
    }

    // example <refuses_even_multiplier>
    let got = affine_encrypt(4, 2, 5, 26);
    if !refused(&got, "non_invertible_key") {
        failures += 1;
        println!("DIFF refuses_even_multiplier EMITTED {:?}; VM = non_invertible_key", got);
    }

    // example <classic_roundtrip_four>: affine_decrypt(17, 3, 5, 26) == 4
    let got = affine_decrypt(17, 3, 5, 26);
    if !exact_ok(&got, 4) {
        failures += 1;
        println!("DIFF classic_roundtrip_four EMITTED {:?}; VM = 4", got);
    }

    // example <roundtrip_law_everywhere>: decrypt(encrypt(p)) == p
    // (3 and 6 shown in the module; all seven residues pinned below).
    let got = affine_roundtrip(3, 5, 8, 7);
    if !exact_ok(&got, 3) {
        failures += 1;
        println!("DIFF roundtrip_law_everywhere EMITTED {:?}; VM = 3", got);
    }
    let got = affine_roundtrip(6, 5, 8, 7);
    if !exact_ok(&got, 6) {
        failures += 1;
        println!("DIFF roundtrip_law_everywhere_six EMITTED {:?}; VM = 6", got);
    }

    // example <roundtrip_exhaustive_all_seven>: the remaining residues
    for p in [0, 1, 2, 4, 5] {
        let got = affine_roundtrip(p, 5, 8, 7);
        if !exact_ok(&got, p) {
            failures += 1;
            println!("DIFF roundtrip_exhaustive_all_seven EMITTED {:?}; VM = {p}", got);
        }
    }

    // --- vigenere ---------------------------------------------------------
    // example <cycles>: key_at([3, 1, 4], 0) == 3
    let got = vigenere_key_at(vec![3, 1, 4], 0);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF cycles EMITTED {:?}; VM = 3", got);
    }

    // example <wraps_to_front>: key_at([3, 1, 4], 3) == 3
    let got = vigenere_key_at(vec![3, 1, 4], 3);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF wraps_to_front EMITTED {:?}; VM = 3", got);
    }

    // example <refuses_empty_key>
    let got = vigenere_key_at(vec![], 0);
    if !refused(&got, "empty_sequence") {
        failures += 1;
        println!("DIFF refuses_empty_key EMITTED {:?}; VM = empty_sequence", got);
    }

    // example <classic_shifts>: HELLO with key CAD, m 26 -> 10 5 15 14 15.
    // The generic tabulate keeps the Int element carrier.
    let got = vigenere_encrypt(vec![7, 4, 11, 11, 14], vec![3, 1, 4], 26);
    match &got {
        Ok(v) if *v == [10i64, 5, 15, 14, 15].map(roundtrip::emath_rt::ExactInt::from) => {}
        _ => {
            failures += 1;
            println!("DIFF classic_shifts EMITTED {:?}; VM = [10, 5, 15, 14, 15]", got);
        }
    }

    // example <roundtrip_hello>: decrypt(encrypt) recovers HELLO.
    let got = vigenere_decrypt(vec![10, 5, 15, 14, 15], vec![3, 1, 4], 26);
    match &got {
        Ok(v) if *v == [7i64, 4, 11, 11, 14].map(roundtrip::emath_rt::ExactInt::from) => {}
        _ => {
            failures += 1;
            println!("DIFF roundtrip_hello EMITTED {:?}; VM = [7, 4, 11, 11, 14]", got);
        }
    }

    // example <refuses_empty_message>
    let got = vigenere_decrypt(vec![], vec![3, 1, 4], 26);
    if !refused(&got, "empty_sequence") {
        failures += 1;
        println!("DIFF refuses_empty_message EMITTED {:?}; VM = empty_sequence", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all cipher pins conform");
}
"#;

#[test]
fn emission_conformance_ciphers() {
    let diffs = run_lane(
        "language/domains/cryptology/roundtrip.emath",
        ROUNDTRIP_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted ciphers disagree with the VM on:\n{}",
        diffs.join("\n")
    );
}

const QUADRATIC_DRIVER: &str = r#"
//! Authored pins of exact/quadratic.emath against the emitted crate.
//!
//! Transcription boundaries (labeled, from the module header):
//! - The Tonelli-Shanks entry lane (legendre, ts_q, tonelli_sqrt,
//!   ts_entry, ts_general) emits `Result<ExactInt, String>`; the
//!   arithmetic-helper lane (ts_two_pow .. ts_finish) emits
//!   `Result<i64, String>`; pins compare accordingly.
//! - Every tonelli row pins the CERTIFICATE r*r == a (mod p) as an
//!   exact identity alongside the root value.
//! - Refusal inventory: non_positive_modulus, even_modulus,
//!   not_a_quadratic_residue.

use quadratic::{
    is_quadratic_residue, legendre, legendre_of, tonelli_sqrt, ts_entry, ts_find_i, ts_find_z,
    ts_finish, ts_general, ts_halve, ts_loop, ts_q, ts_s, ts_two_adic, ts_two_pow,
};

fn ei(n: i64) -> quadratic::emath_rt::ExactInt {
    quadratic::emath_rt::ExactInt::from(n)
}

fn exact_ok(got: &Result<quadratic::emath_rt::ExactInt, String>, want: i64) -> bool {
    matches!(got, Ok(v) if *v == ei(want))
}

fn refused<T: std::fmt::Debug>(got: &Result<T, String>, code: &str) -> bool {
    matches!(got, Err(text) if text == code)
}

/// The authored certificate: mod_pos(r*r, p) == a for the emitted root.
fn cert(root: &quadratic::emath_rt::ExactInt, p: i64, want: i64) -> bool {
    root.to_i64().map(|r| r * r % p == want).unwrap_or(false)
}

fn main() {
    let mut failures = 0usize;

    // --- legendre symbol --------------------------------------------------
    // example <two_is_residue_mod_seven>: (2|7) == 1
    let got = legendre(2, 7);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF two_is_residue_mod_seven EMITTED {:?}; VM = 1", got);
    }

    // example <three_is_nonresidue_mod_seven>: (3|7) == -1
    let got = legendre(3, 7);
    if !exact_ok(&got, -1) {
        failures += 1;
        println!("DIFF three_is_nonresidue_mod_seven EMITTED {:?}; VM = -1", got);
    }

    // example <zero_is_zero>: (0|7) == 0
    let got = legendre(0, 7);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF zero_is_zero EMITTED {:?}; VM = 0", got);
    }

    // example <negative_reduces_first>: (-5|7) == (2|7) == 1
    let got = legendre(-5, 7);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF negative_reduces_first EMITTED {:?}; VM = 1", got);
    }

    // example <refuses_zero_modulus>
    let got = legendre(2, 0);
    if !refused(&got, "non_positive_modulus") {
        failures += 1;
        println!("DIFF refuses_zero_modulus EMITTED {:?}; VM = non_positive_modulus", got);
    }

    // --- legendre on the already-reduced residue ---------------------------
    // example <residue_is_one>: legendre_of(2, 7) == 1
    let got = legendre_of(2, 7);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF residue_is_one EMITTED {:?}; VM = 1", got);
    }

    // example <nonresidue_is_minus_one>: legendre_of(3, 7) == -1
    let got = legendre_of(3, 7);
    if !exact_ok(&got, -1) {
        failures += 1;
        println!("DIFF nonresidue_is_minus_one EMITTED {:?}; VM = -1", got);
    }

    // example <zero_residue_is_zero>: legendre_of(0, 7) == 0
    let got = legendre_of(0, 7);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF zero_residue_is_zero EMITTED {:?}; VM = 0", got);
    }

    // --- the residue predicate --------------------------------------------
    // example <two_mod_seven>: 2 is a QR mod 7
    let got = is_quadratic_residue(2, 7);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF two_mod_seven EMITTED {:?}; VM = true", got);
    }

    // example <three_mod_seven>: 3 is not
    let got = is_quadratic_residue(3, 7);
    if got != Ok(false) {
        failures += 1;
        println!("DIFF three_mod_seven EMITTED {:?}; VM = false", got);
    }

    // --- Tonelli-Shanks arithmetic helpers --------------------------------
    // example <three>: 2^3 == 8
    let got = ts_two_pow(3);
    if got != Ok(8) {
        failures += 1;
        println!("DIFF three EMITTED {:?}; VM = 8", got);
    }

    // example <eight_is_three>: the 2-adic valuation of 8
    let got = ts_two_adic(8);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF eight_is_three EMITTED {:?}; VM = 3", got);
    }

    // example <odd_is_zero>: 5 is odd
    let got = ts_two_adic(5);
    if got != Ok(0) {
        failures += 1;
        println!("DIFF odd_is_zero EMITTED {:?}; VM = 0", got);
    }

    // example <eight_thrice>: 8 halved 3 times is 1
    let got = ts_halve(8, 3);
    if got != Ok(1) {
        failures += 1;
        println!("DIFF eight_thrice EMITTED {:?}; VM = 1", got);
    }

    // example <seventeen>: q = (17-1)/2^4 == 1
    let got = ts_q(17);
    if !exact_ok(&got, 1) {
        failures += 1;
        println!("DIFF seventeen EMITTED {:?}; VM = 1", got);
    }

    // example <forty_one>: q = 40/2^3 == 5
    let got = ts_q(41);
    if !exact_ok(&got, 5) {
        failures += 1;
        println!("DIFF forty_one EMITTED {:?}; VM = 5", got);
    }

    // example <seventeen_is_four>: s(17) == 4
    let got = ts_s(17);
    if got != Ok(4) {
        failures += 1;
        println!("DIFF seventeen_is_four EMITTED {:?}; VM = 4", got);
    }

    // example <forty_one_is_three>: s(41) == 3
    let got = ts_s(41);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF forty_one_is_three EMITTED {:?}; VM = 3", got);
    }

    // example <seventeen_finds_three>: the first nonresidue from z=2
    let got = ts_find_z(2, 17);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF seventeen_finds_three EMITTED {:?}; VM = 3", got);
    }

    // example <two_mod_seventeen>: the doubling index of t=2
    let got = ts_find_i(2, 17, 0);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF two_mod_seventeen EMITTED {:?}; VM = 3", got);
    }

    // example <seventeen_two_full_cycle>: one full Tonelli cycle
    let got = ts_loop(17, 4, 3, 2, 2);
    if got != Ok(6) {
        failures += 1;
        println!("DIFF seventeen_two_full_cycle EMITTED {:?}; VM = 6", got);
    }

    // --- the exactness gate and tie-break ----------------------------------
    // example <gate_passes_and_breaks_tie>: min(r, p - r)
    let got = ts_finish(2, 4, 7);
    if got != Ok(3) {
        failures += 1;
        println!("DIFF gate_passes_and_breaks_tie EMITTED {:?}; VM = 3", got);
    }

    // example <gate_refuses_fabricated_root>: 2*2 != 3 (mod 7)
    let got = ts_finish(3, 2, 7);
    if !refused(&got, "not_a_quadratic_residue") {
        failures += 1;
        println!("DIFF gate_refuses_fabricated_root EMITTED {:?}; VM = not_a_quadratic_residue", got);
    }

    // --- tonelli_sqrt: value AND certificate on every computing row --------
    // example <fast_path_two_mod_seven>: sqrt(2, 7) == 3, 3*3 == 2 (mod 7)
    let got = tonelli_sqrt(2, 7);
    if !exact_ok(&got, 3) || !matches!(&got, Ok(v) if cert(v, 7, 2)) {
        failures += 1;
        println!("DIFF fast_path_two_mod_seven EMITTED {:?}; VM = 3 with r*r == 2 mod 7", got);
    }

    // example <fast_path_four_mod_seven>: sqrt(4, 7) == 2, 2*2 == 4
    let got = tonelli_sqrt(4, 7);
    if !exact_ok(&got, 2) || !matches!(&got, Ok(v) if cert(v, 7, 4)) {
        failures += 1;
        println!("DIFF fast_path_four_mod_seven EMITTED {:?}; VM = 2 with r*r == 4 mod 7", got);
    }

    // example <general_two_mod_seventeen>: full cycle, 6*6 == 36 == 2 (mod 17)
    let got = tonelli_sqrt(2, 17);
    if !exact_ok(&got, 6) || !matches!(&got, Ok(v) if cert(v, 17, 2)) {
        failures += 1;
        println!("DIFF general_two_mod_seventeen EMITTED {:?}; VM = 6 with r*r == 2 mod 17", got);
    }

    // example <general_eighteen_mod_forty_one>: 10*10 == 100 == 18 (mod 41)
    let got = tonelli_sqrt(18, 41);
    if !exact_ok(&got, 10) || !matches!(&got, Ok(v) if cert(v, 41, 18)) {
        failures += 1;
        println!("DIFF general_eighteen_mod_forty_one EMITTED {:?}; VM = 10 with r*r == 18 mod 41", got);
    }

    // example <zero_returns_zero>
    let got = tonelli_sqrt(0, 7);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF zero_returns_zero EMITTED {:?}; VM = 0", got);
    }

    // example <negative_reduces_to_residue>: sqrt(-5, 7) == sqrt(2, 7) == 3
    let got = tonelli_sqrt(-5, 7);
    if !exact_ok(&got, 3) || !matches!(&got, Ok(v) if cert(v, 7, 2)) {
        failures += 1;
        println!("DIFF negative_reduces_to_residue EMITTED {:?}; VM = 3 with r*r == 2 mod 7", got);
    }

    // example <refuses_nonresidue_fast_path>
    let got = tonelli_sqrt(3, 7);
    if !refused(&got, "not_a_quadratic_residue") {
        failures += 1;
        println!("DIFF refuses_nonresidue_fast_path EMITTED {:?}; VM = not_a_quadratic_residue", got);
    }

    // example <refuses_nonresidue_general_path>
    let got = tonelli_sqrt(3, 17);
    if !refused(&got, "not_a_quadratic_residue") {
        failures += 1;
        println!("DIFF refuses_nonresidue_general_path EMITTED {:?}; VM = not_a_quadratic_residue", got);
    }

    // example <refuses_even_modulus_two>
    let got = tonelli_sqrt(1, 2);
    if !refused(&got, "even_modulus") {
        failures += 1;
        println!("DIFF refuses_even_modulus_two EMITTED {:?}; VM = even_modulus", got);
    }

    // example <refuses_even_modulus_eight>
    let got = tonelli_sqrt(1, 8);
    if !refused(&got, "even_modulus") {
        failures += 1;
        println!("DIFF refuses_even_modulus_eight EMITTED {:?}; VM = even_modulus", got);
    }

    // example <refuses_nonpositive_modulus>
    let got = tonelli_sqrt(1, 0);
    if !refused(&got, "non_positive_modulus") {
        failures += 1;
        println!("DIFF refuses_nonpositive_modulus EMITTED {:?}; VM = non_positive_modulus", got);
    }

    // --- the entry dispatch -------------------------------------------------
    // example <zero_short_circuits>: ts_entry(0, 7) == 0
    let got = ts_entry(0, 7);
    if !exact_ok(&got, 0) {
        failures += 1;
        println!("DIFF zero_short_circuits EMITTED {:?}; VM = 0", got);
    }

    // example <two_mod_seventeen>: ts_general(2, 17) == 6, 36 == 2 (mod 17)
    let got = ts_general(2, 17);
    if !exact_ok(&got, 6) || !matches!(&got, Ok(v) if cert(v, 17, 2)) {
        failures += 1;
        println!("DIFF two_mod_seventeen EMITTED {:?}; VM = 6 with r*r == 2 mod 17", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all quadratic pins conform");
}
"#;

#[test]
fn emission_conformance_quadratic() {
    let diffs = run_lane("language/modules/exact/quadratic.emath", QUADRATIC_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted quadratic disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Bounds seam pin: the integer/rational branch join
/// (`if measured == 0: 0 else: lower_bound / measured`) — the int arm
/// widens to the ratio carrier exactly as the VM's Int-to-Rat join
/// does, and the entry carries the declared Rat output.
const BOUNDS_DRIVER: &str = r#"
//! Authored pins of numerics/bounds.emath against the emitted crate.

use bounds::frontier_efficiency;

fn rat_ok(got: &Result<(i128, i128), String>, want: (i128, i128)) -> bool {
    matches!(got, Ok(v) if *v == want)
}

fn main() {
    let mut failures = 0usize;

    // example <exact>: 3 / 4
    let got = frontier_efficiency(3, 4);
    if !rat_ok(&got, (3, 4)) {
        failures += 1;
        println!("DIFF exact EMITTED {:?}; VM = 3/4", got);
    }

    // example <zero_measured>: the int arm joins the Rat divide — 0 / 1
    let got = frontier_efficiency(3, 0);
    if !rat_ok(&got, (0, 1)) {
        failures += 1;
        println!("DIFF zero_measured EMITTED {:?}; VM = 0/1", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all bounds pins conform");
}
"#;

#[test]
fn emission_conformance_bounds() {
    let diffs = run_lane("language/modules/numerics/bounds.emath", BOUNDS_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted bounds disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Surfaces seam pin: the labeled Float64 tier's mixed exact/float
/// arithmetic (`hf = hr * 1.0f64`, `result = hf * sum`) — the exact
/// ratio operand widens through the as-f64 coercion and the float
/// operand locks the carrier.
const SURFACES_DRIVER: &str = r#"
//! Authored pins of geometry/surfaces.emath against the emitted crate.

use std::rc::Rc;

use surfaces::{EmathRecord_Vec3, arc_len_float64};

fn constant_path() -> Rc<dyn Fn(surfaces::emath_rt::ExactRatio) -> Result<EmathRecord_Vec3, String>> {
    // speed 5 at every sample: (3, 4, 0)/1
    Rc::new(move |_t| {
        Ok(EmathRecord_Vec3 { x: (3, 1), y: (4, 1), z: (0, 1) })
    })
}

fn main() {
    let mut failures = 0usize;

    // example <constant_speed_sums_exactly>: sqrt(25) = 5, the
    // trapezoid of a constant over [0, 2] with 4 panels = 10.0
    let got = arc_len_float64(constant_path(), (0, 1), (2, 1), 4);
    if !matches!(&got, Ok(v) if *v == 10.0) {
        failures += 1;
        println!("DIFF constant_speed_sums_exactly EMITTED {:?}; VM = 10.0", got);
    }

    // example <offset_range_sums_exactly>: a = 1 != 0: 5*(b-a) = 10.0
    let got = arc_len_float64(constant_path(), (1, 1), (3, 1), 4);
    if !matches!(&got, Ok(v) if *v == 10.0) {
        failures += 1;
        println!("DIFF offset_range_sums_exactly EMITTED {:?}; VM = 10.0", got);
    }

    // example <refuses_bad_resolution>: n = 0
    let got = arc_len_float64(constant_path(), (0, 1), (2, 1), 0);
    if !matches!(&got, Err(text) if text == "bad_resolution") {
        failures += 1;
        println!("DIFF refuses_bad_resolution EMITTED {:?}; VM = bad_resolution", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all surfaces pins conform");
}
"#;

#[test]
fn emission_conformance_surfaces() {
    let diffs = run_lane("language/modules/geometry/surfaces.emath", SURFACES_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted surfaces disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Oscillations seam pin: the generic self-recursion instantiation
/// (tabulate_at at an `Int -> sequence(Rat)` closure — the CallSelf
/// result kind mirrors the render's crossing decision) through the
/// gated A = M^-1 K entry, plus the admission refusals.
const OSCILLATIONS_DRIVER: &str = r#"
//! Authored pins of mechanics/oscillations.emath against the emitted crate.

use oscillations::osc_mik_gated;

type Rat = (i128, i128);
type Matrix = Vec<Vec<Rat>>;

fn mat(rows: &[[[i128; 2]; 2]]) -> Matrix {
    rows.iter()
        .map(|row| row.iter().map(|[n, d]| (*n, *d)).collect())
        .collect()
}

fn main() {
    let mut failures = 0usize;

    // example <adsrm_gate_passes>: M = I, A is K itself.
    let got = osc_mik_gated(
        mat(&[[[1, 1], [0, 1]], [[0, 1], [1, 1]]]),
        mat(&[[[2, 1], [-1, 1]], [[-1, 1], [2, 1]]]),
    );
    let want = mat(&[[[2, 1], [-1, 1]], [[-1, 1], [2, 1]]]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF adsrm_gate_passes EMITTED {:?}; VM = [[2/1, -1/1], [-1/1, 2/1]]", got);
    }

    // example <refuses_bad_mass>: singular mass [[1, 2], [2, 4]]/1.
    let got = osc_mik_gated(
        mat(&[[[1, 1], [2, 1]], [[2, 1], [4, 1]]]),
        mat(&[[[2, 1], [-1, 1]], [[-1, 1], [2, 1]]]),
    );
    if !matches!(&got, Err(text) if text == "bad_mass") {
        failures += 1;
        println!("DIFF refuses_bad_mass EMITTED {:?}; VM = bad_mass", got);
    }

    // example <refuses_bad_matrix>: ragged stiffness.
    let got = osc_mik_gated(
        mat(&[[[1, 1], [0, 1]], [[0, 1], [1, 1]]]),
        vec![vec![(2, 1), (-1, 1)]],
    );
    if !matches!(&got, Err(text) if text == "bad_matrix") {
        failures += 1;
        println!("DIFF refuses_bad_matrix EMITTED {:?}; VM = bad_matrix", got);
    }

    // example <refuses_not_symmetric>: [[2, 1], [0, 2]].
    let got = osc_mik_gated(
        mat(&[[[1, 1], [0, 1]], [[0, 1], [1, 1]]]),
        mat(&[[[2, 1], [1, 1]], [[0, 1], [2, 1]]]),
    );
    if !matches!(&got, Err(text) if text == "not_symmetric") {
        failures += 1;
        println!("DIFF refuses_not_symmetric EMITTED {:?}; VM = not_symmetric", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all oscillations pins conform");
}
"#;

#[test]
fn emission_conformance_oscillations() {
    let diffs = run_lane(
        "language/domains/mechanics/oscillations.emath",
        OSCILLATIONS_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted oscillations disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const ORDER_DRIVER: &str = r#"
//! Authored pins of discrete/order.emath against the emitted crate.
//! The sort pins drive the mutual-recursion cycle (sort_loop calls
//! sort_step, sort_step calls sort_loop) through the emitted entries
//! for BOTH cycle members - the named-entry seam the emission
//! worklist settles.

use order::{EmathRecord_SortedPair, sort_loop, sort_step, sort_with_permutation, unsort};

type Rat = (i128, i128);

fn rats(rows: &[[i128; 2]]) -> Vec<Rat> {
    rows.iter().map(|[n, d]| (*n, *d)).collect()
}

fn ints(xs: &[i64]) -> Vec<i64> {
    xs.to_vec()
}

fn pair(values: &[[i128; 2]], perm: &[i64]) -> EmathRecord_SortedPair {
    EmathRecord_SortedPair { values: rats(values), perm: ints(perm) }
}

fn main() {
    let mut failures = 0usize;

    // example <simple_sort>: the whole cycle from the public entry.
    let got = sort_with_permutation(rats(&[[5, 2], [1, 2], [3, 2]]));
    let want = pair(&[[1, 2], [3, 2], [5, 2]], &[1, 2, 0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF simple_sort EMITTED {:?}; VM = values [1/2, 3/2, 5/2] perm [1, 2, 0]", got);
    }

    // the same row's unsort round-trip: unsort(result) == the input.
    if let Ok(sorted) = &got {
        let back = unsort(sorted.clone());
        if !matches!(&back, Ok(v) if *v == rats(&[[5, 2], [1, 2], [3, 2]])) {
            failures += 1;
            println!("DIFF simple_sort_unsort EMITTED {:?}; VM = [5/2, 1/2, 3/2]", back);
        }
    }

    // example <stable_first_index_ties>: equal keys keep ascending
    // original index.
    let got = sort_with_permutation(rats(&[[3, 1], [3, 1], [2, 1]]));
    let want = pair(&[[2, 1], [3, 1], [3, 1]], &[2, 0, 1]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF stable_first_index_ties EMITTED {:?}; VM = values [2/1, 3/1, 3/1] perm [2, 0, 1]", got);
    }

    // example <empty_sort>.
    let got = sort_with_permutation(vec![]);
    let want = pair(&[], &[]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF empty_sort EMITTED {:?}; VM = empty pair", got);
    }

    // example <singleton_sort>.
    let got = sort_with_permutation(rats(&[[7, 2]]));
    let want = pair(&[[7, 2]], &[0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF singleton_sort EMITTED {:?}; VM = values [7/2] perm [0]", got);
    }

    // example <green_start_state>: sort_loop entered at the cycle's
    // other member directly.
    let got = sort_loop(
        rats(&[[5, 2], [1, 2], [3, 2]]),
        ints(&[0, 1, 2]),
        vec![],
        vec![],
    );
    let want = pair(&[[1, 2], [3, 2], [5, 2]], &[1, 2, 0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF green_start_state EMITTED {:?}; VM = values [1/2, 3/2, 5/2] perm [1, 2, 0]", got);
    }

    // example <base_unwinds_reversed_accs>: empty remaining unwinds
    // the prepend-built accumulators.
    let got = sort_loop(
        rats(&[[5, 2], [1, 2], [3, 2]]),
        vec![],
        rats(&[[5, 2], [3, 2], [1, 2]]),
        ints(&[0, 2, 1]),
    );
    let want = pair(&[[1, 2], [3, 2], [5, 2]], &[1, 2, 0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF base_unwinds_reversed_accs EMITTED {:?}; VM = values [1/2, 3/2, 5/2] perm [1, 2, 0]", got);
    }

    // example <empty_start>.
    let got = sort_loop(vec![], vec![], vec![], vec![]);
    let want = pair(&[], &[]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF empty_start EMITTED {:?}; VM = empty pair", got);
    }

    // example <first_round_from_green_start>: sort_step entered
    // directly - the cycle edge back into sort_loop.
    let got = sort_step(
        rats(&[[5, 2], [1, 2], [3, 2]]),
        ints(&[0, 1, 2]),
        vec![],
        vec![],
    );
    let want = pair(&[[1, 2], [3, 2], [5, 2]], &[1, 2, 0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF first_round_from_green_start EMITTED {:?}; VM = values [1/2, 3/2, 5/2] perm [1, 2, 0]", got);
    }

    // example <entered_after_first_pick>: round 1 already applied.
    let got = sort_step(
        rats(&[[5, 2], [1, 2], [3, 2]]),
        ints(&[0, 2]),
        rats(&[[1, 2]]),
        ints(&[1]),
    );
    let want = pair(&[[1, 2], [3, 2], [5, 2]], &[1, 2, 0]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF entered_after_first_pick EMITTED {:?}; VM = values [1/2, 3/2, 5/2] perm [1, 2, 0]", got);
    }

    // example <tie_keeps_original_order>: the stable tie law through
    // sort_step's entry.
    let got = sort_step(
        rats(&[[3, 1], [3, 1], [2, 1]]),
        ints(&[0, 1, 2]),
        vec![],
        vec![],
    );
    let want = pair(&[[2, 1], [3, 1], [3, 1]], &[2, 0, 1]);
    if !matches!(&got, Ok(v) if *v == want) {
        failures += 1;
        println!("DIFF tie_keeps_original_order EMITTED {:?}; VM = values [2/1, 3/1, 3/1] perm [2, 0, 1]", got);
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all order pins conform");
}
"#;

#[test]
fn emission_conformance_order() {
    let diffs = run_lane("language/modules/discrete/order.emath", ORDER_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted order disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const WITNESS_DRIVER: &str = r#"
//! Authored pins of the operator-tag witness fixture against the
//! emitted crate: one row per witness example, the quoted form built
//! as a distilled tree (the same shape `quote(...)` distills). The
//! VM truth is the authored `expect result == true` row pinned in
//! step 1 of the lane; every row here must return `Ok(true)` from
//! the emitted entry - the tag-witness comparison law (`Add` bare,
//! `Eq:{}` empty record, and the unary/binary arms `Neg`, `Not`,
//! `Pos`, `And`, `Or`) is one law in both lanes.

use operator_tag_witness::{
    add_bare, and_witness, body_of_named_callee, body_of_operator_callee, eq_record, neg_witness,
    not_witness, or_witness, pos_witness,
};
use operator_tag_witness::emath_rt::code::{open_expr, CodeValue, ExprCode};
use operator_tag_witness::emath_rt::code_tree::{CodeTree, TreeBinary, TreeUnary};

fn int(n: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(n))
}

fn binary(op: TreeBinary, left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn unary(op: TreeUnary, value: CodeTree) -> CodeTree {
    CodeTree::Unary {
        op,
        value: Box::new(value),
    }
}

fn code(tree: CodeTree) -> ExprCode {
    open_expr(Vec::new(), tree, None, std::collections::BTreeMap::new())
}

fn main() {
    let mut failures = 0usize;

    // example <sums>: add_bare(quote(1 + 2)) == true
    let got = add_bare(code(binary(TreeBinary::Add, int(1), int(2))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF sums EMITTED {got:?}; VM = true");
    }

    // example <compares>: eq_record(quote(1 == 2)) == true
    let got = eq_record(code(binary(TreeBinary::Eq, int(1), int(2))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF compares EMITTED {got:?}; VM = true");
    }

    // example <negates>: neg_witness(quote(-(1 + 2))) == true
    let got = neg_witness(code(unary(TreeUnary::Neg, binary(TreeBinary::Add, int(1), int(2)))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF negates EMITTED {got:?}; VM = true");
    }

    // example <negates_bool>: not_witness(quote(not (1 < 2))) == true
    let got = not_witness(code(unary(TreeUnary::Not, binary(TreeBinary::Lt, int(1), int(2)))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF negates_bool EMITTED {got:?}; VM = true");
    }

    // example <conjoins>: and_witness(quote(1 < 2 and 2 < 3)) == true
    let got = and_witness(code(binary(
        TreeBinary::And,
        binary(TreeBinary::Lt, int(1), int(2)),
        binary(TreeBinary::Lt, int(2), int(3)),
    )));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF conjoins EMITTED {got:?}; VM = true");
    }

    // example <disjoins>: or_witness(quote(1 < 2 or 3 < 2)) == true
    let got = or_witness(code(binary(
        TreeBinary::Or,
        binary(TreeBinary::Lt, int(1), int(2)),
        binary(TreeBinary::Lt, int(3), int(2)),
    )));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF disjoins EMITTED {got:?}; VM = true");
    }

    // example <affirms>: pos_witness(quote(+(1 + 2))) == true
    let got = pos_witness(code(unary(TreeUnary::Pos, binary(TreeBinary::Add, int(1), int(2)))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF affirms EMITTED {got:?}; VM = true");
    }

    // example <operator_callee_is_opaque>: quote.body over the Add tag
    // callee is the no-identity Opaque record (the VM's non-Code law).
    let got = body_of_operator_callee(code(binary(TreeBinary::Add, int(1), int(2))));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF operator_callee_is_opaque EMITTED {got:?}; VM = true");
    }

    // example <named_callee_unfolds>: quote.body over the named callee
    // sq unfolds the definition table (transparent -> Available).
    let got = body_of_named_callee(code(CodeTree::Call {
        function: Box::new(CodeTree::Path(vec!["sq".to_string()])),
        args: vec![CodeTree::Path(vec!["y".to_string()])],
    }));
    if got != Ok(true) {
        failures += 1;
        println!("DIFF named_callee_unfolds EMITTED {got:?}; VM = true");
    }

    if failures > 0 {
        panic!("{failures} witness row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_operator_tag_witness() {
    let diffs = run_lane(
        "tests/fixtures/constructor/operator_tag_witness.emath",
        WITNESS_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted witness disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const MATCHING_DRIVER: &str = r#"
//! Authored pins of discrete/matching.emath against the emitted crate.
//!
//! The mixed-kind join pins (emath-49o8): replace_at's declared Rat
//! carriers instantiate over Int vectors, so some inlined arms carry
//! the ratio spelling while the recursive tails declare the Int
//! spelling - the branch join folds both to one carrier and the
//! recursion arguments narrow checked (E-INT-002 by name). The
//! entries settle onto the declared Int carriers (the ratio spelling
//! lives only inside the emitted arms), so every row compares the
//! emitted i64 vectors against the VM's integer answers directly.

use matching::{
    assign_max, asg_row, max_match_loop, max_matching, EmathRecord_Edge as Edge,
    EmathRecord_WeightedGraph as Wg,
};

fn g(vertices: i64, edges: &[(i64, i64, i64)]) -> Wg {
    Wg {
        vertices,
        edges: edges
            .iter()
            .map(|(from, to, weight)| Edge {
                from: *from,
                to: *to,
                weight: (*weight as i128, 1),
            })
            .collect(),
    }
}

fn i_eq(got: &Result<Vec<i64>, String>, want: &[i64]) -> Option<String> {
    match got {
        Ok(v) if v == want => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn main() {
    let mut failures = 0usize;

    // example <path_matches_two> (max_matching)
    let got = max_matching(g(4, &[(0, 1, 1), (1, 2, 1), (2, 3, 1)]));
    if let Some(diff) = i_eq(&got, &[1, 0, 3, 2]) {
        failures += 1;
        println!("DIFF path_matches_two EMITTED {diff}; VM = [1, 0, 3, 2]");
    }
    // example <star_matches_one>
    let got = max_matching(g(4, &[(0, 1, 1), (0, 2, 1), (0, 3, 1)]));
    if let Some(diff) = i_eq(&got, &[3, 0 - 1, 0 - 1, 0]) {
        failures += 1;
        println!("DIFF star_matches_one EMITTED {diff}; VM = [3, -1, -1, 0]");
    }
    // example <c6_matches_perfectly>
    let got = max_matching(g(
        6,
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1), (4, 5, 1), (5, 0, 1)],
    ));
    if let Some(diff) = i_eq(&got, &[5, 2, 1, 4, 3, 0]) {
        failures += 1;
        println!("DIFF c6_matches_perfectly EMITTED {diff}; VM = [5, 2, 1, 4, 3, 0]");
    }
    // example <non_bipartite_refuses>: the odd cycle refuses by name.
    let got = max_matching(g(3, &[(0, 1, 1), (1, 2, 1), (2, 0, 1)]));
    match got {
        Err(e) if e.contains("not_bipartite") => {}
        other => {
            failures += 1;
            println!("DIFF non_bipartite_refuses EMITTED {other:?}; VM = Err(not_bipartite)");
        }
    }

    // example <p4_grows_to_the_maximum> (max_match_loop: the grown
    // join - an augment result folding with the input m - recurses)
    let got = max_match_loop(
        vec![vec![1], vec![0, 2], vec![1, 3], vec![2]],
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
        vec![0, 1, 0, 1],
        4,
    );
    if let Some(diff) = i_eq(&got, &[1, 0, 3, 2]) {
        failures += 1;
        println!("DIFF p4_grows_to_the_maximum EMITTED {diff}; VM = [1, 0, 3, 2]");
    }
    // example <exhausted_input_returns_the_partner_vector>
    let got = max_match_loop(
        vec![vec![1], vec![0, 2], vec![1, 3], vec![2]],
        vec![1, 0, 3, 2],
        vec![0, 1, 0, 1],
        4,
    );
    if let Some(diff) = i_eq(&got, &[1, 0, 3, 2]) {
        failures += 1;
        println!("DIFF exhausted_input_returns_the_partner_vector EMITTED {diff}; VM = [1, 0, 3, 2]");
    }
    // example <star_matches_the_last_declared_leaf>
    let got = max_match_loop(
        vec![vec![1, 2, 3], vec![0], vec![0], vec![0]],
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
        vec![0, 1, 1, 1],
        4,
    );
    if let Some(diff) = i_eq(&got, &[3, 0 - 1, 0 - 1, 0]) {
        failures += 1;
        println!("DIFF star_matches_the_last_declared_leaf EMITTED {diff}; VM = [3, -1, -1, 0]");
    }

    // example <picks_the_heavier_assignment> (assign_max: the full
    // asg_rec/asg_row recursion from the empty baseline)
    let got = assign_max(g(
        4,
        &[(0, 2, 3), (0, 3, 1), (1, 2, 1), (1, 3, 1)],
    ));
    if let Some(diff) = i_eq(&got, &[2, 3, 0, 1]) {
        failures += 1;
        println!("DIFF picks_the_heavier_assignment EMITTED {diff}; VM = [2, 3, 0, 1]");
    }
    // example <tie_picks_the_lex_least>
    let got = assign_max(g(4, &[(0, 2, 1), (0, 3, 1), (1, 2, 1), (1, 3, 1)]));
    if let Some(diff) = i_eq(&got, &[2, 3, 0, 1]) {
        failures += 1;
        println!("DIFF tie_picks_the_lex_least EMITTED {diff}; VM = [2, 3, 0, 1]");
    }
    // example <negative_weights_can_leave_vertices_free>
    let got = assign_max(g(2, &[(0, 1, 0 - 2)]));
    if let Some(diff) = i_eq(&got, &[0 - 1, 0 - 1]) {
        failures += 1;
        println!("DIFF negative_weights_can_leave_vertices_free EMITTED {diff}; VM = [-1, -1]");
    }

    // example <heavier_fixture_full_row_folds_to_the_optimum>
    // (asg_row: the match-or-skip row fold itself)
    let got = asg_row(
        g(4, &[(0, 2, 3), (0, 3, 1), (1, 2, 1), (1, 3, 1)]),
        vec![vec![2, 3], vec![2, 3], vec![0, 1], vec![0, 1]],
        vec![0, 0, 1, 1],
        vec![0, 0, 0, 0],
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
        0,
        1,
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
    );
    if let Some(diff) = i_eq(&got, &[2, 3, 0, 1]) {
        failures += 1;
        println!("DIFF heavier_fixture_full_row_folds_to_the_optimum EMITTED {diff}; VM = [2, 3, 0, 1]");
    }
    // example <tie_breaks_to_the_lex_least_vector>
    let got = asg_row(
        g(4, &[(0, 2, 1), (0, 3, 1), (1, 2, 1), (1, 3, 1)]),
        vec![vec![2, 3], vec![2, 3], vec![0, 1], vec![0, 1]],
        vec![0, 0, 1, 1],
        vec![0, 0, 0, 0],
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
        0,
        1,
        vec![0 - 1, 0 - 1, 0 - 1, 0 - 1],
    );
    if let Some(diff) = i_eq(&got, &[2, 3, 0, 1]) {
        failures += 1;
        println!("DIFF tie_breaks_to_the_lex_least_vector EMITTED {diff}; VM = [2, 3, 0, 1]");
    }

    if failures > 0 {
        panic!("{failures} matching row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_matching() {
    let diffs = run_lane("language/modules/discrete/matching.emath", MATCHING_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted matching disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const SPIKE_DRIVER: &str = r#"
//! Authored pins of research/structural_spike.emath against the emitted
//! crate.
//!
//! The code-template-carrier pins (emath-49o8): the module's template
//! chain is declared `Code[Rat]` - the homogeneous unary program
//! template over Rat - so substitutes bind Rat constants and
//! evaluate yields the specialized Rat -> Rat closure the walks call.
//! The driver never hand-builds a Code value: the template chains
//! from the emitted quadratic_template entry, exactly as the authored
//! rows do. Rat values are the (i128, i128) ratio carrier; rows
//! compare by cross-multiplication and integers land as (n, 1).

use structural_spike::{cand_score, climb_step, quadratic_template, rollout_of, scan_best};

type Q = (i128, i128);

fn q(n: i64, d: i64) -> Q {
    (n as i128, d as i128)
}

fn q_eq(a: Q, b: Q) -> bool {
    a.0 * b.1 == b.0 * a.1
}

fn dq(got: &Result<Q, String>, want: Q) -> Option<String> {
    match got {
        Ok(v) if q_eq(*v, want) => None,
        other => Some(format!("{other:?}")),
    }
}

fn lq(got: &Result<Vec<Q>, String>, want: &[Q]) -> Option<String> {
    match got {
        Ok(v) if v.len() == want.len() && v.iter().zip(want).all(|(a, b)| q_eq(*a, *b)) => None,
        other => Some(format!("{other:?}")),
    }
}

fn main() {
    let mut failures = 0usize;
    let template = quadratic_template(0).expect("quadratic_template answers");
    // The authored target closure f = 2x, one fresh shared handle per
    // row (the parameters are owned `Rc<dyn Fn>` carriers).
    fn f() -> std::rc::Rc<dyn Fn((i128, i128)) -> Result<(i128, i128), String>> {
        std::rc::Rc::new(|x: (i128, i128)| Ok((x.0 * 2, x.1)))
    }

    // example <exact_hypothesis_scores_zero> (cand_score: substitute,
    // evaluate, apply - the called-closure lane itself)
    let got = cand_score(
        template.clone(),
        f(),
        vec![q(1, 1), q(2, 1)],
        vec![q(2, 1), q(0, 1)],
    );
    if let Some(diff) = dq(&got, q(0, 1)) {
        failures += 1;
        println!("DIFF exact_hypothesis_scores_zero EMITTED {diff}; VM = 0/1");
    }
    // example <zero_hypothesis_misses_by_the_data>
    let got = cand_score(
        template.clone(),
        f(),
        vec![q(1, 1), q(2, 1)],
        vec![q(0, 1), q(0, 1)],
    );
    if let Some(diff) = dq(&got, q(20, 1)) {
        failures += 1;
        println!("DIFF zero_hypothesis_misses_by_the_data EMITTED {diff}; VM = 20/1");
    }

    // example <exact_fit_rolls_out_exact> (rollout_of: the evaluated
    // closure drives a free-running two-orbit rollout)
    let got = rollout_of(
        template.clone(),
        f(),
        vec![q(2, 1), q(0, 1)],
        q(1, 1),
        5,
    );
    if let Some(diff) = dq(&got, q(0, 1)) {
        failures += 1;
        println!("DIFF exact_fit_rolls_out_exact EMITTED {diff}; VM = 0/1");
    }
    // example <flat_hypothesis_misses_step_one>
    let got = rollout_of(
        template.clone(),
        f(),
        vec![q(0, 1), q(0, 1)],
        q(1, 1),
        1,
    );
    if let Some(diff) = dq(&got, q(4, 1)) {
        failures += 1;
        println!("DIFF flat_hypothesis_misses_step_one EMITTED {diff}; VM = 4/1");
    }

    // example <one_round_then_budget_refusal> (climb_step: eight
    // cand_score calls per round, recursion, and the budget refusal)
    let got = climb_step(
        template.clone(),
        f(),
        vec![q(1, 1)],
        vec![q(1, 1), q(0, 1), q(1, 1), q(1, 1)],
        1,
        9,
    );
    if let Some(diff) = lq(
        &got,
        &[q(5, 4), q(1, 4), q(1, 4), q(9, 1), q(6, 1)],
    ) {
        failures += 1;
        println!("DIFF one_round_then_budget_refusal EMITTED {diff}; VM = [5/4, 1/4, 1/4, 9, 6]");
    }
    // example <climbs_to_exact_zero_and_stops_clean>
    let got = climb_step(
        template.clone(),
        f(),
        vec![q(1, 1)],
        vec![q(1, 1), q(0, 1), q(1, 1), q(1, 1)],
        1,
        65,
    );
    if let Some(diff) = lq(
        &got,
        &[q(3, 2), q(1, 2), q(0, 1), q(25, 1), q(0, 1)],
    ) {
        failures += 1;
        println!("DIFF climbs_to_exact_zero_and_stops_clean EMITTED {diff}; VM = [3/2, 1/2, 0/1, 25, 0]");
    }

    // example <strict_minimum_wins_and_records_the_hit> (scan_best)
    let got = scan_best(
        template.clone(),
        f(),
        vec![q(1, 1)],
        vec![vec![q(1, 1), q(0, 1)], vec![q(2, 1), q(0, 1)], vec![q(0, 1), q(0, 1)]],
        1,
        vec![q(0, 1), q(0, 1)],
        q(4, 1),
        2,
        3,
    );
    if let Some(diff) = lq(
        &got,
        &[q(2, 1), q(0, 1), q(0, 1), q(3, 1), q(1, 1)],
    ) {
        failures += 1;
        println!("DIFF strict_minimum_wins_and_records_the_hit EMITTED {diff}; VM = [2, 0, 0, 3, 1]");
    }

    if failures > 0 {
        panic!("{failures} structural_spike row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_structural_spike() {
    let diffs = run_lane(
        "language/domains/research/structural_spike.emath",
        SPIKE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted structural_spike disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const COMPOSITION_DRIVER: &str = r#"
//! Authored pins of analysis/evolution/composition.emath against the
//! emitted crate (emath-49o8, the shared-state lane).
//!
//! SimState is one_step's shared evolution object; this module had
//! used it with no import (the VM resolved it structurally), so the
//! emitted lane refused every record literal by name until the
//! dependency was authored. The import closed it: the record
//! literals emit as EmathRecord_SimState, the CURRIED closure inputs
//! (accel: Rat -> Rat -> Rat) ride the two-level
//! Rc<dyn Fn(..) -> Result<Rc<dyn Fn(..) -> Result<..>>>> ABI, and
//! the binary subflow closures (f: Rat -> SimState -> SimState)
//! chain through flow_compose with the record taken by reference at
//! the second stage. Transcription boundaries: every ratio compares
//! cross-multiplied (emitted values are canonical; the drivers never
//! assume a reduced spelling), and the two-step rollout rows assume
//! the emitted recursion consumes i/n as authored.

use composition::{
    composition_contract, damped_drift, damped_step, drift, energy, energy_boundary,
    flow_compose, kick, momentum_reversal, probe_separable, verlet_rollout, verlet_step,
    EmathRecord_SimState as S,
};

type R = (i128, i128);
type Accel = std::rc::Rc<
    dyn Fn(R) -> Result<std::rc::Rc<dyn Fn(R) -> Result<R, String>>, String>,
>;
type Flow = std::rc::Rc<
    dyn Fn(R) -> Result<std::rc::Rc<dyn Fn(&S) -> Result<S, String>>, String>,
>;

fn q(n: i128, d: i128) -> R {
    (n, d)
}

fn s(t: R, y0: R, y1: R) -> S {
    S { t, y: vec![y0, y1] }
}

fn eq(a: &R, b: &R) -> bool {
    a.0 * b.1 == b.0 * a.1
}

fn sq(got: &Result<S, String>, t: R, y0: R, y1: R) -> Option<String> {
    match got {
        Ok(v)
            if eq(&v.t, &t)
                && v.y.len() == 2
                && eq(&v.y[0], &y0)
                && eq(&v.y[1], &y1) => None,
        Ok(v) => Some(format!("{:?} {:?}", v.t, v.y)),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn dq(got: &Result<R, String>, want: R) -> Option<String> {
    match got {
        Ok(v) if eq(&v, &want) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn bq(got: &Result<bool, String>, want: bool) -> Option<String> {
    match got {
        Ok(v) if *v == want => None,
        Ok(v) => Some(format!("{v}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

// accel = -q (the harmonic acceleration, separable in p)
fn minus_q() -> Accel {
    std::rc::Rc::new(|qq: R| {
        Ok(std::rc::Rc::new(move |_pp: R| Ok((-qq.0, qq.1))))
    })
}

// accel = 0 (the free particle)
fn zero_a() -> Accel {
    std::rc::Rc::new(|_qq: R| Ok(std::rc::Rc::new(|_pp: R| Ok((0, 1)))))
}

// accel = q + p (momentum-coupled: the two-momenta probe must catch it)
fn coupled_a() -> Accel {
    std::rc::Rc::new(|qq: R| {
        Ok(std::rc::Rc::new(move |pp: R| {
            Ok((qq.0 * pp.1 + pp.0 * qq.1, qq.1 * pp.1))
        }))
    })
}

// the drift subflow as a binary closure (flow_compose's f/g/k)
fn drift_flow() -> Flow {
    std::rc::Rc::new(|c: R| {
        Ok(std::rc::Rc::new(move |s2: &S| drift(c, s2.clone())))
    })
}

// the potential V(q) = q^2 / 2 (energy's unary closure input)
fn quad_half() -> std::rc::Rc<dyn Fn(R) -> Result<R, String>> {
    std::rc::Rc::new(|qq: R| Ok((qq.0 * qq.0, qq.1 * qq.1 * 2)))
}

fn main() {
    let mut failures = 0usize;

    // example <free_particle_half_step> (drift: the record-carrier
    // lane itself)
    let got = drift(q(1, 2), s(q(0, 1), q(0, 1), q(2, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(2, 1)) {
        failures += 1;
        println!("DIFF free_particle_half_step EMITTED {diff}; VM = t:0 y:[1, 2]");
    }

    // example <harmonic_half_kick> (kick: the curried closure input
    // is CALLED inside the emitted body)
    let got = kick(minus_q(), q(1, 2), s(q(0, 1), q(1, 1), q(0, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(0 - 1, 2)) {
        failures += 1;
        println!("DIFF harmonic_half_kick EMITTED {diff}; VM = t:0 y:[1, -1/2]");
    }

    // example <adjacent_drifts_merge> (flow_compose: three binary
    // subflow closures chained; adjacent drifts merge exactly)
    let got = flow_compose(
        drift_flow(),
        q(1, 4),
        drift_flow(),
        q(1, 2),
        drift_flow(),
        q(1, 4),
        s(q(0, 1), q(0, 1), q(2, 1)),
    );
    if let Some(diff) = sq(&got, q(0, 1), q(2, 1), q(2, 1)) {
        failures += 1;
        println!("DIFF adjacent_drifts_merge EMITTED {diff}; VM = t:0 y:[2, 2]");
    }

    // example <separable_acceleration_passes> (probe_separable: -q
    // ignores p, both momenta agree)
    let got = probe_separable(minus_q(), q(3, 1), q(0, 1), q(5, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF separable_acceleration_passes EMITTED {diff}; VM = true");
    }
    // example <momentum_coupled_acceleration_fails>
    let got = probe_separable(coupled_a(), q(3, 1), q(0, 1), q(5, 1));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF momentum_coupled_acceleration_fails EMITTED {diff}; VM = false");
    }

    // example <free_particle_step_and_inverse> (verlet_step: gates +
    // the kick-drift-kick composition through the closures)
    let got = verlet_step(zero_a(), q(1, 2), s(q(0, 1), q(0, 1), q(2, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(2, 1)) {
        failures += 1;
        println!("DIFF free_particle_step_and_inverse EMITTED {diff}; VM = t:0 y:[1, 2]");
    }
    // example <harmonic_step_museum_values> (the museum recipe:
    // 7/8, -15/32 exactly)
    let got = verlet_step(minus_q(), q(1, 2), s(q(0, 1), q(1, 1), q(0, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(7, 8), q(0 - 15, 32)) {
        failures += 1;
        println!("DIFF harmonic_step_museum_values EMITTED {diff}; VM = t:0 y:[7/8, -15/32]");
    }
    // example <harmonic_exact_reversal> (negative h is legal on THIS
    // path: step(-h, step(h, s)) == s exactly)
    let fwd = verlet_step(minus_q(), q(1, 2), s(q(0, 1), q(1, 1), q(0, 1)))
        .unwrap_or_else(|e| panic!("harmonic_exact_reversal inner: {e}"));
    let got = verlet_step(minus_q(), q(0 - 1, 2), fwd);
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(0, 1)) {
        failures += 1;
        println!("DIFF harmonic_exact_reversal EMITTED {diff}; VM = t:0 y:[1, 0]");
    }

    // example <drifts_and_cools> (damped_drift: 2, 9/5)
    let got = damped_drift(q(1, 5), q(1, 2), s(q(0, 1), q(1, 1), q(2, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(2, 1), q(9, 5)) {
        failures += 1;
        println!("DIFF drifts_and_cools EMITTED {diff}; VM = t:0 y:[2, 9/5]");
    }
    // example <cooling_is_not_backward_integrable> (stepping back
    // with -c does NOT restore the state: 99/50 != 2)
    let fwd = damped_drift(q(1, 5), q(1, 2), s(q(0, 1), q(1, 1), q(2, 1)))
        .unwrap_or_else(|e| panic!("cooling_is_not_backward_integrable inner: {e}"));
    let got = damped_drift(q(1, 5), q(0 - 1, 2), fwd);
    if let Ok(v) = &got {
        if v.y.len() == 2 && eq(&v.y[1], &q(2, 1)) {
            failures += 1;
            println!(
                "DIFF cooling_is_not_backward_integrable EMITTED p = {:?}; VM != 2/1",
                v.y[1]
            );
        }
    } else if let Err(e) = &got {
        failures += 1;
        println!("DIFF cooling_is_not_backward_integrable EMITTED Err({e})");
    }

    // example <damped_step_values> (the dissipative composition:
    // 7/8, -71/160)
    let got = damped_step(minus_q(), q(1, 5), q(1, 2), s(q(0, 1), q(1, 1), q(0, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(7, 8), q(0 - 71, 160)) {
        failures += 1;
        println!("DIFF damped_step_values EMITTED {diff}; VM = t:0 y:[7/8, -71/160]");
    }

    // example <negates_momentum_only> (momentum_reversal)
    let got = momentum_reversal(s(q(0, 1), q(1, 1), q(2, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(0 - 2, 1)) {
        failures += 1;
        println!("DIFF negates_momentum_only EMITTED {diff}; VM = t:0 y:[1, -2]");
    }

    // example <harmonic_energy_at_rest> (energy: the unary closure
    // input + the record field access)
    let got = energy(quad_half(), s(q(0, 1), q(1, 1), q(0, 1)));
    if let Some(diff) = dq(&got, q(1, 2)) {
        failures += 1;
        println!("DIFF harmonic_energy_at_rest EMITTED {diff}; VM = 1/2");
    }

    // example <two_steps_pinned> (verlet_rollout: recursion through
    // the composed step; 17/32, -105/128)
    let got = verlet_rollout(minus_q(), q(1, 2), 2, 0, s(q(0, 1), q(1, 1), q(0, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(17, 32), q(0 - 105, 128)) {
        failures += 1;
        println!("DIFF two_steps_pinned EMITTED {diff}; VM = t:0 y:[17/32, -105/128]");
    }
    // example <rollout_reverses_exactly> (the signed rollout inverts)
    let fwd = verlet_rollout(minus_q(), q(1, 2), 2, 0, s(q(0, 1), q(1, 1), q(0, 1)))
        .unwrap_or_else(|e| panic!("rollout_reverses_exactly inner: {e}"));
    let got = verlet_rollout(minus_q(), q(0 - 1, 2), 2, 0, fwd);
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(0, 1)) {
        failures += 1;
        println!("DIFF rollout_reverses_exactly EMITTED {diff}; VM = t:0 y:[1, 0]");
    }

    // example <both_reversal_contracts_hold> (composition_contract:
    // negation AND the involution, side by side)
    let got = composition_contract(0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF both_reversal_contracts_hold EMITTED {diff}; VM = true");
    }
    // example <free_particle_involution_numbers> (the museum's
    // literal example: R o step(h) == step(-h))
    let got = verlet_step(zero_a(), q(1, 2), s(q(0, 1), q(0, 1), q(2, 1)));
    if let Some(diff) = sq(&got, q(0, 1), q(1, 1), q(2, 1)) {
        failures += 1;
        println!("DIFF free_particle_involution_numbers EMITTED {diff}; VM = t:0 y:[1, 2]");
    }
    let rev = momentum_reversal(s(q(0, 1), q(1, 1), q(2, 1)))
        .unwrap_or_else(|e| panic!("free_particle_involution_numbers reversal: {e}"));
    let got = verlet_step(zero_a(), q(1, 2), rev);
    if let Some(diff) = sq(&got, q(0, 1), q(0, 1), q(0 - 2, 1)) {
        failures += 1;
        println!("DIFF free_particle_involution_numbers EMITTED {diff}; VM = t:0 y:[0, -2]");
    }

    // example <verlet_delta_exact_nonzero> (energy_boundary: symmetry
    // is NOT exact energy conservation — the delta is exact)
    let got = energy_boundary(0);
    match &got {
        Ok(b) if eq(&b.verlet_delta, &q(0 - 15, 2048)) => {}
        Ok(b) => {
            failures += 1;
            println!(
                "DIFF verlet_delta_exact_nonzero EMITTED {:?}; VM = -15/2048",
                b.verlet_delta
            );
        }
        Err(e) => {
            failures += 1;
            println!("DIFF verlet_delta_exact_nonzero EMITTED Err({e})");
        }
    }
    // example <damped_delta_exact_more_negative> (-959/51200, and
    // strictly more dissipative than the verlet delta)
    match &got {
        Ok(b)
            if eq(&b.damped_delta, &q(0 - 959, 51200))
                && b.damped_delta.0 * b.verlet_delta.1
                    < b.verlet_delta.0 * b.damped_delta.1 => {}
        Ok(b) => {
            failures += 1;
            println!(
                "DIFF damped_delta_exact_more_negative EMITTED {:?} vs {:?}; VM = -959/51200 < -15/2048",
                b.damped_delta, b.verlet_delta
            );
        }
        Err(e) => {
            failures += 1;
            println!("DIFF damped_delta_exact_more_negative EMITTED Err({e})");
        }
    }

    if failures > 0 {
        panic!("{failures} composition row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_composition() {
    let diffs = run_lane(
        "language/modules/analysis/evolution/composition.emath",
        COMPOSITION_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted composition disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const STIFF_DRIVER: &str = r#"
//! Authored pins of analysis/evolution/stiff.emath against the emitted crate
//! (emath-49o8, the shared-state lane's second module).
//!
//! stiff composes one_step's SimState machinery with the implicit
//! solves: solve_dense (imported from algebra.linear.systems) is
//! INLINED into the emitted bodies as the sibling lane spells it,
//! and the three laws that refused by record name (the symplectic
//! invariant, the true-energy drift, the determinant) now emit. The
//! adaptive walk rows pin the refusal atoms (bad_step_budget,
//! non_adaptable_step's sibling nonnegative_test_rate,
//! singular_matrix) as exact Err strings. Transcription boundary:
//! ratios compare cross-multiplied, never by spelling.

use stiff::{
    stiff_adaptive_walk, stiff_astable_ok, stiff_astable_walk, stiff_be_factor,
    stiff_be_step_solve, stiff_be_system_law, stiff_cn_factor, stiff_contrast_law,
    stiff_diag2_be_step, stiff_embedded_est, stiff_embedded_step, stiff_explicit_factor,
    stiff_symplectic_invariant_law, stiff_true_energy_drifts_law, stiff_verlet_det_law,
    stiff_verlet_invariant,
};

type R = (i128, i128);

fn q(n: i128, d: i128) -> R {
    (n, d)
}

fn eq(a: &R, b: &R) -> bool {
    a.0 * b.1 == b.0 * a.1
}

fn dq(got: &Result<R, String>, want: R) -> Option<String> {
    match got {
        Ok(v) if eq(&v, &want) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn lq(got: &Result<Vec<R>, String>, want: &[R]) -> Option<String> {
    match got {
        Ok(v)
            if v.len() == want.len()
                && v.iter().zip(want).all(|(a, b)| eq(a, b)) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn bq(got: &Result<bool, String>, want: bool) -> Option<String> {
    match got {
        Ok(v) if *v == want => None,
        Ok(v) => Some(format!("{v}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn xq<T: std::fmt::Debug>(got: &Result<T, String>, atom: &str) -> Option<String> {
    match got {
        Err(e) if e == atom => None,
        Ok(v) => Some(format!("Ok({v:?})")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn main() {
    let mut failures = 0usize;

    // example <half_step_decay> (stiff_be_factor: 1/(1 - h lam))
    let got = stiff_be_factor(q(1, 2), q(0 - 2, 1));
    if let Some(diff) = dq(&got, q(1, 2)) {
        failures += 1;
        println!("DIFF half_step_decay EMITTED {diff}; VM = 1/2");
    }
    // example <half_step_cn> (stiff_cn_factor)
    let got = stiff_cn_factor(q(1, 2), q(0 - 2, 1));
    if let Some(diff) = dq(&got, q(1, 3)) {
        failures += 1;
        println!("DIFF half_step_cn EMITTED {diff}; VM = 1/3");
    }
    // example <quarter_step_explicit> (stiff_explicit_factor: 0 at
    // h lam = -1 — the contrast lane's zero)
    let got = stiff_explicit_factor(q(1, 2), q(0 - 2, 1));
    if let Some(diff) = dq(&got, q(0, 1)) {
        failures += 1;
        println!("DIFF quarter_step_explicit EMITTED {diff}; VM = 0/1");
    }

    // example <solved_step_matches_hand> (stiff_be_step_solve: the
    // implicit solve ASSEMBLED through the inlined solve_dense)
    let got = stiff_be_step_solve(q(0 - 2, 1), q(1, 2), q(1, 1));
    if let Some(diff) = dq(&got, q(1, 2)) {
        failures += 1;
        println!("DIFF solved_step_matches_hand EMITTED {diff}; VM = 1/2");
    }
    // example <resonance_refuses> (h lam = 1: the assembled system is
    // singular — the named refusal, exact Err string)
    let got = stiff_be_step_solve(q(2, 1), q(1, 2), q(1, 1));
    if let Some(diff) = xq(&got, "singular_matrix") {
        failures += 1;
        println!("DIFF resonance_refuses EMITTED {diff}; VM = Err(singular_matrix)");
    }

    // example <solve_is_the_closed_form> (stiff_be_system_law: the
    // machine-algebra certificate)
    let got = stiff_be_system_law(0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF solve_is_the_closed_form EMITTED {diff}; VM = true");
    }

    // example <two_scale_decoupled_step> (stiff_diag2_be_step: the
    // diagonal 2x2 solve, sequence carriers in and out)
    let got = stiff_diag2_be_step(
        q(0 - 2, 1),
        q(0 - 6, 1),
        q(1, 2),
        vec![q(1, 1), q(8, 1)],
    );
    if let Some(diff) = lq(&got, &[q(1, 2), q(2, 1)]) {
        failures += 1;
        println!("DIFF two_scale_decoupled_step EMITTED {diff}; VM = [1/2, 2]");
    }

    // example <decays_everywhere_stiff> (stiff_astable_ok)
    let got = stiff_astable_ok(q(5, 4), q(0 - 2, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF decays_everywhere_stiff EMITTED {diff}; VM = true");
    }
    // example <refuses_nonstiff_demand> (lam >= 0 is not the stiff
    // lane — the named refusal)
    let got = stiff_astable_ok(q(1, 2), q(3, 1));
    if let Some(diff) = xq(&got, "nonnegative_test_rate") {
        failures += 1;
        println!("DIFF refuses_nonstiff_demand EMITTED {diff}; VM = Err(nonnegative_test_rate)");
    }
    // example <zero_step_is_not_decay> (amplitude EXACTLY 1: the
    // strict < is the decidable boundary)
    let got = stiff_astable_ok(q(0, 1), q(0 - 2, 1));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF zero_step_is_not_decay EMITTED {diff}; VM = false");
    }

    // example <dyadic_walk_decays> (stiff_astable_walk: the walk
    // CHECKS every element)
    let got = stiff_astable_walk(
        q(0 - 2, 1),
        vec![q(1, 4), q(1, 2), q(1, 1), q(5, 2), q(21, 2)],
        0,
    );
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF dyadic_walk_decays EMITTED {diff}; VM = true");
    }
    // example <zero_step_breaks_the_walk> (h = 0 breaks it — no
    // accept-at-entry)
    let got = stiff_astable_walk(q(0 - 2, 1), vec![q(0, 1)], 0);
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF zero_step_breaks_the_walk EMITTED {diff}; VM = false");
    }

    // example <implicit_decays_explicit_grows> (stiff_contrast_law:
    // 2/7 vs -3/2 at h = 5/4)
    let got = stiff_contrast_law(0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF implicit_decays_explicit_grows EMITTED {diff}; VM = true");
    }

    // example <half_step_estimate_one_sixth> (stiff_embedded_est:
    // |y_CN - y_BE| exactly)
    let got = stiff_embedded_est(q(0 - 2, 1), q(1, 2), q(1, 1));
    if let Some(diff) = dq(&got, q(1, 6)) {
        failures += 1;
        println!("DIFF half_step_estimate_one_sixth EMITTED {diff}; VM = 1/6");
    }

    // example <loose_tol_accepts> (stiff_embedded_step: the StiffStep
    // record's three fields — accept)
    let got = stiff_embedded_step(q(0 - 2, 1), q(1, 2), q(1, 4), q(1, 1));
    match &got {
        Ok(v)
            if eq(&v.y, &q(1, 3)) && eq(&v.est, &q(1, 6)) && v.ok => {}
        Ok(v) => {
            failures += 1;
            println!(
                "DIFF loose_tol_accepts EMITTED y={:?} est={:?} ok={}; VM = y=1/3 est=1/6 ok=true",
                v.y, v.est, v.ok
            );
        }
        Err(e) => {
            failures += 1;
            println!("DIFF loose_tol_accepts EMITTED Err({e})");
        }
    }
    // example <tight_tol_refuses> (est 1/6 > tol 1/7: ok false — the
    // certificate, not an epsilon)
    let got = stiff_embedded_step(q(0 - 2, 1), q(1, 2), q(1, 7), q(1, 1));
    match &got {
        Ok(v)
            if eq(&v.y, &q(1, 3)) && eq(&v.est, &q(1, 6)) && !v.ok => {}
        Ok(v) => {
            failures += 1;
            println!(
                "DIFF tight_tol_refuses EMITTED y={:?} est={:?} ok={}; VM = y=1/3 est=1/6 ok=false",
                v.y, v.est, v.ok
            );
        }
        Err(e) => {
            failures += 1;
            println!("DIFF tight_tol_refuses EMITTED Err({e})");
        }
    }

    // example <accepts_and_advances> (stiff_adaptive_walk)
    let got = stiff_adaptive_walk(q(0 - 2, 1), q(1, 2), q(1, 4), q(1, 1), 1, 8);
    if let Some(diff) = dq(&got, q(1, 3)) {
        failures += 1;
        println!("DIFF accepts_and_advances EMITTED {diff}; VM = 1/3");
    }
    // example <halves_once_then_accepts> (refused at h = 1/2, halved
    // to h = 1/4 where est = 1/15 <= 1/7: 3/5)
    let got = stiff_adaptive_walk(q(0 - 2, 1), q(1, 2), q(1, 7), q(1, 1), 1, 8);
    if let Some(diff) = dq(&got, q(3, 5)) {
        failures += 1;
        println!("DIFF halves_once_then_accepts EMITTED {diff}; VM = 3/5");
    }
    // example <negative_steps_refuse> (bad_step_budget, exact)
    let got = stiff_adaptive_walk(q(0 - 2, 1), q(1, 2), q(1, 4), q(1, 1), 0 - 1, 8);
    if let Some(diff) = xq(&got, "bad_step_budget") {
        failures += 1;
        println!("DIFF negative_steps_refuse EMITTED {diff}; VM = Err(bad_step_budget)");
    }
    // example <two_accepted_steps_compose> (1/3 then 1/9 — the
    // ok-branch recursion must CONSUME the budget)
    let got = stiff_adaptive_walk(q(0 - 2, 1), q(1, 2), q(1, 4), q(1, 1), 2, 8);
    if let Some(diff) = dq(&got, q(1, 9)) {
        failures += 1;
        println!("DIFF two_accepted_steps_compose EMITTED {diff}; VM = 1/9");
    }

    // example <hand_value> (stiff_verlet_invariant: 15/32)
    let got = stiff_verlet_invariant(q(1, 1), q(0, 1), q(1, 1), q(1, 2));
    if let Some(diff) = dq(&got, q(15, 32)) {
        failures += 1;
        println!("DIFF hand_value EMITTED {diff}; VM = 15/32");
    }

    // example <invariant_over_six_steps>
    // (stiff_symplectic_invariant_law: the modified-Hamiltonian
    // certificate over the verlet rollout — refused by record name
    // before the SimState import)
    let got = stiff_symplectic_invariant_law(q(1, 1), q(1, 2), 6, 0, q(1, 1), q(0, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF invariant_over_six_steps EMITTED {diff}; VM = true");
    }
    // example <zero_steps_accept_at_entry> (the boundary: the law
    // binds only along taken steps)
    let got = stiff_symplectic_invariant_law(q(1, 1), q(1, 2), 0, 0, q(1, 1), q(0, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF zero_steps_accept_at_entry EMITTED {diff}; VM = true");
    }

    // example <first_step_drifts_exactly> (stiff_true_energy_drifts_law:
    // the honesty twin, -15/2048 per step)
    let got = stiff_true_energy_drifts_law(0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF first_step_drifts_exactly EMITTED {diff}; VM = true");
    }

    // example <area_preserved_exactly> (stiff_verlet_det_law: the
    // step map's determinant is EXACTLY 1)
    let got = stiff_verlet_det_law(0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF area_preserved_exactly EMITTED {diff}; VM = true");
    }

    if failures > 0 {
        panic!("{failures} stiff row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_stiff() {
    let diffs = run_lane(
        "language/modules/analysis/evolution/stiff.emath",
        STIFF_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted stiff disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const LINALG_DRIVER: &str = r#"
//! Authored pins of algebra/linalg.emath against the emitted crate
//! (emath-49o8, the substitute-lane pass).
//!
//! The hess/grad family refused through quote.substitute's
//! static-reference seam: d_form's chain rule reads the callee
//! fragment's binder name at runtime (a minted-scope tag record),
//! which the emitted lane had no spelling for. The reference now
//! rides as a node-family operand (the tag record's type name IS the
//! binder - the VM's dynamic reference law), a node-carried
//! replacement substitutes structurally through quote.make, the
//! program template carries its distilled tree so quote.view opens
//! it, tuple domains kind as positional sequences (_N projects
//! checked), and a concat with any node-family operand stays in the
//! family. These rows pin every tier: the sparse carrier records,
//! the dense core, and the quoted differentiator end to end (the
//! substitute + evaluate round trip on the emitted ExprCode).
//! Transcription boundaries: ratios compare cross-multiplied; the
//! driver builds the quoted bodies as distilled trees (the same
//! shapes quote(...) distills) over the EMPTY module table - the
//! pinned bodies reference no module globals.

//! The vector and matrix kernels linalg builds on (spaces vec_dot /
//! vec_axpy / zero_vec, matrices mat_vec) are pinned here too: the
//! emitted linalg crate carries them.

use linalg::{
    grad_quote, hess_col, hess_symmetric, hess_vec, hessian_rows, mat_vec, oscillator_modes,
    place_row, sp_dense_law, sp_matvec, sp_ok, sp_row_at, sp_to_dense, square_walk, vec_axpy,
    vec_dot, zero_vec, EmathRecord_SpEnt as Ent, EmathRecord_SpMat as Sp,
};

type R = (i128, i128);

use linalg::emath_rt::code::CodeValue;
use linalg::emath_rt::code::ExprCode;
use linalg::emath_rt::code_tree::{CodeTree, ModuleTable, TreeBinary};

fn q(n: i128, d: i128) -> R {
    (n, d)
}

fn ent(col: i64, val: R) -> Ent {
    Ent { col, val }
}

fn sp(n: i64, rows: &[Vec<Ent>]) -> Sp {
    Sp {
        n,
        rows: rows.to_vec(),
    }
}

fn eq(a: &R, b: &R) -> bool {
    a.0 * b.1 == b.0 * a.1
}

fn lq(got: &Result<Vec<R>, String>, want: &[R]) -> Option<String> {
    match got {
        Ok(v)
            if v.len() == want.len()
                && v.iter().zip(want).all(|(a, b)| eq(a, b)) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn llq(got: &Result<Vec<Vec<R>>, String>, want: &[&[R]]) -> Option<String> {
    match got {
        Ok(v)
            if v.len() == want.len()
                && v.iter()
                    .zip(want)
                    .all(|(row, want)| {
                        row.len() == want.len()
                            && row.iter().zip(want.iter()).all(|(a, b)| eq(a, b))
                    }) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn bq(got: &Result<bool, String>, want: bool) -> Option<String> {
    match got {
        Ok(v) if *v == want => None,
        Ok(v) => Some(format!("{v}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn rq(got: &Result<R, String>, want: R) -> Option<String> {
    match got {
        Ok(v) if eq(&v, &want) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn xq<T: std::fmt::Debug>(got: &Result<T, String>, atom: &str) -> Option<String> {
    match got {
        Err(e) if e == atom => None,
        Ok(v) => Some(format!("Ok({v:?})")),
        Err(e) => Some(format!("Err({e})")),
    }
}

// Distilled-tree builders (the same shapes quote(...) distills).
fn path(name: &str) -> CodeTree {
    CodeTree::Path(vec![name.to_string()])
}

fn int(value: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(value))
}

fn add(left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op: TreeBinary::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn mul(left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op: TreeBinary::Mul,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn expr(free: &[&str], tree: CodeTree) -> ExprCode {
    linalg::emath_rt::code::open_expr(
        free.iter().map(|name| name.to_string()).collect(),
        tree,
        None,
        std::collections::BTreeMap::new(),
    )
}

fn var(name: &str) -> ExprCode {
    expr(&[name], path(name))
}

// x*x + 2*y*y + x*y (the quadratic fixture every hess row uses)
fn quadratic_body() -> ExprCode {
    expr(
        &["x", "y"],
        add(
            add(mul(path("x"), path("x")), mul(mul(int(2), path("y")), path("y"))),
            mul(path("x"), path("y")),
        ),
    )
}

// x*x*y + y*y*y (the coupled fixture)
fn coupled_body() -> ExprCode {
    expr(
        &["x", "y"],
        add(
            mul(mul(path("x"), path("x")), path("y")),
            mul(mul(path("y"), path("y")), path("y")),
        ),
    )
}

fn main() {
    let mut failures = 0usize;

    // example <sp_ok_true> (sp_ok: the SpMat record lane)
    let got = sp_ok(sp(2, &[vec![ent(1, q(3, 1))], vec![]]));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF sp_ok_true EMITTED {diff}; VM = true");
    }
    // example <sp_ok_bad_col>
    let got = sp_ok(sp(2, &[vec![ent(2, q(3, 1))], vec![]]));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF sp_ok_bad_col EMITTED {diff}; VM = false");
    }

    // zero_vec(3) (spaces: the zero row sp_to_dense fills)
    let got = zero_vec(3);
    if let Some(diff) = lq(&got, &[q(0, 1), q(0, 1), q(0, 1)]) {
        failures += 1;
        println!("DIFF zero_vec_three EMITTED {diff}; VM = [0, 0, 0]");
    }

    // example <place_two> (place_row: replace_at through the walk)
    let got = place_row(
        vec![ent(0, q(5, 1)), ent(2, q(7, 1))],
        0,
        vec![q(0, 1), q(0, 1), q(0, 1)],
    );
    if let Some(diff) = lq(&got, &[q(5, 1), q(0, 1), q(7, 1)]) {
        failures += 1;
        println!("DIFF place_two EMITTED {diff}; VM = [5, 0, 7]");
    }

    // example <dense_identity> (sp_to_dense: the materialization)
    let got = sp_to_dense(
        sp(2, &[vec![ent(0, q(1, 1))], vec![ent(1, q(1, 1))]]),
        0,
    );
    if let Some(diff) = llq(&got, &[&[q(1, 1), q(0, 1)], &[q(0, 1), q(1, 1)]]) {
        failures += 1;
        println!("DIFF dense_identity EMITTED {diff}; VM = [[1, 0], [0, 1]]");
    }

    // example <row_dot> (sp_row_at: the entry walk, exact Rat)
    let got = sp_row_at(
        vec![ent(0, q(2, 1)), ent(2, q(3, 1))],
        vec![q(1, 1), q(10, 1), q(100, 1)],
        0,
        q(0, 1),
    );
    if let Some(diff) = rq(&got, q(302, 1)) {
        failures += 1;
        println!("DIFF row_dot EMITTED {diff}; VM = 302");
    }

    // example <sp_matvec_at_matches_the_wrapper_fixture>
    let a = sp(
        3,
        &[
            vec![ent(0, q(2, 1)), ent(2, q(1, 1))],
            vec![],
            vec![ent(1, q(4, 1))],
        ],
    );
    let x = vec![q(1, 1), q(2, 1), q(3, 1)];
    let got = linalg::sp_matvec_at(a.clone(), x.clone(), 0);
    if let Some(diff) = lq(&got, &[q(5, 1), q(0, 1), q(8, 1)]) {
        failures += 1;
        println!("DIFF sp_matvec_at_matches_the_wrapper_fixture EMITTED {diff}; VM = [5, 0, 8]");
    }

    // example <sparse_times_vector> (sp_matvec: guard-first)
    let got = sp_matvec(a.clone(), x.clone());
    if let Some(diff) = lq(&got, &[q(5, 1), q(0, 1), q(8, 1)]) {
        failures += 1;
        println!("DIFF sparse_times_vector EMITTED {diff}; VM = [5, 0, 8]");
    }
    // example <sparse_refuses_bad_col> (bad_sp_index, exact)
    let got = sp_matvec(sp(2, &[vec![ent(5, q(1, 1))], vec![]]), vec![q(1, 1), q(2, 1)]);
    if let Some(diff) = xq(&got, "bad_sp_index") {
        failures += 1;
        println!("DIFF sparse_refuses_bad_col EMITTED {diff}; VM = Err(bad_sp_index)");
    }
    // example <sparse_refuses_width>
    let got = sp_matvec(sp(2, &[vec![ent(0, q(1, 1))], vec![]]), vec![q(1, 1)]);
    if let Some(diff) = xq(&got, "bad_sp_index") {
        failures += 1;
        println!("DIFF sparse_refuses_width EMITTED {diff}; VM = Err(bad_sp_index)");
    }

    // vec_dot (spaces): 1*3 + 2*4 = 11
    let got = vec_dot(vec![q(1, 1), q(2, 1)], vec![q(3, 1), q(4, 1)]);
    if let Some(diff) = rq(&got, q(11, 1)) {
        failures += 1;
        println!("DIFF vec_dot_basic EMITTED {diff}; VM = 11");
    }
    // vec_dot width mismatch refuses dim_mismatch
    let got = vec_dot(vec![q(1, 1), q(2, 1)], vec![q(3, 1)]);
    if let Some(diff) = xq(&got, "dim_mismatch") {
        failures += 1;
        println!("DIFF vec_dot_width_refuses EMITTED {diff}; VM = Err(dim_mismatch)");
    }

    // example <symmetric_pair> (matrices mat_vec)
    let got = mat_vec(
        vec![vec![q(2, 1), q(1, 1)], vec![q(1, 1), q(2, 1)]],
        vec![q(1, 1), q(1, 1)],
    );
    if let Some(diff) = lq(&got, &[q(3, 1), q(3, 1)]) {
        failures += 1;
        println!("DIFF mat_vec_symmetric_pair EMITTED {diff}; VM = [3, 3]");
    }

    // example <sparse_agrees_dense> (sp_dense_law: THE agreement law)
    let got = sp_dense_law(
        sp(
            2,
            &[
                vec![ent(0, q(2, 1)), ent(1, q(1, 1))],
                vec![ent(0, q(1, 1)), ent(1, q(2, 1))],
            ],
        ),
        vec![q(3, 1), q(5, 1)],
    );
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF sparse_agrees_dense EMITTED {diff}; VM = true");
    }

    // example <square_walk_square>
    let got = square_walk(vec![vec![q(1, 1), q(0, 1)], vec![q(0, 1), q(1, 1)]], 0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF square_walk_square EMITTED {diff}; VM = true");
    }
    // example <square_walk_ragged> (the mat_det ragged refusal fixture)
    let got = square_walk(vec![vec![q(1, 1), q(2, 1)], vec![q(3, 1)]], 0);
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF square_walk_ragged EMITTED {diff}; VM = false");
    }

    // example <grad_quote_value> (grad_quote: THE substitute seam -
    // d_form's dynamic reference, the structural replacement, the
    // static substitute + evaluate round trip on the emitted ExprCode)
    let got = grad_quote(quadratic_body(), var("y"));
    let value = match &got {
        Ok(code) => code
            .substitute("x", CodeValue::Int(5))
            .substitute("y", CodeValue::Int(7))
            .evaluate(&ModuleTable::EMPTY)
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.clone()),
    };
    match &value {
        Ok(CodeValue::Int(v)) if *v == 33 => {}
        Ok(other) => {
            failures += 1;
            println!("DIFF grad_quote_value EMITTED {other:?}; VM = 33");
        }
        Err(e) => {
            failures += 1;
            println!("DIFF grad_quote_value EMITTED Err({e})");
        }
    }

    // example <hess_col_x> (hess_col: one forward sweep on the
    // j-th partial - the emitted d_form feeds the gradient walk)
    let vars = vec![var("x"), var("y")];
    let point = vec![q(5, 1), q(7, 1)];
    let got = hess_col(quadratic_body(), vars.clone(), point.clone(), 0);
    if let Some(diff) = lq(&got, &[q(2, 1), q(1, 1)]) {
        failures += 1;
        println!("DIFF hess_col_x EMITTED {diff}; VM = [2, 1]");
    }
    // example <hess_col_y>
    let got = hess_col(quadratic_body(), vars.clone(), point.clone(), 1);
    if let Some(diff) = lq(&got, &[q(1, 1), q(4, 1)]) {
        failures += 1;
        println!("DIFF hess_col_y EMITTED {diff}; VM = [1, 4]");
    }

    // vec_axpy (spaces, hess_vec's accumulation): 3*[1,2] + [10,20]
    let got = vec_axpy(q(3, 1), vec![q(1, 1), q(2, 1)], vec![q(10, 1), q(20, 1)]);
    if let Some(diff) = lq(&got, &[q(13, 1), q(26, 1)]) {
        failures += 1;
        println!("DIFF vec_axpy_basic EMITTED {diff}; VM = [13, 26]");
    }

    // example <hess_vec_walk_matches_the_wrapper_fixture>
    let got = linalg::hess_vec_walk(
        quadratic_body(),
        vars.clone(),
        point.clone(),
        vec![q(1, 1), q(2, 1)],
        0,
        vec![q(0, 1), q(0, 1)],
    );
    if let Some(diff) = lq(&got, &[q(4, 1), q(9, 1)]) {
        failures += 1;
        println!("DIFF hess_vec_walk_matches_the_wrapper_fixture EMITTED {diff}; VM = [4, 9]");
    }

    // example <hess_vec_quadratic> (hess_vec: H v with no dense matrix)
    let got = hess_vec(
        quadratic_body(),
        vars.clone(),
        point.clone(),
        vec![q(1, 1), q(2, 1)],
    );
    if let Some(diff) = lq(&got, &[q(4, 1), q(9, 1)]) {
        failures += 1;
        println!("DIFF hess_vec_quadratic EMITTED {diff}; VM = [4, 9]");
    }

    // example <hessian_of_quadratic_is_the_matrix> (hessian_rows: THE
    // consumer law - the form's Hessian IS the matrix)
    let got = hessian_rows(quadratic_body(), vars.clone(), point.clone(), 0);
    if let Some(diff) = llq(
        &got,
        &[&[q(2, 1), q(1, 1)], &[q(1, 1), q(4, 1)]],
    ) {
        failures += 1;
        println!("DIFF hessian_of_quadratic_is_the_matrix EMITTED {diff}; VM = [[2, 1], [1, 4]]");
    }

    // example <routes_agree_coupled> (the mixed-partials certificate:
    // column route == row route EXACTLY on the coupled fixture)
    let coupled_point = vec![q(3, 1), q(5, 1)];
    let got = linalg::hess_routes_agree(
        coupled_body(),
        vars.clone(),
        coupled_point.clone(),
        vec![q(2, 1), q(0 - 1, 1)],
    );
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF routes_agree_coupled EMITTED {diff}; VM = true");
    }

    // example <symmetric_coupled> (hess_symmetric: Clairaut on the
    // off-diagonal - both 6 at (3,5))
    let got = hess_symmetric(coupled_body(), vars.clone(), coupled_point.clone(), 0, 1);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF symmetric_coupled EMITTED {diff}; VM = true");
    }

    // example <modes_pinned> (oscillator_modes: the two-mass
    // stiffness eigenvalues [1, 3])
    let got = oscillator_modes(0);
    if let Some(diff) = lq(&got, &[q(1, 1), q(3, 1)]) {
        failures += 1;
        println!("DIFF modes_pinned EMITTED {diff}; VM = [1, 3]");
    }

    if failures > 0 {
        panic!("{failures} linalg row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_linalg() {
    let diffs = run_lane("language/modules/algebra/linalg.emath", LINALG_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted linalg disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const EVENTS_DRIVER: &str = r#"
//! Authored pins of analysis/evolution/events.emath against the
//! emitted crate (emath-49o8, the event-data lane).
//!
//! The module refused on one seam: `events[i].guard(sa.t, sa)` - a
//! record field's CLOSURE called where it is projected. The call
//! parses as an application whose function is the field-access call
//! itself, and that shape hit the blanket `call is not yet emitted`
//! fence. The callee now lowers as a value and the arguments cross
//! through the closure-call lane (the VM's law: the callee is a
//! value, stages apply through the signature). The Event record also
//! carries closure fields (`Rc<dyn Fn>` handles), which derive
//! neither Debug nor PartialEq - the record now hand-impls both,
//! closures by shared-handle identity (the buffer law) and printing
//! as `<closure>`. These rows pin the whole module: the direction
//! walk, the sign-change window with its labeled misses, the scan
//! and priority pick, the exact bisection (dyadic exact, nondyadic
//! banded to 1/2^40), and the event-aware rollout with its terminal,
//! reset, same-instant dedup, and chatter outcomes.
//! Transcription boundaries: ratios compare cross-multiplied; guard
//! and reset closures are hand-built over the same exact arithmetic
//! the authored fixtures spell.

use events::{
    bracket_at, crosses, event_bracket_root, event_directions_ok, event_rollout,
    event_rollout_at, pick_at, pick_event, scan_at, scan_events, EmathRecord_EmbeddedPair as P,
    EmathRecord_Event as E, EmathRecord_EventRun as Run, EmathRecord_SimState as S,
};

type R = (i128, i128);
type Guard = std::rc::Rc<
    dyn Fn(R) -> Result<std::rc::Rc<dyn Fn(&S) -> Result<R, String>>, String>,
>;
type Reset = std::rc::Rc<dyn Fn(&S) -> Result<S, String>>;
type Flow = std::rc::Rc<
    dyn Fn(R) -> Result<std::rc::Rc<dyn Fn(&S) -> Result<S, String>>, String>,
>;
type UnaryR = std::rc::Rc<dyn Fn(R) -> Result<R, String>>;

fn q(n: i128, d: i128) -> R {
    (n, d)
}

// The Cash-Karp 5(4) tableau exactly as one_step's cash_karp_pair
// spells it (the artifact inlines the constructor, so the driver
// transcribes the record the authored fixture names).
fn cash_karp_pair() -> P {
    P {
        s: 6,
        a: vec![
            q(0, 1), q(0, 1), q(0, 1), q(0, 1), q(0, 1), q(0, 1),
            q(1, 5), q(0, 1), q(0, 1), q(0, 1), q(0, 1), q(0, 1),
            q(3, 40), q(9, 40), q(0, 1), q(0, 1), q(0, 1), q(0, 1),
            q(3, 10), q(0 - 9, 10), q(6, 5), q(0, 1), q(0, 1), q(0, 1),
            q(0 - 11, 54), q(5, 2), q(0 - 70, 27), q(35, 27), q(0, 1), q(0, 1),
            q(1631, 55296), q(175, 512), q(575, 13824), q(44275, 110592), q(253, 4096), q(0, 1),
        ],
        b: vec![q(37, 378), q(0, 1), q(250, 621), q(125, 594), q(0, 1), q(512, 1771)],
        c: vec![q(0, 1), q(1, 5), q(3, 10), q(3, 5), q(1, 1), q(7, 8)],
        b_hat: vec![
            q(2825, 27648),
            q(0, 1),
            q(18575, 48384),
            q(13525, 55296),
            q(277, 14336),
            q(1, 4),
        ],
    }
}

fn st(t: R, y0: R) -> S {
    S { t, y: vec![y0] }
}

fn r_sub(a: R, b: R) -> R {
    red((a.0 * b.1 - b.0 * a.1, a.1 * b.1))
}

fn r_mul(a: R, b: R) -> R {
    red((a.0 * b.0, a.1 * b.1))
}

// The emitted lane's checked rational ops refuse noncanonical
// carriers (E-RAT-001), so every driver-produced ratio passes through
// the same canonicalization: positive denominator, gcd-divided,
// zero as (0, 1).
fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn red(r: R) -> R {
    let (mut n, mut d) = r;
    if d < 0 {
        n = -n;
        d = -d;
    }
    if n == 0 {
        return (0, 1);
    }
    let g = gcd(n.abs(), d);
    (n / g, d / g)
}

fn eq(a: &R, b: &R) -> bool {
    a.0 * b.1 == b.0 * a.1
}

fn lt(a: &R, b: &R) -> bool {
    a.0 * b.1 < b.0 * a.1
}

fn bq(got: &Result<bool, String>, want: bool) -> Option<String> {
    match got {
        Ok(v) if *v == want => None,
        Ok(v) => Some(format!("{v}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn dq(got: &Result<R, String>, want: R) -> Option<String> {
    match got {
        Ok(v) if eq(v, &want) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn iq(got: &Result<i64, String>, want: i64) -> Option<String> {
    match got {
        Ok(v) if *v == want => None,
        Ok(v) => Some(format!("{v}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn lq(got: &Result<Vec<i64>, String>, want: &[i64]) -> Option<String> {
    match got {
        Ok(v) if v == want => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn runq(
    got: &Result<Run, String>,
    t: R,
    y: &[R],
    complete: bool,
    attempts_used: i64,
    rejections: i64,
    events_fired: i64,
    outcome: i64,
) -> Option<String> {
    let want = format!(
        "t={:?} y={:?} complete={complete} attempts={attempts_used} rej={rejections} fired={events_fired} outcome={outcome}",
        t, y
    );
    match got {
        Ok(v)
            if eq(&v.t, &t)
                && v.y.len() == y.len()
                && v.y.iter().zip(y).all(|(a, b)| eq(a, b))
                && v.complete == complete
                && v.attempts_used == attempts_used
                && v.rejections == rejections
                && v.events_fired == events_fired
                && v.outcome == outcome => None,
        Ok(v) => Some(format!(
            "t={:?} y={:?} complete={} attempts={} rej={} fired={} outcome={} (want {want})",
            v.t, v.y, v.complete, v.attempts_used, v.rejections, v.events_fired, v.outcome
        )),
        Err(e) => Some(format!("Err({e})")),
    }
}

// guard = y[0] (the identity guard)
fn guard_y0() -> Guard {
    std::rc::Rc::new(|_t: R| {
        Ok(std::rc::Rc::new(|s: &S| Ok(s.y[0].clone())))
    })
}

// guard = y[0] - n/d
fn guard_y0_minus(n: i128, d: i128) -> Guard {
    std::rc::Rc::new(move |_t: R| {
        Ok(std::rc::Rc::new(move |s: &S| {
            let y = s.y[0].clone();
            Ok(red((y.0 * d - n * y.1, y.1 * d)))
        }))
    })
}

// guard = n/d - y[0]
fn guard_const_minus(n: i128, d: i128) -> Guard {
    std::rc::Rc::new(move |_t: R| {
        Ok(std::rc::Rc::new(move |s: &S| {
            let y = s.y[0].clone();
            Ok(red((n * y.1 - d * y.0, d * y.1)))
        }))
    })
}

// guard = (y[0] - 1/2)^2 (the tangency fixture)
fn guard_tangent() -> Guard {
    std::rc::Rc::new(|_t: R| {
        Ok(std::rc::Rc::new(|s: &S| {
            let half = r_sub(s.y[0].clone(), q(1, 2));
            Ok(r_mul(half, half))
        }))
    })
}

// guard = -(y[0] - 1/4)(y[0] - 3/4) (the double-root fixture)
fn guard_double_root() -> Guard {
    std::rc::Rc::new(|_t: R| {
        Ok(std::rc::Rc::new(|s: &S| {
            let a = r_sub(s.y[0].clone(), q(1, 4));
            let b = r_sub(s.y[0].clone(), q(3, 4));
            Ok(r_sub(q(0, 1), r_mul(a, b)))
        }))
    })
}

fn reset_id() -> Reset {
    std::rc::Rc::new(|s: &S| Ok(s.clone()))
}

// reset: y[0] + 1
fn reset_add1() -> Reset {
    std::rc::Rc::new(|s: &S| {
        Ok(S {
            t: s.t,
            y: vec![red((s.y[0].0 + s.y[0].1, s.y[0].1))],
        })
    })
}

// reset: y[0] = 0 (the chatter fixture)
fn reset_zero() -> Reset {
    std::rc::Rc::new(|s: &S| Ok(S { t: s.t, y: vec![q(0, 1)] }))
}

// f: y' = c (constant derivative, the authored fixture family)
fn flow_const(c: i128) -> Flow {
    std::rc::Rc::new(move |t: R| {
        Ok(std::rc::Rc::new(move |_s: &S| Ok(S { t, y: vec![q(c, 1)] })))
    })
}

// g = th - 3/4 (the dyadic-root fixture)
fn g_shift_three_quarters() -> UnaryR {
    std::rc::Rc::new(|th: R| Ok(red((th.0 * 4 - 3 * th.1, th.1 * 4))))
}

// g = 3*th - 1 (the nondyadic 1/3-root fixture)
fn g_three_th_minus_one() -> UnaryR {
    std::rc::Rc::new(|th: R| Ok(red((3 * th.0 - th.1, th.1))))
}

fn ev(guard: Guard, direction: i64, terminal: bool, priority: i64, reset: Reset) -> E {
    E { guard, direction, terminal, priority, reset }
}

fn main() {
    let mut failures = 0usize;

    // example <valid_directions> (event_directions_ok)
    let got = event_directions_ok(vec![ev(guard_y0(), 1, false, 0, reset_id())], 0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF valid_directions EMITTED {diff}; VM = true");
    }
    // example <invalid_direction>
    let got = event_directions_ok(vec![ev(guard_y0(), 2, false, 0, reset_id())], 0);
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF invalid_direction EMITTED {diff}; VM = false");
    }

    // example <strict_rising_crosses> (crosses: the sign-change
    // window)
    let got = crosses(1, q(0 - 1, 1), q(1, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF strict_rising_crosses EMITTED {diff}; VM = true");
    }
    // example <rising_filtered_for_falling_event>
    let got = crosses(0 - 1, q(0 - 1, 1), q(1, 1));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF rising_filtered_for_falling_event EMITTED {diff}; VM = false");
    }
    // example <zero_at_right_endpoint_is_completed>
    let got = crosses(1, q(0 - 1, 1), q(0, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF zero_at_right_endpoint_is_completed EMITTED {diff}; VM = true");
    }
    // example <zero_at_left_endpoint_is_root>
    let got = crosses(1, q(0, 1), q(1, 1));
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF zero_at_left_endpoint_is_root EMITTED {diff}; VM = true");
    }
    // example <both_zero_is_no_crossing>
    let got = crosses(0, q(0, 1), q(0, 1));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF both_zero_is_no_crossing EMITTED {diff}; VM = false");
    }
    // example <tangent_window_misses>
    let got = crosses(0, q(1, 1), q(1, 1));
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF tangent_window_misses EMITTED {diff}; VM = false");
    }

    // example <scans_in_declaration_order> (scan_at: THE field-call
    // seam - events[i].guard projected and applied)
    let scan_events_fixture = || {
        vec![
            ev(guard_y0_minus(1, 2), 1, false, 0, reset_id()),
            ev(guard_const_minus(1, 4), 0, false, 0, reset_id()),
        ]
    };
    let got = scan_at(
        scan_events_fixture(),
        st(q(0, 1), q(0, 1)),
        st(q(1, 1), q(1, 1)),
        1,
        vec![],
    );
    if let Some(diff) = lq(&got, &[0, 1]) {
        failures += 1;
        println!("DIFF scans_in_declaration_order EMITTED {diff}; VM = [0, 1]");
    }
    // example <wrapper_scans_in_declaration_order> (scan_events)
    let got = scan_events(
        scan_events_fixture(),
        st(q(0, 1), q(0, 1)),
        st(q(1, 1), q(1, 1)),
    );
    if let Some(diff) = lq(&got, &[0, 1]) {
        failures += 1;
        println!("DIFF wrapper_scans_in_declaration_order EMITTED {diff}; VM = [0, 1]");
    }

    // example <priority_wins> (pick_at: strictly smaller priority)
    let prio_fixture = |pa: i64, pb: i64| {
        vec![
            ev(guard_y0(), 0, false, pa, reset_id()),
            ev(guard_y0(), 0, false, pb, reset_id()),
        ]
    };
    let got = pick_at(prio_fixture(5, 1), vec![0, 1], 1, 0 - 1);
    if let Some(diff) = iq(&got, 1) {
        failures += 1;
        println!("DIFF priority_wins EMITTED {diff}; VM = 1");
    }
    // example <tie_keeps_declaration_order> (ties REPLACE: the
    // earlier declaration wins)
    let got = pick_at(prio_fixture(2, 2), vec![0, 1], 1, 0 - 1);
    if let Some(diff) = iq(&got, 0) {
        failures += 1;
        println!("DIFF tie_keeps_declaration_order EMITTED {diff}; VM = 0");
    }

    // example <empty_scan_answers_minus_one> (pick_event)
    let got = pick_event(vec![ev(guard_y0(), 0, false, 0, reset_id())], vec![]);
    if let Some(diff) = iq(&got, 0 - 1) {
        failures += 1;
        println!("DIFF empty_scan_answers_minus_one EMITTED {diff}; VM = -1");
    }
    // example <wrapper_picks_the_strict_priority_winner>
    let got = pick_event(prio_fixture(5, 1), vec![0, 1]);
    if let Some(diff) = iq(&got, 1) {
        failures += 1;
        println!("DIFF wrapper_picks_the_strict_priority_winner EMITTED {diff}; VM = 1");
    }

    // example <dyadic_root_found_exactly> (bracket_at: exact
    // bisection - dyadic roots found EXACTLY)
    let got = bracket_at(g_shift_three_quarters(), q(0, 1), q(1, 1), q(0 - 3, 4), 40);
    if let Some(diff) = dq(&got, q(3, 4)) {
        failures += 1;
        println!("DIFF dyadic_root_found_exactly EMITTED {diff}; VM = 3/4");
    }
    // example <nondyadic_root_bracketed> (root 1/3 banded to 1/2^40,
    // never returned exactly)
    let got = bracket_at(g_three_th_minus_one(), q(0, 1), q(1, 1), q(0 - 1, 1), 40);
    let band = q(1, 1099511627776);
    let verdict = match &got {
        Ok(v)
            if lt(&r_sub(v.clone(), q(1, 3)), &band)
                && lt(&r_sub(q(1, 3), v.clone()), &band)
                && !eq(v, &q(1, 3)) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    };
    if let Some(diff) = verdict {
        failures += 1;
        println!("DIFF nondyadic_root_bracketed EMITTED {diff}; VM = within 1/2^40 of 1/3, != 1/3");
    }

    // example <dyadic_root_at_the_entry_seam> (event_bracket_root)
    let got = event_bracket_root(g_shift_three_quarters());
    if let Some(diff) = dq(&got, q(3, 4)) {
        failures += 1;
        println!("DIFF dyadic_root_at_the_entry_seam EMITTED {diff}; VM = 3/4");
    }
    // example <nondyadic_root_banded_at_the_entry_seam>
    let got = event_bracket_root(g_three_th_minus_one());
    let band = q(1, 1099511627776);
    let verdict = match &got {
        Ok(v)
            if lt(&r_sub(v.clone(), q(1, 3)), &band)
                && lt(&r_sub(q(1, 3), v.clone()), &band)
                && !eq(v, &q(1, 3)) => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    };
    if let Some(diff) = verdict {
        failures += 1;
        println!("DIFF nondyadic_root_banded_at_the_entry_seam EMITTED {diff}; VM = within 1/2^40 of 1/3, != 1/3");
    }

    // example <terminal_event_exact_half> (event_rollout_at: y' = -1
    // from y = 1, guard y - 1/2 falls through 1/2 at exactly t = 1/2)
    let pair = cash_karp_pair();
    let tol = || vec![q(1, 4)];
    let got = event_rollout_at(
        flow_const(0 - 1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(1, 1),
        q(1, 4),
        q(1, 1),
        8,
        4,
        vec![ev(guard_y0_minus(1, 2), 0 - 1, true, 0, reset_id())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(1, 1)),
        q(1, 1),
    );
    if let Some(diff) = runq(&got, q(1, 2), &[q(1, 2)], false, 1, 0, 1, 3) {
        failures += 1;
        println!("DIFF terminal_event_exact_half EMITTED {diff}; VM = t=1/2 y=[1/2] terminal");
    }

    // example <reset_continues_to_exact_end> (y' = 1, guard y - 3/4,
    // reset adds 1, completes at t = 1 with y = 2 exactly)
    let got = event_rollout_at(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(1, 1),
        q(1, 4),
        q(1, 1),
        8,
        4,
        vec![ev(guard_y0_minus(3, 4), 1, false, 0, reset_add1())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(0, 1)),
        q(1, 1),
    );
    if let Some(diff) = runq(&got, q(1, 1), &[q(2, 1)], true, 2, 0, 1, 0) {
        failures += 1;
        println!("DIFF reset_continues_to_exact_end EMITTED {diff}; VM = t=1 y=[2] complete");
    }

    // example <initial_root_fires_once_then_dedup> (root AT the
    // initial time fires once; the same-instant refire suppresses)
    let got = event_rollout_at(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(3, 4),
        q(1, 4),
        q(3, 4),
        8,
        4,
        vec![ev(guard_y0(), 1, false, 0, reset_id())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(0, 1)),
        q(3, 2),
    );
    if let Some(diff) = runq(&got, q(3, 2), &[q(3, 2)], true, 3, 0, 1, 0) {
        failures += 1;
        println!("DIFF initial_root_fires_once_then_dedup EMITTED {diff}; VM = t=3/2 y=[3/2] complete");
    }

    // example <chatter_exhausts_event_budget> (reset moves the state
    // back below the threshold: re-crossings exhaust the budget)
    let got = event_rollout_at(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(3, 4),
        q(1, 4),
        q(3, 4),
        16,
        3,
        vec![ev(guard_y0_minus(3, 4), 1, false, 0, reset_zero())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(0, 1)),
        q(4, 1),
    );
    if let Some(diff) = runq(&got, q(9, 4), &[q(0, 1)], false, 3, 0, 3, 2) {
        failures += 1;
        println!("DIFF chatter_exhausts_event_budget EMITTED {diff}; VM = t=9/4 y=[0] chatter");
    }

    // example <tangent_guard_never_fires> ((y - 1/2)^2 touches zero
    // without crossing: the sign-change window misses it - labeled)
    let got = event_rollout_at(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(1, 1),
        q(1, 4),
        q(1, 1),
        8,
        4,
        vec![ev(guard_tangent(), 0, true, 0, reset_id())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(0, 1)),
        q(1, 1),
    );
    if let Some(diff) = runq(&got, q(1, 1), &[q(1, 1)], true, 1, 0, 0, 0) {
        failures += 1;
        println!("DIFF tangent_guard_never_fires EMITTED {diff}; VM = t=1 y=[1] complete, 0 events");
    }

    // example <double_root_window_misses_both> (two crossings inside
    // one segment: endpoint signs agree, the window sees nothing)
    let got = event_rollout_at(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(1, 1),
        q(1, 4),
        q(1, 1),
        8,
        4,
        vec![ev(guard_double_root(), 0, true, 0, reset_id())],
        0,
        0,
        0,
        q(0, 1),
        false,
        st(q(0, 1), q(0, 1)),
        q(1, 1),
    );
    if let Some(diff) = runq(&got, q(1, 1), &[q(1, 1)], true, 1, 0, 0, 0) {
        failures += 1;
        println!("DIFF double_root_window_misses_both EMITTED {diff}; VM = t=1 y=[1] complete, 0 events");
    }

    // example <final_time_root_fires_and_completes> (event_rollout:
    // the crossing completes exactly at the final time, theta = 1)
    let got = event_rollout(
        flow_const(1),
        pair.clone(),
        4,
        5,
        tol(),
        tol(),
        q(3, 4),
        q(1, 4),
        q(3, 4),
        8,
        4,
        vec![ev(guard_y0_minus(3, 4), 1, false, 0, reset_id())],
        st(q(0, 1), q(0, 1)),
        q(3, 4),
    );
    if let Some(diff) = runq(&got, q(3, 4), &[q(3, 4)], true, 1, 0, 1, 0) {
        failures += 1;
        println!("DIFF final_time_root_fires_and_completes EMITTED {diff}; VM = t=3/4 y=[3/4] complete");
    }

    // example <wrong_direction_event_stays_silent> (a falling
    // crossing against a RISING event: direction filtering keeps it
    // silent)
    let got = event_rollout(
        flow_const(0 - 1),
        pair,
        4,
        5,
        tol(),
        tol(),
        q(1, 1),
        q(1, 4),
        q(1, 1),
        8,
        4,
        vec![ev(guard_y0_minus(1, 2), 1, true, 0, reset_id())],
        st(q(0, 1), q(1, 1)),
        q(1, 1),
    );
    if let Some(diff) = runq(&got, q(1, 1), &[q(0, 1)], true, 1, 0, 0, 0) {
        failures += 1;
        println!("DIFF wrong_direction_event_stays_silent EMITTED {diff}; VM = t=1 y=[0] complete, 0 events");
    }

    if failures > 0 {
        panic!("{failures} events row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_events() {
    let diffs = run_lane(
        "language/modules/analysis/evolution/events.emath",
        EVENTS_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted events disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Rewriting lane pins (emath-49o8): search/rewriting refused on two
/// seams - `quote.make(quote.evaluate(source))` rebuilt code from an
/// evaluated SCALAR (the make lane took only node records and codes),
/// and `quote.substitute(quote(function x in Rat: x + 1), x, 7)` crossed
/// an Int constant into a Rat-carried template (refused instead of the
/// VM's exact Int-to-Rat admission). Make now rebuilds a scalar as the
/// closed literal of its exact value and the substitute value crosses
/// the carrier through the shared numeric boundary. The query
/// (AvoidCapture) is not an emitted entry.
const REWRITING_DRIVER: &str = r#"
//! Authored pins of search/rewriting.emath against the emitted crate.
//! Transcription boundary: `quote.identity(a) != quote.identity(b)` pins
//! as structural inequality of the two codes' views (identity is the
//! VM's content hash; the artifact carries no hash counterpart).

use rewriting::emath_rt::code::{evaluate_expr, open_expr, CodeValue, ExprCode};
use rewriting::emath_rt::code_tree::{node_eq, CodeTree, ModuleTable, TreeBinary};
use rewriting::{fold_code, hypothesis_value, shadowing_preserved};

type R = (i128, i128);

// Distilled-tree builders (the same shapes quote(...) distills).
fn int(value: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(value))
}

fn bin(op: TreeBinary, left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn closed(tree: CodeTree) -> ExprCode {
    open_expr(Vec::new(), tree, None, std::collections::BTreeMap::new())
}

// The folded value must stay on the RATIONAL carrier (the int-truncation
// fence): an Int result is a diff even when numerically equal.
fn rat_value(got: &Result<CodeValue, String>, want: R) -> Option<String> {
    match got {
        Ok(CodeValue::Rat((n, d))) if n * want.1 == want.0 * d => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn rq(got: &Result<R, String>, want: R) -> Option<String> {
    match got {
        Ok(v) if v.0 * want.1 == want.0 * v.1 => None,
        Ok(v) => Some(format!("{v:?}")),
        Err(e) => Some(format!("Err({e})")),
    }
}

fn main() {
    let mut failures = 0usize;
    let mut check = |name: &str, diff: Option<String>, vm: &str| {
        if let Some(diff) = diff {
            failures += 1;
            println!("DIFF {name} EMITTED {diff}; VM = {vm}");
        }
    };

    // example <fold_changes_code_preserves_value>: source = quote(5 / 1 + 2 * 3)
    let source = closed(bin(
        TreeBinary::Add,
        bin(TreeBinary::Div, int(5), int(1)),
        bin(TreeBinary::Mul, int(2), int(3)),
    ));
    match fold_code(source.clone()) {
        Ok(folded) => {
            check(
                "fold_changes_code_preserves_value/value",
                rat_value(&evaluate_expr(&folded, &ModuleTable::EMPTY), (11, 1)),
                "11/1",
            );
            let same = node_eq(
                folded.view(&ModuleTable::EMPTY),
                source.view(&ModuleTable::EMPTY),
            );
            check(
                "fold_changes_code_preserves_value/code",
                match same {
                    Ok(false) => None,
                    other => Some(format!("{other:?}")),
                },
                "the folded code differs from the source",
            );
        }
        Err(e) => check("fold_changes_code_preserves_value", Some(format!("Err({e})")), "11/1"),
    }

    // example <fold_keeps_rational_fence>: source = quote(1 / 2 + 1 / 3)
    let source = closed(bin(
        TreeBinary::Add,
        bin(TreeBinary::Div, int(1), int(2)),
        bin(TreeBinary::Div, int(1), int(3)),
    ));
    let got = fold_code(source).and_then(|folded| evaluate_expr(&folded, &ModuleTable::EMPTY));
    check("fold_keeps_rational_fence", rat_value(&got, (5, 6)), "5/6");

    // examples <edit_six_gives_eleven>, <edit_seven_gives_twelve>
    check("edit_six_gives_eleven", rq(&hypothesis_value((6, 1)), (11, 1)), "11/1");
    check("edit_seven_gives_twelve", rq(&hypothesis_value((7, 1)), (12, 1)), "12/1");

    // example <bound_shadow_not_substituted>: unused = 0
    check("bound_shadow_not_substituted", rq(&shadowing_preserved(0), (3, 1)), "3/1");

    if failures > 0 {
        panic!("{failures} rewriting row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_rewriting() {
    let diffs = run_lane("language/modules/search/rewriting.emath", REWRITING_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted rewriting disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Levenberg lane pins (emath-49o8): numerics/levenberg refused on two
/// seams of the curried model closure. `model_at = predict(params)`
/// binds a PARTIAL application of the declared `sequence[Float64] ->
/// Float64 -> Float64` input - a closure value - but the def was not
/// marked closure-valued, so `model_at(t)` lowered as the field
/// projection `t.model_at` (its register kind unknown; the runnable
/// `predict_at` even emitted `&t.model_at` over an f64). Defs now carry
/// the curried stage count of declared arrows and function literals,
/// and an application leaving stages unapplied binds a closure; a
/// projection off a scalar or callable carrier refuses by name. The
/// literal `function p in sequence[Float64]: function t in Float64:
/// ..` fell to the untyped numeric-program lane because the bracket
/// domain spelling `sequence[T]` had no signature; it now types as
/// `Vector<T>` and the literal carries the typed closure ABI.
const LEVENBERG_DRIVER: &str = r#"
//! Authored pins of numerics/levenberg.emath against the emitted crate.

use std::rc::Rc;

use levenberg::EmathRecord_DampingPolicy as Policy;
use levenberg::EmathRecord_LmStep as LmStep;
use levenberg::*;

type Obs = Rc<dyn Fn(f64) -> Result<f64, String>>;
type Pred = Rc<dyn Fn(&Vec<f64>) -> Result<Obs, String>>;

// function p in sequence[Float64]: function t in Float64: p[0] + p[1] * t
fn affine() -> Pred {
    Rc::new(|p: &Vec<f64>| -> Result<Obs, String> {
        let p = p.clone();
        Ok(Rc::new(move |t: f64| Ok(p[0] + p[1] * t)))
    })
}

fn v(xs: &[f64]) -> Vec<f64> {
    xs.to_vec()
}

fn m(rows: &[&[f64]]) -> Vec<Vec<f64>> {
    rows.iter().map(|row| row.to_vec()).collect()
}

fn within(x: f64, lo: f64, hi: f64) -> bool {
    x > lo && x < hi
}

fn policy(max_iters: i64) -> Policy {
    Policy {
        step: 0.000001,
        lambda0: 0.001,
        up: 10.0,
        down: 10.0,
        floor: 0.000000000001,
        break_lambda: 1000000000000.0,
        max_iters,
        tolerance: 0.000000000001,
    }
}

fn marq() -> Policy {
    marquardt_policy(0).expect("marquardt card")
}

fn main() {
    let mut failures = 0usize;
    let mut row = |name: &str, ok: bool, got: String, vm: &str| {
        if !ok {
            failures += 1;
            println!("DIFF {name} EMITTED {got}; VM = {vm}");
        }
    };
    macro_rules! pin {
        ($name:expr, $got:expr, |$v:ident| $ok:expr, $vm:expr) => {{
            let got = $got;
            let ok = match &got {
                Ok($v) => $ok,
                Err(_) => false,
            };
            row($name, ok, format!("{got:?}"), $vm);
        }};
    }
    macro_rules! refuses {
        ($name:expr, $got:expr, $atom:expr) => {{
            let got = $got;
            let ok = matches!(&got, Err(e) if e == $atom);
            row($name, ok, format!("{got:?}"), $atom);
        }};
    }

    pin!("curried_seam_applies", predict_at(affine(), v(&[2.0, 3.0]), 1.0), |r| *r == 5.0, "5");
    pin!("exact_interpolation_is_zero", sum_squares(affine(), v(&[0.0, 1.0]), v(&[2.0, 5.0]), v(&[2.0, 3.0])), |r| *r == 0.0, "0");
    pin!("unit_residual_squares_to_one", sum_squares(affine(), v(&[0.0, 1.0]), v(&[2.0, 6.0]), v(&[2.0, 3.0])), |r| *r == 1.0, "1");
    pin!("residuals_in_data_order", residuals(affine(), v(&[0.0, 1.0]), v(&[2.0, 6.0]), v(&[2.0, 3.0])), |r| *r == [0.0, -1.0], "[0, -1]");
    pin!("shifts_one_slot_keeps_order", shifted(v(&[1.0, 2.0, 3.0]), 1, 0.5), |r| *r == [1.0, 2.5, 3.0], "[1, 2.5, 3]");
    pin!("affine_jacobian_is_exact", jacobian_row(affine(), 2.0, v(&[2.0, 3.0]), v(&[1.0, 1.0]), 0.000001),
        |r| within(r[0], 0.999999999, 1.000000001) && within(r[1], 1.999999999, 2.000000001), "[~1, ~2]");
    pin!("weights_scale_the_column", jacobian_row(affine(), 1.0, v(&[2.0, 3.0]), v(&[1.0, 2.0]), 0.000001),
        |r| within(r[1], 1.999999999, 2.000000001), "[_, ~2]");
    pin!("one_row_per_data_row_ascending", jacobian(affine(), v(&[1.0, 2.0]), v(&[2.0, 3.0]), v(&[1.0, 1.0]), 0.000001),
        |r| within(r[0][1], 0.999999999, 1.000000001) && within(r[1][1], 1.999999999, 2.000000001), "rows [_, ~1], [_, ~2]");
    pin!("finite_matrix_passes", finite_matrix(m(&[&[1.0, 0.0], &[0.0, 1.0]])), |r| *r, "true");
    let jac = || m(&[&[1.0, 1.0], &[1.0, 2.0]]);
    pin!("gram_entry_sums_over_rows", gram(jac(), 1, 1), |r| *r == 5.0, "5");
    pin!("cross_entry_is_three", gram(jac(), 0, 1), |r| *r == 3.0, "3");
    pin!("damped_normal_matrix", normal_of(jac(), 0.5), |r| *r == m(&[&[2.5, 3.0], &[3.0, 5.5]]), "[[2.5, 3], [3, 5.5]]");
    pin!("full_gradient_vector", gradient_of(jac(), v(&[0.0, 1.0])), |r| *r == [1.0, 2.0], "[1, 2]");
    let normal = || m(&[&[2.0, 3.0], &[3.0, 5.0]]);
    pin!("every_row_augmented", augment(normal(), v(&[9.0, 8.0])),
        |r| r[0][2] == 9.0 && r[1][0] == 3.0 && r[1][2] == 8.0, "[[2,3,9],[3,5,8]]");
    let aug = || m(&[&[2.0, 0.0], &[4.0, 4.0]]);
    pin!("clears_the_column_below_the_pivot", elim_below(aug(), 0, true),
        |r| r[0][0] == 2.0 && r[1][0] == 0.0 && r[1][1] == 4.0, "[[2,0],[0,4]]");
    pin!("failed_pivot_leaves_rows_untouched", elim_below(aug(), 0, false),
        |r| r[1][0] == 4.0 && r[1][1] == 4.0, "[[2,0],[4,4]]");
    pin!("already_triangular_system_solves", solve_columns(m(&[&[1.0, 0.0, 3.0], &[0.0, 1.0, 4.0]]), 0),
        |r| r.ok && r.rows[0][2] == 3.0 && r.rows[1][2] == 4.0, "ok, rhs [3, 4]");
    pin!("zero_pivot_column_refuses", solve_columns(m(&[&[0.0, 0.0, 1.0]]), 0), |r| !r.ok, "ok false");
    let tri = || m(&[&[2.0, 1.0, 9.0], &[0.0, 4.0, 8.0]]);
    pin!("upper_triangular_solve", back_substitute(tri(), 2), |r| *r == [3.5, 2.0], "[3.5, 2]");
    pin!("two_by_two_system", solve_damped(m(&[&[3.0, 1.0], &[1.0, 2.0]]), v(&[9.0, 8.0])),
        |r| within(r.step[0], 1.999999999999, 2.000000000001) && within(r.step[1], 2.999999999999, 3.000000000001), "step ~[2, 3]");
    pin!("needs_pivoting", solve_damped(m(&[&[0.0, 1.0], &[1.0, 0.0]]), v(&[7.0, 5.0])),
        |r| within(r.step[0], 4.999999999999, 5.000000000001) && within(r.step[1], 6.999999999999, 7.000000000001), "step ~[5, 7]");
    pin!("singular_refuses_not_faults", solve_damped(m(&[&[1.0, 2.0], &[2.0, 4.0]]), v(&[3.0, 6.0])), |r| !r.ok, "ok false");
    let ok_step = || LmStep { ok: true, step: v(&[1.5, 2.5]) };
    let no_step = || LmStep { ok: false, step: vec![] };
    pin!("ok_step_reads_the_slot", step_or_zero(ok_step(), 0), |r| *r == 1.5, "1.5");
    pin!("index_beyond_the_step_is_zero", step_or_zero(ok_step(), 5), |r| *r == 0.0, "0");
    pin!("not_ok_step_is_zero_everywhere", step_or_zero(no_step(), 0), |r| *r == 0.0, "0");
    pin!("subtracts_the_step", apply_step(v(&[2.0, 3.0]), LmStep { ok: true, step: v(&[0.5, 1.0]) }), |r| *r == [1.5, 2.0], "[1.5, 2]");
    pin!("failed_solve_keeps_the_current_point", apply_step(v(&[2.0, 3.0]), no_step()), |r| *r == [2.0, 3.0], "[2, 3]");
    let line = |got: &Vec<f64>, tol: f64| within(got[0], 2.0 - tol, 2.0 + tol) && within(got[1], 3.0 - tol, 3.0 + tol);
    let ts = || v(&[0.0, 1.0]);
    let ys = || v(&[2.0, 5.0]);
    let ones = || v(&[1.0, 1.0]);
    pin!("affine_recovery_in_one_accepted_chain", lm_advance(affine(), ts(), ys(), ones(), ones(), 10.0, 0.001, 0.0, false, 0, marq()),
        |r| r.converged && line(&r.params, 0.000000001), "converged, params ~(2, 3)");
    pin!("stalled_progress_converges_without_counting", lm_iter(affine(), ts(), ys(), ones(), ones(), 0.001, 10.0, true, 7, marq()),
        |r| r.converged && r.iters == 7 && r.sse == 10.0, "converged, iters 7, sse 10");
    pin!("zero_cap_refuses_to_advance", lm_iter(affine(), ts(), ys(), ones(), ones(), 0.001, 0.0, false, 0, policy(0)),
        |r| !r.converged && r.iters == 0, "not converged, iters 0");
    refuses!("empty_data_refuses", damped_fit(affine(), vec![], vec![], ones(), ones(), marq()), "empty_data");
    refuses!("length_mismatch_refuses", damped_fit(affine(), ts(), v(&[2.0]), ones(), ones(), marq()), "data_length_mismatch");
    refuses!("weight_arity_refuses", damped_fit(affine(), ts(), ys(), ones(), v(&[1.0]), marq()), "weight_arity_mismatch");
    refuses!("empty_parameters_refuse", damped_fit(affine(), v(&[0.0]), v(&[2.0]), vec![], vec![], marq()), "empty_parameters");
    pin!("affine_recovery_converges", damped_fit(affine(), ts(), ys(), ones(), ones(), marq()),
        |r| r.converged && line(&r.params, 0.000001), "converged, params ~(2, 3)");
    pin!("classic_raises_and_sheds_at_the_same_rate", marquardt_policy(0),
        |r| r.up == r.down && r.lambda0 > 0.000001 && r.lambda0 < 0.1, "up == down, lambda0 in (1e-6, 0.1)");
    pin!("bold_retries_ten_times_harder_than_it_sheds", bold_policy(0),
        |r| r.up == 10.0 * r.down && r.lambda0 < 0.00001, "up == 10 down, lambda0 < 1e-5");
    pin!("cautious_sheds_ten_times_slower_than_it_raises", cautious_policy(0),
        |r| r.down == 10.0 * r.up && r.lambda0 > 0.01, "down == 10 up, lambda0 > 0.01");
    pin!("compatibility_entry_recovers_the_line", levenberg_fit(affine(), ts(), ys(), ones(), ones()), |r| line(r, 0.000001), "~(2, 3)");
    pin!("three_passes_charge_three_nanos", iter_penalty(3), |r| within(*r, 0.0000000029, 0.0000000031), "~3e-9");
    pin!("zero_passes_charge_nothing", iter_penalty(0), |r| *r == 0.0, "0");
    pin!("non_converged_is_dominated", fit_cost(affine(), ts(), ys(), ones(), ones(), policy(0)), |r| *r == 1000000.0, "1e6");
    pin!("converged_cost_is_sse_plus_work_charge", fit_cost(affine(), ts(), ys(), ones(), ones(), marq()), |r| *r < 0.000001, "< 1e-6");
    pin!("cautious_leads_the_rack", card_at(0), |r| r.lambda0 == 0.1 && r.down == 100.0, "lambda0 0.1, down 100");
    pin!("classic_sits_in_the_middle", card_at(1), |r| r.lambda0 == 0.001, "lambda0 0.001");
    pin!("bold_closes_the_rack", card_at(2), |r| r.lambda0 == 0.000001, "lambda0 1e-6");
    pin!("beyond_the_rack_falls_to_bold", card_at(9), |r| r.lambda0 == 0.000001, "lambda0 1e-6");
    pin!("exact_tie_breaks_to_the_first_card", tune_damping(affine(), ts(), ys(), v(&[2.0, 3.0]), ones()),
        |r| r.lambda0 == 0.1 && r.down == 100.0, "lambda0 0.1, down 100");
    pin!("recovers_planted_rational_decay", PlantedFit(0), |r| *r, "true");
    pin!("classic_card_constants", MarquardtCardIsClassic(0), |r| *r, "true");
    pin!("cautious_takes_six", CardIters(0), |r| *r == 6, "6");
    pin!("classic_takes_four", CardIters(1), |r| *r == 4, "4");
    pin!("bold_takes_four", CardIters(2), |r| *r == 4, "4");
    pin!("cap_is_a_dial", CustomCardCap(0), |r| *r, "true");
    pin!("tuner_picks_bold", TunePicksWinner(0), |r| *r, "true");

    if failures > 0 {
        panic!("{failures} levenberg row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_levenberg() {
    let diffs = run_lane(
        "language/modules/numerics/levenberg.emath",
        LEVENBERG_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted levenberg disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Walk lane pins (emath-49o8): quote/walk refused on two carriers its
/// declarations name. `packed: Fragment` had no interchange signature,
/// so the entry carried the numeric fallback Int and `quote.open`
/// refused it; the untyped `items: sequence` of walk_list/walk_pairs
/// had none either, so `quote.view(items[i])` refused. A `Fragment`
/// and an unparameterized `sequence` are the dynamic node family (the
/// VM's package records and heterogeneous lists), and tuples built in
/// emitted code are positional family sequences whose `_N` indexes
/// them (the typed lane's tuple law, extended to the family).
const WALK_DRIVER: &str = r#"
//! Authored pins of quote/walk.emath against the emitted crate. The
//! givens are built the way the VM builds them: `quote(...)` as the
//! distilled tree, `quote.view(..)` through the runtime's own view.

use walk::emath_rt::code::{code_eq, evaluate_expr, open_expr, CodeValue, ExprCode};
use walk::emath_rt::code_tree::{
    node_as_code, node_field, node_index, view_quoted, CodeTree, ModuleTable, NodeValue,
    TreeBinary,
};
use walk::{walk, walk_list, walk_pairs, walk_term};

fn int(value: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(value))
}

fn add(left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op: TreeBinary::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn closed(tree: CodeTree) -> ExprCode {
    open_expr(Vec::new(), tree, None, std::collections::BTreeMap::new())
}

fn code_node(tree: CodeTree) -> NodeValue {
    NodeValue::Code(Box::new(closed(tree)))
}

// `quote.evaluate(code) == want` under the VM's value equality.
fn evaluates_to(code: Result<ExprCode, String>, want: i64) -> Option<String> {
    let value = code.and_then(|code| evaluate_expr(&code, &ModuleTable::EMPTY));
    match &value {
        Ok(v) if code_eq(v, &CodeValue::Int(want)) == Ok(true) => None,
        other => Some(format!("{other:?}")),
    }
}

fn empty(got: &Result<NodeValue, String>) -> Option<String> {
    match got {
        Ok(NodeValue::Sequence(items)) if items.is_empty() => None,
        other => Some(format!("{other:?}")),
    }
}

fn main() {
    let mut failures = 0usize;
    let mut check = |name: &str, diff: Option<String>, vm: &str| {
        if let Some(diff) = diff {
            failures += 1;
            println!("DIFF {name} EMITTED {diff}; VM = {vm}");
        }
    };

    // example <open_add_left>: packed = quote.view(quote(2 + 1)).args[0]
    let packed = node_field(view_quoted(closed(add(int(2), int(1))), &ModuleTable::EMPTY), "args")
        .and_then(|args| node_index(args, 0));
    check(
        "open_add_left",
        evaluates_to(packed.and_then(walk), 2),
        "evaluate(result) == 2",
    );

    // examples <literal>, <sum>
    check("literal", evaluates_to(walk_term(closed(int(3))), 3), "3");
    check("sum", evaluates_to(walk_term(closed(add(int(2), int(1)))), 3), "3");

    // walk_list <empty>, <one_literal>
    check("walk_list/empty", empty(&walk_list(NodeValue::Sequence(vec![]), 0)), "length 0");
    let got = walk_list(NodeValue::Sequence(vec![code_node(int(3))]), 0)
        .and_then(|result| node_index(result, 0))
        .and_then(node_as_code);
    check("walk_list/one_literal", evaluates_to(got, 3), "evaluate(result[0]) == 3");

    // walk_pairs <empty>, <one_pair>: items = [(quote(2), quote(1))]
    check("walk_pairs/empty", empty(&walk_pairs(NodeValue::Sequence(vec![]), 0)), "length 0");
    let pair = NodeValue::Tuple(vec![code_node(int(2)), code_node(int(1))]);
    let result = walk_pairs(NodeValue::Sequence(vec![pair]), 0);
    let part = |field: &str| {
        result
            .clone()
            .and_then(|result| node_index(result, 0))
            .and_then(|first| node_field(first, field))
            .and_then(node_as_code)
    };
    check("walk_pairs/one_pair/_0", evaluates_to(part("_0"), 2), "evaluate(result[0]._0) == 2");
    check("walk_pairs/one_pair/_1", evaluates_to(part("_1"), 1), "evaluate(result[0]._1) == 1");

    if failures > 0 {
        panic!("{failures} walk row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_walk() {
    let diffs = run_lane("language/modules/quote/walk.emath", WALK_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted walk disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Discovery lane pins (emath-49o8): search/discovery refused because
/// `claim = quote.evaluate(conjecture.claim)` then `claim(domain[at])`
/// APPLIES the evaluated value - a quoted function literal Rat -> Bool
/// carried by a bare `Code` - and the expression-template lane only
/// knew evaluate as a scalar. A def (or in-place callee) bound to
/// `quote.evaluate` and applied with one consistent arity now lowers as
/// the applied evaluate: the rt closes the code once (the same
/// stale/unbound refusals, at the same point) and returns a callable
/// over the value union that binds the literal's parameters and runs the
/// body through the shared scalar tree evaluator (a non-function code
/// refuses `type: value is not callable`, the VM's words). The union
/// result beside a typed arm joins onto the typed carrier (the render's
/// existing checked projection, now agreed by the kind walk), and code
/// values compare by tree + dependencies (the node family's code law),
/// so records with Code fields derive PartialEq. The quoted literal's
/// binder domain rides the distilled tree, so an argument outside it
/// refuses `type` exactly as the VM's closure does
/// (`claim_domain_is_checked`: an Int claim at the point 1/2).
const DISCOVERY_DRIVER: &str = r#"
//! Authored pins of search/discovery.emath against the emitted crate.
//! The conjectures are built as the distilled trees `quote(...)` yields.

use discovery::emath_rt::code::{open_expr, CodeValue, ExprCode};
use discovery::emath_rt::code_tree::{CodeTree, TreeBinary, TreeDomain};
use discovery::{challenge_conjecture, EmathRecord_Conjecture as Conjecture};

type R = (i128, i128);

fn int(value: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(value))
}

fn x() -> CodeTree {
    CodeTree::Path(vec!["x".to_string()])
}

fn bin(op: TreeBinary, left: CodeTree, right: CodeTree) -> CodeTree {
    CodeTree::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

// quote(function x in <domain>: body)
fn function_in(domain: &str, body: CodeTree) -> ExprCode {
    open_expr(
        Vec::new(),
        CodeTree::Closure {
            param: "x".to_string(),
            domain: TreeDomain::Scalar(domain.to_string()),
            body: Box::new(body),
        },
        None,
        std::collections::BTreeMap::new(),
    )
}

// x * x > 0 / 1
fn square_positive() -> ExprCode {
    function_in("Rat", bin(
        TreeBinary::Gt,
        bin(TreeBinary::Mul, x(), x()),
        bin(TreeBinary::Div, int(0), int(1)),
    ))
}

fn always() -> ExprCode {
    function_in("Rat", CodeTree::Literal(CodeValue::Bool(true)))
}

// x != 0 / 1
fn nonzero() -> ExprCode {
    function_in("Rat", bin(TreeBinary::Ne, x(), bin(TreeBinary::Div, int(0), int(1))))
}

fn main() {
    let mut failures = 0usize;
    let mut check = |name: &str, ok: bool, got: String, vm: &str| {
        if !ok {
            failures += 1;
            println!("DIFF {name} EMITTED {got}; VM = {vm}");
        }
    };
    let domain3: Vec<R> = vec![(1, 1), (0, 1), (-1, 1)];

    // example <falsifies_overbroad_claim>
    let got = challenge_conjecture(
        Conjecture { claim: square_positive(), assumption: always() },
        domain3.clone(),
        3,
    );
    check(
        "falsifies_overbroad_claim",
        matches!(&got, Ok(r) if r.status == 2 && r.counterexample == 1 && r.visited == 2),
        format!("{got:?}"),
        "status 2, counterexample 1, visited 2",
    );

    // example <narrowed_assumption_excludes_counterexample>
    let got = challenge_conjecture(
        Conjecture { claim: square_positive(), assumption: nonzero() },
        domain3.clone(),
        3,
    );
    check(
        "narrowed_assumption_excludes_counterexample",
        matches!(&got, Ok(r) if r.status == 1 && r.counterexample == -1 && r.witnesses.len() == 2),
        format!("{got:?}"),
        "status 1, counterexample -1, 2 witnesses",
    );

    // example <budget_is_not_proof>
    let got = challenge_conjecture(
        Conjecture { claim: square_positive(), assumption: always() },
        vec![(1, 1), (0, 1)],
        1,
    );
    check(
        "budget_is_not_proof",
        matches!(&got, Ok(r) if r.status == 0 && r.visited == 1),
        format!("{got:?}"),
        "status 0, visited 1",
    );

    // example <malformed_claim_refuses>: claim = quote(unbound_conjecture_name)
    let unbound = open_expr(
        vec!["unbound_conjecture_name".to_string()],
        CodeTree::Path(vec!["unbound_conjecture_name".to_string()]),
        None,
        std::collections::BTreeMap::new(),
    );
    let got = challenge_conjecture(
        Conjecture { claim: unbound, assumption: always() },
        vec![(1, 1)],
        1,
    );
    check(
        "malformed_claim_refuses",
        matches!(&got, Err(e) if e.starts_with("unbound_code")),
        format!("{got:?}"),
        "diagnostic.code == unbound_code",
    );

    // example <claim_calls_a_module_function>: the claim's body calls the
    // module predicate `exceeds(x * x, 0 / 1)` - the tree evaluator runs
    // the compiled entry through the module table
    let calls_module = function_in(
        "Rat",
        CodeTree::Call {
            function: Box::new(CodeTree::Path(vec!["exceeds".to_string()])),
            args: vec![bin(TreeBinary::Mul, x(), x()), bin(TreeBinary::Div, int(0), int(1))],
        },
    );
    let got = challenge_conjecture(
        Conjecture { claim: calls_module, assumption: always() },
        domain3.clone(),
        3,
    );
    check(
        "claim_calls_a_module_function",
        matches!(&got, Ok(r) if r.status == 2 && r.counterexample == 1),
        format!("{got:?}"),
        "status 2, counterexample 1",
    );

    // example <claim_domain_is_checked>: claim = quote(function x in Int:
    // x * x > 0), applied to the Rat point 1/2
    let int_claim = function_in("Int", bin(TreeBinary::Gt, bin(TreeBinary::Mul, x(), x()), int(0)));
    let got = challenge_conjecture(
        Conjecture { claim: int_claim, assumption: always() },
        vec![(1, 2)],
        1,
    );
    check(
        "claim_domain_is_checked",
        matches!(&got, Err(e) if e.starts_with("type:")),
        format!("{got:?}"),
        "diagnostic.code == type",
    );

    if failures > 0 {
        panic!("{failures} discovery row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_discovery() {
    let diffs = run_lane("language/modules/search/discovery.emath", DISCOVERY_DRIVER);
    assert!(
        diffs.is_empty(),
        "emitted discovery disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

/// Derivative lane pins (emath-49o8): analysis/derivative's two
/// composed newtons refused because `df = quote.evaluate(derivative_of(
/// f_term))` is a quoted function built at RUN time (the derived
/// derivative), carried by a bare `Code`, and PASSED - not applied - to
/// `newton_solve`'s declared `df: Rat -> Rat`. Inside the solver the
/// call `df(x)` had no callable carrier. An evaluated def that crosses
/// into a declared arrow parameter now lowers as the typed applied
/// evaluate: the rt closes the code once and the artifact adapts the
/// union callable to the declared `Rat -> Rat` (arguments wrap into the
/// union, the result projects checked onto the declared carrier). The
/// derived derivative's body runs through the rt scalar tree evaluator
/// (user-accepted 2026-09-25: run-time code cannot be compiled ahead of
/// time; a formula calling a module function refuses by name there).
const DERIVATIVE_DRIVER: &str = r#"
//! Authored pins of analysis/derivative.emath against the emitted
//! crate. Givens are built as the distilled trees `quote(...)` yields.
//! Transcription boundaries (VM-only rows, labeled): <d_index_refuses_
//! at_the_make_seam> quotes `xs[i]` and <d_receipt_construction_
//! refuses> quotes a nested `quote(x)` - neither Index nor Quotation is
//! in the distilled tree subset, so the given cannot be built here.

use std::collections::BTreeMap;

use derivative::emath_rt::code::{code_eq, open_expr, CodeValue, ExprCode};
use derivative::emath_rt::code_tree::{
    free_names_tree, node_as_code, node_eq, node_field, node_index, node_tag, CodeTree,
    ModuleTable, NodeValue, TreeBinary, TreeDomain, TreeUnary,
};
use derivative::EmathRecord_IterResult as IterResult;
use derivative::*;

type R = (i128, i128);

fn code(tree: CodeTree) -> ExprCode {
    open_expr(free_names_tree(&tree), tree, None, BTreeMap::new())
}
fn int(n: i64) -> CodeTree {
    CodeTree::Literal(CodeValue::Int(n))
}
fn var(name: &str) -> CodeTree {
    CodeTree::Path(vec![name.to_string()])
}
fn bin(op: TreeBinary, l: CodeTree, r: CodeTree) -> CodeTree {
    CodeTree::Binary { op, left: Box::new(l), right: Box::new(r) }
}
fn add(l: CodeTree, r: CodeTree) -> CodeTree {
    bin(TreeBinary::Add, l, r)
}
fn mul(l: CodeTree, r: CodeTree) -> CodeTree {
    bin(TreeBinary::Mul, l, r)
}
fn pow(l: CodeTree, r: CodeTree) -> CodeTree {
    bin(TreeBinary::Pow, l, r)
}
fn z() -> ExprCode {
    code(var("z"))
}
fn fn_z(body: CodeTree) -> ExprCode {
    code(CodeTree::Closure {
        param: "z".to_string(),
        domain: TreeDomain::Scalar("Rat".to_string()),
        body: Box::new(body),
    })
}

// quote.evaluate(...) == want, under the VM's value equality.
fn value(got: Result<ExprCode, String>, want: CodeValue) -> Option<String> {
    let got = got.and_then(|code| code.evaluate(&ModuleTable::EMPTY));
    match &got {
        Ok(v) if code_eq(v, &want) == Ok(true) => None,
        other => Some(format!("{other:?}")),
    }
}
fn at(code: &Result<ExprCode, String>, name: &str, n: i64) -> Result<ExprCode, String> {
    code.clone().map(|code| code.substitute(name, CodeValue::Int(n)))
}
fn refused<T: std::fmt::Debug>(got: &Result<T, String>) -> Option<String> {
    match got {
        Err(e) if e.starts_with("transformation_rule_unavailable") => None,
        other => Some(format!("{other:?}")),
    }
}
fn view(code: &ExprCode) -> NodeValue {
    code.view(&ModuleTable::EMPTY)
}
fn field(node: &NodeValue, name: &str) -> NodeValue {
    node_field(node, name).unwrap_or_else(|e| node_tag(&format!("<{e}>")))
}
fn arg(node: &NodeValue, i: i64) -> NodeValue {
    node_field(node, "args")
        .and_then(|args| node_index(args, i))
        .unwrap_or_else(|e| node_tag(&format!("<{e}>")))
}
fn sub_view(node: NodeValue) -> NodeValue {
    match node_as_code(&node) {
        Ok(code) => view(&code),
        Err(e) => node_tag(&format!("<{e}>")),
    }
}
fn is_tag(node: &NodeValue, tag: &str) -> bool {
    node_eq(node, node_tag(tag)) == Ok(true)
}
fn iter_eq(got: &Result<IterResult, String>, want: &IterResult) -> Option<String> {
    let q = |a: R, b: R| a.0 * b.1 == b.0 * a.1;
    match got {
        Ok(r) if r.kind == want.kind
            && q(r.root, want.root)
            && q(r.residual, want.residual)
            && r.updates == want.updates
            && r.den_bits == want.den_bits
            && q(r.bracket_lo, want.bracket_lo)
            && q(r.bracket_hi, want.bracket_hi) => None,
        other => Some(format!("{other:?}")),
    }
}

fn main() {
    let mut failures = 0usize;
    let mut check = |name: &str, diff: Option<String>, vm: &str| {
        if let Some(diff) = diff {
            failures += 1;
            println!("DIFF {name} EMITTED {diff}; VM = {vm}");
        }
    };
    let ok = |cond: bool, got: &dyn std::fmt::Debug| (!cond).then(|| format!("{got:?}"));
    let int_v = CodeValue::Int;
    let d = |form: CodeTree| d_form(code(form), z());

    let got = literal_exp(code(int(3)));
    check("literal_extracts", ok(matches!(got, Ok(3)), &got), "3");
    check("cube_expands", value(at(&pow_product(z(), 3), "z", 2), int_v(8)), "8");
    let got = d_elems(vec![], 0, z());
    check("elems_empty", ok(matches!(&got, Ok(v) if v.is_empty()), &got), "length 0");
    let arms = d_arms(vec![vec![code(int(0)), z()], vec![code(int(1)), code(mul(var("z"), var("z")))]], 0, z());
    check("arms_kept_in_order/length", ok(matches!(&arms, Ok(v) if v.len() == 2), &arms), "2");
    let part = |i: usize, j: usize| arms.clone().and_then(|v| v.get(i).and_then(|p| p.get(j)).cloned().ok_or_else(|| "missing".to_string()));
    check("arms_kept_in_order/pattern", value(part(0, 0), int_v(0)), "0");
    check("arms_kept_in_order/value", value(at(&part(1, 1), "z", 5), int_v(10)), "10");

    check("d_literal_zero", value(d(int(3)), int_v(0)), "0");
    check("d_var_one", value(d(var("z")), int_v(1)), "1");
    check("d_other_var_zero", value(d(var("w")), int_v(0)), "0");
    check("d_sum_value", value(at(&d(add(mul(int(3), var("z")), int(5))), "z", 7), int_v(3)), "3");
    let product = || mul(var("z"), add(var("z"), int(1)));
    check("d_product_value", value(at(&d(product()), "z", 3), int_v(7)), "7");
    let got = d(product());
    let shape = got.as_ref().map(|result| {
        let root = view(result);
        let left = sub_view(arg(&root, 0));
        let right = sub_view(arg(&root, 1));
        is_tag(&field(&root, "kind"), "Call")
            && is_tag(&field(&root, "callee"), "Add")
            && is_tag(&field(&left, "callee"), "Mul")
            && is_tag(&field(&sub_view(arg(&left, 0)), "kind"), "Literal")
            && is_tag(&field(&right, "callee"), "Mul")
            && is_tag(&field(&sub_view(arg(&right, 1)), "kind"), "Call")
    });
    check("d_product_structure", ok(matches!(shape, Ok(true)), &got), "Add(Mul(Literal, _), Mul(_, Call))");
    let quotient = bin(TreeBinary::Div, add(mul(var("z"), var("z")), int(1)), add(var("z"), int(3)));
    check("d_quotient_value", value(at(&d(quotient), "z", 2), CodeValue::Rat((3, 5))), "3/5");
    let neg = CodeTree::Unary { op: TreeUnary::Neg, value: Box::new(var("z")) };
    check("d_neg_value", value(at(&d(neg), "z", 5), int_v(-1)), "-1");
    check("d_pow_cube", value(at(&d(pow(var("z"), int(3))), "z", 2), int_v(12)), "12");
    check("d_pow_linear", value(at(&d(pow(var("z"), int(1))), "z", 9), int_v(1)), "1");
    check("d_pow_zero", value(d(pow(var("z"), int(0))), int_v(0)), "0");
    let got = d(pow(var("z"), int(3)));
    let root_ok = got.as_ref().map(|r| {
        let root = view(r);
        is_tag(&field(&root, "kind"), "Call") && is_tag(&field(&root, "callee"), "Add")
    });
    check("d_pow_made_has_no_pow_node", ok(matches!(root_ok, Ok(true)), &got), "root Add call");
    check("d_pow_free_exponent_refuses", refused(&d(pow(var("z"), var("w")))), "transformation_rule_unavailable");
    let branch = || CodeTree::If {
        condition: Box::new(bin(TreeBinary::Lt, var("z"), int(10))),
        then_value: Box::new(mul(var("z"), var("z"))),
        else_value: Box::new(add(var("z"), int(1))),
    };
    let got = d(branch());
    check("d_branch_regions/then", value(at(&got, "z", 3), int_v(6)), "6");
    check("d_branch_regions/otherwise", value(at(&got, "z", 20), int_v(1)), "1");
    let untouched = got.as_ref().map(|r| {
        let root = view(r);
        let condition = field(&root, "condition");
        let bound = is_tag(&field(&sub_view(arg(&sub_view(condition.clone()), 1)), "kind"), "Literal");
        let truth = node_as_code(&condition)
            .map(|c| c.substitute("z", CodeValue::Int(3)))
            .and_then(|c| c.evaluate(&ModuleTable::EMPTY));
        is_tag(&field(&root, "kind"), "Branch") && bound && truth == Ok(CodeValue::Bool(true))
    });
    check("d_branch_condition_untouched", ok(matches!(untouched, Ok(true)), &got), "Branch, guard kept, true at 3");
    let got = d(CodeTree::Tuple(vec![var("z"), int(3), mul(var("z"), var("z"))]));
    let element = |i: i64| {
        got.as_ref()
            .map_err(|e| e.clone())
            .and_then(|r| node_field(view(r), "elements"))
            .and_then(|elements| node_index(elements, i))
            .and_then(node_as_code)
    };
    let kind_ok = got.as_ref().map(|r| is_tag(&field(&view(r), "kind"), "Sequence"));
    check("d_sequence_elementwise/kind", ok(matches!(kind_ok, Ok(true)), &got), "Sequence");
    check("d_sequence_elementwise/0", value(at(&element(0), "z", 5), int_v(1)), "1");
    check("d_sequence_elementwise/1", value(element(1), int_v(0)), "0");
    check("d_sequence_elementwise/2", value(at(&element(2), "z", 5), int_v(10)), "10");
    check("d_projection_refuses_at_the_make_seam", refused(&d(CodeTree::Path(vec!["a".into(), "f".into()]))), "transformation_rule_unavailable");
    let got = d(mul(var("w"), var("z"))).map(|r| r.substitute("w", CodeValue::Int(2)));
    check("d_closure_outer_var", value(at(&got, "z", 3), int_v(2)), "2");
    check("d_bool_call_refuses", refused(&d(bin(TreeBinary::Lt, var("z"), int(3)))), "transformation_rule_unavailable");
    let mystery = CodeTree::Call { function: Box::new(var("mystery")), args: vec![var("z")] };
    check("d_opaque_callee_refuses", refused(&d(mystery)), "transformation_rule_unavailable");

    let got = derivative_of(fn_z(mul(var("z"), var("z"))));
    let body = got.as_ref().map_err(|e| e.clone()).and_then(|r| node_field(view(r), "body")).and_then(node_as_code);
    check("derivative_of_square", value(at(&body, "z", 3), int_v(6)), "6");
    let closure_ok = got.as_ref().map(|r| is_tag(&field(&view(r), "kind"), "Closure"));
    check("derivative_of_is_a_closure", ok(matches!(closure_ok, Ok(true)), &got), "Closure");
    check("derivative_of_non_function_refuses", refused(&derivative_of(code(add(var("z"), int(1))))), "transformation_rule_unavailable");

    let got = linearity_law(0);
    check("additivity_and_homogeneity_hold", ok(matches!(got, Ok(true)), &got), "true");
    let got = square_fn((3, 1));
    check("square_fn_values", ok(matches!(got, Ok((9, 1))), &got), "9/1");
    let got = chain_rule_law(0);
    check("composition_identity_holds", ok(matches!(got, Ok(true)), &got), "true");
    let got = quotient_rule_law(0);
    check("quotient_identity_holds", ok(matches!(got, Ok(true)), &got), "true");

    let want = IterResult { kind: 1, root: (17, 12), residual: (1, 144), updates: 2, den_bits: 4, bracket_lo: (0, 1), bracket_hi: (0, 1) };
    check("sqrt_two_receipt_matches_the_hand_supplied_baseline", iter_eq(&sqrt2_newton_composed(0), &want), "kind 1, 17/12, 1/144, 2 updates");
    // The module-callee newton: f and the run-time df both call square_fn.
    let want = IterResult { kind: 1, root: (91, 72), residual: (7075, 373248), updates: 2, den_bits: 7, bracket_lo: (0, 1), bracket_hi: (0, 1) };
    check("cube_root_two_through_a_module_callee", iter_eq(&cube_root_newton_composed(0), &want), "kind 1, 91/72, 7075/373248, 2 updates");
    // A run-time formula calling an IMPORTED module function (abs_rat,
    // from analysis.equations.iteration): quote(abs_rat(z) + 1) at -2.
    // The VM's capture: abs_rat resolves as a module callable (a
    // dependency), so the open set is {z} only.
    let form = open_expr(
        vec!["z".to_string()],
        add(CodeTree::Call { function: Box::new(var("abs_rat")), args: vec![var("z")] }, int(1)),
        None,
        BTreeMap::new(),
    );
    let got = evaluate_at(form, (-2, 1));
    check("imported_callee_in_a_runtime_formula", ok(matches!(got, Ok(r) if r.0 == 3 * r.1), &got), "3/1");
    // int_bits is imported but unreachable from every function of the
    // module - only the run-time formula calls it: quote(int_bits(201) +
    // z) at 1/2 (bits(201) = 8).
    let form = open_expr(
        vec!["z".to_string()],
        add(CodeTree::Call { function: Box::new(var("int_bits")), args: vec![int(201)] }, var("z")),
        None,
        BTreeMap::new(),
    );
    let got = evaluate_at(form, (1, 2));
    check("unreachable_imported_callee_in_a_runtime_formula", ok(matches!(got, Ok(r) if 2 * r.0 == 17 * r.1), &got), "17/2");
    let got = dyadic_exact_newton_composed(0);
    check(
        "dyadic_root_is_exact_kind_zero",
        ok(matches!(&got, Ok(r) if r.kind == 0 && r.root.0 * 2 == r.root.1 && r.residual.0 == 0), &got),
        "kind 0, root 1/2, residual 0",
    );

    if failures > 0 {
        panic!("{failures} derivative row(s) disagree with the VM");
    }
}
"#;

#[test]
fn emission_conformance_derivative() {
    let diffs = run_lane(
        "language/modules/analysis/derivative.emath",
        DERIVATIVE_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted derivative disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
