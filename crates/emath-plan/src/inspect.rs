//!: plan inspection (CLI + machine JSON).
//!
//! The inspection carries goals, retained candidates, every exclusion with
//! its stable reason, the selected plan, planned checks, budget and the
//! artifact disposition class. JSON is emitted by the deterministic
//! in-tree writer.

/// Machine-readable plan inspection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanInspection {
    /// Planner policy.
    pub policy: String,
    /// Retained candidate provider ids.
    pub candidates: Vec<String>,
    /// Exclusions as (provider, code, detail).
    pub exclusions: Vec<(String, String, String)>,
    /// Selected plan id (when a plan was selected).
    pub selected_plan_id: Option<String>,
    /// Planned evidence checks.
    pub checks: Vec<String>,
    /// Budget trace (when constrained).
    pub budget: Option<String>,
    /// Artifact disposition class.
    pub artifact_class: String,
}

impl PlanInspection {
    /// Number of compatible candidates.
    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Renders the inspection as deterministic JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("policy", &self.policy);
        object.strings("candidates", &self.candidates);
        object.int("candidate_count", self.candidate_count() as u64);
        let mut exclusions = String::new();
        for (provider, code, detail) in &self.exclusions {
            exclusions.push_str(provider);
            exclusions.push(':');
            exclusions.push_str(code);
            exclusions.push(':');
            exclusions.push_str(detail);
            exclusions.push('\n');
        }
        object.string("exclusions", exclusions.trim_end());
        if let Some(id) = &self.selected_plan_id {
            object.string("selected_plan", id);
        } else {
            object.string("selected_plan", "");
        }
        object.strings("checks", &self.checks);
        if let Some(budget) = &self.budget {
            object.string("budget", budget);
        } else {
            object.string("budget", "");
        }
        object.string("artifact_class", &self.artifact_class);
        object.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_json_is_stable_golden() {
        let inspection = PlanInspection {
            policy: "deterministic-planner.v1".into(),
            candidates: vec!["exact-a".into(), "exact-b".into()],
            exclusions: vec![(
                "approx".into(),
                "E-PROV-515".into(),
                "goal requires exact results".into(),
            )],
            selected_plan_id: Some("fnv1a64:0000000000000001".into()),
            checks: vec!["sir-checker.v1".into()],
            budget: None,
            artifact_class: "native".into(),
        };
        let first = inspection.to_json();
        assert_eq!(inspection.to_json(), first);
        assert!(first.contains("\"artifact_class\": \"native\""));
        assert!(first.contains("E-PROV-515"));
        assert!(first.contains("\"candidate_count\": 2"));
    }

    #[test]
    fn no_plan_renders_empty_selected() {
        let inspection = PlanInspection {
            policy: "deterministic-planner.v1".into(),
            candidates: vec![],
            exclusions: vec![],
            selected_plan_id: None,
            checks: vec![],
            budget: None,
            artifact_class: "parametric".into(),
        };
        assert!(inspection.to_json().contains("\"selected_plan\": \"\""));
        assert!(inspection
            .to_json()
            .contains("\"artifact_class\": \"parametric\""));
    }
}
