//! Goal decomposition.
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
        // Exactness is never strengthened: higher rank is looser, so the
        // child must be at least as loose as the parent.
        && exactness_rank(&child.exactness) >= exactness_rank(&parent.exactness)
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
