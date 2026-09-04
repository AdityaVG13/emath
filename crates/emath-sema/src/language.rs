use std::cell::RefCell;

use emath_exec_ir::language_image::LanguageDistribution;
use emath_ir::{CapsuleSlot, FeatureClass};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageBinding {
    pub feature_id: String,
    pub aliases: Vec<String>,
    pub arity: Option<usize>,
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub diagnostic: Option<String>,
}

thread_local! {
    static LANGUAGE_BINDINGS: RefCell<Vec<LanguageBinding>> = const { RefCell::new(Vec::new()) };
}

pub fn install_language_distribution(
    distribution: &LanguageDistribution,
) -> Result<(), emath_exec_ir::native_kernel::KernelBindingError> {
    emath_exec_ir::native_kernel::install_language_distribution(distribution)?;
    let mut bindings = Vec::new();
    for capsule in &distribution.capsules {
        let active = distribution
            .authority
            .entries
            .get(&capsule.feature_id)
            .is_some_and(|entry| entry.state.as_str() == "capsule-active");
        if !active || capsule.class != FeatureClass::Capability {
            continue;
        }
        let semantics = slot(capsule, "semantics").unwrap_or_default();
        let aliases = slot(capsule, "presentation")
            .and_then(|value| value.strip_prefix("aliases="))
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|alias| !alias.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        bindings.push(LanguageBinding {
            feature_id: capsule.feature_id.to_string(),
            aliases,
            arity: semantic_field(semantics, "arity").and_then(|value| value.parse().ok()),
            inputs: semantic_field(semantics, "inputs")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            output: semantic_field(semantics, "output").map(str::to_string),
            diagnostic: semantic_field(semantics, "diagnostic").map(str::to_string),
        });
    }
    bindings.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    LANGUAGE_BINDINGS.with(|installed| *installed.borrow_mut() = bindings);
    Ok(())
}

pub(crate) fn language_bindings() -> Vec<LanguageBinding> {
    LANGUAGE_BINDINGS.with(|installed| installed.borrow().clone())
}

fn slot<'a>(capsule: &'a emath_ir::FeatureCapsule, name: &str) -> Option<&'a str> {
    match capsule.slots.get(name) {
        Some(CapsuleSlot::Value(value)) => Some(value),
        _ => None,
    }
}

fn semantic_field<'a>(semantics: &'a str, field: &str) -> Option<&'a str> {
    semantics
        .split(';')
        .find_map(|part| part.trim().strip_prefix(field)?.strip_prefix('='))
}
