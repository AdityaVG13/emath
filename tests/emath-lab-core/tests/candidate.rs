//! Candidate loop dominance and archive tests.

use emath_core::ContentId;
use emath_lab_core::manifest::ArtifactRef;
use emath_lab_core::{Candidate, CandidateLoop, dominates};
use emath_test_harness::Probe;

fn candidate(name: &str, latency: f64, tokens: f64) -> Candidate {
    Candidate { artifact: ArtifactRef { package: "cache-policy".to_string(), content_id: ContentId(name.to_string()), profile: "release".to_string() }, metrics: vec![("latency".to_string(), latency), ("tokens".to_string(), tokens)] }
}

#[test]
fn candidate_loop() {
    let mut p = Probe::new("dominance gates the archive and closed gates refuse");
    p.case("dominance", |p| {
        let (a, b, c) = (candidate("a", 1.0, 1.0), candidate("b", 2.0, 2.0), candidate("c", 0.5, 3.0));
        p.demand("dominates", dominates(&a, &b), "no-worse everywhere, better somewhere");
        p.demand("not-reversed", !dominates(&b, &a), "dominance is one-way");
        p.demand("tradeoff", !dominates(&a, &c), "trade-offs never dominate");
        p.demand("irreflexive", !dominates(&a, &a), "equal vectors do not dominate");
    });
    p.case("archive-front", |p| {
        let mut lab = CandidateLoop::new();
        p.demand("slow-in", lab.propose(candidate("slow", 2.0, 2.0), true).unwrap(), "first candidate enters");
        p.demand("fast-in", lab.propose(candidate("fast", 1.0, 1.0), true).unwrap(), "dominator enters");
        p.eq("evicted", lab.archive().front().len(), 1);
        p.demand("worse-refused", !lab.propose(candidate("worse", 3.0, 3.0), true).unwrap(), "dominated newcomer refused");
        p.eq("kept", lab.archive().front()[0].artifact.content_id.0, "fast");
    });
    p.case("closed-gate", |p| {
        let mut lab = CandidateLoop::new();
        let error = lab.propose(candidate("ungated", 0.1, 0.1), false).expect_err("closed gate refuses");
        p.eq("code", error.code, "E-HOST-005");
        p.demand("empty", lab.archive().is_empty(), "refused candidate never archives");
    });
    p.finish();
}
