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
//! `language/modules/cryptology/modular.emath`,
//! `language/modules/exact/quadratic.emath`,
//! `language/modules/numerics/bounds.emath`,
//! `language/modules/geometry/surfaces.emath`,
//! `language/modules/mechanics/oscillations.emath`,
//! `language/modules/discrete/order.emath` (the mutual-recursion
//! cycle pins), `language/modules/discrete/matching.emath` (the
//! mixed-kind join pins), `language/modules/search/
//! structural_spike.emath` (the code-template-carrier pins),
//! `language/modules/analysis/evolution/composition.emath` (the
//! shared-state record + curried-closure pins), and
//! `language/modules/mechanics/stiff.emath` (the implicit-solve +
//! adaptive-walk pins), and `language/modules/algebra/linalg.emath`
//! (the substitute-seam + sparse/dense/hessian pins) — the same givens,
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

const MODULAR_DRIVER: &str = r#"
//! Authored pins of cryptology/modular.emath against the emitted crate.
//!
//! Transcription boundaries (labeled, from the module header):
//! - The numeric lane (mod_pos .. affine_decrypt) emits
//!   `Result<ExactInt, String>`; pins compare via `ExactInt::from`.
//! - Bezout rows pin the IDENTITY `a*x + b*y == g`, not just g.
//! - The affine roundtrip rows are authored as composed calls
//!   `decrypt(encrypt(p))`; the driver composes through the i64 ABI
//!   seam with a `to_i64` crossing (E-INT-002 named if it ever fires).
//! - Vigenere sequence results carry ExactRatio `(i128, i128)`
//!   elements; integer expectations compare by cross-multiplication.
//! - The Carmichael rows are the module's honesty pins: 561 passes
//!   Fermat for every coprime base (a probable-prime LIE, pinned).

use modular::{
    affine_decrypt, affine_encrypt, bezout, bezout_lift, carmichael_561_pseudoprime,
    fermat_all_pass, fermat_witness, is_fermat_prime, mod_add, mod_inv, mod_mul, mod_pos,
    mod_pow, shares_factor, vigenere_decrypt, vigenere_encrypt, vigenere_key_at,
    EmathRecord_Bezout,
};

fn ei(n: i64) -> modular::emath_rt::ExactInt {
    modular::emath_rt::ExactInt::from(n)
}

fn exact_ok(got: &Result<modular::emath_rt::ExactInt, String>, want: i64) -> bool {
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
) -> Result<modular::emath_rt::ExactInt, String> {
    affine_encrypt(p, a, b, m).and_then(|v| match v.to_i64() {
        Some(c) => affine_decrypt(c, a, b, m),
        None => Err(String::from("E-INT-002")),
    })
}

/// mod_mul(a, inv, m) == 1 — the authored composed inverse check.
fn inverse_roundtrip(a: i64, m: i64) -> Result<modular::emath_rt::ExactInt, String> {
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
    // Emitted sequence elements are ExactRatio; compare by value.
    let got = vigenere_encrypt(vec![7, 4, 11, 11, 14], vec![3, 1, 4], 26);
    match &got {
        Ok(v) if v.len() == 5
            && q_eq(v[0], (10, 1))
            && q_eq(v[1], (5, 1))
            && q_eq(v[2], (15, 1))
            && q_eq(v[3], (14, 1))
            && q_eq(v[4], (15, 1)) => {}
        _ => {
            failures += 1;
            println!("DIFF classic_shifts EMITTED {:?}; VM = [10, 5, 15, 14, 15]", got);
        }
    }

    // example <roundtrip_hello>: decrypt(encrypt) recovers HELLO.
    let got = vigenere_decrypt(vec![10, 5, 15, 14, 15], vec![3, 1, 4], 26);
    match &got {
        Ok(v) if v.len() == 5
            && q_eq(v[0], (7, 1))
            && q_eq(v[1], (4, 1))
            && q_eq(v[2], (11, 1))
            && q_eq(v[3], (11, 1))
            && q_eq(v[4], (14, 1)) => {}
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

    // example <passes_fermat_falsely>: THE honesty pin — 561 = 3*11*17
    // passes Fermat for every coprime base: probable-prime, NOT prime.
    let got = carmichael_561_pseudoprime(0);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF passes_fermat_falsely EMITTED {:?}; VM = true (the Carmichael lie)", got);
    }

    // example <but_shares_factor_certifies_composite>: the SAME number
    // is certified composite by the factor proof.
    let got = shares_factor(3, 561);
    if got != Ok(true) {
        failures += 1;
        println!("DIFF but_shares_factor_certifies_composite EMITTED {:?}; VM = true", got);
    }

    // example <each_coprime_base_alone_lies>: pinned per-base so the
    // lie cannot hide behind the set.
    for base in [2, 7, 13] {
        let got = fermat_witness(base, 561);
        if got != Ok(false) {
            failures += 1;
            println!("DIFF each_coprime_base_alone_lies EMITTED {:?}; VM = false (base {base})", got);
        }
    }

    if failures > 0 {
        println!("SUMMARY {failures} conformance diffs");
        std::process::exit(1);
    }
    println!("SUMMARY all modular pins conform");
}
"#;

#[test]
fn emission_conformance_modular() {
    let diffs = run_lane(
        "language/modules/cryptology/modular.emath",
        MODULAR_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted modular disagrees with the VM on:\n{}",
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
    let diffs = run_lane(
        "language/modules/exact/quadratic.emath",
        QUADRATIC_DRIVER,
    );
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
    let diffs = run_lane(
        "language/modules/numerics/bounds.emath",
        BOUNDS_DRIVER,
    );
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
    let diffs = run_lane(
        "language/modules/geometry/surfaces.emath",
        SURFACES_DRIVER,
    );
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
        "language/modules/mechanics/oscillations.emath",
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

use order::{EmathRecord_SortedPair, sort_loop, sort_step, sort_with_permutation, unsort_at};

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
        let back = unsort_at(sorted.clone(), 0);
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
    let diffs = run_lane(
        "language/modules/discrete/matching.emath",
        MATCHING_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted matching disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}

const SPIKE_DRIVER: &str = r#"
//! Authored pins of search/structural_spike.emath against the emitted
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
        "language/modules/search/structural_spike.emath",
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
//! Authored pins of mechanics/stiff.emath against the emitted crate
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
    stiff_abs, stiff_adaptive_walk, stiff_astable_ok, stiff_astable_walk, stiff_be_factor,
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

    // example <negative_flips_sign> (stiff_abs)
    let got = stiff_abs(q(0 - 3, 4));
    if let Some(diff) = dq(&got, q(3, 4)) {
        failures += 1;
        println!("DIFF negative_flips_sign EMITTED {diff}; VM = 3/4");
    }
    // example <positive_is_itself>
    let got = stiff_abs(q(5, 2));
    if let Some(diff) = dq(&got, q(5, 2)) {
        failures += 1;
        println!("DIFF positive_is_itself EMITTED {diff}; VM = 5/2");
    }
    // example <zero_arm_hits_zero> (the comparison boundary arm)
    let got = stiff_abs(q(0, 1));
    if let Some(diff) = dq(&got, q(0, 1)) {
        failures += 1;
        println!("DIFF zero_arm_hits_zero EMITTED {diff}; VM = 0/1");
    }

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
        "language/modules/mechanics/stiff.emath",
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

use linalg::{
    dense_axpy, dense_dot, dense_matvec, grad_quote, hess_col, hess_symmetric, hess_vec,
    hessian_rows, oscillator_modes, place_row, seq_eq, sp_dense_law, sp_matvec, sp_ok,
    sp_row_at, sp_to_dense, square_walk, zeros_row, EmathRecord_SpEnt as Ent,
    EmathRecord_SpMat as Sp,
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

    // example <zeros_three> (zeros_row: the descending prepend)
    let got = zeros_row(3, 2);
    if let Some(diff) = lq(&got, &[q(0, 1), q(0, 1), q(0, 1)]) {
        failures += 1;
        println!("DIFF zeros_three EMITTED {diff}; VM = [0, 0, 0]");
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

    // example <dot_basic> (dense_dot)
    let got = dense_dot(vec![q(1, 1), q(2, 1)], vec![q(3, 1), q(4, 1)], 0, q(0, 1));
    if let Some(diff) = rq(&got, q(11, 1)) {
        failures += 1;
        println!("DIFF dot_basic EMITTED {diff}; VM = 11");
    }

    // example <dense_matvec_at_matches_the_wrapper_fixture>
    let got = linalg::dense_matvec_at(
        vec![vec![q(2, 1), q(1, 1)], vec![q(1, 1), q(2, 1)]],
        vec![q(1, 1), q(1, 1)],
        0,
    );
    if let Some(diff) = lq(&got, &[q(3, 1), q(3, 1)]) {
        failures += 1;
        println!("DIFF dense_matvec_at_matches_the_wrapper_fixture EMITTED {diff}; VM = [3, 3]");
    }

    // example <dense_times_vector> (dense_matvec)
    let got = dense_matvec(
        vec![vec![q(2, 1), q(1, 1)], vec![q(1, 1), q(2, 1)]],
        vec![q(1, 1), q(1, 1)],
    );
    if let Some(diff) = lq(&got, &[q(3, 1), q(3, 1)]) {
        failures += 1;
        println!("DIFF dense_times_vector EMITTED {diff}; VM = [3, 3]");
    }

    // example <seq_eq_reflexive>
    let xs = vec![q(3, 1), q(5, 1)];
    let got = seq_eq(xs.clone(), xs.clone(), 0);
    if let Some(diff) = bq(&got, true) {
        failures += 1;
        println!("DIFF seq_eq_reflexive EMITTED {diff}; VM = true");
    }
    // example <seq_eq_one_entry_apart>
    let got = seq_eq(xs.clone(), vec![q(3, 1), q(0 - 5, 1)], 0);
    if let Some(diff) = bq(&got, false) {
        failures += 1;
        println!("DIFF seq_eq_one_entry_apart EMITTED {diff}; VM = false");
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

    // example <axpy_basic> (dense_axpy)
    let got = dense_axpy(
        vec![q(1, 1), q(2, 1)],
        q(3, 1),
        vec![q(10, 1), q(20, 1)],
        0,
    );
    if let Some(diff) = lq(&got, &[q(13, 1), q(26, 1)]) {
        failures += 1;
        println!("DIFF axpy_basic EMITTED {diff}; VM = [13, 26]");
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
    let diffs = run_lane(
        "language/modules/algebra/linalg.emath",
        LINALG_DRIVER,
    );
    assert!(
        diffs.is_empty(),
        "emitted linalg disagrees with the VM on:\n{}",
        diffs.join("\n")
    );
}
