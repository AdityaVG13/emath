#![forbid(unsafe_code)]

//! Provider-neutral World IR facade. Record types live in `emath-ir::world`;
//! this crate keeps built-in worlds, translation, and rust codegen.

pub mod builtin;
pub mod fitting;
pub mod translation;
pub mod world_codegen_rust;

pub use emath_ir::world::{
    WORLD_IR_SCHEMA, WORLD_IR_VERSION, CarrierDef, FittedTable, Fixity, MeaningHole, MeaningHoleId,
    MeaningHoleKind, MeaningHoleState, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef,
    WorldId, WorldIr,
};
// Facade fence (C052): the morphism/preservation vocabulary callers
// consume deep (`translation::{...}`) is root-exported; the module path
// stays public for the rest of the translation surface. Homonym watch
// (C057): `emath_provider_api::runtime::EvidenceHandle` is a DIFFERENT type — no
// collision, different crates and paths.
pub use translation::{EvidenceHandle, PreservationRelation, WorldMorphism};
pub use emath_core::fnv1a64_bytes as fnv1a64;
