//! contracts: materialization recipes and
//! deterministic rehydration.
//!
//! Generated Rust/WASM/docs are caches unless published. A recipe binds
//! meaning + generator (toolchain) + target + spec; its identity
//! (`RecipeId`) covers all four, so a provider/toolchain mismatch
//! changes the recipe identity. Materializing a recipe twice (delete +
//! recreate) must land on the SAME `ArtifactId` for a deterministic
//! generator; a nondeterministic generator claiming the same recipe
//! identity is a typed refusal (`E-EVID-601`), never a silent
//! overwrite. Materialization never mints new meaning: specializer
//! outputs are recipes, not new meaning.

use emath_core::{ArtifactId, MeaningId, RecipeId};
use emath_store::materialization::{MaterializationRecipe, MaterializeFault, Materializer};

fn recipe(
    meaning_seed: &[u8],
    toolchain: &str,
    target: &str,
    spec: &[u8],
) -> MaterializationRecipe {
    MaterializationRecipe::new(MeaningId::from_bytes(meaning_seed), toolchain, target, spec)
}

/// The recipe identity binds ALL FOUR inputs: meaning, toolchain,
/// target, spec. A provider/toolchain mismatch changes `RecipeId`
/// (the pinned criterion), as does any other input change.
#[test]
fn recipe_identity_binds_meaning_toolchain_target_spec() {
    let base = recipe(
        b"meaning",
        "rustc-1.90",
        "wasm32-unknown-unknown",
        b"spec-v1",
    );
    let same = recipe(
        b"meaning",
        "rustc-1.90",
        "wasm32-unknown-unknown",
        b"spec-v1",
    );
    assert_eq!(
        base.identity(),
        same.identity(),
        "identical recipe inputs must derive one RecipeId"
    );

    let toolchain_drift = recipe(
        b"meaning",
        "rustc-1.91",
        "wasm32-unknown-unknown",
        b"spec-v1",
    );
    assert_ne!(
        base.identity(),
        toolchain_drift.identity(),
        "toolchain/provider mismatch must change RecipeId"
    );

    let target_drift = recipe(b"meaning", "rustc-1.90", "x86_64-apple-darwin", b"spec-v1");
    assert_ne!(
        base.identity(),
        target_drift.identity(),
        "target mismatch must change RecipeId"
    );

    let spec_drift = recipe(
        b"meaning",
        "rustc-1.90",
        "wasm32-unknown-unknown",
        b"spec-v2",
    );
    assert_ne!(
        base.identity(),
        spec_drift.identity(),
        "spec drift must change RecipeId"
    );

    let meaning_drift = recipe(
        b"meaning2",
        "rustc-1.90",
        "wasm32-unknown-unknown",
        b"spec-v1",
    );
    assert_ne!(
        base.identity(),
        meaning_drift.identity(),
        "meaning change must change RecipeId"
    );
    assert!(base.identity().as_str().starts_with(RecipeId::PREFIX));
}

/// Deterministic rehydration: materialize, delete the artifact (the
/// cache is dropped), materialize again from the same recipe — the
/// ArtifactId matches. Generated output is a cache, not a source.
#[test]
fn delete_and_rematerialize_yields_the_same_artifact_id() {
    let mut materializer = Materializer::default();
    let net = recipe(b"meaning", "specializer-12", "host", b"world-spec");

    let (recipe_id, artifact_id, bytes) = materializer
        .materialize(&net, |recipe| {
            let mut out = b"generated crate ".to_vec();
            out.extend_from_slice(recipe.spec());
            out
        })
        .expect("deterministic generator must materialize");

    // Delete: drop the cache AND the recorded binding.
    materializer.forget(&recipe_id);
    drop(bytes);

    let (recipe_id2, artifact_id2, _) = materializer
        .materialize(&net, |recipe| {
            let mut out = b"generated crate ".to_vec();
            out.extend_from_slice(recipe.spec());
            out
        })
        .expect("re-materialization must succeed");

    assert_eq!(recipe_id, recipe_id2, "the recipe identity is stable");
    assert_eq!(
        artifact_id, artifact_id2,
        "delete + materialize must rehydrate the same ArtifactId"
    );
}

/// Negative control: a nondeterministic generator (different bytes per
/// run) claiming the same recipe identity FAILS on re-materialization
/// with `E-EVID-601` — it never silently rebrands the artifact.
#[test]
fn nondeterministic_generator_is_a_typed_refusal() {
    let mut materializer = Materializer::default();
    let net = recipe(b"meaning", "flaky-gen", "target", b"spec");
    let run = std::cell::Cell::new(0_u32);

    let outcome = materializer.materialize(&net, |_| {
        run.set(run.get() + 1);
        Vec::from(format!("run-{}", run.get()).into_bytes())
    });
    assert!(outcome.is_ok(), "first materialization must succeed");
    let (recipe_id, artifact_id, _) = outcome.unwrap();

    let second = materializer.materialize(&net, |_| {
        run.set(run.get() + 1);
        Vec::from(format!("run-{}", run.get()).into_bytes())
    });
    match second {
        Err(MaterializeFault::RehydrationMismatch {
            code,
            recorded,
            derived,
        }) => {
            assert_eq!(code, "E-EVID-601");
            assert_eq!(recorded, artifact_id, "the recorded id is the first run's");
            assert_ne!(derived, recorded, "the flaky run derives a different id");
        }
        other => panic!("expected E-EVID-601 RehydrationMismatch, got {other:?}"),
    }

    // No tamper laundering: the original binding survives the refusal.
    assert_eq!(
        materializer.artifact_of(&recipe_id),
        Some(&artifact_id),
        "a refused re-materialization must not overwrite the recorded artifact"
    );
    assert!(artifact_id.as_str().starts_with(ArtifactId::PREFIX));
}

/// The artifact address binds the recipe identity AND the generated
/// content: the same bytes generated under two different recipes
/// (different toolchains) land on DIFFERENT artifact identities — an
/// artifact is never rebranded across recipe identities.
#[test]
fn artifact_identity_binds_recipe_and_content() {
    let mut materializer = Materializer::default();
    let net_a = recipe(b"meaning", "specializer-12", "host", b"spec");
    let net_b = recipe(b"meaning", "specializer-12-drift", "host", b"spec");
    assert_ne!(net_a.identity(), net_b.identity());

    let (_, artifact_a, _) = materializer
        .materialize(&net_a, |_| b"identical bytes".to_vec())
        .expect("materialize a must succeed");
    let (_, artifact_b, _) = materializer
        .materialize(&net_b, |_| b"identical bytes".to_vec())
        .expect("materialize b must succeed");
    assert_ne!(
        artifact_a, artifact_b,
        "identical content under different recipes must not share an ArtifactId"
    );
}

/// Materialization never mints new meaning: the recipe's MeaningId is
/// carried through untouched, and the artifact lands in the artifact
/// identity domain, not the meaning domain (specializer outputs are
/// recipes, not new meaning).
#[test]
fn materialization_never_mints_new_meaning() {
    let mut materializer = Materializer::default();
    let meaning = MeaningId::from_bytes(b"meaning");
    let net = MaterializationRecipe::new(meaning.clone(), "specializer-12", "host", b"spec");
    let (_, artifact_id, _) = materializer
        .materialize(&net, |_| b"generated".to_vec())
        .expect("materialize must succeed");
    assert_eq!(
        net.meaning(),
        &meaning,
        "the recipe carries the meaning it was given"
    );
    assert!(
        !artifact_id.as_str().starts_with(MeaningId::PREFIX),
        "artifact identity is not a meaning identity"
    );
}
