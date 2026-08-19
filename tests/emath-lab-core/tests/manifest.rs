//! Manifest tests (origin `crates/emath-lab-core/src/manifest.rs`).

use emath_core::ContentId;
use emath_lab_core::manifest::{
    ArtifactRef, CorpusPartition, EnvironmentPin, FallbackAction, FallbackPlan, LabManifest,
    MetricDirection, MetricSpec, PartitionKind, Thresholds,
};

fn sample() -> LabManifest {
    LabManifest {
        schema: "lab".to_string(),
        experiment_id: ContentId("exp-cache".to_string()),
        baseline: ArtifactRef {
            package: "cache".to_string(),
            content_id: ContentId("fnv1a64:base".to_string()),
            profile: "release".to_string(),
        },
        candidate: ArtifactRef {
            package: "cache".to_string(),
            content_id: ContentId("fnv1a64:cand".to_string()),
            profile: "release".to_string(),
        },
        partitions: vec![CorpusPartition {
            name: "holdout".to_string(),
            kind: PartitionKind::Holdout,
            operations: 16,
            fingerprint: ContentId("fnv1a64:part".to_string()),
        }],
        metrics: vec![MetricSpec {
            id: "latency".to_string(),
            kind: "latency".to_string(),
            unit: "ns".to_string(),
            direction: MetricDirection::LowerIsBetter,
            weight: 1.0,
        }],
        thresholds: Thresholds::default(),
        kill_rules: Vec::new(),
        fallback: FallbackPlan {
            on_gate_failure: FallbackAction::RetainBaseline,
            on_regression: FallbackAction::RetainBaseline,
            on_measurement_failure: FallbackAction::RetainBaseline,
        },
        environment: EnvironmentPin {
            toolchain: "stable".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            features: vec!["std".to_string()],
            host: "darwin".to_string(),
        },
        generator: "algebraic-rewrite".to_string(),
        seed: 42,
        frozen: true,
    }
}

#[test]
fn campaign_identity_binds_generator_and_seed() {
    let manifest = sample();
    assert!(manifest.validate().is_empty());
    let json = manifest.to_json();
    let parsed = LabManifest::from_json(&json).expect("round-trip");
    assert_eq!(parsed.generator, "algebraic-rewrite");
    assert_eq!(parsed.seed, 42);
    assert_eq!(parsed.identity(), manifest.identity());

    let mut other_generator = sample();
    other_generator.generator = "evolutionary".to_string();
    assert_ne!(other_generator.identity(), manifest.identity());

    let mut other_seed = sample();
    other_seed.seed = 7;
    assert_ne!(other_seed.identity(), manifest.identity());
}

#[test]
fn partition_kinds_occupy_protocol_stages_a_through_e() {
    assert_eq!(PartitionKind::Training.stage(), 'A');
    assert_eq!(PartitionKind::Calibration.stage(), 'B');
    assert_eq!(PartitionKind::Validation.stage(), 'C');
    assert_eq!(PartitionKind::Holdout.stage(), 'D');
    assert_eq!(PartitionKind::Stress.stage(), 'E');
}
