//! `emath dream` end to end (bead emath-3vium, SPEC §15 A11): the
//! launching command returns while its detached driver keeps
//! committing, and a new `watch` reports the batch the journal holds.
//! (The TTY viewer's `q` maps to detach; the key contract is pinned in
//! `tests/emath-tui/tests/dream_dashboard.rs`.)

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emath(args: &[&str]) -> (String, Option<i32>) {
    let output = Command::new(common::emath_bin())
        .args(args)
        .current_dir(workspace())
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (text, output.status.code())
}

fn batches(journal: &Path) -> usize {
    std::fs::read_to_string(journal).unwrap_or_default().lines().filter(|l| l.contains("\"t\":\"batch\"")).count()
}

fn ended(journal: &Path) -> bool {
    std::fs::read_to_string(journal).unwrap_or_default().lines().any(|l| l.contains("\"t\":\"end\""))
}

#[test]
fn dream_cmd() {
    let mut probe = Probe::new("emath dream: the viewer detaches, the run continues");
    let root = std::env::temp_dir().join(format!("emath_dream_cmd_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module = workspace().join("tests/fixtures/constructor/dream_world_linear.emath");
    let module_s = module.display().to_string();
    let run = root.join("a11");
    let journal = run.join("journal.jsonl");

    // stdout is a pipe, not a terminal: the command starts the driver,
    // prints the run name and returns without attaching.
    let (text, code) = emath(&[
        "dream", &module_s, "--target", "StepDreamFamilies", "--budget", "16", "--budget-step", "16", "--run", "a11",
        "--root", &root_s,
    ]);
    let at_return = batches(&journal);
    probe.demand("start-returns", code == Some(0) && text.lines().next() == Some("a11"), format!("exit {code:?}\n{text}"));

    let deadline = Instant::now() + Duration::from_secs(120);
    while !ended(&journal) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let last = batches(&journal);
    probe.demand("driver-outlives-viewer", ended(&journal) && last > at_return, format!("{at_return} batches at return, {last} at the end"));
    probe.demand("lock-released", !run.join("driver.lock").exists(), "the driver left its lock behind");

    // A new watch (non-terminal: a plain snapshot) shows the same batch.
    let (text, code) = emath(&["dream", "watch", "a11", "--root", &root_s]);
    probe.demand(
        "watch-same-batch",
        code == Some(0) && text.contains(&format!("L1 · batch {last}")) && text.contains("PLATEAU"),
        format!("exit {code:?}\n{text}"),
    );
    let (text, _) = emath(&["dream", "list", "--root", &root_s]);
    probe.demand("list-names-the-run", text.contains(&format!("a11  StepDreamFamilies  plateau  b{last}")), text);
    probe.finish();
}

fn wait_ended(journal: &Path) {
    let deadline = Instant::now() + Duration::from_secs(300);
    while !ended(journal) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// (cache_hits, computed_units) per metrics line.
fn fact_metrics(metrics: &Path) -> Vec<(i64, i64)> {
    let num = |line: &str, key: &str| -> i64 {
        let tail = line.split(&format!("\"{key}\":")).nth(1).unwrap_or("");
        tail.split(|c: char| c == ',' || c == '}').next().unwrap_or("").parse().unwrap_or(-1)
    };
    std::fs::read_to_string(metrics)
        .unwrap_or_default()
        .lines()
        .map(|line| (num(line, "cache_hits"), num(line, "computed_units")))
        .collect()
}

#[test]
fn dream_ledger() {
    let mut probe = Probe::new("emath dream: evaluations are appended once and reused by every later run");
    let root = std::env::temp_dir().join(format!("emath_dream_ledger_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module = workspace().join("tests/fixtures/constructor/dream_evolve_crossover.emath");
    let module_s = module.display().to_string();
    let launch = |name: &str| {
        let (text, code) =
            emath(&["dream", &module_s, "--target", "StepEvo", "--plateau-close", "20", "--run", name, "--root", &root_s]);
        assert_eq!(code, Some(0), "{text}");
        wait_ended(&root.join(name).join("journal.jsonl"));
    };
    let ledger_lines = || -> Vec<String> {
        let dir = root.join("ledger");
        let files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|it| it.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.first().map_or(Vec::new(), |f| std::fs::read_to_string(f).unwrap_or_default().lines().map(str::to_string).collect())
    };

    launch("first");
    let after_first = ledger_lines();
    let first = fact_metrics(&root.join("first/metrics.jsonl"));
    probe.demand(
        "ledger-created",
        after_first.len() > 1 && after_first[0].contains("emath.dream.ledger.v1"),
        format!("{} ledger lines", after_first.len()),
    );
    // Inside one run the refresh re-scores the archive every batch: from
    // the second batch on, those scores come from carried facts.
    probe.demand(
        "in-run-reuse",
        first.len() > 2 && first[1..].iter().all(|(hits, _)| *hits > 0),
        format!("{first:?}"),
    );

    launch("second");
    let after_second = ledger_lines();
    let second = fact_metrics(&root.join("second/metrics.jsonl"));
    let journal = |name: &str| std::fs::read_to_string(root.join(name).join("journal.jsonl")).unwrap_or_default();
    probe.demand("same-journal", journal("first") == journal("second"), "a carried fact changed the run");
    probe.demand(
        "nothing-recomputed",
        !second.is_empty() && second.iter().all(|(hits, computed)| *computed == 0 && *hits > 0),
        format!("{second:?}"),
    );
    probe.demand("append-only", after_second == after_first, format!("{} -> {} lines", after_first.len(), after_second.len()));

    // A crash that tore the header leaves a file with no facts: the next
    // run recomputes and restarts the file, and the run after that
    // reuses everything again.
    let file = std::fs::read_dir(root.join("ledger")).expect("ledger dir").next().expect("ledger file").expect("entry").path();
    std::fs::write(&file, &after_first[0][..10]).expect("tear the header");
    launch("third");
    launch("fourth");
    let fourth = fact_metrics(&root.join("fourth/metrics.jsonl"));
    probe.demand(
        "torn-header-recovers",
        journal("third") == journal("first")
            && ledger_lines().len() == after_first.len()
            && ledger_lines()[0] == after_first[0]
            && !fourth.is_empty()
            && fourth.iter().all(|(hits, computed)| *computed == 0 && *hits > 0),
        format!("{fourth:?}\n{}", journal("fourth")),
    );
    probe.finish();
}

#[test]
fn dream_ledger_parallel() {
    let mut probe = Probe::new("emath dream: parallel runs share one ledger, and a faulted batch keeps its facts");
    let root = std::env::temp_dir().join(format!("emath_dream_parallel_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module = workspace().join("tests/fixtures/constructor/dream_evolve_crossover.emath");
    let module_s = module.display().to_string();
    let ledger_text = |root: &Path| -> String {
        std::fs::read_dir(root.join("ledger"))
            .ok()
            .and_then(|mut it| it.next())
            .and_then(Result::ok)
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
            .unwrap_or_default()
    };

    // Two launches at the same moment, both asking for the default name.
    let spawn = || {
        Command::new(common::emath_bin())
            .args(["dream", &module_s, "--target", "StepEvo", "--plateau-close", "20", "--root", &root_s])
            .current_dir(workspace())
            .output()
    };
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(spawn);
        let b = s.spawn(spawn);
        (a.join().expect("join").expect("run"), b.join().expect("join").expect("run"))
    });
    let name = |o: &std::process::Output| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string();
    let (na, nb) = (name(&a), name(&b));
    probe.demand(
        "distinct-default-names",
        a.status.success() && b.status.success() && !na.is_empty() && !nb.is_empty() && na != nb,
        format!("{na:?} {nb:?}\n{}{}", String::from_utf8_lossy(&a.stderr), String::from_utf8_lossy(&b.stderr)),
    );
    for n in [&na, &nb] {
        wait_ended(&root.join(n).join("journal.jsonl"));
    }
    let journal = |name: &str| std::fs::read_to_string(root.join(name).join("journal.jsonl")).unwrap_or_default();
    probe.demand(
        "both-reach-the-goal",
        journal(&na).contains("goal_attained") && journal(&na) == journal(&nb),
        format!("{}\n---\n{}", journal(&na), journal(&nb)),
    );
    let text = ledger_text(&root);
    let keys: Vec<&str> = text.lines().skip(1).filter_map(|l| l.split("\"key\":\"").nth(1)?.split('"').next()).collect();
    let unique: std::collections::BTreeSet<&str> = keys.iter().copied().collect();
    probe.demand(
        "one-header-no-duplicates",
        text.lines().filter(|l| l.contains("emath.dream.ledger.v1")).count() == 1
            && !keys.is_empty()
            && keys.len() == unique.len()
            && keys.len() == text.lines().count() - 1,
        format!("{} keys, {} unique\n{text}", keys.len(), unique.len()),
    );

    // The trap target faults in the batch that proposes the goal, after
    // that batch computed the goal's fact_half calls. Its own root, so
    // nothing is carried in from the runs above.
    let trap_root = root.join("trap-root");
    let trap_root_s = trap_root.display().to_string();
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepTrap", "--plateau-close", "20", "--run", "trap", "--root", &trap_root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&trap_root.join("trap/journal.jsonl"));
    let trap = std::fs::read_to_string(trap_root.join("trap/journal.jsonl")).unwrap_or_default();
    let committed = trap.lines().filter(|l| l.contains("\"t\":\"batch\"")).count();
    let faulted_batch = format!("\"run\":\"trap\",\"batch\":{}}}", committed + 1);
    probe.demand(
        "faulted-batch-facts-kept",
        trap.contains("\"outcome\":\"fault\"") && ledger_text(&trap_root).lines().any(|l| l.ends_with(&faulted_batch)),
        format!("{trap}\n---\n{}", ledger_text(&trap_root)),
    );
    probe.finish();
}

/// The incumbent key of batch `b`'s journal event.
fn incumbent_at(journal: &str, b: u64) -> Option<i64> {
    let line = journal.lines().find(|l| l.contains("\"t\":\"batch\"") && l.contains(&format!("\"batch\":{b},")))?;
    let incumbent = line.split("\"incumbent\":{").nth(1)?;
    let key = incumbent.split("\"key\":").nth(1)?;
    key.split(|c: char| c == ',' || c == '}').next()?.parse().ok()
}

#[test]
fn dream_evidence() {
    let mut probe = Probe::new("emath dream: inbox evidence moves the run, and the journal alone carries it");
    let root = std::env::temp_dir().join(format!("emath_dream_evidence_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module = workspace().join("tests/fixtures/constructor/dream_evidence.emath");
    let module_s = module.display().to_string();
    let run = root.join("ev");
    let journal = || std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();

    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepEvidence", "--budget", "1000", "--budget-step", "1000", "--plateau-close",
        "90", "--max-batches", "2", "--checkpoint-every", "1", "--run", "ev", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    probe.demand("prior-holds-without-evidence", incumbent_at(&journal(), 2) == Some(0), journal());

    // Dropped while the run is stopped: taken at the next boundary.
    let inbox = run.join("inbox");
    std::fs::create_dir_all(&inbox).expect("inbox");
    let a = "{\"subject\":1,\"case\":0,\"value\":-2.25}\n{\"subject\":1,\"case\":1,\"value\":\"-2/2\"}\n\
             {\"subject\":1,\"case\":2,\"value\":0.5}\n{\"subject\":0,\"case\":0,\"value\":3}\n";
    std::fs::write(inbox.join("a.jsonl"), a).expect("a");
    std::fs::write(inbox.join("b.jsonl"), "{\"subject\":1,\"case\":0,\"value\":1e3}\n").expect("b");
    std::fs::write(inbox.join("c.jsonl"), "{\"subject\":1,\"case\":0,\"value\":-9}").expect("c");
    let (out, code) = emath(&["dream", "resume", "ev", "--max-batches", "4", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    // The old `end` stays until the resumed driver truncates it.
    let deadline = Instant::now() + Duration::from_secs(300);
    while !journal().contains("\"t\":\"end\",\"batch\":4") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let text = journal();
    let evidence: Vec<&str> = text.lines().filter(|l| l.contains("\"t\":\"evidence")).collect();
    probe.demand(
        "taken-once-exact-and-journaled",
        evidence.len() == 2
            && evidence[0].contains("\"t\":\"evidence\",\"batch\":2,\"file\":\"a.jsonl\",\"source\":1")
            && evidence[0].contains("{\"rat\": [-9, 4]}")
            && evidence[0].contains("{\"rat\": [-1, 1]}")
            && evidence[1].contains("\"t\":\"evidence_rejected\",\"batch\":2,\"file\":\"b.jsonl\""),
        text.clone(),
    );
    probe.demand(
        "evidence-moves-the-incumbent",
        incumbent_at(&text, 2) == Some(0) && incumbent_at(&text, 3) == Some(1) && incumbent_at(&text, 4) == Some(1),
        text.clone(),
    );
    probe.demand(
        "inbox-untouched",
        ["a.jsonl", "b.jsonl", "c.jsonl"].iter().all(|f| inbox.join(f).exists()),
        "the driver removed an inbox file",
    );

    // The fork has no inbox: its evidence comes from the kept journal.
    let (out, code) = emath(&["dream", "fork", "ev", "--at", "2", "--run", "evf", "--max-batches", "4", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    let fork = root.join("evf").join("journal.jsonl");
    wait_ended(&fork);
    let forked = std::fs::read_to_string(&fork).unwrap_or_default();
    probe.demand("fork-replays-evidence", forked == text, format!("{forked}\n---\n{text}"));
    probe.finish();
}

#[test]
fn dream_chain() {
    let mut probe = Probe::new("emath dream: a closed level writes, verifies and continues on the next level");
    let root = std::env::temp_dir().join(format!("emath_dream_chain_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module = workspace().join("tests/fixtures/constructor/dream_chain.emath");
    let module_s = module.display().to_string();
    let source = std::fs::read(&module).expect("source");
    let run = root.join("ch");
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepChain", "--budget", "1000", "--plateau-close", "50", "--checkpoint-every",
        "1", "--keep-checkpoints", "20", "--run", "ch", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
    let has = |needle: &str| text.contains(needle);
    probe.demand(
        "levels-chain-on-verified-files",
        has("\"t\":\"level_next\",\"batch\":4,\"level\":2,\"file\":\"world/level_002.emath\",\"carry\":\"3\"")
            && has("\"t\":\"level_next\",\"batch\":8,\"level\":3,\"file\":\"world/level_003.emath\",\"carry\":\"6\"")
            && std::fs::read_to_string(run.join("world/level_003.emath")).unwrap_or_default().contains("        result = 6\n"),
        text.clone(),
    );
    probe.demand(
        "failing-authored-test-rejects-the-level",
        has("\"t\":\"level_rejected\",\"batch\":12,\"level\":4,\"file\":\"world/level_004.emath\"")
            && has("start_within_reach")
            && run.join("world/level_004.emath").exists()
            && has("\"t\":\"end\",\"batch\":12,\"outcome\":\"domain_exhausted\"")
            && incumbent_at(&text, 12) == Some(9),
        text.clone(),
    );
    probe.demand("source-untouched", std::fs::read(&module).expect("source") == source, "the launch module changed");

    // Resume inside level 3 (from its checkpoint) and at its first batch
    // boundary (from its level file) both reproduce the journal.
    for at in ["10", "8"] {
        let name = format!("ch-{at}");
        let (out, code) = emath(&["dream", "fork", "ch", "--at", at, "--run", &name, "--root", &root_s]);
        assert_eq!(code, Some(0), "{out}");
        let fork = root.join(&name).join("journal.jsonl");
        let deadline = Instant::now() + Duration::from_secs(300);
        while !std::fs::read_to_string(&fork).unwrap_or_default().contains("\"t\":\"end\"") && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let forked = std::fs::read_to_string(&fork).unwrap_or_default();
        probe.demand(&format!("fork-at-{at}-reproduces"), forked == text, format!("{forked}\n---\n{text}"));
    }
    probe.finish();
}

#[test]
fn dream_strategy() {
    let mut probe = Probe::new("emath dream: each level runs on a strategy the last level evolved on held-out tasks");
    let root = std::env::temp_dir().join(format!("emath_dream_strategy_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module_s = workspace().join("tests/fixtures/constructor/dream_strategy.emath").display().to_string();
    let run = root.join("st");
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepStrategy", "--budget", "1000", "--plateau-close", "50",
        "--checkpoint-every", "1", "--keep-checkpoints", "30", "--run", "st", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
    // Step 1 needs 13 batches for keys 0..12, step 2 needs 7, step 3 needs 5.
    probe.demand(
        "evolved-strategy-shortens-each-level",
        text.contains("\"t\":\"level_next\",\"batch\":13,\"level\":2,\"file\":\"world/level_002.emath\",\"carry\":\"[2]\"")
            && text.contains("\"t\":\"level_next\",\"batch\":20,\"level\":3,\"file\":\"world/level_003.emath\",\"carry\":\"[3]\""),
        text.clone(),
    );
    // Training tasks favour step 4; the held-out tasks refuse it.
    probe.demand(
        "held-out-tasks-stop-the-climb",
        text.contains("\"t\":\"end\",\"batch\":25,\"outcome\":\"domain_exhausted\"")
            && !text.contains("\"level\":4")
            && !run.join("world/level_004.emath").exists(),
        text.clone(),
    );
    let (out, code) = emath(&["dream", "fork", "st", "--at", "22", "--run", "st-22", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    let fork = root.join("st-22").join("journal.jsonl");
    wait_ended(&fork);
    let forked = std::fs::read_to_string(&fork).unwrap_or_default();
    probe.demand("fork-on-an-evolved-level-reproduces", forked == text, format!("{forked}\n---\n{text}"));
    probe.finish();
}

#[test]
fn dream_invariants() {
    let mut probe = Probe::new("emath dream: levels grow a library of exact conservation laws, each a readable term");
    let root = std::env::temp_dir().join(format!("emath_dream_invariants_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module_s = workspace().join("tests/fixtures/constructor/dream_invariants.emath").display().to_string();
    let run = root.join("inv");
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepLaws", "--budget", "1000000", "--plateau-close", "4", "--work",
        "2000000000", "--run", "inv", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
    // Momentum and energy (collisions), the rotation's radius, and the
    // leapfrog's step-dependent shadow energy: nothing else, and nothing
    // from the contraction.
    let library = "\"carry\":\"[Invariant: {checked: 15, coefs: [1], law: quote(m * v), monos: [[1, 1]], world: 0}, \
                   Invariant: {checked: 15, coefs: [1], law: quote(m * v * v), monos: [[1, 2]], world: 0}, \
                   Invariant: {checked: 15, coefs: [1, 1], law: quote(p * p + q * q), monos: [[0, 2], [2, 0]], world: 1}, \
                   Invariant: {checked: 60, coefs: [4, 4, -1], law: quote(4 * (p * p) + 4 * (q * q) - q * q * h * h), \
                   monos: [[0, 2, 0], [2, 0, 0], [2, 0, 2]], world: 2}]\"";
    probe.demand(
        "four-laws-one-per-level",
        text.contains("\"t\":\"level_next\",\"batch\":21,\"level\":4,") && text.contains(library),
        text.clone(),
    );
    let level = std::fs::read_to_string(run.join("world/level_004.emath")).unwrap_or_default();
    probe.demand(
        "the-library-is-authored-source",
        level.contains("law: quote(4 * (p * p) + 4 * (q * q) - q * q * h * h)"),
        level.clone(),
    );
    probe.demand(
        "no-new-law-closes-the-run",
        text.contains("\"outcome\":\"domain_exhausted\"") && !text.contains("\"level\":5") && !text.contains("world: 3"),
        text.clone(),
    );
    probe.finish();
}

#[test]
fn dream_worlds() {
    let mut probe = Probe::new("emath dream: discoveries ask the next questions, over worlds built from authored parts");
    let root = std::env::temp_dir().join(format!("emath_dream_worlds_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module_s = workspace().join("tests/fixtures/constructor/dream_worlds.emath").display().to_string();
    let run = root.join("wo");
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepWorlds", "--budget", "1000000", "--plateau-close", "50", "--work",
        "2000000000", "--checkpoint-every", "1", "--keep-checkpoints", "40", "--run", "wo", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
    let last_carry = text.lines().filter(|l| l.contains("\"t\":\"level_next\"")).last().unwrap_or("").to_string();
    // The module only asks for the laws of rotate (1) and contract (4).
    // Swap (3) and the two reflections (1 then 3 = 25, 3 then 1 = 11)
    // enter as preservers of the rotation's law, and their own laws
    // follow.
    probe.demand(
        "laws-of-worlds-nobody-listed",
        last_carry.contains("law: quote(p + q), monos: [[0, 1], [1, 0]], world: 3}")
            && last_carry.contains("law: quote(3 * p + q), monos: [[0, 1], [1, 0]], world: 11}")
            && last_carry.contains("law: quote(p + 3 * q), monos: [[0, 1], [1, 0]], world: 25}"),
        last_carry.clone(),
    );
    // The identity (5) and swap-then-swap (27) move nothing: no
    // question or law ever names them. The contract finds nothing and
    // retires; so do the preserver questions of the three new laws.
    probe.demand(
        "vacuous-worlds-never-asked",
        !text.contains("world: 5}") && !text.contains("world: 27}") && !text.contains("world: 13}"),
        text.clone(),
    );
    probe.demand(
        "retired-without-progress",
        last_carry.contains("retired: [Question: {kind: 0, parent: -1, seed: 0, template: 0, world: 4}, ")
            && last_carry.contains("tried: [6, 4], won: [4, 1]"),
        last_carry.clone(),
    );
    probe.demand(
        "empty-agenda-closes-the-run",
        last_carry.contains("open: []")
            && last_carry.contains("current: Question: {kind: -1,")
            && text.contains("\"t\":\"end\",\"batch\":35,\"outcome\":\"domain_exhausted\"")
            && !run.join("world/level_012.emath").exists(),
        text.clone(),
    );
    let level = std::fs::read_to_string(run.join("world/level_011.emath")).unwrap_or_default();
    probe.demand("the-agenda-is-authored-source", level.contains("law: quote(p + 3 * q)"), level.clone());
    let (out, code) = emath(&["dream", "fork", "wo", "--at", "15", "--run", "wo-15", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    let fork = root.join("wo-15").join("journal.jsonl");
    wait_ended(&fork);
    let forked = std::fs::read_to_string(&fork).unwrap_or_default();
    probe.demand("fork-mid-agenda-reproduces", forked == text, format!("{forked}\n---\n{text}"));
    probe.finish();
}

#[test]
fn dream_many_worlds() {
    let mut probe = Probe::new("emath dream: one judge dreams across the run's configured carrier worlds");
    let root = std::env::temp_dir().join(format!("emath_dream_many_worlds_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module_s = workspace().join("tests/fixtures/constructor/dream_many_worlds.emath").display().to_string();
    let last_incumbent = |run: &str| {
        let journal = root.join(run).join("journal.jsonl");
        wait_ended(&journal);
        let text = std::fs::read_to_string(&journal).unwrap_or_default();
        let last = text.lines().filter(|l| l.contains("\"t\":\"batch\"")).count() as u64;
        (incumbent_at(&text, last), text)
    };
    for (run, worlds) in [("one", "exact"), ("two", "exact,float64")] {
        let (out, code) = emath(&[
            "dream", &module_s, "--worlds", worlds, "--budget", "100000", "--plateau-close", "50",
            "--checkpoint-every", "1", "--run", run, "--root", &root_s,
        ]);
        assert_eq!(code, Some(0), "{out}");
    }
    let (one, text) = last_incumbent("one");
    probe.demand("exact-alone-sees-no-divergence", one == Some(1), text);
    let (two, text) = last_incumbent("two");
    probe.demand("float64-parts-from-exact-at-five", two == Some(5), text.clone());

    let (out, code) = emath(&["dream", "fork", "two", "--at", "6", "--run", "two-6", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    let (_, forked) = last_incumbent("two-6");
    probe.demand("fork-keeps-the-worlds", forked == text, format!("{forked}\n---\n{text}"));

    let (out, code) = emath(&["dream", &module_s, "--worlds", "exact,float65", "--run", "bad", "--root", &root_s]);
    probe.demand("unknown-world-refuses", code != Some(0) && out.contains("float65"), format!("exit {code:?}\n{out}"));
    probe.finish();
}

#[test]
fn dream_revision() {
    let mut probe = Probe::new("emath dream: evidence revises the model, and the model finds a plan nobody measured");
    let root = std::env::temp_dir().join(format!("emath_dream_revision_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("runs root");
    let root_s = root.display().to_string();
    let module_s = workspace().join("tests/fixtures/constructor/dream_revision.emath").display().to_string();
    let run = root.join("rev");
    let journal = || std::fs::read_to_string(run.join("journal.jsonl")).unwrap_or_default();
    let (out, code) = emath(&[
        "dream", &module_s, "--target", "StepRevision", "--budget", "1000", "--budget-step", "1000", "--plateau-close",
        "90", "--max-batches", "2", "--checkpoint-every", "1", "--run", "rev", "--root", &root_s,
    ]);
    assert_eq!(code, Some(0), "{out}");
    wait_ended(&run.join("journal.jsonl"));
    probe.demand("unrevised-model-sees-no-difference", incumbent_at(&journal(), 2) == Some(0), journal());

    // Readings of (k - 3)^2 + case on plans 0, 1, 5 and 6; plan 3 is never measured.
    let mut lines = String::new();
    for k in [0i64, 1, 5, 6] {
        for c in 0..4 {
            lines.push_str(&format!("{{\"subject\":{k},\"case\":{c},\"value\":{}}}\n", (k - 3) * (k - 3) + c));
        }
    }
    std::fs::create_dir_all(run.join("inbox")).expect("inbox");
    std::fs::write(run.join("inbox/readings.jsonl"), lines).expect("readings");
    let (out, code) = emath(&["dream", "resume", "rev", "--max-batches", "4", "--root", &root_s]);
    assert_eq!(code, Some(0), "{out}");
    let deadline = Instant::now() + Duration::from_secs(300);
    while !journal().contains("\"t\":\"end\",\"batch\":4") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let text = journal();
    probe.demand(
        "revised-model-finds-the-unmeasured-plan",
        text.contains("\"t\":\"evidence\",\"batch\":2")
            && incumbent_at(&text, 3) == Some(3)
            && incumbent_at(&text, 4) == Some(3),
        text.clone(),
    );
    probe.finish();
}
