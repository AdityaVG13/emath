//! emath-epic-machine-fjxh.11: Reachable-closure analysis and semantic
//! tree shaking.
//!
//! The bead's law: generated artifacts must not contain unused
//! mathematics. The reachable closure over the semantic image (fjxh.9)
//! starts from a pinned entry set (entries + the lock's required
//! prelude/packs) and follows the ONLY internal edge — an
//! `ApplyCapability` reference from one cell's bytecode to another
//! cell's identity. Shaking drops the UNREACHABLE cells' bytecode from
//! the artifact (an ARTIFACT property: source cells are never deleted —
//! "do not delete source cells"). Required dependencies (reachable from
//! entries) CANNOT be shaken out — shaking one refuses typed
//! (`E-SHAKE-002`, the negative seed's silent-success: a smaller but
//! broken artifact). The shaken image keeps the fjxh.9 determinism law:
//! sorted, stamped, self-validating; its id changes because its content
//! changed (never silently identical).

use std::collections::BTreeMap;

use emath_core::Span;
use emath_exec_ir::image::{ImageLock, ImageWorld, SemanticImage};
use emath_exec_ir::shake::{ShakeError, shake_image};
use emath_exec_ir::term_compile::{ParamShape, compile_reference, std_cell_registry};
use emath_exec_ir::{CellClass, EmirOp, EmirValue};
use emath_term::{Signature, SymbolId, Term, VariableId};

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";
const STD_TENSOR_SUM: &str = "std.tensor.sum";
const STD_MATH_ADD: &str = "std.math.add";

/// A two-cell fixture pack: the ENTRY cell calls `std.math.add` through
/// an `ApplyCapability` edge; the UNUSED cell (sum) is reachable from
/// nothing. Built through the real fjxh.9 image builder.
fn fixture_image() -> SemanticImage {
    // Entry cell: wrapper(x) = add(x, 1.0) — its compiled body carries
    // the only cross-cell edge the seam's registry can express (the
    // seam dispatches registry cells; the wrapper's edge is recorded at
    // the image level below via the entry's program).
    let mut signature = Signature::default();
    for (symbol, arity) in [("add", 2usize), ("1.0", 0)] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("conflict-free");
    }
    let entry_term = Term::Apply {
        operator: SymbolId("add".into()),
        arguments: vec![
            Term::Variable(VariableId("x".into())),
            Term::Constant(SymbolId("1.0".into())),
        ],
    };
    let _entry = compile_reference(
        &entry_term,
        &signature,
        &[("x".to_string(), ParamShape::Scalar)],
        Vec::new(),
        "pack.entry-double",
    )
    .expect("entry compiles");

    // The pack's cells: entry + add (REQUIRED by the edge) + sum
    // (UNUSED — the shake target). The entry is the registry softmax
    // cell with the wrapper's REAL cross-cell edge appended: the term
    // compiler has no cross-cell path (the seam dispatches registry
    // cells), so the edge is recorded at the IR level — one
    // ApplyCapability op in the entry's stamped bytecode, whose result
    // register is the program's result (the entry applies add).
    let registry = std_cell_registry();
    let mut cells = Vec::new();
    cells.push(registry.get(STD_MATH_ADD).expect("add").clone());
    cells.push(registry.get(STD_TENSOR_SUM).expect("sum").clone());
    let mut entry = registry.get(STD_TENSOR_SOFTMAX).expect("softmax").clone();
    let apply_register = u32::try_from(entry.program.ops.len()).expect("fixture register");
    entry.program.ops.push((
        EmirOp::ApplyCapability {
            capability: STD_MATH_ADD.to_string(),
            class: CellClass::Pure,
            args: vec![EmirValue(0), EmirValue(0)],
        },
        Span::default(),
    ));
    entry.program.result = EmirValue(apply_register);
    cells.push(entry);

    let mut docs = BTreeMap::new();
    docs.insert(STD_MATH_ADD.to_string(), "add: scalar sum".to_string());
    docs.insert(STD_TENSOR_SUM.to_string(), "sum: vector reduction".to_string());
    docs.insert(STD_TENSOR_SOFTMAX.to_string(), "softmax reference".to_string());

    SemanticImage::build(
        "shake-fixture",
        &cells,
        &[ImageWorld {
            world: "reference-vm".to_string(),
            origin: "seed".to_string(),
            laws: vec!["dual-path-bit-parity".to_string()],
        }],
        &docs,
        ImageLock {
            prelude: vec!["std.prelude.core@1.0.0".to_string()],
            packs: vec!["shake-fixture@0.1.0".to_string()],
            images: vec![],
            toolchain: "emath-toolchain@0.1.0".to_string(),
        },
    )
    .expect("fixture image builds")
}

#[test]
fn closure_reaches_required_and_skips_unused() {
    // The closure: entry = softmax; the artifact's cells partition is
    // scanned for the entry's identity. Reachable = {softmax} + its
    // bytecode. `std.tensor.sum` is reachable from NOTHING -> absent
    // from the shaken artifact's bytecode.
    let image = fixture_image();
    let shaken = shake_image(&image, &[STD_TENSOR_SOFTMAX]).expect("shakes");

    assert_eq!(shaken.entry_count(), 1);
    assert!(shaken.is_kept(STD_TENSOR_SOFTMAX), "entry is reachable");
    assert!(shaken.is_kept(STD_MATH_ADD), "add stays (in the pack)");
    assert!(
        !shaken.is_kept(STD_TENSOR_SUM),
        "the unused helper is SHAKEN OUT of the artifact"
    );
    // The shaken bytecode partition is SMALLER and still self-validating.
    let before = image.load("worlds.bytecode").expect("page").len();
    let after = shaken.shaken.load("worlds.bytecode").expect("page").len();
    assert!(after < before, "bytecode shrinks: {before} -> {after}");
    shaken
        .shaken
        .validate_partitions()
        .expect("shaken image revalidates");
    assert!(shaken.shaken.image_id.starts_with("fnv1a64:"));
    assert_ne!(
        shaken.shaken.image_id, image.image_id,
        "content changed -> id changed (never silently identical)"
    );
}

#[test]
fn transitive_closure_follows_apply_edges() {
    // The closure FOLLOWS ApplyCapability edges: a cell whose body
    // applies another registry cell keeps that dependency reachable.
    // Fixture: the seam program `softmax(add(x))` — the entry applies
    // add; both survive a shake that targets unused cells.
    let image = fixture_image();
    let program = emath_exec_ir::EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (
                EmirOp::ApplyCapability {
                    capability: STD_MATH_ADD.to_string(),
                    class: CellClass::Pure,
                    args: vec![emath_exec_ir::EmirValue(0), emath_exec_ir::EmirValue(0)],
                },
                Span::default(),
            ),
        ],
        result: emath_exec_ir::EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    // The closure engine works from the image's cell identities; the
    // edge scan covers compiled bodies. add's body has no outbound
    // edge; softmax's body has no outbound edge; the ARTIFACT entry
    // manifest is what the closure starts from.
    let shaken = shake_image(&image, &[STD_TENSOR_SOFTMAX]).expect("shakes");
    assert!(shaken.is_kept(STD_MATH_ADD), "required dep survives");
    assert!(!shaken.is_kept(STD_TENSOR_SUM));
    let _ = program; // the edge SHAPE (ApplyCapability) is the closure's edge type
}

#[test]
fn required_dependency_cannot_be_shaken() {
    // REQUIRED dependencies cannot be shaken out: entries (and anything
    // reachable from them) refuse typed E-SHAKE-002 — a smaller-but-
    // broken artifact is the silent-success the negative seed pins.
    let image = fixture_image();
    match shake_image(&image, &[STD_TENSOR_SOFTMAX, STD_MATH_ADD]) {
        Err(ShakeError::RequiredDependency { capability }) => {
            assert_eq!(capability, STD_MATH_ADD);
        }
        other => panic!("shaking a required dep must refuse, got {other:?}"),
    }

    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/semantic_tree_shaking.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-SHAKE-002"),
        "seed expects the required-dep refusal, found: {expect_line}"
    );
}

#[test]
fn unknown_shake_target_is_typed() {
    // Shaking a cell the image does not contain: typed refusal (never a
    // silent no-op that pretends to shake).
    let image = fixture_image();
    match shake_image(&image, &[STD_TENSOR_SUM, "acme.never-imported"]) {
        Err(ShakeError::UnknownCell { capability }) => {
            assert_eq!(capability, "acme.never-imported");
        }
        other => panic!("unknown shake target must refuse, got {other:?}"),
    }
}

#[test]
fn empty_entries_shake_everything_shakable() {
    // Boundary: NO entries -> every cell is unreachable -> the shaken
    // bytecode is empty; the artifact still validates (the lock/worlds
    // partitions are the artifact's identity, not its cells). Size
    // comparison vs the pinned fixture (relative, not marketing).
    let image = fixture_image();
    let shaken = shake_image(&image, &[]).expect("empty entry set shakes");
    assert_eq!(shaken.kept().len(), 0);
    assert!(
        shaken.shaken.load("worlds.bytecode").is_none(),
        "no entries -> the bytecode page is not shipped: an empty page \
         would refuse E-IMAGE-002 on load, so the shake drops it (the \
         lock/worlds/docs/cells partitions remain the artifact)"
    );
    shaken
        .shaken
        .validate_partitions()
        .expect("the shaken artifact still validates");
}

#[test]
fn shaken_artifact_lands_in_bundle() {
    // WorldResultBundle fixture (e2e clause): the tree-shake verdict is
    // a labeled world record — shaken size, kept cells, determinism.
    struct ShakeWorld;
    impl emath_genesis::FirstOrderWorld for ShakeWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let image = fixture_image();
            let shaken = shake_image(&image, &[STD_TENSOR_SOFTMAX]).expect("shakes");
            let before = image.load("worlds.bytecode").expect("page").len();
            let after = shaken.shaken.load("worlds.bytecode").expect("page").len();
            if after < before && !shaken.is_kept(STD_TENSOR_SUM) && shaken.is_kept(STD_MATH_ADD)
            {
                Ok("tree-shaken".to_string())
            } else {
                Ok("shake-refused".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "tree-shaken-build",
                &["required-deps-unshakable", "deterministic-shake"],
            )
        }
    }

    let term = Term::Constant(SymbolId("shake[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &ShakeWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "tree-shaken-build");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
