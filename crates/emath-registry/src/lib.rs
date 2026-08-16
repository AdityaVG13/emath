#![forbid(unsafe_code)]

//! Package/provider registry slice (P11).
//!
//! A std-only, deterministic index/lock model:
//!
//! - [`IndexSnapshot`] holds package name → version → [`PackageVersion`]
//!   records (kind schemas, provider descriptors, yanked/revoked state,
//!   license/security metadata, evidence summary, artifact links);
//! - [`IndexSnapshot::canonical_json`] renders a byte-stable document and
//!   [`IndexSnapshot::snapshot_id`] fingerprints it with the shared
//!   FNV-1a64 (`emath_core`);
//! - [`Resolver`] picks versions deterministically and refuses yanked or
//!   revoked pins; [`RegistryLock`] verifies that a pinned snapshot is
//!   reproducible offline;
//! - [`check_kind_schema`] / [`check_provider_capability`] emit typed
//!   compatibility diagnostics.
//!
//! Registry *services* are not implemented; this slice is the reproducible
//! index+lock core with tests on both sides of every refusal.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_core::fnv1a64_bytes;

/// Index document schema id.
pub const INDEX_SCHEMA: &str = "emath.registry-index.v1";
/// Lock document schema id.
pub const LOCK_SCHEMA: &str = "emath.registry-lock.v1";

/// One pinned version record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageVersion {
    /// Version string (compared lexicographically; documented limitation).
    pub version: String,
    /// Content id of the package source.
    pub content_id: String,
    /// Source location (path or URI).
    pub source_location: String,
    /// Custom-kind schemas this version provides.
    pub kind_schemas: Vec<String>,
    /// Provider capability ids this version can serve.
    pub provider_descriptors: Vec<String>,
    /// Yanked: removed from new resolution (typed refusal).
    pub yanked: bool,
    /// Revoked: withdrawn with prejudice (typed refusal).
    pub revoked: bool,
    /// Package license expression.
    pub license: String,
    /// Security notes (CVE ids or vendor advisories).
    pub security_notes: Vec<String>,
    /// Evidence summary pointer.
    pub evidence_summary: String,
    /// Artifact link (optional).
    pub artifact_link: Option<String>,
}

/// A reproducible registry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSnapshot {
    /// Package name → version string → record.
    pub packages: BTreeMap<String, BTreeMap<String, PackageVersion>>,
}

impl IndexSnapshot {
    /// An empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            packages: BTreeMap::new(),
        }
    }

    /// Renders the deterministic canonical JSON (sorted keys throughout).
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut out = String::from(r#"{"schema":"emath.registry-index.v1","packages":{"#);
        for (name, versions) in &self.packages {
            json_string(name, &mut out);
            out.push_str(":{");
            for (version, record) in versions {
                json_string(version, &mut out);
                out.push(':');
                record.write(&mut out);
                out.push(',');
            }
            if !versions.is_empty() {
                out.pop();
            }
            out.push_str("},");
        }
        if !self.packages.is_empty() {
            out.pop();
        }
        out.push_str("}}");
        out
    }

    /// FNV-1a64 fingerprint of the canonical JSON.
    #[must_use]
    pub fn snapshot_id(&self) -> u64 {
        fnv1a64_bytes(self.canonical_json().as_bytes())
    }

    /// Resolves `package` under `constraint`, refusing yanked/revoked pins.
    pub fn resolve(
        &self,
        package: &str,
        constraint: Constraint,
    ) -> Result<&PackageVersion, RegistryError> {
        let versions = self.packages.get(package).ok_or_else(|| {
            RegistryError::new("E-REG-020", format!("unknown package `{package}`"))
        })?;
        let usable = versions
            .iter()
            .filter(|(_, record)| !record.yanked && !record.revoked)
            .collect::<Vec<_>>();
        let chosen = match constraint {
            Constraint::Any => usable.iter().map(|(version, _)| (*version).clone()).max(),
            Constraint::Exact(version) => {
                if usable.iter().any(|(candidate, _)| *candidate == &version) {
                    Some(version)
                } else {
                    let refused = match versions.get(&version) {
                        Some(record) if record.yanked => Some("E-REG-022"),
                        Some(record) if record.revoked => Some("E-REG-023"),
                        _ => None,
                    };
                    if let Some(code) = refused {
                        return Err(RegistryError::new(
                            code,
                            format!("version `{version}` of `{package}` is unavailable"),
                        ));
                    }
                    None
                }
            }
            Constraint::Major(major) => usable
                .iter()
                .filter(|(version, _)| major_of(version) == Some(major))
                .map(|(version, _)| (*version).clone())
                .max(),
        };
        chosen
            .and_then(|version| versions.get(&version))
            .ok_or_else(|| {
                RegistryError::new(
                    "E-REG-024",
                    format!("no usable version of `{package}` satisfies the constraint"),
                )
            })
    }
}

impl Default for IndexSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Version selection constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Newest usable version.
    Any,
    /// A specific version.
    Exact(String),
    /// Newest usable version with this major number.
    Major(u64),
}

/// A reproducible lock: snapshot fingerprint + pinned versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryLock {
    /// Schema of the lock document.
    pub schema: String,
    /// Snapshot fingerprint this lock was built against.
    pub snapshot_id: u64,
    /// Package name → pinned version.
    pub pins: BTreeMap<String, String>,
}

impl RegistryLock {
    /// Builds a lock from a snapshot and explicit pins.
    #[must_use]
    pub fn from_pins(snapshot: &IndexSnapshot, pins: BTreeMap<String, String>) -> Self {
        Self {
            schema: LOCK_SCHEMA.into(),
            snapshot_id: snapshot.snapshot_id(),
            pins,
        }
    }

    /// Renders the deterministic canonical JSON.
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut out = format!(
            r#"{{"schema":"emath.registry-lock.v1","snapshot_id":"{}","pins":{{"#,
            self.snapshot_id
        );
        for (name, version) in &self.pins {
            json_string(name, &mut out);
            out.push(':');
            json_string(version, &mut out);
            out.push(',');
        }
        if !self.pins.is_empty() {
            out.pop();
        }
        out.push_str("}}");
        out
    }

    /// Verifies the lock against a snapshot: fingerprint must match and
    /// every pin must resolve. This is the "lock reproduces offline" gate.
    pub fn verify(&self, snapshot: &IndexSnapshot) -> Result<(), RegistryError> {
        if self.snapshot_id != snapshot.snapshot_id() {
            return Err(RegistryError::new(
                "E-REG-021",
                format!(
                    "lock snapshot {} does not match index {}",
                    self.snapshot_id,
                    snapshot.snapshot_id()
                ),
            ));
        }
        for (name, version) in &self.pins {
            snapshot
                .resolve(name, Constraint::Exact(version.clone()))
                .map_err(|error| {
                    RegistryError::new(
                        "E-REG-021",
                        format!("pin `{name}@{version}` does not resolve: {}", error.message),
                    )
                })?;
        }
        Ok(())
    }
}

/// Extracts the major component of a dotted version string.
fn major_of(version: &str) -> Option<u64> {
    version
        .split_once('.')
        .and_then(|(head, _)| head.parse::<u64>().ok())
}

/// A typed registry error; codes are stable (E-REG-0xx).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    /// Stable code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

impl RegistryError {
    fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }
}

/// Checks that `record` serves the required custom-kind schema.
pub fn check_kind_schema(record: &PackageVersion, required: &str) -> Result<(), RegistryError> {
    if record.kind_schemas.iter().any(|schema| schema == required) {
        Ok(())
    } else {
        Err(RegistryError::new(
            "E-REG-030",
            format!(
                "package version {} does not serve kind schema `{required}` (has {})",
                record.version,
                record.kind_schemas.join(", ")
            ),
        ))
    }
}

/// Checks that `record` serves the required provider capability.
pub fn check_provider_capability(
    record: &PackageVersion,
    required: &str,
) -> Result<(), RegistryError> {
    if record
        .provider_descriptors
        .iter()
        .any(|capability| capability == required)
    {
        Ok(())
    } else {
        Err(RegistryError::new(
            "E-REG-031",
            format!(
                "package version {} does not serve provider capability `{required}`",
                record.version
            ),
        ))
    }
}

impl PackageVersion {
    fn write(&self, out: &mut String) {
        out.push_str(r#"{"artifact_link":"#);
        match &self.artifact_link {
            Some(link) => json_string(link, out),
            None => out.push_str("null"),
        }
        out.push_str(r#","content_id":"#);
        json_string(&self.content_id, out);
        out.push_str(r#","evidence_summary":"#);
        json_string(&self.evidence_summary, out);
        out.push_str(r#","kind_schemas":["#);
        push_strings(&self.kind_schemas, out);
        out.push_str(r#"],"license":"#);
        json_string(&self.license, out);
        out.push_str(r#","provider_descriptors":["#);
        push_strings(&self.provider_descriptors, out);
        out.push_str(r#"],"revoked":"#);
        out.push_str(if self.revoked { "true" } else { "false" });
        out.push_str(r#","security_notes":["#);
        push_strings(&self.security_notes, out);
        out.push_str(r#"],"source_location":"#);
        json_string(&self.source_location, out);
        out.push_str(r#","version":"#);
        json_string(&self.version, out);
        out.push_str(r#","yanked":"#);
        out.push_str(if self.yanked { "true" } else { "false" });
        out.push('}');
    }
}

fn push_strings(values: &[String], out: &mut String) {
    for value in values {
        json_string(value, out);
        out.push(',');
    }
    if !values.is_empty() {
        out.pop();
    }
}

/// Renders a JSON string with the default escaping table.
fn json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(version: &str, yanked: bool, revoked: bool) -> PackageVersion {
        PackageVersion {
            version: version.into(),
            content_id: format!("fnv1a64:{}", fnv1a64_bytes(version.as_bytes())),
            source_location: format!("mirror/emath-fundamentals-{version}"),
            kind_schemas: vec!["emath.kind.scalar.v1".into(), "emath.kind.policy.v1".into()],
            provider_descriptors: vec!["native.rust".into(), "dew.rust".into()],
            yanked,
            revoked,
            license: "MIT OR Apache-2.0".into(),
            security_notes: vec![],
            evidence_summary: "evidence/emath-fundamentals.json".into(),
            artifact_link: None,
        }
    }

    fn snapshot() -> IndexSnapshot {
        let mut packages = BTreeMap::new();
        let mut versions = BTreeMap::new();
        versions.insert("0.1.0".into(), record("0.1.0", false, false));
        versions.insert("0.1.1".into(), record("0.1.1", false, false));
        versions.insert("0.2.0".into(), record("0.2.0", true, false));
        versions.insert("0.3.0".into(), record("0.3.0", false, true));
        packages.insert("emath-fundamentals".into(), versions);
        IndexSnapshot { packages }
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let first = snapshot();
        let second = snapshot();
        assert_eq!(first.canonical_json(), second.canonical_json());
        assert_eq!(first.snapshot_id(), second.snapshot_id());
    }

    #[test]
    fn resolve_any_picks_newest_usable() {
        let index = snapshot();
        let resolved = index
            .resolve("emath-fundamentals", Constraint::Any)
            .unwrap();
        assert_eq!(resolved.version, "0.1.1");
    }

    #[test]
    fn yanked_version_is_refused() {
        let error = snapshot()
            .resolve("emath-fundamentals", Constraint::Exact("0.2.0".into()))
            .unwrap_err();
        assert_eq!(error.code, "E-REG-022");
    }

    #[test]
    fn revoked_version_is_refused() {
        let error = snapshot()
            .resolve("emath-fundamentals", Constraint::Exact("0.3.0".into()))
            .unwrap_err();
        assert_eq!(error.code, "E-REG-023");
    }

    #[test]
    fn unknown_package_is_refused() {
        let error = snapshot().resolve("nope", Constraint::Any).unwrap_err();
        assert_eq!(error.code, "E-REG-020");
    }

    #[test]
    fn major_constraint_filters() {
        let index = snapshot();
        let resolved = index
            .resolve("emath-fundamentals", Constraint::Major(0))
            .unwrap();
        assert_eq!(resolved.version, "0.1.1");
    }

    #[test]
    fn unsatisfied_constraint_is_refused() {
        let error = snapshot()
            .resolve("emath-fundamentals", Constraint::Major(9))
            .unwrap_err();
        assert_eq!(error.code, "E-REG-024");
    }

    #[test]
    fn lock_verifies_and_detects_tampering() {
        let index = snapshot();
        let pins = BTreeMap::from([("emath-fundamentals".into(), "0.1.1".into())]);
        let lock = RegistryLock::from_pins(&index, pins.clone());
        assert!(lock.verify(&index).is_ok());

        let mut tampered = index.clone();
        tampered
            .packages
            .get_mut("emath-fundamentals")
            .expect("package")
            .insert("0.1.1".into(), record("0.1.1", true, false));
        let error = lock.verify(&tampered).unwrap_err();
        assert_eq!(error.code, "E-REG-021");

        let unrelated = IndexSnapshot::new();
        let error = lock.verify(&unrelated).unwrap_err();
        assert_eq!(error.code, "E-REG-021");
    }

    #[test]
    fn lock_document_round_trips_deterministically() {
        let index = snapshot();
        let pins = BTreeMap::from([("emath-fundamentals".into(), "0.1.1".into())]);
        let lock = RegistryLock::from_pins(&index, pins);
        let first = lock.canonical_json();
        let second = lock.canonical_json();
        assert_eq!(first, second);
        assert!(first.contains(INDEX_SCHEMA) || first.contains(LOCK_SCHEMA));
    }

    #[test]
    fn kind_schema_compat_positive_and_negative() {
        let record = record("0.1.1", false, false);
        assert!(check_kind_schema(&record, "emath.kind.scalar.v1").is_ok());
        let error = check_kind_schema(&record, "emath.kind.matrix.v1").unwrap_err();
        assert_eq!(error.code, "E-REG-030");
    }

    #[test]
    fn provider_capability_compat_positive_and_negative() {
        let record = record("0.1.1", false, false);
        assert!(check_provider_capability(&record, "native.rust").is_ok());
        let error = check_provider_capability(&record, "rumoca.structural").unwrap_err();
        assert_eq!(error.code, "E-REG-031");
    }

    #[test]
    fn snapshot_resolution_used_by_lock_is_offline() {
        // The lock verifies against the snapshot alone; no service call.
        let index = snapshot();
        let pins = BTreeMap::from([("emath-fundamentals".into(), "0.1.0".into())]);
        let lock = RegistryLock::from_pins(&index, pins);
        assert!(lock.verify(&index).is_ok());
    }
}
