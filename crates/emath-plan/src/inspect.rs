//! Plan inspection (CLI + machine JSON).
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

    /// Renders a deterministic human-readable plan explanation: why the
    /// selected candidate won, why every excluded candidate was refused
    /// (stable code + detail), which checks are planned and which artifact
    /// disposition applies.
    #[must_use]
    pub fn explain(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("policy: {}", self.policy));
        match &self.selected_plan_id {
            Some(id) => lines.push(format!("selected: {id}")),
            None => lines.push("selected: none".to_string()),
        }
        if self.candidates.is_empty() {
            lines.push("candidates: none".to_string());
        } else {
            lines.push(format!(
                "candidates ({}), in deterministic tie-break order:",
                self.candidates.len()
            ));
            for (rank, candidate) in self.candidates.iter().enumerate() {
                lines.push(format!("  {}. {candidate}", rank + 1));
            }
        }
        if self.exclusions.is_empty() {
            lines.push("exclusions: none".to_string());
        } else {
            lines.push(format!("exclusions ({}):", self.exclusions.len()));
            for (provider, code, detail) in &self.exclusions {
                lines.push(format!("  {provider}: {code}: {detail}"));
            }
        }
        if self.checks.is_empty() {
            lines.push("checks: none".to_string());
        } else {
            lines.push(format!("checks: {}", self.checks.join(", ")));
        }
        if let Some(budget) = &self.budget {
            lines.push(format!("budget: {budget}"));
        }
        lines.push(format!("disposition: {}", self.artifact_class));
        lines.join("\n")
    }

    /// Renders the inspection as deterministic JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.plan-explanation v1");
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
