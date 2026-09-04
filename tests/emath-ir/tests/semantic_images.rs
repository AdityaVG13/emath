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
use emath_exec_ir::term_compile::std_cell_registry;
use emath_genesis::{
    Disposition, EvalError, FirstOrderWorld, ResultBundle, WorldBudget, evaluate_labeled,
};
use emath_term::SymbolId;

#[test]
fn image_is_deterministic_and_partitioned() {
    // Build the std softmax cell's compiled image twice: identical id,
    // identical canonical encoding, every partition independently
    // loadable (each validates its own content id).
    let first = build_softmax_image();
    let second = build_softmax_image();
    assert_eq!(first.image_id, second.image_id);
    assert_eq!(first.to_canonical(), second.to_canonical());
    assert!(first.image_id.starts_with("fnv1a64:"), "content id shape");

    // Partitions, sorted by name, each loadable alone.
    let names: Vec<&str> = first
        .partitions
        .iter()
        .map(|partition| partition.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["cells", "docs", "lock", "worlds", "worlds.bytecode"]
    );
    for partition in &first.partitions {
        partition
            .validate()
            .expect("each partition loads independently");
        assert!(!partition.body.is_empty(), "no empty page: {partition:?}");
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
    assert!(bytecode.contains("cell:std.tensor.softmax"), "{bytecode}");
    assert!(bytecode.contains("load-input"), "{bytecode}");
    assert!(bytecode.contains("result:"), "{bytecode}");
    assert!(
        !bytecode.contains("fn main") && !bytecode.contains("impl "),
        "image is not generated Rust source: {bytecode}"
    );

    // The lock records the four required identities.
    let lock = &first
        .partitions
        .iter()
        .find(|partition| partition.name == "lock")
        .expect("lock partition")
        .body;
    assert!(lock.contains("prelude:"), "{lock}");
    assert!(lock.contains("packs:"), "{lock}");
    assert!(lock.contains("images:"), "{lock}");
    assert!(lock.contains("toolchain:"), "{lock}");
}

#[test]
fn corrupt_page_refuses_typed() {
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
        Err(ImageRefusal::CorruptPartition { name }) => assert_eq!(name, partition.name),
        other => panic!("expected CorruptPartition, got {other:?}"),
    }
    assert_eq!(
        ImageRefusal::CorruptPartition {
            name: String::new()
        }
        .code(),
        "E-IMAGE-001"
    );

    // A partition with an empty name/page refuses (no blank pages).
    let blank = ImagePartition {
        name: String::new(),
        kind: PartitionKind::Docs,
        content_id: partition.content_id.clone(),
        body: partition.body.clone(),
    };
    assert!(matches!(
        blank.validate(),
        Err(ImageRefusal::MalformedPartition { .. })
    ));

    // The whole image refuses if ANY partition is corrupt.
    let mut tampered = image.clone();
    tampered.partitions[1].body.push_str("/*tampered*/");
    assert!(tampered.validate_partitions().is_err());
    assert!(image.validate_partitions().is_ok());
}

#[test]
fn image_paths_cells_and_bundle_fixture() {
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
    assert!(
        bytecode.contains("vector-map"),
        "generic vocabulary: {bytecode}"
    );

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
    assert!(matches!(result.disposition, Disposition::Answer { .. }));
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
    assert!(image.image_id.starts_with("fnv1a64:"));

    // Negative seed: corrupt partition is a typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/semantic_images.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-IMAGE"),
        "seed expects a typed image refusal, found: {expect_line}"
    );
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
    let cell = std_cell_registry()
        .get("std.tensor.softmax")
        .expect("std cell present");
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
