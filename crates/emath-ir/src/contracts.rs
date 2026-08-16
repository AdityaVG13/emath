//! Provider representation contracts.
//!
//! Exact-relation guarantees between a provider representation and the SIR
//! canonical form, with a conversion cost budget and a typed failure code.
//! The registry is deterministic and sealed at construction.

/// A provider representation contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRepresentationContract {
    /// Provider representation name.
    pub representation: &'static str,
    /// SIR canonical family.
    pub family: &'static str,
    /// Exactness relation to the SIR form.
    pub exact_relation: &'static str,
    /// Conversion cost (0 = identity, ascending = more costly).
    pub conversion_cost: u8,
    /// Stable failure code when the relation is breached.
    pub error_code: &'static str,
}

impl ProviderRepresentationContract {
    /// Stable canonical body.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "contract:v1:{}:{}:{}:{}:{}",
            self.representation,
            self.family,
            self.exact_relation,
            self.conversion_cost,
            self.error_code
        )
    }
}

/// Deterministic (sorted) registry of provider representation contracts.
#[derive(Clone, Debug)]
pub struct ContractRegistry {
    contracts: Vec<ProviderRepresentationContract>,
}

impl ContractRegistry {
    /// Sealed registry with the built-in contracts.
    #[must_use]
    pub fn builtin() -> Self {
        let mut contracts = vec![
            ProviderRepresentationContract {
                representation: "f64",
                family: "primitive:float64",
                exact_relation: "bit-identical",
                conversion_cost: 0,
                error_code: "E-PROV-410",
            },
            ProviderRepresentationContract {
                representation: "i64",
                family: "primitive:int64",
                exact_relation: "bit-identical",
                conversion_cost: 0,
                error_code: "E-PROV-410",
            },
            ProviderRepresentationContract {
                representation: "degC",
                family: "unit:kelvin-affine",
                exact_relation: "value-conserving affine mapping",
                conversion_cost: 1,
                error_code: "E-PROV-411",
            },
            ProviderRepresentationContract {
                representation: "csc-matrix",
                family: "shape:matrix",
                exact_relation: "index-conserving sparse mapping",
                conversion_cost: 2,
                error_code: "E-PROV-412",
            },
        ];
        contracts.sort_by(|left, right| left.representation.cmp(right.representation));
        Self { contracts }
    }

    /// Lookup by representation name.
    #[must_use]
    pub fn find(&self, representation: &str) -> Option<&ProviderRepresentationContract> {
        self.contracts
            .iter()
            .find(|contract| contract.representation == representation)
    }

    /// Iterates contracts in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = &ProviderRepresentationContract> {
        self.contracts.iter()
    }
}
