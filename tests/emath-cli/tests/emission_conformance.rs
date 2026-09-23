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
//! `language/modules/probability/distributions.emath`, and
//! `language/modules/analysis/powers.emath` — the same givens, the
//! same expectations, executed against emitted code instead of the VM.
//! Refusal rows (`expect diagnostic.code == <atom>`) pin the emitted
//! `Err(<atom>)` strings exactly. Labeled transcription boundaries live
//! in the driver headers (cross-module bracket rows, i64 ABI limits).

use std::path::PathBuf;
use std::process::Command;

use common::cli;

mod common;

/// Workspace root (tests/emath-cli/tests -> tests/emath-cli -> repo).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Scratch dir for one lane run. Unique per process so repeated runs never
/// collide and nothing is ever deleted (no cleanup per repo law).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "emath_conf_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Full conformance lane for one module. Returns the named diff text for
/// every failing row (empty when emitted == VM on all pins).
fn run_lane(module_rel: &str, driver_src: &str) -> Vec<String> {
    let name = std::path::Path::new(module_rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("module file stem")
        .to_string();
    let module = repo_root().join(module_rel);

    // 1. VM truth: the authored rows pass in the VM.
    let (text, code) = cli(&["test", module.to_str().expect("utf8 module path")]);
    assert_eq!(
        code, 0,
        "authored module must pass its own tests in the VM first\n{text}"
    );

    // 2. Emit: runnable, nothing unresolved.
    let out = scratch(&name);
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
    std::fs::write(bin_dir.join("conf_driver.rs"), driver_src).expect("write driver");

    // 4. Compile the emitted crate offline (no deps by construction).
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args([
            "build",
            "--offline",
            "--manifest-path",
            out.join("Cargo.toml").to_str().expect("utf8 manifest"),
            "--bin",
            "conf_driver",
        ])
        .env("CARGO_TARGET_DIR", out.join("target"))
        .output()
        .expect("run cargo build for emitted crate");
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
    let diffs = run_lane("language/modules/optimization/allocate.emath", ALLOCATE_DRIVER);
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
    if !matches!(&got, Ok(v) if *v == ei(3)) {
        failures += 1;
        println!("DIFF sqrt_nine_unclamped EMITTED {:?}; VM = 3", got);
    }

    // example <clamped_low>
    let got = floor_root_between(9, 2, 4, 100);
    if !matches!(&got, Ok(v) if *v == ei(4)) {
        failures += 1;
        println!("DIFF clamped_low EMITTED {:?}; VM = 4", got);
    }

    // example <sqrt_nine>
    let got = floor_root(9, 2);
    if !matches!(&got, Ok(v) if *v == ei(3)) {
        failures += 1;
        println!("DIFF sqrt_nine EMITTED {:?}; VM = 3", got);
    }

    // example <zeroth_root>: q <= 0 is not a root
    let got = floor_root(9, 0);
    if !matches!(&got, Ok(v) if *v == ei(0)) {
        failures += 1;
        println!("DIFF zeroth_root EMITTED {:?}; VM = 0", got);
    }

    // example <cbrt_eight>
    let got = floor_root(8, 3);
    if !matches!(&got, Ok(v) if *v == ei(2)) {
        failures += 1;
        println!("DIFF cbrt_eight EMITTED {:?}; VM = 2", got);
    }

    // example <sqrt_eight>
    let got = floor_root(8, 2);
    if !matches!(&got, Ok(v) if *v == ei(2)) {
        failures += 1;
        println!("DIFF sqrt_eight EMITTED {:?}; VM = 2", got);
    }

    // example <zero>
    let got = floor_root(0, 2);
    if !matches!(&got, Ok(v) if *v == ei(0)) {
        failures += 1;
        println!("DIFF zero EMITTED {:?}; VM = 0", got);
    }

    // example <first_root>
    let got = floor_root(11, 1);
    if !matches!(&got, Ok(v) if *v == ei(11)) {
        failures += 1;
        println!("DIFF first_root EMITTED {:?}; VM = 11", got);
    }

    // example <next_after_sqrt_nine>
    let got = upper_root_at(9, 2, 0);
    if !matches!(&got, Ok(v) if *v == ei(4)) {
        failures += 1;
        println!("DIFF next_after_sqrt_nine EMITTED {:?}; VM = 4", got);
    }

    // example <already_past>
    let got = upper_root_at(9, 2, 5);
    if !matches!(&got, Ok(v) if *v == ei(5)) {
        failures += 1;
        println!("DIFF already_past EMITTED {:?}; VM = 5", got);
    }

    // example <ceil_sqrt_ten>
    let got = ceil_root(10, 2);
    if !matches!(&got, Ok(v) if *v == ei(4)) {
        failures += 1;
        println!("DIFF ceil_sqrt_ten EMITTED {:?}; VM = 4", got);
    }

    // example <exact_cbrt>
    let got = ceil_root(8, 3);
    if !matches!(&got, Ok(v) if *v == ei(2)) {
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
    if !matches!(&got, Ok(v) if *v == ei(8)) {
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
