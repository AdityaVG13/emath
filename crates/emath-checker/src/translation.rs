//!: translation validation.
//!
//! For the supported EMIR/Rust subset, the validator compares the source
//! relation to the semantics recovered from the generated crate, emits an
//! equivalence witness and checks the witness independently. Any mismatch
//! refuses the artifact (`E-EVID-301`); a witness that does not recompute
//! from the observed samples is refused (`E-EVID-302`).

use emath_core::fnv1a64_bytes;

use crate::{identity_of, CheckerError};

/// One row of the source (EMIR) relation: `inputs -> outputs`.
#[derive(Clone, Debug, PartialEq)]
pub struct TranslationRelation {
    /// Relation label (shared with the recovered samples).
    pub label: String,
    /// Input vector.
    pub inputs: Vec<f64>,
    /// Expected output vector.
    pub outputs: Vec<f64>,
}

/// One recovered behavior sample from the generated Rust crate.
#[derive(Clone, Debug, PartialEq)]
pub struct TranslationSample {
    /// Relation label.
    pub label: String,
    /// Input vector.
    pub inputs: Vec<f64>,
    /// Observed output vector.
    pub outputs: Vec<f64>,
}

/// Independently verifiable equivalence witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquivalenceWitness {
    /// FNV-1a64 footprint of the source relation.
    pub relation_footprint: String,
    /// FNV-1a64 footprint of the observed samples.
    pub sample_footprint: String,
}

/// Deterministic row encoding (bit-exact floats) for footprints.
fn encode_row(label: &str, inputs: &[f64], outputs: &[f64]) -> String {
    let input_token: Vec<String> = inputs
        .iter()
        .map(|value| value.to_bits().to_string())
        .collect();
    let output_token: Vec<String> = outputs
        .iter()
        .map(|value| value.to_bits().to_string())
        .collect();
    format!(
        "{}:[{}]->[{}]",
        label,
        input_token.join(","),
        output_token.join(",")
    )
}

fn relation_footprint(relations: &[TranslationRelation]) -> String {
    let mut rows: Vec<String> = relations
        .iter()
        .map(|row| encode_row(&row.label, &row.inputs, &row.outputs))
        .collect();
    rows.sort();
    format!("{:016x}", fnv1a64_bytes(rows.join(";").as_bytes()))
}

fn sample_footprint(samples: &[TranslationSample]) -> String {
    let mut rows: Vec<String> = samples
        .iter()
        .map(|row| encode_row(&row.label, &row.inputs, &row.outputs))
        .collect();
    rows.sort();
    format!("{:016x}", fnv1a64_bytes(rows.join(";").as_bytes()))
}

/// Validates the recovered semantics against the source relation.
///
/// Refuses with `E-EVID-301` when any input row is missing from the
/// recovered samples or produces a different output; otherwise returns
/// the equivalence witness for the artifact.
pub fn validate_translation(
    relations: &[TranslationRelation],
    samples: &[TranslationSample],
) -> Result<EquivalenceWitness, CheckerError> {
    for relation in relations {
        let recovered = samples
            .iter()
            .find(|sample| sample.label == relation.label && sample.inputs == relation.inputs);
        match recovered {
            None => {
                return Err(CheckerError::new(
                    "E-EVID-301",
                    format!(
                        "translation mismatch: no recovered sample for relation row {} {:?}",
                        relation.label, relation.inputs
                    ),
                ));
            }
            Some(sample) => {
                if sample.outputs != relation.outputs {
                    return Err(CheckerError::new(
                        "E-EVID-301",
                        format!(
                            "translation mismatch: row {} {:?} produced {:?}, expected {:?}",
                            relation.label, relation.inputs, sample.outputs, relation.outputs
                        ),
                    ));
                }
            }
        }
    }
    Ok(EquivalenceWitness {
        relation_footprint: relation_footprint(relations),
        sample_footprint: sample_footprint(samples),
    })
}

/// Independently checks a witness against the retained relations/samples
/// (`E-EVID-302` when the footprints do not recompute).
pub fn check_witness(
    witness: &EquivalenceWitness,
    relations: &[TranslationRelation],
    samples: &[TranslationSample],
) -> Result<(), CheckerError> {
    let relation_recomputed = relation_footprint(relations);
    let sample_recomputed = sample_footprint(samples);
    if witness.relation_footprint != relation_recomputed {
        return Err(CheckerError::new(
            "E-EVID-302",
            format!(
                "witness relation footprint {} does not recompute to {}",
                witness.relation_footprint, relation_recomputed
            ),
        ));
    }
    if witness.sample_footprint != sample_recomputed {
        return Err(CheckerError::new(
            "E-EVID-302",
            format!(
                "witness sample footprint {} does not recompute to {}",
                witness.sample_footprint, sample_recomputed
            ),
        ));
    }
    Ok(())
}

/// Deterministic witness identity for artifact records.
#[must_use]
pub fn witness_identity(witness: &EquivalenceWitness) -> emath_core::ContentId {
    identity_of(&format!(
        "witness:v1:{}:{}",
        witness.relation_footprint, witness.sample_footprint
    ))
}
