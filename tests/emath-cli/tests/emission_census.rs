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
//! Determinism: modules walk in sorted order and report sorted; the
//! compile lanes pull from one shared sorted queue. Each module emits
//! into its stable dir (`common::emitted_crate_dir`, shared with the
//! conformance lanes, never deleted): an unchanged emission re-runs as a
//! cargo no-op and a changed one recompiles incrementally, so only a
//! cold cache pays the full sweep.
//!
//! Knobs: `EMATH_EMIT_LANES=<n>` sets the compile-lane count (default
//! 4); `EMATH_CENSUS_ONLY=<substring>` restricts the sweep to matching
//! module paths (the pin is skipped - a partial census cannot count).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

mod common;

/// The pinned not-runnable census (2026-09-24, the refusal-lane pass):
/// 6 modules across the named refusal lanes. Update this number ONLY
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
/// ExprCode included). 7 -> 6 with the event-data pass (emath-49o8):
/// analysis/evolution/events refused on ONE seam - `events[i].guard(
/// sa.t, sa)` is a record field's CLOSURE called where it is
/// projected, and the application parses with the field-access call
/// as its function expression, a shape the blanket `call is not yet
/// emitted` fence refused. The callee now lowers as a VALUE and the
/// arguments cross through the closure-call lane (the VM's law: the
/// callee is a value, curried stages apply through the signature; a
/// callee without a callable carrier still refuses by name at the
/// render boundary). The Event record also carries closure fields
/// (`Rc<dyn Fn>` handles) which derive neither Debug nor PartialEq -
/// records with closure-carrier fields now hand-impl both, closures
/// by shared-handle identity (the buffer law) and printing as
/// `<closure>`. events emits and compiles offline with its 26
/// authored rows still green in the VM and 26 conformance rows
/// pinning emitted == VM (the sign-change window with its labeled
/// tangent/double-root misses, the priority pick, the exact
/// bisection with dyadic-exact and nondyadic-banded rows, and the
/// event-aware rollout's terminal/reset/dedup/chatter outcomes).
/// 6 -> 5 with the rewriting pass (emath-49o8): search/rewriting
/// refused on `quote.make(quote.evaluate(source))` - make over an
/// evaluated SCALAR, where the make lane took only node records and
/// codes - and on `quote.substitute(quote(function x in Rat: x + 1),
/// x, 7)`, an Int constant crossing into a Rat-carried template.
/// Make now rebuilds a scalar (union, Int, Rat, Bool) as the closed
/// literal of its exact value (rationals stay rational - the 5/6
/// fence), and the substitute value crosses the template carrier
/// through the shared numeric boundary (exact Int-to-Rat widening,
/// checked narrowing). rewriting emits and compiles offline with its
/// 6 authored rows still green in the VM and 6 conformance pins
/// (the AvoidCapture query is not an emitted entry). 5 -> 4 with the
/// curried-model pass (emath-49o8): numerics/levenberg's model seam
/// `model_at = predict(params)` binds a PARTIAL application of a
/// declared `A -> B -> C` input, but the def was not marked
/// closure-valued, so `model_at(t)` lowered as the projection
/// `t.model_at` (unknown carriers downstream; the runnable
/// `predict_at` emitted `&t.model_at` over an f64). Defs now carry
/// the curried stage count of declared arrows and function literals
/// - an application leaving stages unapplied binds a closure - and a
/// projection off a scalar or callable carrier refuses by name. The
/// planted-problem functions' literal `function p in
/// sequence[Float64]: function t in Float64: ..` fell to the untyped
/// numeric-program lane because the bracket domain `sequence[T]` had
/// no signature; it now types as `Vector<T>`. levenberg emits and
/// compiles offline with its 61 authored rows still green in the VM
/// and 61 conformance pins. 4 -> 3 with the dynamic-family pass
/// (emath-49o8): quote/walk declared `packed: Fragment` and untyped
/// `items: sequence`, neither of which had an interchange signature,
/// so the entries fell back to Int and quote.open/quote.view refused.
/// Both are the dynamic node family (`Fragment` package records, the
/// heterogeneous `sequence`); emitted tuples are positional family
/// sequences whose `_N` indexes them. Fixing that exposed a lying
/// `runnable: true`: the callee inside the `quote.open term in packed:
/// walk_term(term)` binder body never joined the sibling map (the
/// called-name walk skipped binder bodies), so the call lowered as
/// the projection `term.walk_term` - the walk now descends binder
/// bodies and a projection off a code carrier refuses by name. walk
/// emits and compiles offline with its 7 authored rows still green
/// in the VM and 8 conformance pins. 3 -> 2 with the applied-evaluate
/// pass (emath-49o8): search/discovery evaluates a bare `Code`
/// conjecture (`claim = quote.evaluate(conjecture.claim)`) and APPLIES
/// it (`claim(domain[at])`) - a quoted function literal Rat -> Bool,
/// where the expression-template lane knew evaluate only as a scalar.
/// An evaluated def (or in-place callee) applied with one consistent
/// arity now lowers as the applied evaluate: the rt closes the code
/// once (stale/unbound refusals at the same point) and returns a
/// callable over the value union that binds the literal's parameters
/// and runs the body through the scalar tree evaluator. A union arm
/// beside a typed arm joins onto the typed carrier (the render's
/// existing checked projection, now agreed by the kind walk), and code
/// values compare by tree + dependencies so records with Code fields
/// derive PartialEq. discovery emits and compiles offline with its 4
/// authored rows still green in the VM and 4 conformance pins.
/// 2 -> 1 with the declared-arrow evaluate pass (emath-49o8, the user
/// accepted the design 2026-09-25): analysis/derivative's newtons pass
/// `df = quote.evaluate(derivative_of(f_term))` - a quoted function
/// built at run time - into `newton_solve`'s declared `df: Rat -> Rat`.
/// An evaluated def passed into a declared arrow parameter now lowers
/// as `CodeApplication::Declared`: the artifact adapts the rt's closed
/// callable to the declared scalar arrow (arguments wrap into the
/// union, the result projects checked). The derived body runs through
/// the rt scalar tree evaluator (run-time code cannot be compiled
/// ahead of time; a formula calling a module function refuses by name
/// there). Compiling the module for the first time also exposed the
/// d_form <-> d_arms cycle edge passing a viewed node sequence into
/// the declared `sequence((Code, Code))` parameter: the call-marshal
/// bridge now maps family sequences into declared sequences of code.
/// derivative emits and compiles offline with its 35 authored rows
/// still green in the VM and 33 conformance rows pinning emitted == VM
/// (two rows' givens - `xs[i]`, a nested quote - are outside the
/// distilled tree subset and stay VM-only, labeled). Remaining 1:
/// sieve (buffer carrier ops are VM-only in this cut - an intentional
/// fence, `machine_buffer_basename`).
const NOT_RUNNABLE_PINNED: usize = 1;

/// Default compile-lane count (the sweep's `-P 4`); `EMATH_EMIT_LANES`
/// overrides it.
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
/// the emitted crate's lib offline (`--lib`: a conformance driver left
/// in the shared dir is the conformance lane's business, not the
/// census's).
fn census_row(bin: &Path, module: &Path) -> Row {
    let out = common::emitted_crate_dir(module);
    let _lock = common::lock_emitted_crate(&out);
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
    let compiled = common::cargo_build_emitted(&out, &["--lib"]);
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
    let out = common::emitted_crate_dir(&module);
    let _lock = common::lock_emitted_crate(&out);
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
    let only = std::env::var("EMATH_CENSUS_ONLY")
        .ok()
        .filter(|only| !only.is_empty());
    let modules: Vec<PathBuf> = census_modules()
        .into_iter()
        .filter(|module| {
            only.as_deref()
                .is_none_or(|only| module.to_string_lossy().contains(only))
        })
        .collect();
    assert!(
        !modules.is_empty(),
        "census found no modules under language/modules (EMATH_CENSUS_ONLY={only:?})"
    );
    let lanes = std::env::var("EMATH_EMIT_LANES")
        .ok()
        .and_then(|lanes| lanes.parse::<usize>().ok())
        .filter(|lanes| *lanes > 0)
        .unwrap_or(LANES);

    // One shared sorted queue: each lane pulls the next module when it
    // finishes one, so a heavy module never strands a lane's whole
    // chunk behind it. Every module owns its target dir, so the lanes
    // never contend on a build lock.
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<Row>();
    std::thread::scope(|scope| {
        for _ in 0..lanes {
            let tx = tx.clone();
            let (next, modules) = (&next, &modules);
            scope.spawn(move || {
                while let Some(module) = modules.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let _ = tx.send(census_row(bin, module));
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
    if only.is_some() {
        return;
    }
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
