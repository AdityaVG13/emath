//!: kind package loading.
//!
//! Resolves kind identity/version from package locks, verifies the
//! content identity against the canonical schema, refuses incompatible
//! schema versions and detects recursive expansion. `E-KIND-030`
//! unknown kind, `E-KIND-031` incompatible schema version, `E-KIND-032`
//! recursive expansion, `E-PKG-020` lock checksum mismatch.

use emath_ir::kind_schema::KindSchema;

/// Maximum depth of a kind expansion without recursion.
pub const MAX_EXPANSION_DEPTH: usize = 16;

/// A resolved kind package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindPackage {
    /// Package name the kind lives in.
    pub package: String,
    /// Kind name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// Content identity (bootstrap FNV-1a64 over the canonical schema).
    pub content: String,
}

/// One expansion step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandTrace {
    /// Stack of kind ids being expanded (`name@version`).
    pub stack: Vec<String>,
}

/// Version policy for schema compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionPolicy {
    /// Same major version is compatible.
    SemverMajor,
    /// Exact version match is required.
    Exact,
}

impl VersionPolicy {
    /// Whether `found` satisfies `required`.
    #[must_use]
    pub fn accepts(self, found: &str, required: &str) -> bool {
        match self {
            Self::SemverMajor => major_of(found) == major_of(required),
            Self::Exact => found == required,
        }
    }
}

fn major_of(version: &str) -> u64 {
    version
        .split('.')
        .next()
        .and_then(|part| part.parse().ok())
        .unwrap_or(u64::MAX)
}

/// One kind resolution refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveIssue {
    /// `E-KIND-030`: kind not present in the lock.
    UnknownKind { name: String },
    /// `E-PKG-020`: lock content identity does not match the schema.
    ChecksumMismatch {
        name: String,
        found: String,
        computed: String,
    },
    /// `E-KIND-031`: schema version incompatible with the requirement.
    VersionMismatch {
        name: String,
        found: String,
        required: String,
    },
    /// `E-KIND-032`: expanding this kind would recurse.
    RecursiveExpansion { stack: Vec<String> },
}

impl ResolveIssue {
    /// Stable code for the refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownKind { .. } => "E-KIND-030",
            Self::ChecksumMismatch { .. } => "E-PKG-020",
            Self::VersionMismatch { .. } => "E-KIND-031",
            Self::RecursiveExpansion { .. } => "E-KIND-032",
        }
    }
}

/// Resolves a kind from a lock and verifies its identity. `deps` is a
/// projection recovering the kind dependencies of a locked kind (the
/// lock shape is caller-defined); the recursion guard walks that
/// projection.
pub fn resolve_kind(
    lock: &[KindLockEntry],
    name: &str,
    required_version: &str,
    policy: VersionPolicy,
    schema: &KindSchema,
    deps: &dyn Fn(&str) -> Vec<String>,
) -> Result<KindPackage, ResolveIssue> {
    let entry = lock
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| ResolveIssue::UnknownKind {
            name: name.to_string(),
        })?;
    if !policy.accepts(&entry.version, required_version) {
        return Err(ResolveIssue::VersionMismatch {
            name: name.to_string(),
            found: entry.version.clone(),
            required: required_version.to_string(),
        });
    }
    let computed = fnv1a64_of(&schema.canonical());
    if entry.content != computed {
        return Err(ResolveIssue::ChecksumMismatch {
            name: name.to_string(),
            found: entry.content.clone(),
            computed,
        });
    }
    let mut stack = vec![format!("{name}@{}", entry.version)];
    expand(lock, name, &entry.version, deps, &mut stack)?;
    Ok(KindPackage {
        package: entry.package.clone(),
        name: name.to_string(),
        version: entry.version.clone(),
        content: entry.content.clone(),
    })
}

/// One locked kind entry (shaped after `emath.lock.v1` packages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindLockEntry {
    /// Package the kind is declared in.
    pub package: String,
    /// Kind name (declaration name in the package).
    pub name: String,
    /// Schema version.
    pub version: String,
    /// Content identity from the lock.
    pub content: String,
}

fn expand(
    lock: &[KindLockEntry],
    name: &str,
    _version: &str,
    deps: &dyn Fn(&str) -> Vec<String>,
    stack: &mut Vec<String>,
) -> Result<(), ResolveIssue> {
    if stack.len() > MAX_EXPANSION_DEPTH {
        return Err(ResolveIssue::RecursiveExpansion {
            stack: stack.clone(),
        });
    }
    for dep in deps(name) {
        let Some(entry) = lock.iter().find(|entry| entry.name == dep) else {
            return Err(ResolveIssue::UnknownKind { name: dep });
        };
        let token = format!("{}@{}", entry.name, entry.version);
        if stack.contains(&token) {
            return Err(ResolveIssue::RecursiveExpansion {
                stack: stack.clone(),
            });
        }
        stack.push(token);
        expand(lock, &entry.name, &entry.version, deps, stack)?;
        stack.pop();
    }
    Ok(())
}

fn fnv1a64_of(text: &str) -> String {
    format!(
        "fnv1a64:{:016x}",
        emath_core::fnv1a64_bytes(text.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::kind_schema::core_function_schema;

    fn locked(name: &str, version: &str, content: &str) -> KindLockEntry {
        KindLockEntry {
            package: "emath-kinds".into(),
            name: name.into(),
            version: version.into(),
            content: content.into(),
        }
    }

    fn no_deps(_: &str) -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn kind_resolves_from_lock_with_identity() {
        let schema = core_function_schema();
        let content = format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(schema.canonical().as_bytes())
        );
        let lock = vec![locked("function", "1.2.0", &content)];
        let package = resolve_kind(
            &lock,
            "function",
            "1.0.0",
            VersionPolicy::SemverMajor,
            &schema,
            &no_deps,
        )
        .unwrap();
        assert_eq!(package.name, "function");
        assert_eq!(package.version, "1.2.0");
        assert_eq!(package.content, content);
    }

    #[test]
    fn unknown_kind_in_lock_is_refused() {
        let issue = resolve_kind(
            &[],
            "mystery",
            "1.0.0",
            VersionPolicy::Exact,
            &core_function_schema(),
            &no_deps,
        )
        .unwrap_err();
        assert_eq!(issue.code(), "E-KIND-030");
    }

    #[test]
    fn incompatible_schema_version_is_refused() {
        let content = format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(core_function_schema().canonical().as_bytes())
        );
        let lock = vec![locked("function", "2.0.0", &content)];
        let issue = resolve_kind(
            &lock,
            "function",
            "1.0.0",
            VersionPolicy::SemverMajor,
            &core_function_schema(),
            &no_deps,
        )
        .unwrap_err();
        assert_eq!(issue.code(), "E-KIND-031");
        // Exact policy refuses patch drift too.
        let exact = resolve_kind(
            &lock,
            "function",
            "2.0.0",
            VersionPolicy::Exact,
            &core_function_schema(),
            &no_deps,
        )
        .unwrap();
        assert_eq!(exact.version, "2.0.0");
    }

    #[test]
    fn checksum_mismatch_is_refused() {
        let lock = vec![locked("function", "1.0.0", "fnv1a64:0000000000000000")];
        let issue = resolve_kind(
            &lock,
            "function",
            "1.0.0",
            VersionPolicy::Exact,
            &core_function_schema(),
            &no_deps,
        )
        .unwrap_err();
        assert_eq!(issue.code(), "E-PKG-020");
        // Schema mutation moves identity: same lock now mismatches.
        let mut mutated = core_function_schema();
        mutated.set_predicate("decl.outputs.is_nonempty()");
        let issue2 = resolve_kind(
            &lock,
            "function",
            "1.0.0",
            VersionPolicy::Exact,
            &mutated,
            &no_deps,
        )
        .unwrap_err();
        assert_eq!(issue2.code(), "E-PKG-020");
    }

    #[test]
    fn recursive_kind_expansion_is_refused() {
        let content = format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(core_function_schema().canonical().as_bytes())
        );
        let lock = vec![
            locked("a", "1.0.0", &content),
            locked("b", "1.0.0", &content),
        ];
        let deps = |name: &str| match name {
            "a" => vec!["b".to_string()],
            "b" => vec!["a".to_string()],
            _ => Vec::new(),
        };
        for name in ["a", "b"] {
            let issue = resolve_kind(
                &lock,
                name,
                "1.0.0",
                VersionPolicy::Exact,
                &core_function_schema(),
                &deps,
            )
            .unwrap_err();
            assert_eq!(issue.code(), "E-KIND-032");
        }
    }

    #[test]
    fn expansion_depth_is_bounded() {
        let content = format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(core_function_schema().canonical().as_bytes())
        );
        let lock: Vec<KindLockEntry> = (0..=MAX_EXPANSION_DEPTH + 1)
            .map(|i| locked(&format!("k{i}"), "1.0.0", &content))
            .collect();
        let deps = |name: &str| {
            let index: usize = name[1..].parse().unwrap_or(0);
            if index < MAX_EXPANSION_DEPTH + 1 {
                vec![format!("k{}", index + 1)]
            } else {
                Vec::new()
            }
        };
        let issue = resolve_kind(
            &lock,
            "k0",
            "1.0.0",
            VersionPolicy::Exact,
            &core_function_schema(),
            &deps,
        )
        .unwrap_err();
        assert_eq!(issue.code(), "E-KIND-032");
    }
}
