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
    while let Some((current, path)) = frontier.pop_front() {
        for conversion in conversions {
            if conversion.from != current {
                continue;
            }
            let next = conversion.to.clone();
            if visited.contains(&next) {
                continue; // cycle prevention: a representation is visited once
            }
            visited.push(next.clone());
            let mut extended = path.clone();
            extended.push(ConversionNode {
                conversion: conversion.clone(),
                step: extended.len(),
            });
            if next == to {
                // Exact goals refuse any lossy edge in the path.
                if matches!(exactness, ExactnessPolicy::Exact)
                    && extended
                        .iter()
                        .any(|node| is_lossy(node.conversion.exact_relation))
                {
                    return Err(RepresentationError {
                        code: "E-PROV-515",
                        message: format!(
                            "lossy conversion path {from} -> {to} not authorized by exact goal"
                        ),
                    });
                }
                return Ok(extended);
            }
            frontier.push_back((next, extended));
        }
    }
    Err(RepresentationError {
        code: "E-PROV-517",
        message: format!("no conversion path {from} -> {to} (or cycle refused)"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Vec<Conversion> {
        vec![
            Conversion {
                from: "sir".into(),
                to: "csc-matrix".into(),
                cost: 3,
                exact_relation: "index-conserving sparse mapping",
            },
            Conversion {
                from: "csc-matrix".into(),
                to: "coo-matrix".into(),
                cost: 2,
                exact_relation: "index-conserving sparse mapping",
            },
            Conversion {
                from: "sir".into(),
                to: "f64".into(),
                cost: 0,
                exact_relation: "bit-identical",
            },
        ]
    }

    #[test]
    fn shortest_path_is_planned_with_steps() {
        let path = find_conversion_path(
            "sir",
            "coo-matrix",
            &tables(),
            &ExactnessPolicy::AnyExplicit,
        )
        .unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].conversion.to, "csc-matrix");
        assert_eq!(path[0].step, 0);
        assert_eq!(path[1].conversion.to, "coo-matrix");
        assert_eq!(path[1].step, 1);
    }

    #[test]
    fn identity_conversion_is_empty() {
        let path = find_conversion_path("f64", "f64", &tables(), &ExactnessPolicy::Exact).unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn cyclic_conversion_is_refused() {
        let cyclic = vec![
            Conversion {
                from: "a".into(),
                to: "b".into(),
                cost: 1,
                exact_relation: "value-conserving",
            },
            Conversion {
                from: "b".into(),
                to: "a".into(),
                cost: 1,
                exact_relation: "value-conserving",
            },
        ];
        let error =
            find_conversion_path("a", "c", &cyclic, &ExactnessPolicy::AnyExplicit).unwrap_err();
        assert_eq!(error.code, "E-PROV-517");
        // The cycle a <-> b is never re-entered; a -> b is planned once.
        let path = find_conversion_path("a", "b", &cyclic, &ExactnessPolicy::AnyExplicit).unwrap();
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn lossy_conversion_refused_under_exact_goal() {
        let error = find_conversion_path("sir", "coo-matrix", &tables(), &ExactnessPolicy::Exact)
            .unwrap_err();
        assert_eq!(error.code, "E-PROV-515");
    }

    #[test]
    fn lossy_conversion_allowed_under_tolerance() {
        let path = find_conversion_path(
            "sir",
            "coo-matrix",
            &tables(),
            &ExactnessPolicy::Bounded {
                tolerance_literal: "1e-9".into(),
            },
        )
        .unwrap();
        assert_eq!(path.len(), 2);
    }
}
