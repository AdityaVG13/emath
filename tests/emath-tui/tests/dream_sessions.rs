//! Dream driver acceptance (bead emath-gav6o).
//!
//! The dream driver is thin host-side orchestration over an authored
//! dream target (a Step/Seed session surface whose module owns every
//! loop law - charging, freshness, promotion, the self-escalating
//! curriculum, graduation). The driver owns only the dream laws:
//!   - resume: a budget-exhausted batch (verdict 4) raises the
//!     logical-unit watermark by the configured step and continues;
//!   - plateau: three consecutive batches with no promotion and no
//!     audit growth (verdict 3) close the level - any growth or
//!     promotion (verdict 0) resets the counter;
//!   - close: domain exhaustion (verdict 2) or goal attainment
//!     (verdict 1) close the level immediately;
//!   - stop: max_batches is the external stop button - the run ends
//!     with no level and no emitted file;
//!   - emission: at close the driver emits the level file (the
//!     discovery audit trail: the incumbent, the frozen case set,
//!     the incumbent's observed predictions, the observed world -
//!     pinned as literals) with authored tests, and verifies that
//!     certificate in-process before reporting the level verified.
//!
//! Every assertion below was pinned from a real driver run; the
//! fixtures' own authored walks (dream_*.emath) pin the same walks
//! module-side.

use std::path::PathBuf;

use emath_exec_ir::constructor_layer::evaluate_tree_at;
use emath_syntax::parse_str;
use emath_test_harness::Probe;
use emath_tui::dream::{run_dream, DreamConfig, DreamStopReason};
use emath_tui::host::HostFault;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn temp_out(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "emath_dream_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&path).expect("make dream out dir");
    path
}

fn level_text(path: &Option<PathBuf>) -> String {
    std::fs::read_to_string(path.as_ref().expect("emitted level path"))
        .expect("read emitted level file")
}

/// Parse and evaluate the emitted level file independently: every
/// authored test in it must pass (the cheap re-verification the level
/// file exists for).
fn level_certificate(text: &str) -> (bool, usize, usize) {
    emath_syntax::install_source_parser();
    let (tree, diagnostics) = parse_str(text);
    if diagnostics.has_errors() {
        return (false, 0, 0);
    }
    match evaluate_tree_at(&tree, None) {
        Ok(report) => {
            let total = report.tests.len();
            let passed = report.tests.iter().filter(|t| t.passed).count();
            (passed == total && total > 0, passed, total)
        }
        Err(_) => (false, 0, 0),
    }
}

#[test]
fn dream_sessions() {
    let mut probe = Probe::new("dream driver: open-ended loop with level emission and certificate");

    // 1. The linear world: the dream discovers the law (key 11, full
    //    audit), the mid-walk budget halt resumes and still closes,
    //    and the emitted level file re-verifies independently.
    probe.case("linear-closes-with-certificate", |p| {
        let out = temp_out("linear");
        let config = DreamConfig {
            budget0: 16,
            budget_step: 16,
            max_batches: 200,
            plateau_close: 3,
            out_dir: Some(out.clone()),
        };
        let outcome =
            run_dream(&fixture_path("dream_world_linear.emath"), Some("StepDream"), &config)
                .expect("the linear dream runs");
        p.demand(
            "closed",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        p.demand("resumed-mid-walk", outcome.resumes >= 1, format!("{:?}", outcome.resumes));
        let level = outcome.level.as_ref().expect("a closed run emits its level");
        p.eq("close-reason", level.close_reason, "domain_exhausted");
        p.eq("incumbent", level.incumbent_key, 11);
        p.eq("case-ids", level.case_ids.clone(), vec![0, 1, 2, 3, 4]);
        p.eq("score", level.incumbent_score, (0, 1));
        p.demand("certificate-verified", level.certificate, "the driver must verify the level");
        p.demand(
            "level-file-named",
            level.path.as_deref() == Some(out.join("level_001.emath").as_path()),
            format!("{:?}", level.path),
        );
        let text = level_text(&level.path);
        let (ok, passed, total) = level_certificate(&text);
        p.demand(
            "level-reverifies",
            ok,
            format!("{passed}/{total} authored tests passed in the emitted file"),
        );
    });

    // 2. The budget law from a dead start: budget 1 cannot afford the
    //    first refresh, every early batch halts, and the resume ladder
    //    still climbs to the same close as case 1.
    probe.case("dead-start-budget-resumes", |p| {
        let config = DreamConfig {
            budget0: 1,
            budget_step: 8,
            max_batches: 400,
            plateau_close: 3,
            out_dir: None,
        };
        let outcome =
            run_dream(&fixture_path("dream_world_linear.emath"), Some("StepDream"), &config)
                .expect("the dead-start dream runs");
        p.demand(
            "closed-from-dead-start",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        p.demand(
            "many-resumes",
            outcome.resumes >= 3,
            format!("{:?} resumes: the ladder climbed from budget 1", outcome.resumes),
        );
        let level = outcome.level.as_ref().expect("closed");
        p.eq("same-winner", level.incumbent_key, 11);
        p.eq("same-close", level.close_reason, "domain_exhausted");
        p.demand("no-file-when-out-dir-none", level.path.is_none(), "no emission dir configured");
    });

    // 3. The unmasterable world: the honest partial close. Nothing
    //    admittable masters the frozen {0, 1}, so no growth and no
    //    promotion ever come again - the third consecutive plateau
    //    closes the level (before the domain even exhausts: the
    //    level's answer arrived, the dream stops spending).
    probe.case("unmasterable-honest-partial", |p| {
        let out = temp_out("partial");
        let config = DreamConfig {
            budget0: 256,
            budget_step: 64,
            max_batches: 200,
            plateau_close: 3,
            out_dir: Some(out),
        };
        let outcome =
            run_dream(&fixture_path("dream_unmasterable_linear.emath"), Some("StepDream"), &config)
                .expect("the unmasterable dream runs");
        p.demand(
            "closed",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        let level = outcome.level.as_ref().expect("closed");
        p.eq("close-reason", level.close_reason, "plateau");
        p.eq("incumbent", level.incumbent_key, 1);
        p.eq("case-ids", level.case_ids.clone(), vec![0, 1]);
        p.eq("score", level.incumbent_score, (1, 1));
        p.demand("certificate-verified", level.certificate, "the partial level verifies too");
        let text = level_text(&level.path);
        let (ok, passed, total) = level_certificate(&text);
        p.demand(
            "level-reverifies",
            ok,
            format!("{passed}/{total} authored tests passed in the emitted file"),
        );
    });

    // 4. The plateau law: the plateau world never exhausts its
    //    unbounded domain; the third consecutive plateau closes the
    //    level (batch 1 grows the one-case audit, batches 2-4
    //    plateau).
    probe.case("plateau-closes-at-three", |p| {
        let out = temp_out("plateau");
        let config = DreamConfig {
            budget0: 64,
            budget_step: 16,
            max_batches: 200,
            plateau_close: 3,
            out_dir: Some(out),
        };
        let outcome =
            run_dream(&fixture_path("dream_plateau_const.emath"), Some("StepDream"), &config)
                .expect("the plateau dream runs");
        p.demand(
            "closed",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        let level = outcome.level.as_ref().expect("closed");
        p.eq("close-reason", level.close_reason, "plateau");
        p.eq("incumbent", level.incumbent_key, 0);
        p.eq("case-ids", level.case_ids.clone(), vec![0]);
        p.eq("batches", level.batches, 4);
        p.demand("certificate-verified", level.certificate, "the plateau level verifies");
        let text = level_text(&level.path);
        let (ok, passed, total) = level_certificate(&text);
        p.demand(
            "level-reverifies",
            ok,
            format!("{passed}/{total} authored tests passed in the emitted file"),
        );
    });

    // 5. The stop law: max_batches is the external stop button. The
    //    plateau world would never close on its own (plateau_close
    //    1000); the run stops with no level and no emitted file.
    probe.case("max-batches-stops-without-level", |p| {
        let out = temp_out("stop");
        let config = DreamConfig {
            budget0: 64,
            budget_step: 16,
            max_batches: 5,
            plateau_close: 1000,
            out_dir: Some(out.clone()),
        };
        let outcome =
            run_dream(&fixture_path("dream_plateau_const.emath"), Some("StepDream"), &config)
                .expect("the stopped dream runs");
        p.demand(
            "stopped",
            outcome.stop == DreamStopReason::MaxBatches,
            format!("{:?}", outcome.stop),
        );
        p.demand("no-level", outcome.level.is_none(), "a stopped run emits no level");
        let emitted: Vec<_> = std::fs::read_dir(&out)
            .expect("out dir exists")
            .collect::<Result<Vec<_>, _>>()
            .expect("list out dir");
        p.demand("no-file", emitted.is_empty(), format!("{emitted:?}"));
    });

    // 6. The emission seam: a module without dream_world/dream_pred
    //    refuses by name at open (fail fast, no driving, no file).
    probe.case("emission-seam-refuses", |p| {
        let out = temp_out("seam");
        let config = DreamConfig {
            budget0: 64,
            budget_step: 16,
            max_batches: 200,
            plateau_close: 3,
            out_dir: Some(out.clone()),
        };
        let result =
            run_dream(&fixture_path("research_step_valley.emath"), Some("StepValley"), &config);
        match result {
            Err(HostFault { code, message }) => {
                p.eq("refusal-code", code, "dream_emission_surface".to_string());
                p.demand(
                    "refusal-names-the-seams",
                    message.contains("dream_world") && message.contains("dream_pred"),
                    format!("{message}"),
                );
            }
            Ok(outcome) => {
                p.demand(
                    "must-refuse",
                    false,
                    format!("the valley module has no dream emission seam: {outcome:?}"),
                );
            }
        }
        let emitted: Vec<_> = std::fs::read_dir(&out)
            .expect("out dir exists")
            .collect::<Result<Vec<_>, _>>()
            .expect("list out dir");
        p.demand("no-file", emitted.is_empty(), format!("{emitted:?}"));
    });

    // 7. The plateau reset law: the trickle surface (quota 2, no
    //    graduation - the module-side interleave is pinned in the
    //    fixture) interleaves plateau runs with growth batches. The
    //    counter must reset on every growth verdict, so the close is
    //    the post-mastery plateau run (batches 8-10), not the
    //    cumulative plateau count (which would close at batch 8,
    //    mid-walk).
    probe.case("plateau-counter-resets-on-growth", |p| {
        let config = DreamConfig {
            budget0: 256,
            budget_step: 64,
            max_batches: 200,
            plateau_close: 3,
            out_dir: None,
        };
        let outcome = run_dream(
            &fixture_path("dream_world_linear.emath"),
            Some("StepDreamTrickle"),
            &config,
        )
        .expect("the trickle dream runs");
        p.demand(
            "closed",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        let level = outcome.level.as_ref().expect("closed");
        p.eq("close-reason", level.close_reason, "plateau");
        p.eq("incumbent", level.incumbent_key, 11);
        p.eq("case-ids", level.case_ids.clone(), vec![0, 1, 2, 3, 4]);
        p.eq("batches", level.batches, 10);
    });

    // 8. The Rat world: a program-space dream target (candidates as
    //    quoted programs, the pred seam over Rat) closes goal_attained
    //    and emits a level whose world table and predictions are Rat
    //    literals. The driver converts its Int key and case ordinal to
    //    the seam's declared Rat carriers at the pred call; the
    //    certificate pins the exact rational rows.
    probe.case("rat-world-program-space-dream", |p| {
        let out = temp_out("pspace");
        let config = DreamConfig {
            budget0: 64,
            budget_step: 64,
            max_batches: 200,
            plateau_close: 3,
            out_dir: Some(out.clone()),
        };
        let outcome = run_dream(
            &fixture_path("dream_program_space.emath"),
            Some("StepPS"),
            &config,
        )
        .expect("the program-space dream runs");
        p.demand(
            "closed",
            outcome.stop == DreamStopReason::Closed,
            format!("{:?}", outcome.stop),
        );
        let level = outcome.level.as_ref().expect("closed");
        p.eq("close-reason", level.close_reason, "goal_attained");
        p.eq("incumbent", level.incumbent_key, 1);
        p.eq("case-ids", level.case_ids.clone(), vec![0, 1, 2, 3, 4]);
        p.eq("score", level.incumbent_score, (0, 1));
        p.demand("certificate-verified", level.certificate, "the driver must verify the level");
        let text = level_text(&level.path);
        p.demand(
            "world-rows-are-rat",
            text.contains("world = [1 / 2, 3 / 2, 5 / 2, 7 / 2, 9 / 2]"),
            format!("the pinned world table must be Rat literals:\n{text}"),
        );
        p.demand(
            "predictions-are-rat",
            text.contains("predictions = [1 / 2, 3 / 2, 5 / 2, 7 / 2, 9 / 2]"),
            format!("the pinned predictions must be Rat literals:\n{text}"),
        );
        p.demand(
            "level-carriers-declared",
            text.contains("world: sequence(Rat)") && text.contains("predictions: sequence(Rat)"),
            format!("the level's carriers must be declared Rat:\n{text}"),
        );
        let (ok, passed, total) = level_certificate(&text);
        p.demand(
            "independent-certificate",
            ok && passed == total && total >= 2,
            format!("independent re-verification: {passed}/{total}"),
        );
    });

    probe.finish();
}
