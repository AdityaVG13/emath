//! Loop host core acceptance (bead emath-kz3ll).
//!
//! A headless host opens a module's authored session surface (the
//! Step/Seed function pair), drives one batch per call, projects the
//! batch ledger, and checkpoints through emath.scratch.v1. The laws:
//!   - the valley session reproduces the authored E5a outcome batch by
//!     batch (plateau, the key-3 promotion, the key-7 goal);
//!   - the targets sessions reproduce that fixture's authored fitting
//!     and reachability outcomes exactly;
//!   - a module without a session surface gets the named lift
//!     diagnostic (what to author, with the example pointer), never a
//!     silent refusal or a guess;
//!   - near-miss surfaces (a Step with no Seed, a Step with the wrong
//!     signature) are reported as problems, not ignored;
//!   - a session saved and reloaded through the scratch contract
//!     resumes exactly where the straight run is (identity and ledger
//!     laws enforced by the host).

use std::path::PathBuf;

use emath_exec_ir::constructor_layer::CValue;
use emath_syntax::parse_str;
use emath_test_harness::Probe;
use emath_tui::host::{
    value_bool, value_int, value_rat, value_sequence, verdict_name, LoopHost, LoopSession,
    scan_session_surfaces, SurfaceScan,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn module_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/modules")
        .join(name)
}

fn temp_emath(label: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("emath_host_{label}_{}.emath", std::process::id()));
    std::fs::write(&path, source).expect("write temp module");
    path
}

fn temp_scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("emath_host_{label}_{}.json", std::process::id()))
}

/// One-field text surgery on the deterministic scratch encoding.
fn replace_field(text: &str, field: &str, from: &str, to: &str) -> String {
    let needle = format!("\"{field}\": {from}");
    let replacement = format!("\"{field}\": {to}");
    assert!(
        text.contains(&needle),
        "the encoding does not render `{field}: {from}` as the test expected"
    );
    text.replacen(&needle, &replacement, 1)
}

fn archive_keys(state: &CValue) -> Vec<i128> {
    value_sequence(state, "archive")
        .expect("archive")
        .iter()
        .map(|record| value_int(record, "key").expect("key"))
        .collect()
}

fn accepted_keys(state: &CValue) -> Vec<i128> {
    value_sequence(state, "archive")
        .expect("archive")
        .iter()
        .filter(|record| value_bool(record, "accepted").expect("accepted"))
        .map(|record| value_int(record, "key").expect("key"))
        .collect()
}

#[test]
fn host_sessions() {
    let mut probe = Probe::new(
        "loop host core: authored session surfaces driven batch by batch with scratch resume",
    );

    probe.case("host-drives-valley-to-goal", |p| {
        let host =
            LoopHost::open(&fixture_path("research_step_valley.emath"), Some("StepValley"))
                .expect("open valley");
        p.demand("surface-step", host.surface().step == "StepValley", format!("{:?}", host.surface()));
        p.demand("surface-seed", host.surface().seed == "SeedValley", format!("{:?}", host.surface()));
        p.demand(
            "surface-state-type",
            host.surface().state_type == "LoopState",
            format!("{:?}", host.surface()),
        );
        p.demand(
            "identity-meaning",
            !host.identity().meaning_id.is_empty(),
            format!("{:?}", host.identity()),
        );
        p.demand(
            "identity-target",
            host.identity().target == "StepValley",
            format!("{:?}", host.identity()),
        );

        let mut session = host.begin().expect("begin valley");
        p.eq("seed-batch", session.state_int("batch").expect("batch"), 0);
        p.eq("seed-incumbent", session.state_int("incumbent").expect("incumbent"), 0);
        p.eq("seed-mode", session.state_int("mode").expect("mode"), 1);
        p.eq("seed-revision", session.revision(), 0);

        let e1 = session.step(&host, 60).expect("step 1");
        p.demand(
            "batch1-plateau",
            e1.batch == 1 && e1.verdict == 3 && e1.promoted.is_empty() && e1.quarantined.is_empty(),
            format!("{e1:?}"),
        );
        let e2 = session.step(&host, 60).expect("step 2");
        p.demand(
            "batch2-retains-stepping-stones",
            e2.batch == 2
                && e2.verdict == 3
                && e2.promoted.is_empty()
                && e2.incumbent_key == 0
                && e2.archive_len == 7,
            format!("{e2:?}"),
        );
        let e3 = session.step(&host, 60).expect("step 3");
        p.demand(
            "batch3-goal",
            e3.batch == 3
                && e3.verdict == 1
                && e3.promoted == vec![7]
                && e3.incumbent_key == 7
                && verdict_name(e3.verdict) == "goal_attained",
            format!("{e3:?}"),
        );
        p.eq("revision-after-three", session.revision(), 3);

        let incumbent = session.incumbent_record().expect("incumbent record");
        p.eq("winner-key", value_int(&incumbent, "key").expect("key"), 7);
        p.eq("winner-score", value_rat(&incumbent, "score").expect("score"), (6, 1));
        let keys = archive_keys(session.state());
        p.demand(
            "stepping-stones-retained",
            keys.contains(&1) && keys.contains(&3),
            format!("{keys:?}"),
        );
        p.eq(
            "baseline-plus-winner-accepted",
            accepted_keys(session.state()),
            vec![0, 7],
        );
    });

    probe.case("host-reproduces-targets-authored-outcomes", |p| {
        // (a) The fitting target: the incumbent-only hillclimb
        // 0 -> 1/4 -> 1/2, one promoted record per batch, ending at
        // exact SSE zero with the parent-linked chain.
        let fitting = LoopHost::open(
            &fixture_path("research_step_targets.emath"),
            Some("StepFitting"),
        )
        .expect("open fitting");
        p.demand(
            "fitting-target-identity",
            fitting.identity().target == "StepFitting",
            format!("{:?}", fitting.identity()),
        );
        let mut fit = fitting.begin().expect("begin fitting");
        p.eq("fitting-seed-mode", fit.state_int("mode").expect("mode"), 0);
        let f1 = fit.step(&fitting, 60).expect("fit step 1");
        p.demand(
            "fitting-batch1-promotes-quarter",
            f1.batch == 1 && f1.verdict == 0 && f1.promoted == vec![1] && f1.incumbent_key == 1,
            format!("{f1:?}"),
        );
        let f2 = fit.step(&fitting, 60).expect("fit step 2");
        p.demand(
            "fitting-batch2-promotes-half-goal",
            f2.batch == 2
                && f2.verdict == 1
                && f2.promoted == vec![2]
                && f2.incumbent_key == 2
                && verdict_name(f2.verdict) == "goal_attained",
            format!("{f2:?}"),
        );
        let incumbent = fit.incumbent_record().expect("fitting incumbent");
        p.eq("fitting-winner-value", value_rat(&incumbent, "value").expect("value"), (1, 2));
        let archive = value_sequence(fit.state(), "archive").expect("fitting archive");
        p.eq("fitting-chain-len", archive.len() as i128, 3);
        p.eq(
            "fitting-lineage",
            (
                value_int(&archive[1], "parent").expect("parent"),
                value_int(&archive[2], "parent").expect("parent"),
            ),
            (0, 1),
        );

        // (b) The reachability target: state 0 verified as the
        // witness-not-forced counterexample in one batch.
        let reach =
            LoopHost::open(&fixture_path("research_step_targets.emath"), Some("StepReach"))
                .expect("open reach");
        p.demand(
            "targets-share-module-meaning",
            reach.identity().meaning_id == fitting.identity().meaning_id
                && reach.identity().target != fitting.identity().target,
            format!("{:?} vs {:?}", reach.identity(), fitting.identity()),
        );
        let mut r = reach.begin().expect("begin reach");
        p.eq("reach-seed-mode", r.state_int("mode").expect("mode"), 1);
        let r1 = r.step(&reach, 15).expect("reach step 1");
        p.demand(
            "reach-batch1-counterexample",
            r1.batch == 1 && r1.verdict == 1 && r1.promoted == vec![0] && r1.incumbent_key == 0,
            format!("{r1:?}"),
        );

        // Cross-module identity differs.
        let valley =
            LoopHost::open(&fixture_path("research_step_valley.emath"), Some("StepValley"))
                .expect("reopen valley");
        p.demand(
            "valley-and-targets-differ",
            valley.identity().meaning_id != fitting.identity().meaning_id,
            format!(
                "{:?} vs {:?}",
                valley.identity(),
                fitting.identity()
            ),
        );
    });

    probe.case("host-missing-surface-diagnostic", |p| {
        // A loop module with no lifted surface gets the named
        // diagnostic: exactly what to author, with the example pointer.
        match LoopHost::open(&module_fixture_path("research_loop_valley.emath"), None) {
            Err(fault) => {
                p.demand("no-surface-code", fault.code == "loop_surface", fault.to_string());
                p.demand(
                    "no-surface-shows-the-lift",
                    fault.message.contains("StepValley")
                        && fault.message.contains("research_step_valley.emath"),
                    fault.to_string(),
                );
            }
            Ok(_) => {
                p.fail("no-surface-code", "opened a module with no session surface");
            }
        };

        // Naming a non-surface function is a target fault, not a guess.
        match LoopHost::open(
            &module_fixture_path("research_loop_valley.emath"),
            Some("ValleyLandscape"),
        ) {
            Err(fault) => {
                p.demand("wrong-target-code", fault.code == "loop_target", fault.to_string());
                p.demand(
                    "wrong-target-shows-the-lift",
                    fault.message.contains("research_step_valley.emath"),
                    fault.to_string(),
                );
            }
            Ok(_) => {
                p.fail("wrong-target-code", "opened a non-surface target");
            }
        };

        // Near-miss surfaces are problems, never silently ignored: a
        // Step with no Seed, and a Step-named function with the wrong
        // signature.
        let lonely = "use search.research\n\nemath function StepLonely:\n    inputs:\n        state: LoopState\n        budget: Int\n    outputs:\n        result: LoopState\n    definitions:\n        result = state\n";
        let (tree, diagnostics) = parse_str(lonely);
        p.demand("lonely-parses", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let scan: SurfaceScan = scan_session_surfaces(&tree);
        p.demand(
            "lonely-has-no-surfaces",
            scan.surfaces.is_empty(),
            format!("{:?}", scan.surfaces),
        );
        p.demand(
            "lonely-problem-names-the-missing-seed",
            scan.problems.iter().any(|problem| {
                problem.contains("StepLonely") && problem.contains("SeedLonely")
            }),
            format!("{:?}", scan.problems),
        );

        let bad_arity = "use search.research\n\nemath function StepBad:\n    inputs:\n        state: LoopState\n    outputs:\n        result: LoopState\n    definitions:\n        result = state\n";
        let (tree, diagnostics) = parse_str(bad_arity);
        p.demand("bad-arity-parses", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let scan = scan_session_surfaces(&tree);
        p.demand(
            "bad-arity-is-a-problem",
            scan.problems.iter().any(|problem| {
                problem.contains("StepBad") && problem.contains("budget")
            }),
            format!("{:?}", scan.problems),
        );

        // Opening the lonely module by name reports the missing seed.
        let path = temp_emath("lonely", lonely);
        match LoopHost::open(&path, Some("StepLonely")) {
            Err(fault) => {
                p.demand("lonely-target-code", fault.code == "loop_target", fault.to_string());
                p.demand(
                    "lonely-target-names-the-seed",
                    fault.message.contains("SeedLonely"),
                    fault.to_string(),
                );
            }
            Ok(_) => {
                p.fail("lonely-target-code", "opened an unpaired Step");
            }
        };

        // Ambiguity without a chosen target lists the surfaces: the
        // targets module lifts two, so it must name both.
        match LoopHost::open(&fixture_path("research_step_targets.emath"), None) {
            Err(fault) => {
                p.demand("ambiguous-code", fault.code == "loop_ambiguous", fault.to_string());
                p.demand(
                    "ambiguous-lists-both",
                    fault.message.contains("StepFitting") && fault.message.contains("StepReach"),
                    fault.to_string(),
                );
            }
            Ok(_) => {
                p.fail("ambiguous-code", "opened a two-surface module without a target");
            }
        };
    });

    probe.case("host-scratch-roundtrip-and-resume", |p| {
        let host =
            LoopHost::open(&fixture_path("research_step_valley.emath"), Some("StepValley"))
                .expect("open valley");
        let mut straight = host.begin().expect("begin straight");
        for _ in 0..3 {
            straight.step(&host, 60).expect("straight step");
        }

        let mut saved = host.begin().expect("begin saved");
        for _ in 0..2 {
            saved.step(&host, 60).expect("saved step");
        }
        let file = temp_scratch("resume");
        saved.save(&host, &file).expect("save");

        let mut resumed = LoopSession::load(&host, &file).expect("load");
        p.eq("resumed-revision", resumed.revision(), 2);
        p.eq("resumed-ledger-len", resumed.ledger().len() as i128, 2);
        p.eq("resumed-state-equals-saved", resumed.state().clone(), saved.state().clone());
        resumed.step(&host, 60).expect("resumed step");
        p.eq(
            "resumed-continuation-equals-straight",
            resumed.state().clone(),
            straight.state().clone(),
        );
        p.eq("resumed-revision-after-step", resumed.revision(), 3);

        // A foreign host (different target) refuses by identity.
        let fitting = LoopHost::open(
            &fixture_path("research_step_targets.emath"),
            Some("StepFitting"),
        )
        .expect("open fitting");
        match LoopSession::load(&fitting, &file) {
            Err(fault) => p.demand(
                "foreign-host-refused",
                fault.code == "scratch_identity",
                fault.to_string(),
            ),
            Ok(_) => p.fail("foreign-host-refused", "a fitting host loaded a valley scratch"),
        };

        // A torn ledger refuses through the host as well.
        let text = std::fs::read_to_string(&file).expect("read scratch");
        let torn = replace_field(&text, "revision", "2", "3");
        let torn_path = temp_scratch("torn");
        std::fs::write(&torn_path, torn).expect("write torn scratch");
        match LoopSession::load(&host, &torn_path) {
            Err(fault) => p.demand(
                "torn-scratch-refused",
                fault.code == "scratch_ledger",
                fault.to_string(),
            ),
            Ok(_) => p.fail("torn-scratch-refused", "a torn scratch loaded"),
        };

        // The module-semantic ledger law: the state's own batch count
        // must equal the committed revision after every step (already
        // exercised above) and on load. Mutate the state's batch field
        // away from the ledger to demand the refusal.
        let desynced = replace_field(&text, "batch", "2", "5");
        let desynced_path = temp_scratch("desynced");
        std::fs::write(&desynced_path, desynced).expect("write desynced scratch");
        match LoopSession::load(&host, &desynced_path) {
            Err(fault) => p.demand(
                "batch-ledger-law-refused",
                fault.code == "loop_ledger_law",
                fault.to_string(),
            ),
            Ok(_) => p.fail("batch-ledger-law-refused", "a desynced batch counter loaded"),
        };
    });

    probe.finish();
}
