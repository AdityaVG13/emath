//! The ten-layer IR stack registry: one row per layer with its durable
//! schema id, schema version and owning crate — the single enumeration
//! of the stack (syntax, HIR, MIG, SIR, GIR, resolution, EIR, evidence,
//! rust-ir, artifact). Ids reuse durable artifact strings; versions are
//! explicit so a schema change is a visible, versioned event.

use emath_core::SchemaId;

/// One layer of the IR stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IrLayer {
    /// Lossless syntax tree (`emath-syntax` / `emath-core::tree`).
    Syntax,
    /// HIR: open declarations and section families (`emath-hir`).
    Hir,
    /// MIG: mathematical intent graph, the spine (`emath-ir::mig`).
    Mig,
    /// SIR: semantic IR (`emath-ir` package/declaration arena).
    Sir,
    /// GIR: goal IR (`emath-ir::goal` + `emath-goal` schema).
    Gir,
    /// Resolution graph (`emath-ir::ResolutionPlan` + `emath-plan`).
    Resolution,
    /// EIR: strict-f64 execution IR (`emath-exec-ir`).
    Eir,
    /// Evidence IR (`emath-evidence`).
    Evidence,
    /// Structured Rust IR (`emath-rust-ir`).
    RustIr,
    /// Artifact graph + identity (`emath-artifact`).
    Artifact,
}

impl IrLayer {
    /// Every layer, in pipeline order.
    pub const ALL: [Self; 10] = [
        Self::Syntax,
        Self::Hir,
        Self::Mig,
        Self::Sir,
        Self::Gir,
        Self::Resolution,
        Self::Eir,
        Self::Evidence,
        Self::RustIr,
        Self::Artifact,
    ];

    /// Stable layer name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Hir => "hir",
            Self::Mig => "mig",
            Self::Sir => "sir",
            Self::Gir => "gir",
            Self::Resolution => "resolution",
            Self::Eir => "eir",
            Self::Evidence => "evidence",
            Self::RustIr => "rust-ir",
            Self::Artifact => "artifact",
        }
    }

    /// Durable schema id string. Matches the strings already written into
    /// durable artifacts where those exist; never repurposed.
    #[must_use]
    pub const fn schema_base(self) -> &'static str {
        match self {
            Self::Syntax => "emath.syntax",
            Self::Hir => "emath.hir",
            Self::Mig => crate::mig::MIG_SCHEMA,
            Self::Sir => "emath.sir",
            Self::Gir => "emath.goal",
            Self::Resolution => "emath.resolution-plan",
            Self::Eir => "emath.eir",
            Self::Evidence => "emath.evidence-bundle",
            Self::RustIr => "emath.rust-ir",
            Self::Artifact => "emath.artifact",
        }
    }

    /// Current schema version of the layer.
    #[must_use]
    pub const fn schema_version(self) -> u32 {
        match self {
            Self::Mig => crate::mig::MIG_SCHEMA_VERSION,
            _ => 1,
        }
    }

    /// The versioned schema id (`<base>.v<version>`).
    #[must_use]
    pub fn versioned_schema(self) -> SchemaId {
        SchemaId(format!("{}.v{}", self.schema_base(), self.schema_version()))
    }

    /// The crate that owns the layer's types.
    #[must_use]
    pub const fn owner_crate(self) -> &'static str {
        match self {
            Self::Syntax => "emath-syntax",
            Self::Hir => "emath-hir",
            Self::Mig | Self::Sir | Self::Gir | Self::Resolution => "emath-ir",
            Self::Eir => "emath-exec-ir",
            Self::Evidence => "emath-evidence",
            Self::RustIr => "emath-rust-ir",
            Self::Artifact => "emath-artifact",
        }
    }
}

// Stack witness tests moved to `tests/emath-ir/tests/layers.rs`.
