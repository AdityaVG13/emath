//! Deterministic example partitions (spec 12: construction / validation /
//! held-out / adversarial).

use emath_term::SymbolId;
use emath_world_ir::fnv1a64;

use crate::example_id;

/// One behavioral example, e.g. `⧖(1 ⋈ 2) ⊛ ζ => 9`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalibrationExample {
    /// Operator the example constrains.
    pub operator: SymbolId,
    /// Input terms, in argument order.
    pub inputs: Vec<String>,
    /// Expected output.
    pub output: String,
    /// Deterministic content identity.
    pub id: u64,
}

impl CalibrationExample {
    /// Builds an example with a deterministic content identity.
    #[must_use]
    pub fn new(
        operator: impl Into<SymbolId>,
        inputs: Vec<String>,
        output: impl Into<String>,
    ) -> Self {
        let operator = operator.into();
        let output = output.into();
        let id = example_id(&operator, &inputs, &output);
        Self {
            operator,
            inputs,
            output,
            id,
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "example:{}:{}:{}",
            self.operator.0,
            self.inputs.join(","),
            self.output
        )
    }
}

/// Which partition an example belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExampleKind {
    /// Examples used during construction/fitting.
    Construction,
    /// Examples used for validation during calibration.
    Validation,
    /// Examples reserved for the held-out challenge; never shown to
    /// construction.
    HeldOut,
    /// Adversarial examples probing edge cases.
    Adversarial,
}

impl ExampleKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Construction => "construction",
            Self::Validation => "validation",
            Self::HeldOut => "held-out",
            Self::Adversarial => "adversarial",
        }
    }
}

/// A deterministic partition of examples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedExamples {
    /// Examples per kind, sorted by example id.
    pub by_kind: Vec<(ExampleKind, Vec<CalibrationExample>)>,
}

impl PartitionedExamples {
    /// Deterministically partitions `examples` by content identity.
    ///
    /// `boundaries_permille` are cumulative bucket ends: construction is
    /// `[0, b0)`, validation `[b0, b1)`, adversarial `[b1, b2)`, and
    /// held-out `[b2, 1000)`. Boundaries must be ascending and at most
    /// 1000. The `salt` re-keys the partition so a new split cannot
    /// replay a memorized one.
    #[must_use]
    pub fn partition(
        examples: &[CalibrationExample],
        boundaries_permille: [u64; 3],
        salt: &str,
    ) -> Self {
        assert!(
            boundaries_permille
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
                && boundaries_permille[2] <= 1000,
            "boundaries_permille must be ascending and at most 1000"
        );
        let mut buckets = [
            (ExampleKind::Construction, Vec::new()),
            (ExampleKind::Validation, Vec::new()),
            (ExampleKind::Adversarial, Vec::new()),
            (ExampleKind::HeldOut, Vec::new()),
        ];
        for example in examples {
            let key = fnv1a64(format!("{}:{}", example.canonical(), salt).as_bytes()) % 1000;
            let kind = match key {
                k if k < boundaries_permille[0] => ExampleKind::Construction,
                k if k < boundaries_permille[1] => ExampleKind::Validation,
                k if k < boundaries_permille[2] => ExampleKind::Adversarial,
                _ => ExampleKind::HeldOut,
            };
            let bucket = buckets
                .iter_mut()
                .find(|(existing, _)| *existing == kind)
                .expect("bucket exists for every kind");
            bucket.1.push(example.clone());
        }
        for (_, bucket) in &mut buckets {
            bucket.sort_by_key(|example| example.id);
        }
        Self {
            by_kind: buckets.to_vec(),
        }
    }

    /// Examples of one kind, sorted by example id.
    #[must_use]
    pub fn kind(&self, kind: ExampleKind) -> &[CalibrationExample] {
        self.by_kind
            .iter()
            .find(|(existing, _)| *existing == kind)
            .map_or(&[], |(_, examples)| examples)
    }

    /// Construction examples.
    #[must_use]
    pub fn construction(&self) -> &[CalibrationExample] {
        self.kind(ExampleKind::Construction)
    }

    /// Held-out challenge examples.
    #[must_use]
    pub fn held_out(&self) -> &[CalibrationExample] {
        self.kind(ExampleKind::HeldOut)
    }
}
