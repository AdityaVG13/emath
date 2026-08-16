//!: goal decomposition.
//!
//! Rules split a goal into a subgoal DAG with source anchors; every child
//! must preserve the parent's requirements (never strengthen evidence or
//! exactness, never widen determinism).

use emath_ir::{EvidenceLevel, ExactnessPolicy, Goal, GoalRequirements};

/// One subgoal node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubgoalNode {
    /// Ordinal id.
    pub id: usize,
    /// Target name.
    pub target: String,
    /// Requirements (preserved/weakened from the parent).
    pub requirements: GoalRequirements,
    /// Source anchor: which decomposition rule produced it.
    pub rule: String,
}

/// Subgoal DAG edges (child depends on parent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubgoalDag {
    /// Nodes in creation order.
    pub nodes: Vec<SubgoalNode>,
    /// Edges as (parent, child).
    pub edges: Vec<(usize, usize)>,
}

/// One decomposition rule: `applies` gates, `split` produces children.
#[derive(Clone, Debug)]
pub struct DecompositionRule {
    /// Stable rule id.
    pub id: &'static str,
    /// Gate on the goal kind.
    pub applies: fn(&Goal) -> bool,
    /// Produces (target, requirements) children of the goal.
    pub split: fn(&Goal) -> Vec<(String, GoalRequirements)>,
}

/// Requirement-preservation: children must not strengthen evidence or
/// exactness of the parent.
#[must_use]
pub fn requirements_preserved(parent: &GoalRequirements, child: &GoalRequirements) -> bool {
    evidence_rank(child.evidence) <= evidence_rank(parent.evidence)
        && exactness_rank(&child.exactness) <= exactness_rank(&parent.exactness)
        && child.determinism == parent.determinism
}

/// Evidence rank (higher = stronger).
fn evidence_rank(level: EvidenceLevel) -> u8 {
    match level {
        EvidenceLevel::E0 => 0,
        EvidenceLevel::E1 => 1,
        EvidenceLevel::E2 => 2,
        EvidenceLevel::E3 => 3,
        EvidenceLevel::E4 => 4,
        EvidenceLevel::E5 => 5,
    }
}

/// Exactness rank (higher = looser).
fn exactness_rank(policy: &ExactnessPolicy) -> u8 {
    match policy {
        ExactnessPolicy::Exact => 0,
        ExactnessPolicy::CheckedNumeric => 1,
        ExactnessPolicy::Bounded { .. } => 2,
        ExactnessPolicy::Estimate => 3,
        ExactnessPolicy::AnyExplicit => 4,
    }
}

/// Applies rules in declaration order, building the subgoal DAG.
#[must_use]
pub fn decompose(goal: &Goal, rules: &[DecompositionRule]) -> SubgoalDag {
    let mut dag = SubgoalDag::default();
    dag.nodes.push(SubgoalNode {
        id: 0,
        target: goal.target.clone(),
        requirements: goal.requirements.clone(),
        rule: "root".into(),
    });
    let mut frontier: Vec<usize> = vec![0];
    let mut next_id = 1;
    while let Some(parent) = frontier.pop() {
        let parent_node = &dag.nodes[parent];
        if parent_node.rule != "root" {
            continue; // only the root is decomposed directly
        }
        let Some(rule) = rules.iter().find(|rule| {
            (rule.applies)(goal)
                && (rule.split)(goal)
                    .iter()
                    .all(|(_, child)| requirements_preserved(&parent_node.requirements, child))
        }) else {
            continue;
        };
        for (target, requirements) in (rule.split)(goal) {
            let child_id = next_id;
            next_id += 1;
            dag.nodes.push(SubgoalNode {
                id: child_id,
                target,
                requirements,
                rule: rule.id.to_string(),
            });
            dag.edges.push((parent, child_id));
            frontier.push(child_id);
        }
    }
    dag
}

/// Deterministic evaluation of requirement preservation for every edge.
#[must_use]
pub fn preserves_checks(dag: &SubgoalDag) -> bool {
    dag.edges.iter().all(|(parent, child)| {
        dag.nodes
            .get(*parent)
            .zip(dag.nodes.get(*child))
            .is_some_and(|(p, c)| requirements_preserved(&p.requirements, &c.requirements))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::Span;
    use emath_ir::{DeterminismPolicy, FallbackPolicy, GoalId, GoalKind, TargetProfile};

    fn goals() -> (Goal, GoalRequirements) {
        let requirements = GoalRequirements {
            evidence: EvidenceLevel::E2,
            exactness: ExactnessPolicy::Bounded {
                tolerance_literal: "1e-6".into(),
            },
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".into(),
                triple: None,
                features: vec![],
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: "rust.library".into(),
        };
        (
            Goal {
                id: GoalId(0),
                kind: GoalKind::Benchmark,
                target: "score".into(),
                expression: None,
                requirements: requirements.clone(),
                source: Span::default(),
            },
            requirements,
        )
    }

    fn splitting_rule() -> DecompositionRule {
        DecompositionRule {
            id: "benchmark.warmup-run.v1",
            applies: |goal| matches!(goal.kind, GoalKind::Benchmark),
            split: |_| {
                vec![
                    (
                        "warmup".to_string(),
                        GoalRequirements {
                            evidence: EvidenceLevel::E1,
                            exactness: ExactnessPolicy::Bounded {
                                tolerance_literal: "1e-3".into(),
                            },
                            determinism: DeterminismPolicy::Required,
                            target: TargetProfile {
                                family: "benchmark".into(),
                                triple: None,
                                features: vec![],
                            },
                            fallback: FallbackPolicy::NativeOnly,
                            produce: "bench.report".into(),
                        },
                    ),
                    (
                        "run".to_string(),
                        GoalRequirements {
                            evidence: EvidenceLevel::E1,
                            exactness: ExactnessPolicy::Bounded {
                                tolerance_literal: "1e-3".into(),
                            },
                            determinism: DeterminismPolicy::Required,
                            target: TargetProfile {
                                family: "benchmark".into(),
                                triple: None,
                                features: vec![],
                            },
                            fallback: FallbackPolicy::NativeOnly,
                            produce: "bench.report".into(),
                        },
                    ),
                ]
            },
        }
    }

    fn strengthening_rule() -> DecompositionRule {
        let mut rule = splitting_rule();
        rule.id = "benchmark.strengthens.v1";
        rule.split = |_| {
            vec![(
                "run".to_string(),
                GoalRequirements {
                    evidence: EvidenceLevel::E4,
                    exactness: ExactnessPolicy::Exact,
                    determinism: DeterminismPolicy::Required,
                    target: TargetProfile {
                        family: "benchmark".into(),
                        triple: None,
                        features: vec![],
                    },
                    fallback: FallbackPolicy::NativeOnly,
                    produce: "bench.report".into(),
                },
            )]
        };
        rule
    }

    #[test]
    fn decomposition_builds_dag_with_source_anchors() {
        let (goal, _) = goals();
        let dag = decompose(&goal, &[splitting_rule()]);
        assert_eq!(dag.nodes.len(), 3);
        assert_eq!(dag.edges, [(0, 1), (0, 2)]);
        assert!(dag.nodes[1].rule.starts_with("benchmark."));
        assert!(preserves_checks(&dag));
    }

    #[test]
    fn strengthening_rules_are_not_applied() {
        let (goal, _) = goals();
        let dag = decompose(&goal, &[strengthening_rule()]);
        assert_eq!(dag.nodes.len(), 1, "strengthening rule must not apply");
        assert!(preserves_checks(&dag));
    }

    #[test]
    fn requirement_rank_checks() {
        let (_, parent) = goals();
        // Same-or-weaker evidence and same-or-stricter exactness are preserved.
        let weaker = GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Bounded {
                tolerance_literal: "1e-6".into(),
            },
            determinism: DeterminismPolicy::Required,
            target: parent.target.clone(),
            fallback: parent.fallback,
            produce: parent.produce.clone(),
        };
        assert!(requirements_preserved(&parent, &weaker));
        let stronger_evidence = GoalRequirements {
            evidence: EvidenceLevel::E5,
            ..weaker.clone()
        };
        assert!(!requirements_preserved(&parent, &stronger_evidence));
        // Looser exactness silently weakens the parent contract: refused.
        let looser_exactness = GoalRequirements {
            exactness: ExactnessPolicy::Estimate,
            ..weaker.clone()
        };
        assert!(!requirements_preserved(&parent, &looser_exactness));
        let looser_determinism = GoalRequirements {
            determinism: DeterminismPolicy::Unspecified,
            ..weaker
        };
        assert!(!requirements_preserved(&parent, &looser_determinism));
    }
}
