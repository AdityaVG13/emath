//! contracts: materialization recipes and
//! deterministic rehydration.

use emath_core::{ArtifactId, MeaningId, RecipeId};
use emath_store::materialization::{MaterializationRecipe, MaterializeFault, Materializer};
use emath_test_harness::Probe;

fn recipe(meaning: &[u8], toolchain: &str, target: &str, spec: &[u8]) -> MaterializationRecipe {
    MaterializationRecipe::new(MeaningId::from_bytes(meaning), toolchain, target, spec)
}

#[test]
fn materialization_recipes() {
    let mut p = Probe::new("recipes bind meaning+toolchain+target+spec and rehydrate deterministically");
    p.case("identity", |p| {
        let base = recipe(b"meaning", "rustc-1.90", "wasm32-unknown-unknown", b"spec-v1");
        p.eq("same", base.identity(), recipe(b"meaning", "rustc-1.90", "wasm32-unknown-unknown", b"spec-v1").identity());
        p.ne("toolchain", base.identity(), recipe(b"meaning", "rustc-1.91", "wasm32-unknown-unknown", b"spec-v1").identity());
        p.ne("target", base.identity(), recipe(b"meaning", "rustc-1.90", "x86_64-apple-darwin", b"spec-v1").identity());
        p.ne("spec", base.identity(), recipe(b"meaning", "rustc-1.90", "wasm32-unknown-unknown", b"spec-v2").identity());
        p.ne("meaning", base.identity(), recipe(b"meaning2", "rustc-1.90", "wasm32-unknown-unknown", b"spec-v1").identity());
        p.demand("prefix", base.identity().as_str().starts_with(RecipeId::PREFIX), "recipe prefix");
    });
    p.case("rehydrate", |p| {
        let mut m = Materializer::default();
        let net = recipe(b"meaning", "specializer-12", "host", b"world-spec");
        let (rid, aid, bytes) = m.materialize(&net, |r| { let mut o = b"generated crate ".to_vec(); o.extend_from_slice(r.spec()); o }).expect("materialize");
        m.forget(&rid);
        drop(bytes);
        let (rid2, aid2, _) = m.materialize(&net, |r| { let mut o = b"generated crate ".to_vec(); o.extend_from_slice(r.spec()); o }).expect("re-materialize");
        p.eq("recipe-stable", rid, rid2);
        p.eq("artifact-stable", aid, aid2);
    });
    p.case("nondeterministic", |p| {
        let mut m = Materializer::default();
        let net = recipe(b"meaning", "flaky-gen", "target", b"spec");
        let run = std::cell::Cell::new(0_u32);
        let first = m.materialize(&net, |_| { run.set(run.get() + 1); format!("run-{}", run.get()).into_bytes() });
        p.demand("first-ok", first.is_ok(), "first materialization succeeds");
        let (rid, aid, _) = first.unwrap();
        match m.materialize(&net, |_| { run.set(run.get() + 1); format!("run-{}", run.get()).into_bytes() }) {
            Err(MaterializeFault::RehydrationMismatch { code, recorded, derived }) => {
                p.eq("code", code, "E-EVID-601".to_string());
                p.eq("recorded", recorded, aid.clone());
                p.ne("derived", derived, aid.clone());
            }
            other => { p.fail("mismatch", format!("expected E-EVID-601, got {other:?}")); },
        }
        p.eq("no-overwrite", m.artifact_of(&rid), Some(&aid));
        p.demand("artifact-prefix", aid.as_str().starts_with(ArtifactId::PREFIX), "artifact prefix");
    });
    p.case("binding", |p| {
        let mut m = Materializer::default();
        let net_a = recipe(b"meaning", "specializer-12", "host", b"spec");
        let net_b = recipe(b"meaning", "specializer-12-drift", "host", b"spec");
        p.ne("recipes", net_a.identity(), net_b.identity());
        let (_, a, _) = m.materialize(&net_a, |_| b"identical bytes".to_vec()).unwrap();
        let (_, b, _) = m.materialize(&net_b, |_| b"identical bytes".to_vec()).unwrap();
        p.ne("artifacts", a, b);
        let meaning = MeaningId::from_bytes(b"meaning");
        let net = MaterializationRecipe::new(meaning.clone(), "specializer-12", "host", b"spec");
        let mut fresh = Materializer::default();
        let (_, aid, _) = fresh.materialize(&net, |_| b"generated".to_vec()).unwrap();
        p.eq("carries", net.meaning().clone(), meaning);
        p.demand("not-meaning", !aid.as_str().starts_with(MeaningId::PREFIX), "artifact is not meaning");
    });
    p.finish();
}
