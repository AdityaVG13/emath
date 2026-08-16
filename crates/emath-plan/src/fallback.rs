//!: fallback graph.
//!
//! Precomputed contract-preserving fallback/deopt paths: an exact node
//! falls back to a bounded node, then to an estimate node, within the same
//! contract family. Every edge preserves the provider contract.

use std::collections::BTreeMap;

/// One node in the fallback graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackNode {
    /// Provider id.
    pub provider: String,
    /// Exactness tier (`exact`, `bounded`, `estimate`).
    pub tier: String,
    /// Contract family.
    pub contract: String,
}

impl FallbackNode {
    /// Stable identity.
    #[must_use]
    pub fn identity(&self) -> String {
        format!("{}:{}:{}", self.provider, self.tier, self.contract)
    }
}

/// Directed fallback edges within contract families.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FallbackGraph {
    nodes: Vec<FallbackNode>,
    edges: Vec<(usize, usize)>,
}

impl FallbackGraph {
    /// Builds the graph from (provider, tier, contract) triplets. A fallback
    /// edge `exact -> bounded -> estimate` is added for every contract
    /// family, provider by provider, in deterministic order.
    #[must_use]
    pub fn build(triplets: &[(String, String, String)]) -> Self {
        let mut order: BTreeMap<String, usize> = BTreeMap::new();
        let mut nodes: Vec<FallbackNode> = Vec::new();
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (provider, tier, contract) in triplets {
            let node = FallbackNode {
                provider: provider.clone(),
                tier: tier.clone(),
                contract: contract.clone(),
            };
            let index = nodes.len();
            order.insert(node.identity(), index);
            nodes.push(node);
        }
        // Tier ladder within each contract family.
        let tiers = ["exact", "bounded", "estimate"];
        for contract_family in contract_families(&nodes) {
            for provider in providers_of(&nodes, &contract_family) {
                for window in tiers.windows(2) {
                    let from = order.get(&format!("{provider}:{}:{contract_family}", window[0]));
                    let to = order.get(&format!("{provider}:{}:{contract_family}", window[1]));
                    if let (Some(from), Some(to)) = (from, to) {
                        edges.push((*from, *to));
                    }
                }
            }
        }
        Self { nodes, edges }
    }

    /// Nodes in deterministic order.
    #[must_use]
    pub fn nodes(&self) -> &[FallbackNode] {
        &self.nodes
    }

    /// Fallback edges.
    #[must_use]
    pub fn edges(&self) -> &[(usize, usize)] {
        &self.edges
    }

    /// Every edge must stay within one contract family (nothing crosses
    /// contracts silently).
    #[must_use]
    pub fn contracts_preserved(&self) -> bool {
        self.edges.iter().all(|(from, to)| {
            self.nodes
                .get(*from)
                .zip(self.nodes.get(*to))
                .is_some_and(|(left, right)| left.contract == right.contract)
        })
    }

    /// Finds the fallback successor of a node, if any.
    #[must_use]
    pub fn fallback_of(&self, provider: &str, tier: &str, contract: &str) -> Option<&FallbackNode> {
        let identity = format!("{provider}:{tier}:{contract}");
        let from = self
            .nodes
            .iter()
            .position(|node| node.identity() == identity)?;
        self.edges
            .iter()
            .find(|(edge_from, _)| *edge_from == from)
            .and_then(|(_, to)| self.nodes.get(*to))
    }
}

/// Contract families in deterministic order.
fn contract_families(nodes: &[FallbackNode]) -> Vec<String> {
    let mut families: Vec<String> = nodes.iter().map(|node| node.contract.clone()).collect();
    families.sort();
    families.dedup();
    families
}

/// Providers of one contract family in deterministic order.
fn providers_of(nodes: &[FallbackNode], contract: &str) -> Vec<String> {
    let mut providers: Vec<String> = nodes
        .iter()
        .filter(|node| node.contract == contract)
        .map(|node| node.provider.clone())
        .collect();
    providers.sort();
    providers.dedup();
    providers
}
