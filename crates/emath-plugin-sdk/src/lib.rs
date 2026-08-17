#![forbid(unsafe_code)]

//! Plugin SDK slice: descriptors, sandbox policy decisions, and a
//! deterministic test-harness contract.
//!
//! A plugin is a component with a declared capability set and a sandbox
//! policy. This crate provides:
//!
//! - [`PluginDescriptor`] (schema `emath.plugin`) with a canonical JSON
//!   rendering and FNV-1a64 content id;
//! - [`admit`]: the sandbox/fuel/permission gate. Untrusted descriptors
//!   must declare positive fuel; `network` requires the `network`
//!   permission; every capability must be inside `allowed_capabilities`.
//!   Refusals are typed (`E-PLG-002`, `E-PLG-003`) and deterministic;
//! - [`execute`]: the harness entry. The Phase 1 subset has no component
//!   runtime, so execution is a typed refusal (`E-PLG-001`); the shape of
//!   the call (`descriptor, input -> output`) is the stable contract that
//!   the Phase 2+ runtime will fill. Execution re-enforces positive fuel
//!   under every trust class (`E-PLG-002`), so `Trust::Local` can never
//!   admit an unmetered plugin onto an execution path;
//! - [`compatible`]: interface-core compatibility check.
//!
//! No network, no component host, std-only.

use std::fmt::Write as _;

use emath_core::content_id_of_str;

/// Descriptor document schema.
pub const PLUGIN_SCHEMA: &str = "emath.plugin";
/// The single interface core this SDK slice speaks.
pub const INTERFACE_CORE: &str = "emath.plugin.interface";

/// Sandbox policy attached to a plugin descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SandboxPolicy {
    /// Fuel budget: `None` means "unmetered". [`admit`] tolerates an
    /// unmetered [`Trust::Local`] descriptor, but [`execute`] requires
    /// positive fuel under every trust class (`E-PLG-002`).
    pub fuel: Option<u64>,
    /// Granted permissions (e.g. `"network"`, `"fs-read"`).
    pub permissions: Vec<String>,
    /// Whether the plugin may open network connections.
    pub network: bool,
    /// Capabilities the sandbox will allow the plugin to exercise.
    pub allowed_capabilities: Vec<String>,
}

/// A plugin descriptor (`emath.plugin`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginDescriptor {
    /// Stable plugin id.
    pub id: String,
    /// Provider kind served by this plugin.
    pub kind: String,
    /// Interface core id.
    pub interface_core: String,
    /// Declared capabilities (each must be sandbox-allowed).
    pub capabilities: Vec<String>,
    /// Sandbox policy.
    pub sandbox: SandboxPolicy,
}

/// Trust classification of a plugin source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trust {
    /// First-party / locally audited; admission tolerates an unmetered
    /// sandbox (execution still requires positive fuel).
    Local,
    /// Third-party; must declare positive fuel.
    Untrusted,
}

/// A plugin result (bytes): the runtime contract.
pub type PluginOutput = Vec<u8>;

/// A typed plugin error; codes are stable (E-PLG-0xx).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginError {
    /// Stable code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

impl PluginError {
    fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }
}

impl PluginDescriptor {
    /// Renders the deterministic canonical JSON.
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut out = String::from(r#"{"capabilities":["#);
        push_strings(&self.capabilities, &mut out);
        out.push_str(r#"],"id":"#);
        json_string(&self.id, &mut out);
        out.push_str(r#","interface_core":"#);
        json_string(&self.interface_core, &mut out);
        out.push_str(r#","kind":"#);
        json_string(&self.kind, &mut out);
        out.push_str(r#","sandbox":{"allowed_capabilities":["#);
        push_strings(&self.sandbox.allowed_capabilities, &mut out);
        out.push_str(r#"],"fuel":"#);
        match self.sandbox.fuel {
            Some(fuel) => out.push_str(&fuel.to_string()),
            None => out.push_str("null"),
        }
        out.push_str(r#","network":"#);
        out.push_str(if self.sandbox.network {
            "true"
        } else {
            "false"
        });
        out.push_str(r#","permissions":["#);
        push_strings(&self.sandbox.permissions, &mut out);
        out.push_str(r#"]},"schema":"emath.plugin"}"#);
        out
    }

    /// FNV-1a64 content id of the canonical JSON (shared convention).
    #[must_use]
    pub fn content_id(&self) -> String {
        content_id_of_str(&self.canonical_json()).0
    }
}

/// The admission gate: sandbox/fuel/permission decision.
///
/// Refusals: `E-PLG-002` (sandbox violation), `E-PLG-003` (capability
/// outside the allowed set or none declared).
pub fn admit(descriptor: &PluginDescriptor, trust: Trust) -> Result<(), PluginError> {
    if descriptor.capabilities.is_empty() {
        return Err(PluginError::new(
            "E-PLG-003",
            format!("plugin `{}` declares no capabilities", descriptor.id),
        ));
    }
    for capability in &descriptor.capabilities {
        if !descriptor
            .sandbox
            .allowed_capabilities
            .iter()
            .any(|allowed| allowed == capability)
        {
            return Err(PluginError::new(
                "E-PLG-003",
                format!(
                    "plugin `{}` declares capability `{capability}` outside its allowed set",
                    descriptor.id
                ),
            ));
        }
    }
    // A capability that touches a resource class requires the matching
    // granted permission: "fs-read" in permissions is
    // enforced against fs-class capabilities, "network" against
    // network-class ones. A declared permission is only as good as the
    // gate that enforces it.
    for capability in &descriptor.capabilities {
        let required = if capability == "fs-read" || capability.starts_with("fs:") {
            Some("fs-read")
        } else if capability == "network" || capability.starts_with("net:") {
            Some("network")
        } else {
            None
        };
        if let Some(required) = required {
            if !descriptor
                .sandbox
                .permissions
                .iter()
                .any(|permission| permission == required)
            {
                return Err(PluginError::new(
                    "E-PLG-002",
                    format!(
                        "plugin `{}` declares capability `{capability}` without the `{required}` permission",
                        descriptor.id
                    ),
                ));
            }
        }
    }
    if descriptor.sandbox.network
        && !descriptor
            .sandbox
            .permissions
            .iter()
            .any(|permission| permission == "network")
    {
        return Err(PluginError::new(
            "E-PLG-002",
            format!(
                "plugin `{}` declares network access without the `network` permission",
                descriptor.id
            ),
        ));
    }
    if trust == Trust::Untrusted {
        match descriptor.sandbox.fuel {
            Some(fuel) if fuel > 0 => {}
            _ => {
                return Err(PluginError::new(
                    "E-PLG-002",
                    format!(
                        "untrusted plugin `{}` must declare positive fuel",
                        descriptor.id
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// The harness entry point (deterministic contract).
///
/// Phase 1 has no component runtime, so every execution is a typed refusal
/// (`E-PLG-001`); the signature is the stable surface the Phase 2+ runtime
/// will implement. Output never bypasses [`admit`] — and the fuel gate is
/// re-enforced here under every trust class: claiming [`Trust::Local`]
/// cannot skip it, so a fuel-less descriptor hits `E-PLG-002` before
/// `E-PLG-001`. Phase 2 cannot inherit a gate-less execution path.
pub fn execute(
    descriptor: &PluginDescriptor,
    _input: &[u8],
    trust: Trust,
) -> Result<PluginOutput, PluginError> {
    admit(descriptor, trust)?;
    match descriptor.sandbox.fuel {
        Some(fuel) if fuel > 0 => {}
        _ => {
            return Err(PluginError::new(
                "E-PLG-002",
                format!(
                    "plugin `{}` cannot execute without positive fuel",
                    descriptor.id
                ),
            ));
        }
    }
    Err(PluginError::new(
        "E-PLG-001",
        format!(
            "plugin `{}` cannot execute: component runtime absent in the Phase 1 subset",
            descriptor.id
        ),
    ))
}

/// Interface-core compatibility: the plugin must speak the SDK's interface.
pub fn compatible(descriptor: &PluginDescriptor, expected_core: &str) -> Result<(), PluginError> {
    if descriptor.interface_core == expected_core {
        Ok(())
    } else {
        Err(PluginError::new(
            "E-PLG-004",
            format!(
                "plugin `{}` speaks interface `{}`, expected `{expected_core}`",
                descriptor.id, descriptor.interface_core
            ),
        ))
    }
}

/// Builds a descriptor with the SDK interface core.
#[must_use]
pub fn descriptor_for(
    id: &str,
    kind: &str,
    capabilities: Vec<String>,
    sandbox: SandboxPolicy,
) -> PluginDescriptor {
    PluginDescriptor {
        id: id.into(),
        kind: kind.into(),
        interface_core: INTERFACE_CORE.into(),
        capabilities,
        sandbox,
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
