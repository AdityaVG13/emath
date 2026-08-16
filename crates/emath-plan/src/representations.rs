//!: representation planning.
//!
//! Conversion nodes carry cost and exact relation evidence; cyclic
//! conversion paths are refused (`E-PROV-517`) and lossy conversions that
//! the goal has not authorized are excluded (`E-PROV-515`).

use emath_ir::ExactnessPolicy;
use std::collections::VecDeque;

/// A declared representation conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversion {
    /// From representation.
    pub from: String,
    /// To representation.
    pub to: String,
    /// Conversion cost.
    pub cost: u8,
    /// Exactness relation to the SIR canonical form.
    pub exact_relation: &'static str,
}

/// One planned conversion node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversionNode {
    /// Conversion used.
    pub conversion: Conversion,
    /// Position in the path.
    pub step: usize,
}

/// Representation planning failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationError {
    /// Stable code (`E-PROV-515`/`E-PROV-517`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Lossy relation detection: exact goals only authorize conservation.
#[must_use]
pub fn is_lossy(relation: &str) -> bool {
    !matches!(relation, "bit-identical" | "value-conserving")
}

/// Finds a shortest conversion path `from -> to` with cycle prevention
/// (BFS over the declared conversion table; revisiting a representation is
/// treated as a cycle and refused).
pub fn find_conversion_path(
    from: &str,
    to: &str,
    conversions: &[Conversion],
    exactness: &ExactnessPolicy,
) -> Result<Vec<ConversionNode>, RepresentationError> {
    if from == to {
        return Ok(vec![]);
    }
    let mut frontier: VecDeque<(String, Vec<ConversionNode>)> = VecDeque::new();
    frontier.push_back((from.to_string(), vec![]));
    let mut visited: Vec<String> = vec![from.to_string()];
    // Exact goals keep looking past a first lossy hit; the refusal fires only
    // when every reachable path into the target is lossy. The target is never
    // marked visited so a later conserving path can still land on it.
    let mut first_lossy: Option<Vec<ConversionNode>> = None;
    while let Some((current, path)) = frontier.pop_front() {
        for conversion in conversions {
            if conversion.from != current {
                continue;
            }
            let next = conversion.to.clone();
            if visited.contains(&next) {
                continue; // cycle prevention: a representation is visited once
            }
            let mut extended = path.clone();
            extended.push(ConversionNode {
                conversion: conversion.clone(),
                step: extended.len(),
            });
            if next == to {
                let lossy = extended
                    .iter()
                    .any(|node| is_lossy(node.conversion.exact_relation));
                if !matches!(exactness, ExactnessPolicy::Exact) || !lossy {
                    return Ok(extended);
                }
                if first_lossy.is_none() {
                    first_lossy = Some(extended);
                }
                continue;
            }
            visited.push(next.clone());
            frontier.push_back((next, extended));
        }
    }
    if first_lossy.is_some() {
        return Err(RepresentationError {
            code: "E-PROV-515",
            message: format!("lossy conversion path {from} -> {to} not authorized by exact goal"),
        });
    }
    Err(RepresentationError {
        code: "E-PROV-517",
        message: format!("no conversion path {from} -> {to} (or cycle refused)"),
    })
}
