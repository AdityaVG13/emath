//!: generated crate profiles.
//!
//! Profiles: library, binary, `no_std`, `wasm/component`, FFI, provider
//! plugin and host patch/candidate. Each profile determines the manifest
//! fragment, crate layout and std requirement. Permitted profiles refuse
//! unsafe code (`E-CODEGEN-002`); unknown profiles are typed refusals
//! (`E-CODEGEN-003`).

use crate::ast::{FnDef, Item, Module, Stmt};
use crate::render::coverage_gaps;

/// Generated crate profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CrateProfile {
    /// `[lib]` rlib.
    Library,
    /// `[[bin]]` with `src/main.rs`.
    Binary,
    /// `#![no_std]` library.
    NoStd,
    /// `wasm/component` cdylib.
    WasmComponent,
    /// C FFI cdylib.
    Ffi,
    /// Provider plugin dylib/cdylib pair.
    ProviderPlugin,
    /// Host patch/candidate crate.
    HostPatchCandidate,
}

impl CrateProfile {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Binary => "binary",
            Self::NoStd => "no_std",
            Self::WasmComponent => "wasm/component",
            Self::Ffi => "ffi",
            Self::ProviderPlugin => "provider-plugin",
            Self::HostPatchCandidate => "host-patch/candidate",
        }
    }

    /// Whether the profile needs the standard library.
    #[must_use]
    pub const fn requires_std(self) -> bool {
        !matches!(self, Self::NoStd)
    }

    /// Crate layout: `lib` or `bin`.
    #[must_use]
    pub const fn layout(self) -> &'static str {
        match self {
            Self::Binary => "bin",
            _ => "lib",
        }
    }

    /// Manifest fragment for the profile (deterministic; the workspace
    /// adds name/version/deps).
    #[must_use]
    pub fn manifest_fragment(self) -> String {
        match self {
            Self::Library => "[lib]\npath = \"src/lib.rs\"\n".to_string(),
            Self::Binary => "[[bin]]\nname = \"generated\"\npath = \"src/main.rs\"\n".to_string(),
            Self::NoStd => "[lib]\npath = \"src/lib.rs\"\ncrate-type = [\"rlib\"]\n".to_string(),
            Self::WasmComponent | Self::Ffi => {
                "[lib]\npath = \"src/lib.rs\"\ncrate-type = [\"cdylib\"]\n".to_string()
            }
            Self::ProviderPlugin => {
                "[lib]\npath = \"src/lib.rs\"\ncrate-type = [\"cdylib\", \"rlib\"]\n".to_string()
            }
            Self::HostPatchCandidate => {
                "[lib]\npath = \"src/lib.rs\"\nhost-patch = true\n".to_string()
            }
        }
    }

    /// Crate-level attribute for the profile (empty when none).
    #[must_use]
    pub fn crate_attribute(self) -> Option<&'static str> {
        match self {
            Self::NoStd => Some("#![no_std]"),
            _ => None,
        }
    }

    /// Validates a generated module against the profile: no `unsafe` items
    /// in safe profiles, and every public item covered by an anchor.
    #[must_use]
    pub fn validate(self, module: &Module) -> Vec<ProfileProblem> {
        let mut problems = Vec::new();
        if self.requires_std() {
            for id in module.items.iter().filter_map(unsafe_item) {
                problems.push(ProfileProblem::UnsafeInSafeProfile(id));
            }
        }
        for gap in coverage_gaps(module) {
            problems.push(ProfileProblem::SourceMapGap(gap));
        }
        problems
    }
}

/// Detect `unsafe` in function attributes.
fn unsafe_item(item: &Item) -> Option<String> {
    match item {
        Item::Fn(FnDef { name, attrs, .. }) => {
            if attrs.iter().any(|attribute| attribute.contains("unsafe")) {
                Some(format!("fn {name}"))
            } else {
                None
            }
        }
        Item::RawAttribute(attribute) => {
            if attribute.contains("unsafe") {
                Some(attribute.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Profile validation problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileProblem {
    /// Unsafe code found while the profile is safe (`E-CODEGEN-002`).
    UnsafeInSafeProfile(String),
    /// Public item without a source-map anchor (`E-CODEGEN-004`).
    SourceMapGap(String),
}

impl ProfileProblem {
    /// Stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsafeInSafeProfile(_) => "E-CODEGEN-002",
            Self::SourceMapGap(_) => "E-CODEGEN-004",
        }
    }
}

/// Parses a profile name into a `CrateProfile`; unknown names are typed
/// refusals (`E-CODEGEN-003`).
pub fn parse_profile(name: &str) -> Result<CrateProfile, &'static str> {
    match name {
        "library" => Ok(CrateProfile::Library),
        "binary" => Ok(CrateProfile::Binary),
        "no_std" => Ok(CrateProfile::NoStd),
        "wasm/component" => Ok(CrateProfile::WasmComponent),
        "ffi" => Ok(CrateProfile::Ffi),
        "provider-plugin" => Ok(CrateProfile::ProviderPlugin),
        "host-patch/candidate" => Ok(CrateProfile::HostPatchCandidate),
        _ => Err("E-CODEGEN-003"),
    }
}

/// Fn bodies that use `unsafe` blocks (statement-level scan).
#[allow(dead_code)]
fn uses_unsafe(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Block(block) => block.statements.iter().any(uses_unsafe),
        Stmt::Let { .. } | Stmt::Return(_) | Stmt::Expr(_) => false,
    }
}
