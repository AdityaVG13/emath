//! Compact runtime-table projection from Feature Capsules.

use std::collections::{BTreeMap, BTreeSet};

use emath_core::{CanonicalField, DistributionHash, FeatureId};
use emath_ir::{CapsuleSlot, FeatureCapsule, FeatureClass};

pub const GENERATED_HEADER: &str = "# @generated from Feature Capsules; DO NOT EDIT\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTableEntry {
    pub feature_id: FeatureId,
    pub capsule_hash: String,
    pub source: String,
    pub handle: String,
    pub aliases: Vec<String>,
    pub precedence: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTables {
    pub tables: BTreeMap<String, Vec<RuntimeTableEntry>>,
    pub bytes: String,
    pub lock: DistributionHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableError {
    DuplicateFeature(FeatureId),
    AliasCollision(String),
    PrecedenceAmbiguity(String),
    ConfusableCollision(String),
    MissingHandle(FeatureId),
    StaleLock,
    UnsafeGeneratedText,
}

pub fn generate_runtime_tables(capsules: &[FeatureCapsule]) -> Result<RuntimeTables, TableError> {
    let mut seen = BTreeSet::new();
    let mut aliases = BTreeMap::<String, FeatureId>::new();
    let mut table = BTreeMap::<String, Vec<RuntimeTableEntry>>::new();
    for capsule in capsules {
        if !seen.insert(capsule.feature_id.clone()) {
            return Err(TableError::DuplicateFeature(capsule.feature_id.clone()));
        }
        let category = category(capsule.class).to_string();
        let handle = slot(capsule, "semantics").unwrap_or_default();
        if handle.is_empty() {
            let active = slot(capsule, "authority_target").as_deref() == Some("capsule-active");
            if capsule.has_blocking_hole() && !active {
                continue;
            }
            return Err(TableError::MissingHandle(capsule.feature_id.clone()));
        }
        if capsule.class == FeatureClass::Provider
            && (handle.contains("::") || handle.contains('<'))
        {
            return Err(TableError::UnsafeGeneratedText);
        }
        let entry_aliases = slot(capsule, "presentation").map_or_else(Vec::new, |text| {
            text.strip_prefix("aliases=")
                .map_or_else(Vec::new, |aliases| {
                    aliases
                        .split(',')
                        .map(|item| item.trim().to_string())
                        .filter(|item| !item.is_empty())
                        .collect::<Vec<_>>()
                })
        });
        for alias in &entry_aliases {
            if let Some(existing) = aliases.insert(alias.clone(), capsule.feature_id.clone()) {
                if existing != capsule.feature_id {
                    return Err(TableError::AliasCollision(alias.clone()));
                }
            }
        }
        let precedence = slot(capsule, "surface").and_then(|surface| {
            surface.split(';').find_map(|part| {
                part.trim()
                    .strip_prefix("precedence=")
                    .and_then(|number| number.parse::<u16>().ok())
            })
        });
        if precedence.is_some() && entry_aliases.is_empty() {
            return Err(TableError::PrecedenceAmbiguity(
                capsule.feature_id.to_string(),
            ));
        }
        let folded = confusable_fold(capsule.feature_id.as_str());
        if seen
            .iter()
            .any(|other| other != &capsule.feature_id && confusable_fold(other.as_str()) == folded)
        {
            return Err(TableError::ConfusableCollision(
                capsule.feature_id.to_string(),
            ));
        }
        table.entry(category).or_default().push(RuntimeTableEntry {
            feature_id: capsule.feature_id.clone(),
            capsule_hash: capsule.semantic_hash.to_string(),
            source: capsule.source.clone(),
            handle,
            aliases: entry_aliases,
            precedence,
        });
    }
    for entries in table.values_mut() {
        entries.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    }
    let bytes = encode(&table);
    let lock = DistributionHash::new(&[CanonicalField::new("image", bytes.as_bytes())
        .expect("generated table field name is fixed")])
    .expect("generated table has no forbidden fields");
    Ok(RuntimeTables {
        tables: table,
        bytes,
        lock,
    })
}

impl RuntimeTables {
    pub fn verify(&self) -> Result<(), TableError> {
        if !self.bytes.starts_with(GENERATED_HEADER) {
            return Err(TableError::UnsafeGeneratedText);
        }
        let computed =
            DistributionHash::new(&[
                CanonicalField::new("image", self.bytes.as_bytes()).expect("fixed field")
            ])
            .expect("fixed domain");
        if computed != self.lock {
            return Err(TableError::StaleLock);
        }
        if self.bytes.contains("unsafe {") || self.bytes.contains("match feature_id") {
            return Err(TableError::UnsafeGeneratedText);
        }
        Ok(())
    }

    #[must_use]
    pub fn lookup(&self, table: &str, id: &FeatureId) -> Option<&RuntimeTableEntry> {
        self.tables
            .get(table)?
            .iter()
            .find(|entry| &entry.feature_id == id)
    }
}

fn encode(tables: &BTreeMap<String, Vec<RuntimeTableEntry>>) -> String {
    let mut output = GENERATED_HEADER.to_string();
    for (name, entries) in tables {
        output.push_str(&format!("table {name}\n"));
        for entry in entries {
            output.push_str(&format!(
                "{} hash={} source={} handle={} aliases={} precedence={}\n",
                entry.feature_id,
                entry.capsule_hash,
                entry.source,
                entry.handle,
                entry.aliases.join(","),
                entry
                    .precedence
                    .map_or_else(|| "-".to_string(), |value| value.to_string()),
            ));
        }
    }
    output
}

fn slot(capsule: &FeatureCapsule, name: &str) -> Option<String> {
    capsule.slots.get(name).and_then(|slot| match slot {
        CapsuleSlot::Value(value) => Some(value.clone()),
        _ => None,
    })
}

const fn category(class: FeatureClass) -> &'static str {
    match class {
        FeatureClass::Symbol | FeatureClass::Surface | FeatureClass::Syntax => "symbols",
        FeatureClass::Binder => "binders",
        FeatureClass::Kind | FeatureClass::Section => "kinds-sections",
        FeatureClass::Diagnostic => "diagnostics",
        FeatureClass::World => "worlds",
        FeatureClass::Provider => "providers",
        _ => "capabilities",
    }
}

fn confusable_fold(value: &str) -> String {
    value.replace(['α', 'а'], "a").replace(['ο', 'о'], "o")
}
