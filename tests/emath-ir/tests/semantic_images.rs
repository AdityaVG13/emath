//! compiled semantic image — partitions, lock,
//! not generated Rust source.
//!
//! The law: a field pack compiles into a COMPACT image — symbols,
//! cells, bytecode, worlds, docs offsets, identities — organized in
//! independently loadable partitions under a deterministic content id,
//! with a lock recording prelude/packs/images/toolchain. The image is
//! data (canonical text), NOT a tree of generated .rs files as source of
//! truth, and a corrupt partition refuses typed — never a silent load,
//! never partial authority.

use emath_exec_ir::image::{ImageLock, ImagePartition, ImageRefusal, PartitionKind, SemanticImage};
use emath_genesis::{
    Disposition, EvalError, FirstOrderWorld, ResultBundle, WorldBudget, evaluate_labeled,
};
use emath_term::SymbolId;
use emath_test_harness::Probe;

fn std_cell_registry() -> std::collections::HashMap<String, emath_exec_ir::term_compile::CompiledCell> {
    std::collections::HashMap::new()
}

#[test]
fn intent() {
    let mut p = Probe::new("compiled semantic image — partitions, lock,");
    p.case("image_is_deterministic_and_partitioned", |p| {

    // Build the std softmax cell's compiled image twice: identical id,
    // identical canonical encoding, every partition independently
    // loadable (each validates its own content id).
    let first = build_softmax_image();
    let second = build_softmax_image();
    p.eq("image_is_deterministic_and_partitioned#1", first.image_id.as_str(), second.image_id.as_str());
    p.eq("image_is_deterministic_and_partitioned#2", first.to_canonical(), second.to_canonical());
    p.demand("content id shape", first.image_id.starts_with("fnv1a64:"), "content id shape");

    // Partitions, sorted by name, each loadable alone.
    let names: Vec<&str> = first
        .partitions
        .iter()
        .map(|partition| partition.name.as_str())
        .collect();
    p.eq("image_is_deterministic_and_partitioned#4", names, ["cells", "docs", "lock", "worlds", "worlds.bytecode"].to_vec());
    for partition in &first.partitions {
        partition
            .validate()
            .expect("each partition loads independently");
        p.demand(format!("no empty page: {partition:?}"), !partition.body.is_empty(), format!("no empty page: {partition:?}"));
    }

    // The bytecode partition carries the cell's compiled SSA program (the
    // .5 compiler output), not generated Rust source. A leaf capability's
    // own program is its body: load-input/ops/result — apply-capability
    // appears in callers, not inside the cell itself.
    let bytecode = &first
        .partitions
        .iter()
        .find(|partition| partition.name == "worlds.bytecode")
        .expect("bytecode partition")
        .body;
    p.demand(format!("{bytecode}"), bytecode.contains("cell:std.tensor.softmax"), format!("{bytecode}"));
    p.demand(format!("{bytecode}"), bytecode.contains("load-input"), format!("{bytecode}"));
    p.demand(format!("{bytecode}"), bytecode.contains("result:"), format!("{bytecode}"));
    p.demand(format!("image is not generated Rust source: {bytecode}"), !bytecode.contains("fn main") && !bytecode.contains("impl "), format!("image is not generated Rust source: {bytecode}"));

    // The lock records the four required identities.
    let lock = &first
        .partitions
        .iter()
        .find(|partition| partition.name == "lock")
        .expect("lock partition")
        .body;
    p.demand(format!("{lock}"), lock.contains("prelude:"), format!("{lock}"));
    p.demand(format!("{lock}"), lock.contains("packs:"), format!("{lock}"));
    p.demand(format!("{lock}"), lock.contains("images:"), format!("{lock}"));
    p.demand(format!("{lock}"), lock.contains("toolchain:"), format!("{lock}"));

    });
    p.case("corrupt_page_refuses_typed", |p| {

    let image = build_softmax_image();
    let partition = &image.partitions[0];

    // A partition whose body was flipped after stamping: the content id
    // no longer matches — typed refusal, never a silent load.
    let corrupted = ImagePartition {
        name: partition.name.clone(),
        kind: partition.kind,
        content_id: partition.content_id.clone(),
        body: format!("{}/*corrupt*/", partition.body),
    };
    match corrupted.validate() {
        Err(ImageRefusal::CorruptPartition { name }) => { p.eq("corrupt_page_refuses_typed#1", name, partition.name.clone()); },
        other => { p.fail("corrupt_page_refuses_typed#2", format!("expected CorruptPartition, got {other:?}")); return; },
    }
    p.demand("corrupt_page_refuses_typed#3", ImageRefusal::CorruptPartition {
            name: String::new()
        }
        .code() == "E-IMAGE-001", format!("expected {:?}, got {:?}", "E-IMAGE-001", ImageRefusal::CorruptPartition {
            name: String::new()
        }
        .code()));

    // A partition with an empty name/page refuses (no blank pages).
    let blank = ImagePartition {
        name: String::new(),
        kind: PartitionKind::Docs,
        content_id: partition.content_id.clone(),
        body: partition.body.clone(),
    };
    p.demand("corrupt_page_refuses_typed#4", matches!(
        blank.validate(),
        Err(ImageRefusal::MalformedPartition { .. })
    ), "corrupt_page_refuses_typed#4: matches!(\n        blank.validate(),\n        Err(ImageRefusal::MalformedPartition { .. })\n    )");

    // The whole image refuses if ANY partition is corrupt.
    let mut tampered = image.clone();
    tampered.partitions[1].body.push_str("/*tampered*/");
    p.demand("corrupt_page_refuses_typed#5", tampered.validate_partitions().is_err(), "corrupt_page_refuses_typed#5: tampered.validate_partitions().is_err()");
    p.demand("corrupt_page_refuses_typed#6", image.validate_partitions().is_ok(), "corrupt_page_refuses_typed#6: image.validate_partitions().is_ok()");

    });
    p.case("image_paths_cells_and_bundle_fixture", |p| {

    // The image is built FROM cells (the .5 compiler output) — the
    // bytecode partition carries the leaf cell's compiled SSA program in
    // the generic vocabulary, and its labeled reference answer lands in
    // a WorldResultBundle (envelope).
    let image = build_softmax_image();
    let bytecode = &image
        .partitions
        .iter()
        .find(|partition| partition.name == "worlds.bytecode")
        .expect("bytecode partition")
        .body;
    p.demand(format!("generic vocabulary: {bytecode}"), bytecode.contains("vector-map"), format!("generic vocabulary: {bytecode}"));

    let result = evaluate_labeled(
        &reference_softmax_term(),
        &ReferenceWorld,
        &[].into_iter().collect(),
        WorldBudget { max_steps: 8 },
        |answer: &f64| format!("{answer:.6}"),
    );
    // The world evaluates the cell's reference semantics through the
    // envelope: a labeled answer, bundleable with the image id recorded
    // alongside (the image id is the artifact identity in the lock).
    p.demand("image_paths_cells_and_bundle_fixture#2", matches!(result.disposition, Disposition::Answer { .. }), "image_paths_cells_and_bundle_fixture#2: matches!(result.disposition, Disposition::Answer { .. })");
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("image_paths_cells_and_bundle_fixture#3", bundle.bundle_id.starts_with("fnv1a64:"), "image_paths_cells_and_bundle_fixture#3: bundle.bundle_id.starts_with(\"fnv1a64:\")");
    p.demand("image_paths_cells_and_bundle_fixture#4", image.image_id.starts_with("fnv1a64:"), "image_paths_cells_and_bundle_fixture#4: image.image_id.starts_with(\"fnv1a64:\")");

    // Negative seed: corrupt partition is a typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/semantic_images.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects a typed image refusal, found: {expect_line}"), expect_line.contains("E-IMAGE"), format!("seed expects a typed image refusal, found: {expect_line}"));

    });
    p.finish();
}






// ── Fixture: a minimal custom world evaluating the softmax reference ──

/// A labeled reference world: its `apply` evaluates the std softmax
/// reference semantics for the fixture input [1.0, 2.0, 3.0] and reports
/// the leading probability. Defined here — adding a world touches no
/// parser/sema/backend code.
struct ReferenceWorld;

impl FirstOrderWorld for ReferenceWorld {
    type Value = f64;
    type Error = EvalError;

    fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        // The leading probability of softmax([1,2,3]) under the strict-f64
        // reference semantics (pinned: e^0 / (e^0+e^1+e^2) ≈ 0.09003057).
        let logits = [1.0_f64, 2.0, 3.0];
        let probabilities = emath_ir::capability::softmax_reference_strict_f64(&logits)
            .expect("reference semantics compute");
        Ok(probabilities[0])
    }

    fn apply(
        &self,
        operator: &SymbolId,
        _arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        Err(EvalError::UnknownSymbol(operator.clone()))
    }

    fn evidence(&self) -> emath_genesis::WorldEvidence {
        emath_genesis::WorldEvidence::seed("reference-vm", &["stable-max-shift-invariance"])
    }
}

fn reference_softmax_term() -> emath_term::Term {
    emath_term::Term::Constant(SymbolId("softmax[1.0,2.0,3.0]".into()))
}

fn build_softmax_image() -> SemanticImage {
    let registry = std_cell_registry();
    let cell = registry.get("std.tensor.softmax").expect("std cell present");
    let mut docs = std::collections::BTreeMap::new();
    docs.insert(
        "std.tensor.softmax".to_string(),
        "softmax reference: stable-max strict-f64; laws: shift invariance, ".to_string()
            + "nonnegativity, normalization",
    );
    SemanticImage::build(
        "fixture-pack",
        std::slice::from_ref(cell),
        &[emath_exec_ir::image::ImageWorld {
            world: "reference-vm".to_string(),
            origin: "seed".to_string(),
            laws: vec!["stable-max-shift-invariance".to_string()],
        }],
        &docs,
        ImageLock {
            prelude: vec!["std.prelude.core@1.0.0".to_string()],
            packs: vec!["fixture-pack@0.1.0".to_string()],
            images: vec![],
            toolchain: "emath-toolchain@0.1.0".to_string(),
        },
    )
    .expect("fixture image builds")
}
