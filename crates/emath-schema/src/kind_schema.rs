//! Kind schema bedrock (core subset): which sections are required,
//! optional or repeatable, each section's payload policy (`suite`,
//! `field`, `command`), defaults, and a static predicate. The schema is
//! the admission surface builder and compiler share; lowering is not
//! in this crate.

use std::collections::BTreeMap;

/// Repeat policy for a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RepeatPolicy {
    ExactlyOne,
    AtMostOne,
    Repeatable,
}

impl RepeatPolicy {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactlyOne => "exactly-one",
            Self::AtMostOne => "at-most-one",
            Self::Repeatable => "repeatable",
        }
    }
}

/// Payload policy for a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PayloadPolicy {
    /// Section body is a suite of statements.
    Suite,
    /// Section body is a suite of `name: Type` declarations.
    Fields,
    /// Section body is a suite of commands.
    Commands,
}

impl PayloadPolicy {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suite => "suite",
            Self::Fields => "fields",
            Self::Commands => "commands",
        }
    }
}

/// Declared kind plus its core section ground truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreKind {
    /// Kind name (`object`, `function`, `query`).
    pub name: String,
}

/// Schema for one section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionSchema {
    /// Required, optional or repeatable.
    pub repeat: RepeatPolicy,
    /// Payload policy.
    pub payload: PayloadPolicy,
    /// Whether an absent section gets a defaulted value.
    pub has_default: bool,
}

/// Kind schema (core subset).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindSchema {
    name: String,
    sections: BTreeMap<String, SectionSchema>,
    defaults: BTreeMap<String, String>,
    predicate: Option<String>,
}

impl KindSchema {
    /// Frozen core object schema (the constitution).
    #[must_use]
    pub fn core_object() -> Self {
        Self {
            name: "object".into(),
            sections: BTreeMap::from([
                (
                    "parameters".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Fields,
                        has_default: false,
                    },
                ),
                (
                    "representation".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "invariants".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "tests".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "exports".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Commands,
                        has_default: false,
                    },
                ),
                (
                    "about".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Commands,
                        has_default: false,
                    },
                ),
            ]),
            defaults: BTreeMap::new(),
            predicate: None,
        }
    }

    /// Frozen core query schema (the constitution).
    #[must_use]
    pub fn core_query() -> Self {
        Self {
            name: "query".into(),
            sections: BTreeMap::from([
                (
                    "inputs".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Fields,
                        has_default: false,
                    },
                ),
                (
                    "definitions".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "question".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "using".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "answer".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "budget".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "tests".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "exports".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Commands,
                        has_default: false,
                    },
                ),
            ]),
            defaults: BTreeMap::new(),
            predicate: None,
        }
    }

    /// Frozen core function schema.
    #[must_use]
    pub fn core_function() -> Self {
        Self {
            name: "function".into(),
            sections: BTreeMap::from([
                (
                    "parameters".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Fields,
                        has_default: false,
                    },
                ),
                (
                    "inputs".into(),
                    SectionSchema {
                        // Constitution required, and may be empty.
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Fields,
                        has_default: false,
                    },
                ),
                (
                    "outputs".into(),
                    SectionSchema {
                        // Constitution required named typed results.
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Fields,
                        has_default: false,
                    },
                ),
                (
                    "definitions".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::ExactlyOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
                (
                    "exports".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Commands,
                        has_default: false,
                    },
                ),
                (
                    "tests".into(),
                    SectionSchema {
                        repeat: RepeatPolicy::AtMostOne,
                        payload: PayloadPolicy::Suite,
                        has_default: false,
                    },
                ),
            ]),
            defaults: BTreeMap::new(),
            predicate: None,
        }
    }

    /// Not a constructor kind. Kept so historical call sites compile;
    /// the returned schema has no kind name and no sections.
    #[must_use]
    pub fn core_policy() -> Self {
        Self::default()
    }

    /// Not a constructor kind. Kept so historical call sites compile;
    /// the returned schema has no kind name and no sections.
    #[must_use]
    pub fn core_model() -> Self {
        Self::default()
    }

    /// Section schema; absent = unknown section (caller refuses).
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&SectionSchema> {
        self.sections.get(name)
    }

    /// Removes a section (used by restricted lowering renames).
    pub fn remove_section(&mut self, name: &str) {
        self.sections.remove(name);
    }

    /// Sections in deterministic order.
    #[must_use]
    pub fn sections(&self) -> Vec<(&str, &SectionSchema)> {
        self.sections.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Declared default for a section.
    #[must_use]
    pub fn default_for(&self, section: &str) -> Option<&str> {
        self.defaults.get(section).map(String::as_str)
    }

    /// Schema name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets the schema name (schema-language `kind <name>`).
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Deterministic identity over the schema (schema mutation moves
    /// identity; acceptance gate).
    #[must_use]
    pub fn canonical(&self) -> String {
        let sections: Vec<String> = self
            .sections
            .iter()
            .map(|(name, schema)| {
                format!(
                    "{}:{}:{}{}",
                    name,
                    schema.repeat.as_str(),
                    schema.payload.as_str(),
                    if schema.has_default { ":default" } else { "" }
                )
            })
            .collect();
        let defaults: Vec<String> = self
            .defaults
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        format!(
            "kind-schema:{}:[{}]:[{}]:{}",
            self.name,
            sections.join(";"),
            defaults.join(";"),
            self.predicate.clone().unwrap_or_default()
        )
    }

    /// Mutable sections (builder admission surface).
    pub fn insert_section(&mut self, name: impl Into<String>, schema: SectionSchema) {
        self.sections.insert(name.into(), schema);
    }

    /// Mutable default.
    pub fn insert_default(&mut self, section: impl Into<String>, value: impl Into<String>) {
        self.defaults.insert(section.into(), value.into());
    }

    /// Removes a default entry (used when renaming migrates provenance keys).
    pub fn remove_default(&mut self, section: &str) {
        self.defaults.remove(section);
    }

    /// Set the static predicate.
    pub fn set_predicate(&mut self, predicate: impl Into<String>) {
        self.predicate = Some(predicate.into());
    }
}

/// Payload policy expert: which statements are allowed in a section.
#[must_use]
pub fn payload_allows(payload: PayloadPolicy, statement: &str) -> bool {
    match payload {
        PayloadPolicy::Fields => statement == "field",
        PayloadPolicy::Commands => statement == "command",
        PayloadPolicy::Suite => statement == "suite",
    }
}

/// Free-function accessors matching the crate re-exports.
#[must_use]
pub fn core_function_schema() -> KindSchema {
    KindSchema::core_function()
}

#[must_use]
pub fn core_object_schema() -> KindSchema {
    KindSchema::core_object()
}

#[must_use]
pub fn core_query_schema() -> KindSchema {
    KindSchema::core_query()
}

#[must_use]
pub fn core_policy_schema() -> KindSchema {
    KindSchema::core_policy()
}

#[must_use]
pub fn core_model_schema() -> KindSchema {
    KindSchema::core_model()
}
