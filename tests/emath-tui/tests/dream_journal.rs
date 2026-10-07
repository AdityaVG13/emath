//! `emath dream` driver acceptance (bead emath-3vium, SPEC §15 A1-A7,
//! A12): the run directory's journal is the canonical record of a
//! run, so it must be byte-deterministic, invariant under pausing and
//! resuming, reproducible by the resume law after a stop or a crash,
//! and replayable under steering and forks.

use std::path::{Path, PathBuf};

use emath_test_harness::Probe;
use emath_tui::dream::journal::read_events;
use emath_tui::dream::driver::{drive, Driver};
use emath_tui::dream::export::export_run;
use emath_tui::dream::resume::fork_run;
use emath_tui::dream::rundir::{create_run, send_request, RunConfig, RunDir};
use emath_tui::dream::{run_dream, DreamConfig};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn temp_root(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "emath_dreamrun_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&path).expect("make run root");
    path
}

fn linear(checkpoint_every: u64) -> RunConfig {
    let mut config = RunConfig::new(fixture("dream_world_linear.emath"));
    config.target = Some("StepDream".into());
    config.budget0 = 16;
    config.budget_step = 16;
    config.max_batches = 200;
    config.checkpoint_every = checkpoint_every;
    config
}

fn create(root: &Path, name: &str, config: RunConfig) -> RunDir {
    create_run(root, Some(name), false, config, Vec::new()).expect("create the run")
}

fn finish(driver: &mut Driver) {
    driver.run().expect("the driver runs to its end");
    assert!(driver.is_ended(), "the run ends");
}

fn run_whole(root: &Path, name: &str, config: RunConfig) -> (RunDir, String) {
    let run = create(root, name, config);
    let mut driver = Driver::open(&run.dir).expect("open the driver");
    finish(&mut driver);
    let journal = std::fs::read_to_string(run.journal()).expect("read the journal");
    (run, journal)
}

fn journal_of(run: &RunDir) -> String {
    std::fs::read_to_string(run.journal()).expect("read the journal")
}

fn end_outcome(run: &RunDir) -> String {
    read_events(&run.journal())
        .expect("journal parses")
        .iter()
        .rev()
        .find(|e| e.t == "end")
        .and_then(|e| e.str("outcome").map(str::to_string))
        .unwrap_or_default()
}

#[test]
fn dream_journal() {
    let mut probe =
        Probe::new("emath dream: the run journal is canonical, resumable, and replayable");
    let root = temp_root("journal");
    let (base_run, base) = run_whole(&root, "base", linear(25));

    // A1: identical module + config => byte-identical journal.
    probe.case("a1-determinism", |p| {
        let (_, again) = run_whole(&root, "again", linear(25));
        p.demand(
            "journal-nonempty",
            base.lines().count() > 4,
            format!("{} lines", base.lines().count()),
        );
        p.demand(
            "identical",
            again == base,
            "two identical runs wrote different journals",
        );
        p.eq(
            "outcome",
            end_outcome(&base_run),
            "domain_exhausted".to_string(),
        );
        let events = read_events(&base_run.journal()).expect("parse");
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        let contiguous: Vec<u64> = (1..=events.len() as u64).collect();
        p.eq("seq-contiguous", seqs, contiguous);
        p.eq(
            "first-two",
            events
                .iter()
                .take(2)
                .map(|e| e.t.clone())
                .collect::<Vec<_>>(),
            vec!["start".to_string(), "seed".to_string()],
        );
    });

    // A2: pausing (with its checkpoint) and resuming never changes the journal.
    probe.case("a2-pause-invariance", |p| {
        let run = create(&root, "paused", linear(25));
        let mut driver = Driver::open(&run.dir).expect("open");
        for _ in 0..3 {
            driver.run_batch().expect("batch");
        }
        send_request(&run, "pause", None).expect("request pause");
        driver.poll_control().expect("apply pause");
        p.demand(
            "paused",
            driver.is_paused(),
            "the pause request was not applied",
        );
        p.demand(
            "pause-checkpoint",
            run.checkpoint(3).exists(),
            "pausing writes a checkpoint",
        );
        send_request(&run, "step", None).expect("request step");
        driver.poll_control().expect("apply step");
        p.eq("stepped-one", driver.batches(), 4);
        p.demand(
            "still-paused",
            driver.is_paused(),
            "step leaves the run paused",
        );
        send_request(&run, "resume", None).expect("request resume");
        driver.poll_control().expect("apply resume");
        finish(&mut driver);
        p.demand(
            "identical",
            journal_of(&run) == base,
            "pause/step/resume changed the journal",
        );
    });

    // A3: stop + resume, and crash + resume, reproduce the uninterrupted journal.
    probe.case("a3-stop-resume", |p| {
        let run = create(&root, "stopped", linear(25));
        let mut driver = Driver::open(&run.dir).expect("open");
        for _ in 0..4 {
            driver.run_batch().expect("batch");
        }
        send_request(&run, "stop", None).expect("request stop");
        driver.poll_control().expect("apply stop");
        p.eq("stopped", end_outcome(&run), "stopped".to_string());
        drop(driver);
        let mut resumed = Driver::open(&run.dir).expect("resume a stopped run");
        finish(&mut resumed);
        p.demand(
            "identical",
            journal_of(&run) == base,
            "stop + resume diverged from the uninterrupted run",
        );
    });
    probe.case("a3-crash-resume", |p| {
        let (_, reference) = run_whole(&root, "ckpt-ref", linear(2));
        let (run, _) = run_whole(&root, "crashed", linear(2));
        // Simulate a crash inside batch 5: keep everything before batch
        // 5's `batch` line, drop the rest (and the end).
        let text = journal_of(&run);
        let mut kept = String::new();
        for line in text.lines() {
            if line.contains("\"t\":\"batch\"") && line.contains("\"batch\":5,") {
                break;
            }
            kept.push_str(line);
            kept.push('\n');
        }
        std::fs::write(run.journal(), kept).expect("truncate");
        std::fs::write(run.lock(), "999999").expect("dead lock");
        drive(&run.dir).expect("drive resumes the crashed run");
        p.demand(
            "identical",
            journal_of(&run) == reference,
            "crash + resume diverged from the uninterrupted run",
        );
        p.demand(
            "lock-released",
            !run.lock().exists(),
            "the driver removes its lock on exit",
        );
    });
    // A loop state with no archive drives, and `dream_keep` grows the
    // run's own library as rules are learned; stop + resume keeps the
    // journal and the library identical to the uninterrupted run.
    probe.case("learn-keeps", |p| {
        let learn = || {
            let mut config = RunConfig::new(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/keep/dream_learn.emath"),
            );
            config.target = Some("StepLearn".into());
            config.work = 50_000_000;
            config.checkpoint_every = 1;
            config
        };
        let (run, whole) = run_whole(&root, "learn", learn());
        p.eq("outcome", end_outcome(&run), "domain_exhausted".to_string());
        let kept: Vec<(u64, String)> = read_events(&run.journal())
            .expect("parse")
            .iter()
            .filter(|e| e.t == "kept")
            .map(|e| (e.batch().unwrap_or(0), e.str("name").unwrap_or_default().to_string()))
            .collect();
        p.eq(
            "kept-as-learned",
            kept,
            vec![
                (1, "triangular".to_string()),
                (1, "fibonacci".to_string()),
                (2, "sum_of_cubes".to_string()),
                (2, "fibonacci_even".to_string()),
            ],
        );
        // The same definitions `emath keep` writes from the one-shot learn.
        let code = |text: &str| -> Vec<String> {
            text.lines().filter(|l| !l.trim_start().starts_with('#')).map(str::to_string).collect()
        };
        let library = std::fs::read_to_string(run.world().join("kept.emath")).unwrap_or_default();
        let golden = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/keep/lib/learned.emath"),
        )
        .expect("golden");
        p.eq("library-matches-keep", code(&library), code(&golden));

        let stopped = create(&root, "learn-stopped", learn());
        let mut driver = Driver::open(&stopped.dir).expect("open");
        driver.run_batch().expect("b1");
        send_request(&stopped, "stop", None).expect("request stop");
        driver.poll_control().expect("apply stop");
        drop(driver);
        let mut resumed = Driver::open(&stopped.dir).expect("resume");
        finish(&mut resumed);
        p.demand("resume-journal", journal_of(&stopped) == whole, "stop + resume diverged");
        p.demand(
            "resume-library",
            std::fs::read_to_string(stopped.world().join("kept.emath")).unwrap_or_default() == library,
            "stop + resume wrote a different library",
        );
    });
    // Optimizing a user's program in user space: the knob climb scores
    // settings by quote.cost work, keeps its best under one name (kept
    // again when it improves), and the kept library imports the user's
    // module it calls.
    probe.case("tune-keeps-best", |p| {
        let tune = || {
            let mut config = RunConfig::new(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/keep/dream_tune.emath"),
            );
            config.target = Some("StepTune".into());
            config.work = 50_000_000;
            config.checkpoint_every = 1;
            config
        };
        let (run, whole) = run_whole(&root, "tune", tune());
        p.eq("outcome", end_outcome(&run), "domain_exhausted".to_string());
        let kept: Vec<(u64, String)> = read_events(&run.journal())
            .expect("parse")
            .iter()
            .filter(|e| e.t == "kept")
            .map(|e| (e.batch().unwrap_or(0), e.str("name").unwrap_or_default().to_string()))
            .collect();
        p.eq(
            "kept-each-improvement",
            kept,
            vec![(1, "count_primes_fast".to_string()), (2, "count_primes_fast".to_string())],
        );
        let library = std::fs::read_to_string(run.world().join("kept.emath")).unwrap_or_default();
        p.demand(
            "library-holds-best",
            library.lines().any(|l| l == "use tune_prog")
                && library.contains("result = count_primes_with([2, 1], n)")
                // Work is measured under lexical closure binding, without
                // inheriting unrelated caller locals.
                && library.contains("Saving(work_after=36418, work_before=178179)"),
            library.clone(),
        );

        let stopped = create(&root, "tune-stopped", tune());
        let mut driver = Driver::open(&stopped.dir).expect("open");
        driver.run_batch().expect("b1");
        send_request(&stopped, "stop", None).expect("request stop");
        driver.poll_control().expect("apply stop");
        drop(driver);
        let mut resumed = Driver::open(&stopped.dir).expect("resume");
        finish(&mut resumed);
        p.demand("resume-journal", journal_of(&stopped) == whole, "stop + resume diverged");
        p.demand(
            "resume-library",
            std::fs::read_to_string(stopped.world().join("kept.emath")).unwrap_or_default() == library,
            "stop + resume wrote a different library",
        );
    });
    // Rewriting a user's program: rules are quoted pattern pairs, the
    // climb keeps each cheaper rewrite that still agrees with the
    // checks, and a rule that never lowers the work (swapping the
    // operands of +) is passed by.
    probe.case("rewrite-keeps-best", |p| {
        let mut config = RunConfig::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/keep/dream_rewrite.emath"),
        );
        config.target = Some("StepRewrite".into());
        config.work = 50_000_000;
        config.checkpoint_every = 1;
        let (run, _) = run_whole(&root, "rewrite", config);
        p.eq("outcome", end_outcome(&run), "domain_exhausted".to_string());
        let kept: Vec<u64> = read_events(&run.journal())
            .expect("parse")
            .iter()
            .filter(|e| e.t == "kept")
            .map(|e| e.batch().unwrap_or(0))
            .collect();
        p.eq("kept-each-improvement", kept, vec![1, 2]);
        let library = std::fs::read_to_string(run.world().join("kept.emath")).unwrap_or_default();
        p.demand(
            "library-holds-factored",
            library.lines().any(|l| l == "use fold.reduce")
                && library.contains("result = sum(tabulate(function i in Int: i * n, n)) * (3 + 4 + 1)")
                && library.contains("Saving(work_after=842, work_before=2478)"),
            library.clone(),
        );
    });
    probe.case("a13-live-lock-refuses", |p| {
        // Liveness is the OS lock a driver holds on driver.lock, not the
        // pid written in it.
        let run = create(&root, "live", linear(25));
        let held = std::fs::File::create(run.lock()).expect("lock file");
        held.try_lock().expect("hold the driver lock");
        let refused = drive(&run.dir);
        p.eq(
            "code",
            refused.err().map(|f| f.code),
            Some("dream_run_live".to_string()),
        );
        drop(held);
        // A pid of some live process with no lock held is a stale lock
        // (a crashed driver whose pid was reused): the run proceeds.
        let mut sleeper = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");
        std::fs::write(run.lock(), sleeper.id().to_string()).expect("stale lock");
        let stale = drive(&run.dir);
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        p.demand("stale-lock-proceeds", stale.is_ok(), format!("{stale:?}"));
    });

    // A4: steering is journaled once and replays through a fork.
    probe.case("a4-steer-and-fork", |p| {
        let run = create(&root, "steered", linear(2));
        let mut driver = Driver::open(&run.dir).expect("open");
        driver.run_batch().expect("b1");
        driver.run_batch().expect("b2");
        send_request(&run, "grow_case", Some(4)).expect("request grow");
        driver.poll_control().expect("apply grow");
        finish(&mut driver);
        let steered = journal_of(&run);
        let steers = read_events(&run.journal())
            .expect("parse")
            .into_iter()
            .filter(|e| e.t == "steer")
            .count();
        p.eq("one-steer", steers, 1);
        let (source, _) = run_whole(&root, "unsteered", linear(2));
        let fork =
            fork_run(&root, &source, Some(2), Some("forked"), None, None).expect("fork at b2");
        let mut forked = Driver::open(&fork.dir).expect("open the fork");
        send_request(&fork, "grow_case", Some(4)).expect("request grow");
        forked.poll_control().expect("apply grow");
        finish(&mut forked);
        p.demand(
            "identical",
            journal_of(&fork) == steered,
            "the forked replay diverged from the steered run",
        );
        let early = fork_run(&root, &source, Some(1), Some("no-checkpoint"), None, None);
        p.eq(
            "no-checkpoint",
            early.err().map(|f| f.code),
            Some("dream_no_checkpoint".to_string()),
        );
    });

    // A5: the journal agrees with the dream driver on the same config.
    probe.case("a5-parity-with-run-dream", |p| {
        let out = root.join("parity-level");
        std::fs::create_dir_all(&out).expect("level dir");
        let outcome = run_dream(
            &fixture("dream_world_linear.emath"),
            Some("StepDream"),
            &DreamConfig {
                budget0: 16,
                budget_step: 16,
                max_batches: 200,
                plateau_close: 3,
                out_dir: Some(out),
                ..DreamConfig::default()
            },
        )
        .expect("run_dream");
        let level = outcome.level.expect("closed");
        let events = read_events(&base_run.journal()).expect("parse");
        let closed = events
            .iter()
            .find(|e| e.t == "level")
            .expect("a level event");
        p.eq(
            "reason",
            closed.str("reason").map(str::to_string),
            Some(level.close_reason.to_string()),
        );
        p.eq(
            "incumbent",
            closed
                .record("incumbent")
                .and_then(|r| r.get("key").map(|k| k.to_string())),
            Some(level.incumbent_key.to_string()),
        );
        p.eq("used", closed.int("used"), Some(level.used));
        p.eq("batches", closed.uint("batches"), Some(level.batches));
        p.eq(
            "certificate",
            closed.bool("certificate"),
            Some(level.certificate),
        );
        p.eq(
            "resumes",
            events.iter().filter(|e| e.t == "budget").count() as u64,
            outcome.resumes,
        );
        p.demand(
            "level-file",
            base_run.level().join("level_001.emath").exists(),
            "the run emits its level",
        );
    });

    // A6: the journal carries no context and no measurement.
    probe.case("a6-no-leaks", |p| {
        let root_text = root.display().to_string();
        for line in base.lines() {
            p.demand("no-run-path", !line.contains(&root_text), line.to_string());
            p.demand(
                "no-module-path",
                !line.contains("fixtures/constructor"),
                line.to_string(),
            );
            p.demand(
                "no-time-keys",
                !line.contains("_ms\"") && !line.contains("unix"),
                line.to_string(),
            );
        }
        let allowed: &[(&str, &[&str])] = &[
            (
                "start",
                &[
                    "seq",
                    "t",
                    "schema",
                    "meaning_id",
                    "language_id",
                    "target",
                    "level",
                    "config",
                    "emission",
                ],
            ),
            (
                "seed",
                &[
                    "seq",
                    "t",
                    "batch",
                    "used",
                    "watermark",
                    "incumbent",
                    "archive",
                    "case_ids",
                ],
            ),
            ("records", &["seq", "t", "batch", "added", "changed"]),
            ("cases", &["seq", "t", "batch", "added"]),
            (
                "batch",
                &[
                    "seq",
                    "t",
                    "batch",
                    "verdict",
                    "verdict_name",
                    "used",
                    "delta_used",
                    "watermark",
                    "halted",
                    "plateau_streak",
                    "incumbent",
                    "promoted",
                    "quarantined",
                    "case_ids",
                    "archive_len",
                ],
            ),
            ("budget", &["seq", "t", "batch", "from", "to"]),
            ("steer", &["seq", "t", "batch", "action", "case_id"]),
            (
                "level",
                &[
                    "seq",
                    "t",
                    "batch",
                    "reason",
                    "incumbent",
                    "case_ids",
                    "used",
                    "batches",
                    "file",
                    "certificate",
                ],
            ),
            ("end", &["seq", "t", "batch", "outcome", "code"]),
        ];
        for event in read_events(&base_run.journal()).expect("parse") {
            let keys: Vec<&str> = event.fields.iter().map(|(k, _)| k.as_str()).collect();
            let want = allowed
                .iter()
                .find(|(t, _)| *t == event.t)
                .map(|(_, keys)| keys.to_vec());
            p.eq(&format!("keys-{}", event.t), Some(keys), want);
        }
    });

    // A7: every outcome ends the journal with its own `end`.
    probe.case("a7-outcomes", |p| {
        let mut goal = RunConfig::new(fixture("research_step_valley.emath"));
        goal.target = Some("StepValley".into());
        let (goal_run, _) = run_whole(&root, "goal", goal);
        p.eq("goal", end_outcome(&goal_run), "goal_attained".to_string());

        let mut plateau = RunConfig::new(fixture("dream_world_linear.emath"));
        plateau.target = Some("StepDreamTrickle".into());
        plateau.budget0 = 256;
        plateau.budget_step = 64;
        let (plateau_run, _) = run_whole(&root, "plateau", plateau);
        p.eq("plateau", end_outcome(&plateau_run), "plateau".to_string());

        let mut ceiling = linear(25);
        ceiling.max_units = Some(17);
        let (ceiling_run, _) = run_whole(&root, "ceiling", ceiling);
        p.eq(
            "budget-exhausted",
            end_outcome(&ceiling_run),
            "budget_exhausted".to_string(),
        );

        let mut capped = linear(25);
        capped.max_batches = 2;
        let (capped_run, _) = run_whole(&root, "capped", capped);
        p.eq(
            "batch-cap",
            end_outcome(&capped_run),
            "batch_cap".to_string(),
        );
        let batches = read_events(&capped_run.journal())
            .expect("parse")
            .into_iter()
            .filter(|e| e.t == "batch")
            .count();
        p.eq("batch-cap-count", batches, 2);
        p.demand(
            "closed-not-resumable",
            Driver::open(&goal_run.dir).is_err(),
            "a closed run must refuse to resume",
        );
        p.demand(
            "capped-resumable",
            Driver::open(&capped_run.dir).is_ok(),
            "a batch-capped run resumes",
        );

        let mut refusing = linear(25);
        refusing.target = Some("StepDreamFault".into());
        let (fault_run, _) = run_whole(&root, "fault", refusing);
        let events = read_events(&fault_run.journal()).expect("parse");
        let end = events.last().expect("an end event");
        p.eq(
            "fault",
            end.str("outcome").map(str::to_string),
            Some("fault".to_string()),
        );
        p.eq("fault-after-b1", end.uint("batch"), Some(1));
        p.demand(
            "fault-code",
            end.str("code")
                .is_some_and(|code| !code.is_empty() && !code.starts_with("dream_")),
            "the fault carries the engine's own code",
        );
        p.eq(
            "fault-batches",
            events.iter().filter(|e| e.t == "batch").count(),
            1,
        );
        p.demand(
            "fault-not-resumable",
            Driver::open(&fault_run.dir).is_err(),
            "a faulted run must refuse to resume",
        );
    });

    // A12: export tables.
    probe.case("a12-export", |p| {
        let events = read_events(&base_run.journal()).expect("parse");
        let batch_count = events.iter().filter(|e| e.t == "batch").count();
        let csv = export_run(&base_run, "batches", "csv").expect("csv");
        let jsonl = export_run(&base_run, "batches", "jsonl").expect("jsonl");
        p.eq("csv-rows", csv.lines().count(), batch_count + 1);
        p.eq("jsonl-rows", jsonl.lines().count(), batch_count);
        p.eq(
            "csv-header",
            csv.lines().next().map(str::to_string),
            Some("batch,verdict,verdict_name,used,delta_used,watermark,plateau_streak,incumbent_key,incumbent_score,promoted,quarantined,case_count,archive_len".to_string()),
        );
        let last = csv.lines().last().unwrap_or_default().split(',').map(str::to_string).collect::<Vec<_>>();
        p.eq("last-incumbent", last.get(7).cloned(), Some("11".to_string()));
        p.eq("last-verdict", last.get(2).cloned(), Some("domain_exhausted".to_string()));
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dream/golden/export-batches.csv");
        if std::env::var_os("EMATH_DREAM_BLESS").is_some() {
            std::fs::write(&golden, &csv).expect("bless the export golden");
        }
        p.eq("csv-golden", Some(csv.clone()), std::fs::read_to_string(&golden).ok());
        let mut rows = csv.lines();
        let header: Vec<&str> = rows.next().unwrap_or_default().split(',').collect();
        let expected: String = rows
            .map(|row| {
                let cells: Vec<String> = header.iter().zip(row.split(',')).map(|(c, v)| format!("\"{c}\":\"{v}\"")).collect();
                format!("{{{}}}\n", cells.join(","))
            })
            .collect();
        p.eq("jsonl-equals-csv", jsonl.clone(), expected);
        let records = export_run(&base_run, "records", "csv").expect("records");
        p.demand("records-header-key-first", records.starts_with("key,"), records.lines().next().unwrap_or_default().to_string());
        p.eq("unknown-table", export_run(&base_run, "nope", "csv").err().map(|f| f.code), Some("dream_export".to_string()));
    });

    probe.finish();
}
