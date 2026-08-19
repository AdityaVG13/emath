//! Candidate loop tests (origin `crates/emath-lab-core/src/candidate.rs`).

use emath_core::ContentId;
use emath_lab_core::manifest::ArtifactRef;
use emath_lab_core::{Candidate, CandidateLoop, dominates};

fn candidate(name: &str, latency: f64, tokens: f64) -> Candidate {
    Candidate {
        artifact: ArtifactRef {
            package: "cache-policy".to_string(),
            content_id: ContentId(name.to_string()),
            profile: "release".to_string(),
        },
        metrics: vec![
            ("latency".to_string(), latency),
            ("tokens".to_string(), tokens),
        ],
    }
}

#[test]
fn dominance_requires_no_worse_everywhere_and_strictly_better_somewhere() {
    let a = candidate("a", 1.0, 1.0);
    let b = candidate("b", 2.0, 2.0);
    let c = candidate("c", 0.5, 3.0);
    assert!(dominates(&a, &b));
    assert!(!dominates(&b, &a));
    assert!(!dominates(&a, &c), "trade-off candidates never dominate");
    assert!(!dominates(&a, &a), "equal vectors do not dominate");
}

#[test]
fn archive_keeps_only_the_non_dominated_front() {
    let mut lab = CandidateLoop::new();
    assert!(lab.propose(candidate("slow", 2.0, 2.0), true).unwrap());
    assert!(lab.propose(candidate("fast", 1.0, 1.0), true).unwrap());
    // The dominated candidate was evicted; a dominated newcomer is refused.
    assert_eq!(lab.archive().front().len(), 1);
    assert!(!lab.propose(candidate("worse", 3.0, 3.0), true).unwrap());
    assert_eq!(lab.archive().front()[0].artifact.content_id.0, "fast");
}

/// Admission negative: a candidate whose evidence gate is closed is
/// refused with `E-HOST-005` and never reaches the archive.
#[test]
fn gate_refused_candidate_never_enters_the_archive() {
    let mut lab = CandidateLoop::new();
    let error = lab
        .propose(candidate("ungated", 0.1, 0.1), false)
        .expect_err("closed gate refuses");
    assert_eq!(error.code, "E-HOST-005");
    assert!(lab.archive().is_empty());
}
