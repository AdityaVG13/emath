//! Equation admission helpers: state-name resolution.

use super::Admitter;

pub(super) fn state_variable_name(admitter: &Admitter, segments: &[String], name: &str) -> String {
    if name.starts_with("state.") {
        return name.to_string();
    }
    if segments.len() == 1
        && admitter.states.contains_key(&segments[0])
        && !admitter.inputs.contains_key(&segments[0])
        && !admitter.params.contains_key(&segments[0])
        && !admitter.definitions.contains_key(&segments[0])
    {
        return format!("state.{}", segments[0]);
    }
    name.to_string()
}
