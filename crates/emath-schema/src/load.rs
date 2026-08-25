//! Kind package loading.
//!
//! Resolves identity/version from package locks and verifies content
//! identity. Refusals: `E-KIND-030/031/032`, `E-PKG-020`.

use emath_ir::kind_schema::KindSchema;

/// Maximum depth of a kind expansion without recursion.
pub const MAX_EXPANSION_DEPTH: usize = 16;

/// A resolved kind package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindPackage {
    pub package: String,
    pub name: String,
    pub version: String,
    /// Content identity (FNV-1a64 over the canonical schema).
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandTrace {
    /// Stack of kind ids being expanded (`name@version`).
    pub stack: Vec<String>,
}

/// Version policy for schema compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionPolicy {
    SemverMajor,
    Exact,
}

impl VersionPolicy {
    /// Whether `found` satisfies `required`.
    #[must_use]
    pub fn accepts(self, found: &str, required: &str) -> bool {
        match self {
            // Unparseable versions never satisfy a SemverMajor gate:
            // `nightly`/`local` are not "compatible with any major".
            Self::SemverMajor => match (major_of(found), major_of(required)) {
                (Some(found), Some(required)) => found == required,
                _ => false,
            },
            Self::Exact => found == required,
        }
    }
}

fn major_of(version: &str) -> Option<u64> {
    version.split('.').next().and_then(|part| part.parse().ok())
}

/// One kind resolution refusal (stable codes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveIssue {
    UnknownKind { name: String },
    ChecksumMismatch {
        name: String,
        found: String,
        computed: String,
    },
    VersionMismatch {
        name: String,
        found: String,
        required: String,
    },
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

/// Resolves a kind from a lock and verifies its identity. `deps` recovers
/// kind dependencies (lock shape is caller-defined) for the recursion guard.
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

/// One locked kind entry (shaped after `emath.lock` packages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindLockEntry {
    pub package: String,
    pub name: String,
    pub version: String,
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
