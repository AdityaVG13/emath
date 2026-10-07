//! Generated crate profiles.
//!
//! Profiles: library, binary, `no_std`, `wasm/component`, FFI, provider
//! plugin and host patch/candidate. Each profile determines the manifest
//! fragment, crate layout and std requirement. Permitted profiles refuse
//! unsafe code (`E-CODEGEN-002`); unknown profiles are typed refusals
//! (`E-CODEGEN-003`).

use crate::rust_ir::ast::{FnDef, Item, Module};
use crate::rust_ir::render::coverage_gaps;

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

    /// Crate layout: `lib` or `bin`.
    #[must_use]
    pub const fn layout(self) -> &'static str {
        match self {
            Self::Binary => "bin",
            _ => "lib",
        }
    }

    /// Validates a generated module against the profile: no `unsafe` items
    /// in safe profiles, and every public item covered by an anchor.
    #[must_use]
    pub fn validate(self, module: &Module) -> Vec<ProfileProblem> {
        let _ = self;
        let mut problems = Vec::new();
        // Every profile bans unsafe code (E-CODEGEN-002), including
        // `no_std`: no_std forbids `unsafe` just as strictly.
        for id in module.items.iter().filter_map(unsafe_item) {
            problems.push(ProfileProblem::UnsafeInSafeProfile(id));
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
            // `#![forbid(unsafe_code)]` is the profile's own safety seal
            // (the backend emits it first in every crate): it
            // declares the absence of unsafe rather than using it.
            if attribute.contains("unsafe") && !attribute.contains("forbid(unsafe_code)") {
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

// NOTE: the AST has no statement-level unsafe construct (Stmt is Block/
// Let/Return/Expr only), so `unsafe` is only expressible via item
// attributes, which `unsafe_item` already scans for every profile. A
// statement-level scanner was removed instead of kept as a dead stub.
