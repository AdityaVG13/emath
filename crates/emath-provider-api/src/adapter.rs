//! Generated provider adapter contracts and conformance fixtures (fjxh.17).
//!
//! A cell schema (fjxh.2, `emath.capability-cell.v1`) GENERATES the
//! adapter contract a provider must satisfy: the capability key (derived
//! from the cell's content identity), the IR-facing trait shape (as
//! DATA — the Rust trait stays generic), deterministic conformance
//! fixtures, and the reference-oracle comparison the adapter output must
//! pass. Providers are checked WORKERS, never the public meaning of an
//! operation: the local oracle stays in `emath-ir`; the adapter carries
//! only the comparison contract.
//!
//! IR purity gate (Neutral IR Constitution §7; same rule as
//! emath-epic-fm-0c8f.12): the IR-facing signature is an ALLOWLIST of
//! IR-owned type tokens. Provider-native types (torch/jax/ndarray, …)
//! refuse typed (`E-PROVIDER-001`) — they cannot leak into the public
//! IR surface, and the gate is enforced at contract generation, not
//! left to caller discipline.
//!
//! Zero core delta: the contract is generated data; no provider is
//! linked, no IR enum grows, and the strict vs Genesis/custom firewall
//! is untouched (adapters live on the provider seam only).

use std::collections::BTreeMap;
use std::fmt;

use emath_ir::capability::{CellSchema, cell_id};

/// Stable schema id for the generated adapter contract.
pub const ADAPTER_CONTRACT_SCHEMA: &str = "emath.provider-adapter.v1";

/// The IR-owned type tokens allowed in a public IR-facing adapter
/// signature. Anything else is a provider-native type and refuses typed.
pub const IR_OWNED_TYPES: [&str; 5] = [
    "scalar<f64>",
    "vector<f64>",
    "matrix<f64>",
    "tensor<f64>",
    "bool",
];

/// Adapter-contract refusal. Closed set; every variant names what was
/// wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterContractError {
    /// `E-PROVIDER-001` — a provider-native type token in the public
    /// IR-facing signature (the gate is an allowlist; torch/jax/ndarray
    /// never pass).
    NativeTypeInIr {
        /// The rejected token.
        token: String,
    },
    /// `E-PROVIDER-002` — the provider reported a reduction axis other
    /// than the one the binding declares (wrong-axis must FAIL).
    AxisMismatch {
        /// The capability the binding is for.
        capability: String,
        /// The declared axis.
        declared: usize,
        /// The axis the provider reported.
        reported: usize,
    },
    /// `E-PROVIDER-003` — an oracle comparison was demanded for a cell
    /// without local reference semantics (handwritten kernels only; a
    /// provider is never its own oracle).
    NoLocalOracle {
        /// The cell without an oracle.
        cell: String,
    },
}

impl AdapterContractError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NativeTypeInIr { .. } => "E-PROVIDER-001",
            Self::AxisMismatch { .. } => "E-PROVIDER-002",
            Self::NoLocalOracle { .. } => "E-PROVIDER-003",
        }
    }
}

impl fmt::Display for AdapterContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativeTypeInIr { token } => write!(
                formatter,
                "{code}: provider-native type `{token}` in the public IR-facing \
                 signature; only IR-owned tokens pass the gate ({allowed})",
                code = self.code(),
                allowed = IR_OWNED_TYPES.join(", ")
            ),
            Self::AxisMismatch {
                capability,
                declared,
                reported,
            } => write!(
                formatter,
                "{code}: `{capability}` provider binding declares reduction axis \
                 {declared}, provider reported {reported}",
                code = self.code()
            ),
            Self::NoLocalOracle { cell } => write!(
                formatter,
                "{code}: `{cell}` has no local reference oracle; a provider is \
                 never its own meaning (handwrite a real kernel first)",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for AdapterContractError {}

/// The IR-facing trait shape of the adapter, as DATA (provider-native
/// types must not leak into IR: every token is allowlist-gated).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterSpec {
    /// Canonical capability path.
    pub cell: String,
    /// Closed cell class token.
    pub class: String,
    /// Declared input arity (from the schema).
    pub arity: u16,
    /// IR-facing type tokens, gated.
    pub ir_signature: Vec<String>,
    /// The declared numeric policy for any comparison.
    pub numeric_policy: &'static str,
}

/// One generated conformance fixture: a deterministic input battery and
/// (when the cell has a local oracle) the pinned oracle output.
#[derive(Clone, Debug, PartialEq)]
pub struct ConformanceFixture {
    /// Deterministic case name.
    pub name: String,
    /// Per-argument input vectors (arity inputs per case).
    pub inputs: Vec<Vec<f64>>,
    /// The local oracle's output, when one exists (`None` = structural
    /// fixture only; a provider is never its own oracle).
    pub expected: Option<Vec<f64>>,
}

/// The verdict of one native-vs-provider comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformanceVerdict {
    /// Bit-exact agreement over the compared outputs.
    Conformant,
    /// The outputs disagree at the named element.
    Diverged {
        /// First diverging element index.
        index: usize,
        /// Native bit pattern.
        native_bits: u64,
        /// Provider bit pattern.
        provider_bits: u64,
    },
    /// The outputs have different shapes (a structural fault, before
    /// any numeric comparison).
    ShapeMismatch {
        /// Native length.
        native_len: usize,
        /// Provider length.
        provider_len: usize,
    },
}

/// Compare one output pair BIT-FOR-BIT: divergence names the position
/// and both bit patterns. A shape difference is a structural fault
/// before any numeric comparison.
#[must_use]
pub fn compare_outputs(native: &[f64], provider: &[f64]) -> ConformanceVerdict {
    if native.len() != provider.len() {
        return ConformanceVerdict::ShapeMismatch {
            native_len: native.len(),
            provider_len: provider.len(),
        };
    }
    for (index, (n, p)) in native.iter().zip(provider.iter()).enumerate() {
        if n.to_bits() != p.to_bits() {
            return ConformanceVerdict::Diverged {
                index,
                native_bits: n.to_bits(),
                provider_bits: p.to_bits(),
            };
        }
    }
    ConformanceVerdict::Conformant
}

/// A provider binding for one capability: which reduction axis the
/// provider must report. Part of the generated contract, checked at the
/// seam — a wrong axis fails typed, never silently reinterpreted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderBinding {
    /// The capability the binding is for.
    pub capability: String,
    /// The declared reduction axis.
    pub reduction_axis: usize,
}

impl ProviderBinding {
    /// Check the provider's reported axis against the declared one.
    pub fn check_axis(&self, reported: usize) -> Result<(), AdapterContractError> {
        if reported != self.reduction_axis {
            return Err(AdapterContractError::AxisMismatch {
                capability: self.capability.clone(),
                declared: self.reduction_axis,
                reported,
            });
        }
        Ok(())
    }
}

/// The generated adapter contract for one cell.
#[derive(Clone, Debug, PartialEq)]
pub struct AdapterContract {
    /// Deterministic capability key: `adapter:<cell>@<version>:<cell-id>`.
    pub capability_key: String,
    /// The IR-facing trait shape (data, gated).
    pub spec: AdapterSpec,
    /// Deterministic conformance fixtures.
    pub fixtures: Vec<ConformanceFixture>,
}

/// The IR purity gate: only [`IR_OWNED_TYPES`] pass. Provider-native
/// types refuse typed — this is the negative-seed scenario.
pub fn ir_type_gate(token: &str) -> Result<(), AdapterContractError> {
    if IR_OWNED_TYPES.contains(&token) {
        Ok(())
    } else {
        Err(AdapterContractError::NativeTypeInIr {
            token: token.to_string(),
        })
    }
}

/// Gate a whole declared signature (role → IR-facing type token): every
/// token must pass the allowlist; the refusal names the offending token.
pub fn gate_signature(signature: &BTreeMap<String, String>) -> Result<(), AdapterContractError> {
    for (_role, token) in signature {
        ir_type_gate(token)?;
    }
    Ok(())
}

/// The local reference oracle table (closed set, data): cells with
/// handwritten strict-f64 semantics pinned in `emath-ir`. Adding an
/// oracle is one entry — providers are never their own meaning.
fn local_oracle(cell: &str) -> Option<fn(&[f64]) -> Option<Vec<f64>>> {
    match cell {
        "std.tensor.softmax" => {
            // The emath-ir oracle returns Result; the fixture contract
            // records `None` for refused inputs (non-finite/empty), so
            // the fixture pin is the oracle's admitted output.
            Some(|logits: &[f64]| {
                emath_ir::capability::softmax_reference_strict_f64(logits)
                    .ok()
            })
        }
        _ => None,
    }
}

/// The deterministic conformance battery per arity (closed, documented):
/// finite fixtures, exact boundaries, and the stable-max stress cases.
fn battery(arity: u16) -> Vec<Vec<Vec<f64>>> {
    let base: [&[f64]; 5] = [
        &[1.0, 2.0, 3.0],
        &[0.0],
        &[-5.0, 0.0, 5.0, 500.0],
        &[1e-300, 1e-300, 1e300],
        &[-742.0, -741.5, 0.0],
    ];
    match arity {
        1 => base.iter().map(|case| vec![case.to_vec()]).collect(),
        _ => base[..2]
            .iter()
            .map(|case| (0..arity).map(|_| case.to_vec()).collect())
            .collect(),
    }
}

/// Generates the adapter contract for an admitted cell schema.
///
/// The capability key is derived from the cell's content identity (so
/// any identity-affecting schema mutation moves the key and invalidates
/// old bindings; `about` does not). Fixtures are the deterministic
/// battery; a cell with local reference semantics additionally pins the
/// oracle output on every fixture. The IR-facing signature is gated
/// through the allowlist at generation time. The generated contract
/// carries NO provider implementation — conformance is a comparison the
/// harness runs (native oracle vs the provider's outputs).
pub fn adapter_contract(schema: &CellSchema) -> Result<AdapterContract, AdapterContractError> {
    let identity = cell_id(schema);
    let capability_key = format!(
        "adapter:{}@{}:{}",
        schema.name.0, schema.version, identity.0
    );
    let signature: Vec<String> = (0..schema.arity)
        .map(|index| {
            if index == 0 {
                "vector<f64>".to_string()
            } else {
                format!("vector<f64>#{index}")
            }
        })
        .collect();
    for token in &signature {
        ir_type_gate(token)?;
    }
    let oracle = local_oracle(&schema.name.0);
    let fixtures = battery(schema.arity)
        .into_iter()
        .enumerate()
        .map(|(index, mut inputs)| {
            let name = format!("{}:case-{index}", schema.name.0);
            // Arity-1 cells pin their oracle output; multi-input oracle
            // pins are later spine work.
            let expected = match oracle {
                Some(oracle) if inputs.len() == 1 => oracle(&inputs[0]),
                _ => {
                    // No oracle for this cell (or arity): structural
                    // fixture only.
                    let _ = &mut inputs;
                    None
                }
            };
            ConformanceFixture {
                name,
                inputs,
                expected,
            }
        })
        .collect();
    Ok(AdapterContract {
        capability_key,
        spec: AdapterSpec {
            cell: schema.name.0.clone(),
            class: schema.class.as_str().to_string(),
            arity: schema.arity,
            ir_signature: signature,
            numeric_policy: "strict-f64",
        },
        fixtures,
    })
}
