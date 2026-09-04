//! Materialization recipes and deterministic rehydration
//!.
//!
//! Generated Rust/WASM/docs are caches unless published: a
//! [`MaterializationRecipe`] binds the meaning it was specialized from,
//! the generator identity (toolchain/provider), the target, and the
//! canonical spec. Its `RecipeId` covers all four inputs — a
//! provider/toolchain mismatch changes the recipe identity. The
//! [`Materializer`] records recipe → artifact bindings and refuses a
//! nondeterministic generator that derives a different `ArtifactId`
//! under a recorded recipe identity (`E-EVID-601`) — delete +
//! re-materialize with a deterministic generator rehydrates the same
//! address, so generated output stays a cache, never a source.
//!
//! Determinism class: pure sequence. No wall-clock timestamps; the
//! artifact address is content-derived (recipe identity + bytes).

use std::collections::BTreeMap;

use emath_core::{ArtifactId, MeaningId, RecipeId};

const MATERIALIZATION_SCHEMA_V1: &str = "emath.materialization-recipe.v1";
const ARTIFACT_FRAMING_V1: &str = "emath.materialization-artifact.v1";

/// A materialization recipe: the four inputs whose identity is
/// `RecipeId`. The recipe carries the meaning it was specialized from —
/// it never mints a new one (specializer outputs are recipes, not new
/// meaning; generated Cargo is not a source of truth).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializationRecipe {
    meaning: MeaningId,
    toolchain: String,
    target: String,
    spec: Vec<u8>,
}

impl MaterializationRecipe {
    #[must_use]
    pub fn new(meaning: MeaningId, toolchain: &str, target: &str, spec: &[u8]) -> Self {
        Self {
            meaning,
            toolchain: toolchain.to_string(),
            target: target.to_string(),
            spec: spec.to_vec(),
        }
    }

    #[must_use]
    pub fn meaning(&self) -> &MeaningId {
        &self.meaning
    }

    #[must_use]
    pub fn toolchain(&self) -> &str {
        &self.toolchain
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    #[must_use]
    pub fn spec(&self) -> &[u8] {
        &self.spec
    }

    /// The recipe identity: the durable identity over the framed
    /// (schema, meaning, toolchain, target, spec). Any provider,
    /// toolchain, target, or spec mismatch changes the identity.
    #[must_use]
    pub fn identity(&self) -> RecipeId {
        let mut bytes = Vec::new();
        crate::object_graph::frame(&mut bytes, MATERIALIZATION_SCHEMA_V1.as_bytes());
        crate::object_graph::frame(&mut bytes, self.meaning.as_str().as_bytes());
        crate::object_graph::frame(&mut bytes, self.toolchain.as_bytes());
        crate::object_graph::frame(&mut bytes, self.target.as_bytes());
        crate::object_graph::frame(&mut bytes, &self.spec);
        RecipeId::from_bytes(&bytes)
    }
}

/// Materialization refusals. `RehydrationMismatch` carries the house
/// code `E-EVID-601`: a re-materialization derived a different artifact
/// address than the one recorded under the recipe identity — a
/// nondeterministic generator claiming the same recipe identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterializeFault {
    /// Recorded artifact id does not match the re-derived address
    /// (`E-EVID-601`). The recorded binding is never overwritten.
    RehydrationMismatch {
        code: String,
        recorded: ArtifactId,
        derived: ArtifactId,
    },
}

impl std::fmt::Display for MaterializeFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RehydrationMismatch {
                code,
                recorded,
                derived,
            } => write!(
                formatter,
                "{code}: rehydration identity mismatch: recipe re-materialized to {derived} \
                 but {recorded} is recorded — the generator is nondeterministic or the cache \
                 was tampered with; the recorded binding is preserved"
            ),
        }
    }
}

impl std::error::Error for MaterializeFault {}

/// The materialization book: recipe identity → recorded artifact
/// identity. Generated outputs are caches — GC may drop a binding
/// ([`Materializer::forget`]) and the recipe rehydrates it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Materializer {
    recorded: BTreeMap<RecipeId, ArtifactId>,
}

impl Materializer {
    /// Run the generator for a recipe and record the artifact binding.
    /// The artifact address binds the recipe identity AND the generated
    /// content: a deterministic generator rehydrates the same
    /// `ArtifactId` on every run (idempotent), and drifting content
    /// under a recorded recipe identity refuses `E-EVID-601` before
    /// anything is overwritten.
    pub fn materialize(
        &mut self,
        recipe: &MaterializationRecipe,
        generator: impl Fn(&MaterializationRecipe) -> Vec<u8>,
    ) -> Result<(RecipeId, ArtifactId, Vec<u8>), MaterializeFault> {
        let recipe_id = recipe.identity();
        let bytes = generator(recipe);
        let mut framed = Vec::new();
        crate::object_graph::frame(&mut framed, recipe_id.as_str().as_bytes());
        crate::object_graph::frame(&mut framed, &bytes);
        let artifact_id = ArtifactId::from_bytes(&framed);
        if let Some(recorded) = self.recorded.get(&recipe_id) {
            if *recorded != artifact_id {
                return Err(MaterializeFault::RehydrationMismatch {
                    code: "E-EVID-601".to_string(),
                    recorded: recorded.clone(),
                    derived: artifact_id,
                });
            }
            return Ok((recipe_id, artifact_id, bytes));
        }
        self.recorded.insert(recipe_id.clone(), artifact_id.clone());
        Ok((recipe_id, artifact_id, bytes))
    }

    /// The recorded artifact identity for a recipe, if any.
    #[must_use]
    pub fn artifact_of(&self, recipe_id: &RecipeId) -> Option<&ArtifactId> {
        self.recorded.get(recipe_id)
    }

    /// Delete a rebuildable materialization: the cache and its binding
    /// go together; the recipe remains the source of truth for
    /// rehydration.
    pub fn forget(&mut self, recipe_id: &RecipeId) {
        self.recorded.remove(recipe_id);
    }
}
