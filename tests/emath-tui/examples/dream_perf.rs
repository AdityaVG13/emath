//! `emath dream` dashboard measurement (SPEC §15 performance targets,
//! recorded, not gating): builds a synthetic run directory with a
//! 10,000-batch / 50,000-record journal, then times the viewer's real
//! ingest path (`RunWatch` open + poll) and the pure frame render at
//! 160 × 50. Run in release:
//!
//! `cargo run --release -p emath-tui-tests --example dream_perf [batches]`
//!
//! The run directory is kept (its path is printed) so the same journal
//! can be opened with `emath dream watch <dir>`.

use std::fmt::Write as _;
use std::time::Instant;

use emath_tui::dashboard::render::render;
use emath_tui::dashboard::view::{Ctx, RunBadge, View};
use emath_tui::dashboard::watch::RunWatch;
use emath_tui::dream::rundir::RunDir;

const RECORDS_PER_BATCH: u64 = 5;

/// A deterministic LCG: the same journal on every machine.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound.max(1)
    }
}

fn rat(n: u64, d: u64) -> String {
    format!("{{\"rat\": [{n}, {d}]}}")
}

#[allow(clippy::too_many_arguments)]
fn record(key: u64, parent: i64, family: i64, mv: i64, score: u64, tier: u64, accepted: bool, counterexample: i64, refusal: u64) -> String {
    format!(
        "{{\"accepted\":{accepted},\"bound\":{s},\"counterexample\":{counterexample},\"evaluator_id\":0,\"family\":{family},\"input_id\":1,\"key\":{key},\"move\":{mv},\"parent\":{parent},\"refusal\":{refusal},\"retained\":true,\"score\":{s},\"target_id\":1,\"tier\":{tier},\"value\":{v},\"witness\":-1}}",
        s = rat(score, 1000),
        v = rat(key * 7 % 997, 13),
    )
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn main() {
    let batches: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(10_000);
    let dir = std::env::temp_dir().join(format!("emath_dream_perf_{}_{batches}", std::process::id()));
    std::fs::create_dir_all(dir.join("control")).expect("run dir");
    let mut rng = Lcg(0x5eed);
    let mut journal = String::new();
    let mut metrics = String::new();
    let mut seq = 0u64;
    let mut line = |journal: &mut String, body: String| {
        seq += 1;
        let _ = writeln!(journal, "{{\"seq\":{seq},{body}}}");
        seq
    };
    line(&mut journal, format!(
        "\"t\":\"start\",\"schema\":\"emath.dream.journal.v1\",\"meaning_id\":\"emath:meaning:v1:{}\",\"language_id\":\"constructor-layer\",\"target\":\"StepPerf\",\"level\":1,\"config\":{{\"budget0\":256,\"budget_step\":256,\"plateau_close\":8,\"max_batches\":{batches},\"max_units\":null,\"checkpoint_every\":25}},\"emission\":true",
        "ab".repeat(32)
    ));
    let mut scores: Vec<u64> = vec![100];
    let mut incumbent = 0u64;
    let seed = record(0, -1, -1, -1, 100, 1, true, -1, 0);
    line(&mut journal, format!("\"t\":\"seed\",\"batch\":0,\"used\":40,\"watermark\":256,\"incumbent\":{seed},\"archive\":[{seed}],\"case_ids\":[0]"));
    let (mut used, mut watermark, mut streak) = (40u64, 256u64, 0u64);
    let mut cases: Vec<u64> = vec![0];
    let mut best = 100u64;
    for batch in 1..=batches {
        let mut added = Vec::new();
        for _ in 0..RECORDS_PER_BATCH {
            let key = scores.len() as u64;
            let parent = rng.next(key) as i64;
            let score = (best + rng.next(7)).saturating_sub(3).clamp(1, 999);
            let refusal = if rng.next(100) < 3 { 2 } else { 0 };
            added.push(record(key, parent, rng.next(6) as i64, rng.next(4) as i64, score, u64::from(rng.next(10) < 7), false, -1, refusal));
            scores.push(score);
        }
        let mut changed = Vec::new();
        let mut promoted = String::new();
        let mut quarantined = String::new();
        if batch % 47 == 0 && best < 999 {
            best += 1;
            let key = scores.len() as u64 - 1;
            scores[key as usize] = best;
            added.pop();
            added.push(record(key, rng.next(key) as i64, 3, 1, best, 1, true, -1, 0));
            incumbent = key;
            promoted = key.to_string();
        }
        if batch % 211 == 0 {
            let key = 1 + rng.next(scores.len() as u64 - 1);
            if key != incumbent {
                let ce = cases[rng.next(cases.len() as u64) as usize] as i64;
                changed.push(record(key, 0, 1, 2, scores[key as usize], 1, false, ce, 0));
                quarantined = key.to_string();
            }
        }
        line(&mut journal, format!("\"t\":\"records\",\"batch\":{batch},\"added\":[{}],\"changed\":[{}]", added.join(","), changed.join(",")));
        if batch % 257 == 0 && cases.len() < 40 {
            let id = cases.len() as u64;
            cases.push(id);
            line(&mut journal, format!("\"t\":\"cases\",\"batch\":{batch},\"added\":[{id}]"));
        }
        let mut delta = 60 + rng.next(200);
        let halted = used + delta >= watermark;
        if halted {
            delta = watermark - used;
        }
        used += delta;
        let (verdict, name) = if halted {
            (4, "budget_exhausted")
        } else if !promoted.is_empty() || batch % 257 == 0 {
            (0, "running")
        } else {
            (3, "plateau")
        };
        streak = match verdict {
            3 => (streak + 1).min(7),
            4 => streak,
            _ => 0,
        };
        let inc = record(incumbent, 0, 3, 1, scores[incumbent as usize], 1, true, -1, 0);
        let ids: Vec<String> = cases.iter().map(u64::to_string).collect();
        let batch_seq = line(&mut journal, format!(
            "\"t\":\"batch\",\"batch\":{batch},\"verdict\":{verdict},\"verdict_name\":\"{name}\",\"used\":{used},\"delta_used\":{delta},\"watermark\":{watermark},\"halted\":{halted},\"plateau_streak\":{streak},\"incumbent\":{inc},\"promoted\":[{promoted}],\"quarantined\":[{quarantined}],\"case_ids\":[{}],\"archive_len\":{}",
            ids.join(","),
            scores.len()
        ));
        let _ = writeln!(metrics, "{{\"batch\":{batch},\"seq\":{batch_seq},\"wall_ms\":{},\"batch_ms\":100,\"cache_lookups\":null,\"cache_hits\":null,\"computed_units\":null}}", batch * 100);
        if halted {
            let to = 2 * used + 256;
            line(&mut journal, format!("\"t\":\"budget\",\"batch\":{batch},\"from\":{watermark},\"to\":{to}"));
            watermark = to;
        }
    }
    std::fs::write(dir.join("journal.jsonl"), &journal).expect("journal");
    std::fs::write(dir.join("metrics.jsonl"), &metrics).expect("metrics");
    std::fs::write(
        dir.join("run.json"),
        format!("{{\"schema\":\"emath.dream.run.v1\",\"module\":\"/synthetic/perf.emath\",\"target\":\"StepPerf\",\"config\":{{\"budget0\":256,\"budget_step\":256,\"plateau_close\":8,\"max_batches\":{batches},\"max_units\":null,\"checkpoint_every\":25,\"keep_checkpoints\":8}},\"argv\":[],\"created_unix_ms\":0,\"engine\":\"vm\",\"ended\":null}}"),
    )
    .expect("run.json");

    let mut ingest = Vec::new();
    let mut watch = None;
    for _ in 0..5 {
        let started = Instant::now();
        let mut w = RunWatch::open(RunDir::new(&dir));
        w.refresh();
        w.poll();
        ingest.push(started.elapsed().as_secs_f64() * 1000.0);
        watch = Some(w);
    }
    let watch = watch.expect("watched");
    let model = &watch.model;
    ingest.sort_by(f64::total_cmp);
    println!("journal      {} bytes, {} events, {} batches, {} records", journal.len(), seq, model.last_batch(), model.records.len());
    println!("ingest_ms    median {:.1}  max {:.1}  (RunWatch open+refresh+poll, 5 runs)", percentile(&ingest, 0.5), ingest[ingest.len() - 1]);
    // The first render after new data also rebuilds the model's derived
    // record orderings (once per ingest, not per frame): `first`.
    let ctx = Ctx::headless("perf", "perf.emath", RunBadge::Running);
    for (tab, name) in [(0usize, "overview"), (1, "search"), (2, "evidence"), (3, "journal")] {
        let view = View { tab, ..View::default() };
        let mut frames = Vec::new();
        for _ in 0..200 {
            let started = Instant::now();
            let frame = render(model, &view, &ctx, 160, 50);
            frames.push(started.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(frame);
        }
        let first = frames[0];
        frames.sort_by(f64::total_cmp);
        println!("frame_ms     {name:<9} p50 {:.2}  p95 {:.2}  max {:.2}  first {first:.2}  (160x50, 200 renders)", percentile(&frames, 0.5), percentile(&frames, 0.95), frames[frames.len() - 1]);
    }
    println!("run_dir      {}", dir.display());
}
