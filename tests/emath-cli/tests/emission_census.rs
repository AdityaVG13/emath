//! Emission census gate (emath-nwmm6): every RUNNABLE module in
//! `language/modules` must compile offline as an emitted crate.
//!
//! The dogfooding census, made repeatable: walk every authored module,
//! `emath build --json` each one, and offline-compile every crate that
//! reports `runnable: true`. The gate law is `runnable implies compiles`
//! - a module the emitter claims must actually build with no network and
//! no dependencies. Not-runnable modules are the named refusal lanes
//! (unknown-value carriers, quote.view, non-authored records); their
//! COUNT is pinned so a module silently losing runnable status fails the
//! gate and forces a conscious update, with the drift named.
//!
//! Determinism: modules walk in sorted order and report sorted; scratch
//! dirs are unique per run and never deleted (repo law); the four
//! compile lanes take disjoint sorted chunks. Runtime is the sweep's
//! (~7 minutes on one machine) - this is the DSR census lane, not part
//! of a fast feedback loop.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

mod common;

/// The pinned not-runnable census (2026-09-24, the refusal-lane pass):
/// 7 modules across the named refusal lanes. Update this number ONLY
/// with a bead that names the lane that changed. History: 15 until
/// the operator-tag-witness pass, which unblocked analysis/autodiff,
/// analysis/bvp_newton, and optimization/advanced (the schema-tag
/// family completed with the unary/binary view arms Neg/Not/Pos/
/// And/Or; quote.body over a node callee renders through
/// quote_body_over_node; the definition-table bodies distill their
/// FunctionAbs wrappers as Closure trees; the modules declare their
/// vars carriers as sequence(Code)). Still 12 after the walk-lane
/// crash fix (emath-49o8): quote/walk used to hit `unreachable!` in
/// the node-family fold and the census silently counted the crash as
/// a refusal lane - the family is now closed under nested sequences
/// and non-family record fields refuse by name, so walk answers with
/// its real lanes (quote.open, quote.view, and the Cases.arms tuple
/// kind, all named). `not_runnable_lanes_answer_by_name` pins that
/// no lane may crash the emitter again. 12 -> 11 with the
/// mixed-kind-join pass (emath-49o8): discrete/matching's three
/// refusals (max_match_loop, max_matching, asg_row) were the
/// mixed-kind vector join - a Rat-typed inlined carrier (replace_at's
/// declared Rat signatures instantiated over Int vectors) joining a
/// function's Int-recursive tail left the branch register unkinded
/// and the capture typing refused. The join now folds element-wise
/// under the same scalar law (an Int-element arm widens exactly into
/// the Rat-element vector at the arm boundary) and a ratio value
/// crossing an Int-declared parameter narrows checked (E-INT-002 by
/// name), so matching emits and compiles offline with its 154
/// authored rows still green in the VM. 11 -> 10 with the
/// code-template-carrier seam (emath-49o8): search/structural_spike's
/// four closure-call refusals (cand_score, climb_step, rollout_of,
/// scan_best) were the interchange's missing spelling for the
/// PROGRAM template - bare `Code` kinds as the expression template
/// (evaluate yields the union scalar), so a quoted unary program
/// declared bare refused the moment its evaluated result was called.
/// The authored `Code[T]` spelling now carries through the
/// interchange (`Code<Rat>`: the homogeneous unary program template
/// over Rat - substitutes bind Rat constants, evaluate yields the
/// Rat -> Rat closure) and the module declares its template chain
/// precisely: it emits and compiles offline with its 40 authored rows
/// still green in the VM. search/discovery stays a named lane (its
/// Rat -> Bool quoted functions are outside the homogeneous
/// unary-program lane the runtime's Code<V> carries). 10 -> 8 with
/// the shared-state pass (emath-49o8): analysis/evolution/composition
/// (10 refusals) and mechanics/stiff (3) refused every SimState
/// record literal by name because composition USED one_step's shared
/// evolution object with no import - the VM resolved it structurally
/// (open records), the emitted lane honestly demands the declaration.
/// The authored `use analysis.evolution.one_step` closes it: the
/// record literals emit as EmathRecord_SimState, the curried closure
/// inputs (Rat -> Rat -> Rat) ride the two-level Rc<dyn Fn> ABI, the
/// binary subflow closures chain through flow_compose, and stiff's
/// inlined implicit solves emit with the three laws that refused by
/// record name. Both modules emit and compile offline with their 19
/// and 28 authored rows still green in the VM (the conformance lanes
/// pin every row emitted == VM, refusal atoms exact). 8 -> 7 with
/// the substitute-seam pass (emath-49o8): algebra/linalg refused
/// through quote.substitute's static reference seam - d_form's chain
/// rule reads the callee fragment's binder name at runtime (a
/// minted-scope tag record whose type name IS the binder), which the
/// emitted lane had no spelling for. The reference now rides as a
/// node-family operand (CodeSubRef's dynamic arm through
/// reference_name), a node-carried replacement substitutes
/// structurally through quote.make (substitute_tree - the VM's
/// value_to_expr law), the program template carries its distilled
/// tree so quote.view opens it (the distillation is soft: a template
/// outside the distilled subset, like a float carrier, keeps the
/// factory lane and view refuses by name at runtime), tuple domains
/// kind as positional sequences (_N projects checked), and a concat
/// with any node-family operand stays in the family. Three root
/// engine laws landed with the lane: the residual fold KEEPS
/// quote.evaluate and quote.substitute over a runtime code input
/// (the call survives to emission - before, literal_exp's Int
/// collapsed onto the un-evaluated code and every numeric use
/// misrendered); the branch join is one law in kind and render
/// (branch_carrier: a degenerate `[]` base arm yields to the family
/// tail, so `if done: [] else: family-recursive` renders one carrier
/// instead of `vec!()` against the family concat); and the
/// sibling-kind table carries the ENTRY ABI (a CodeValue body
/// projects onto the declared carrier at the boundary - the callee's
/// emitted return type is what every caller renders against).
/// linalg emits and compiles offline with its 94 authored rows still
/// green in the VM and 28 conformance rows pinning emitted == VM
/// (the grad_quote substitute+evaluate round trip over the emitted
/// ExprCode included). Remaining 7: derivative (its two newtons call
/// a closure yielded by quote.evaluate over a transformed template -
/// the closure-call callee-signature seam; every other row emits
/// through linalg's import), events (the general Call guard),
/// levenberg (unknown value carriers through the numeric-program
/// law), walk (quote.open/quote.view carriers), rewriting (quote.make
/// and the substitute carrier mismatch), discovery (heterogeneous
/// Rat -> Bool quoted functions), sieve (buffer carrier ops are
/// VM-only in this cut).
const NOT_RUNNABLE_PINNED: usize = 7;

/// Fixed compile-lane count (the sweep's `-P 4`).
const LANES: usize = 4;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every authored module under `language/modules`, sorted for
/// deterministic walking and reporting.
fn census_modules() -> Vec<PathBuf> {
    let root = repo_root().join("language/modules");
    let mut modules = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("emath") {
                modules.push(path);
            }
        }
    }
    modules.sort();
    modules
}

/// One census row: the module, whether the emitter claimed it runnable,
/// and the first `error` lines when the emitted crate fails to compile
/// offline.
#[derive(Debug)]
struct Row {
    module: String,
    runnable: bool,
    error: Option<String>,
}

/// Emit one module and, when the emitter claims it runnable, compile
/// the emitted crate offline.
fn census_row(bin: &Path, module: &Path, out: &Path) -> Row {
    let build = Command::new(bin)
        .args([
            "build",
            module.to_str().expect("utf8 module path"),
            "--out",
            out.to_str().expect("utf8 scratch path"),
            "--json",
        ])
        .output()
        .expect("run emath build");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let row = Row {
        module: module.display().to_string(),
        runnable: text.contains("\"runnable\": true"),
        error: None,
    };
    if !row.runnable {
        return row;
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let compiled = Command::new(&cargo)
        .args([
            "build",
            "--offline",
            "--manifest-path",
            out.join("Cargo.toml").to_str().expect("utf8 manifest"),
        ])
        .env("CARGO_TARGET_DIR", out.join("target"))
        .output()
        .expect("run cargo build for emitted crate");
    if compiled.status.success() {
        row
    } else {
        let stderr = String::from_utf8_lossy(&compiled.stderr);
        let first = stderr
            .lines()
            .filter(|line| line.starts_with("error"))
            .take(4)
            .collect::<Vec<_>>()
            .join("\n");
        Row {
            error: Some(first),
            ..row
        }
    }
}

/// A not-runnable lane must answer with a named refusal, never a
/// crash: `quote/walk.emath` once hit `unreachable!` in the emitter's
/// node-family fold (`Vector(Vector(ExprCode))`, the walk's lists of
/// all-made argument lists) and the census silently counted the crash
/// as a refusal lane because a panicked build prints no
/// `"runnable": true`. This lane pins the answer shape: a clean exit
/// whose JSON either claims runnable or names what is unresolved.
#[test]
fn not_runnable_lanes_answer_by_name() {
    let bin = common::emath_bin();
    let module = repo_root().join("language/modules/quote/walk.emath");
    let run_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let out = std::env::temp_dir().join(format!("emath_walk_lane_{run_id}"));
    std::fs::create_dir_all(&out).expect("create scratch dir");
    let build = Command::new(bin)
        .args([
            "build",
            module.to_str().expect("utf8 module path"),
            "--out",
            out.to_str().expect("utf8 scratch path"),
            "--json",
        ])
        .output()
        .expect("run emath build for the walk lane");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        !text.contains("panicked"),
        "a not-runnable lane must not crash the emitter:\n{text}"
    );
    assert!(
        build.status.success() && text.contains("\"runnable\""),
        "the walk lane must answer: runnable true/false with named unresolved entries\n{text}"
    );
}

#[test]
fn emission_census_every_runnable_module_compiles_offline() {
    let bin = common::emath_bin();
    let modules = census_modules();
    assert!(
        !modules.is_empty(),
        "census found no modules under language/modules"
    );

    let run_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let scratch = |module: &Path| -> PathBuf {
        let stem = module.file_stem().and_then(|s| s.to_str()).expect("stem");
        let parent = module
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .expect("parent");
        std::env::temp_dir().join(format!(
            "emath_census_{run_id}_{}__{parent}",
            stem.replace(['-', '.'], "_")
        ))
    };

    // Disjoint sorted chunks: lane i takes modules i, i+LANES, ... -
    // each lane's cargo runs in its own per-module target dir, so the
    // lanes never contend on a build lock.
    let (tx, rx) = mpsc::channel::<Row>();
    std::thread::scope(|scope| {
        for lane in 0..LANES {
            let tx = tx.clone();
            let chunk: Vec<PathBuf> = modules
                .iter()
                .enumerate()
                .filter(|(index, _)| index % LANES == lane)
                .map(|(_, module)| module.clone())
                .collect();
            let bin = bin.to_path_buf();
            scope.spawn(move || {
                for module in chunk {
                    let out = scratch(&module);
                    let row = census_row(&bin, &module, &out);
                    let _ = tx.send(row);
                }
            });
        }
    });
    drop(tx);

    let mut rows: Vec<Row> = rx.iter().collect();
    rows.sort_by(|a, b| a.module.cmp(&b.module));

    let failures: Vec<&Row> = rows
        .iter()
        .filter(|row| row.runnable && row.error.is_some())
        .collect();
    assert!(
        failures.is_empty(),
        "runnable modules must compile offline ({} of {} failed):\n{}",
        failures.len(),
        rows.len(),
        failures
            .iter()
            .map(|row| {
                format!(
                    "{}\n{}",
                    row.module,
                    row.error.as_deref().unwrap_or("<no error lines>")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    let not_runnable: Vec<&Row> = rows.iter().filter(|row| !row.runnable).collect();
    assert_eq!(
        not_runnable.len(),
        NOT_RUNNABLE_PINNED,
        "the not-runnable census drifted (pinned {NOT_RUNNABLE_PINNED}); name the lane that changed:\n{}",
        not_runnable
            .iter()
            .map(|row| row.module.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
