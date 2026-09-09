//! Lab manifest identity and partition-stage tests.

use emath_core::ContentId;
use emath_lab_core::manifest::{ArtifactRef, CorpusPartition, EnvironmentPin, FallbackAction, FallbackPlan, LabManifest, MetricDirection, MetricSpec, PartitionKind, Thresholds};
use emath_test_harness::{Case, Probe, check_all, expect_ok};

fn sample() -> LabManifest {
    LabManifest { schema: "lab".to_string(), experiment_id: ContentId("exp-cache".to_string()), baseline: ArtifactRef { package: "cache".to_string(), content_id: ContentId("fnv1a64:base".to_string()), profile: "release".to_string() }, candidate: ArtifactRef { package: "cache".to_string(), content_id: ContentId("fnv1a64:cand".to_string()), profile: "release".to_string() }, partitions: vec![CorpusPartition { name: "holdout".to_string(), kind: PartitionKind::Holdout, operations: 16, fingerprint: ContentId("fnv1a64:part".to_string()) }], metrics: vec![MetricSpec { id: "latency".to_string(), kind: "latency".to_string(), unit: "ns".to_string(), direction: MetricDirection::LowerIsBetter, weight: 1.0 }], thresholds: Thresholds::default(), kill_rules: Vec::new(), fallback: FallbackPlan { on_gate_failure: FallbackAction::RetainBaseline, on_regression: FallbackAction::RetainBaseline, on_measurement_failure: FallbackAction::RetainBaseline }, environment: EnvironmentPin { toolchain: "stable".to_string(), target_triple: "aarch64-apple-darwin".to_string(), features: vec!["std".to_string()], host: "darwin".to_string() }, generator: "algebraic-rewrite".to_string(), seed: 42, frozen: true }
}

#[test]
fn lab_manifest() {
    let mut p = Probe::new("manifest identity binds generator and seed; stages run A through E");
    p.case("identity", |p| {
        let manifest = sample();
        p.demand("valid", manifest.validate().is_empty(), "sample validates");
        let parsed = LabManifest::from_json(&manifest.to_json()).expect("round-trip");
        p.eq("generator", parsed.generator, "algebraic-rewrite");
        p.eq("seed", parsed.seed, 42);
        p.eq("stable", parsed.identity(), manifest.identity());
        let mut other_generator = sample();
        other_generator.generator = "evolutionary".to_string();
        p.ne("generator-binds", other_generator.identity(), manifest.identity());
        let mut other_seed = sample();
        other_seed.seed = 7;
        p.ne("seed-binds", other_seed.identity(), manifest.identity());
    });
    p.case("stages", |p| {
        expect_ok(check_all(&[Case::new("training", PartitionKind::Training, 'A'), Case::new("calibration", PartitionKind::Calibration, 'B'), Case::new("validation", PartitionKind::Validation, 'C'), Case::new("holdout", PartitionKind::Holdout, 'D'), Case::new("stress", PartitionKind::Stress, 'E')], |kind| kind.stage()));
    });
    p.finish();
}
