//!: shapes and layouts.
//!
//! Ranks, fixed and symbolic extents, broadcasting, slices and sparse
//! target representations. Invalid tensor/matrix operations are typed
//! refusals with stable `E-SHAPE-*` codes.

use emath_core::fnv1a64_bytes;

/// A single extent: fixed or symbolic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Extent {
    /// Fixed size.
    Fixed(usize),
    /// Symbolic size (resolved downstream).
    Symbolic(String),
}

/// Shape of a tensor/matrix value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Extents in rank order.
    pub extents: Vec<Extent>,
}

impl Shape {
    /// Scalar shape.
    #[must_use]
    pub fn scalar() -> Self {
        Self {
            extents: Vec::new(),
        }
    }

    /// Vector of symbolic extent `n`.
    #[must_use]
    pub fn vector(symbolic: &str) -> Self {
        Self {
            extents: vec![Extent::Symbolic(symbolic.to_string())],
        }
    }

    /// Fixed matrix shape.
    #[must_use]
    pub fn matrix(rows: usize, cols: usize) -> Self {
        Self {
            extents: vec![Extent::Fixed(rows), Extent::Fixed(cols)],
        }
    }

    /// Rank.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.extents.len()
    }

    /// True when all extents are equal (same rank and symbolic/fixed
    /// structure); used by broadcasting and elementwise ops.
    #[must_use]
    pub fn conforms(&self, other: &Self) -> bool {
        self == other
    }

    /// Broadcast-compatible when ranks differ by at most one trailing
    /// dimension (deterministic check; no silent broadcasting of rank-0).
    #[must_use]
    pub fn broadcastable_with(&self, other: &Self) -> bool {
        if self.rank() > other.rank() + 1 || other.rank() > self.rank() + 1 {
            return false;
        }
        if self.conforms(other) {
            return true;
        }
        // Same rank: extents must agree pairwise or one side must be 1.
        if self.rank() == other.rank() {
            return self
                .extents
                .iter()
                .zip(&other.extents)
                .all(|(l, r)| l == r || is_one(l) || is_one(r));
        }
        let (smaller, larger) = if self.rank() < other.rank() {
            (self, other)
        } else {
            (other, self)
        };
        larger.extents[1..]
            .iter()
            .zip(&smaller.extents)
            .all(|(l, r)| l == r || is_one(r))
    }

    /// Validated matrix product `(m, n) x (n, p) -> (m, p)`.
    pub fn mat_mul(&self, right: &Self) -> Result<Shape, ShapeError> {
        if self.rank() != 2 || right.rank() != 2 {
            return Err(ShapeError {
                code: "E-SHAPE-001",
                message: "matrix product requires rank-2 operands".into(),
            });
        }
        if self.extents[1] != right.extents[0] {
            return Err(ShapeError {
                code: "E-SHAPE-002",
                message: format!(
                    "inner extents differ: {:?} vs {:?}",
                    self.extents[1], right.extents[0]
                ),
            });
        }
        Ok(Shape {
            extents: vec![self.extents[0].clone(), right.extents[1].clone()],
        })
    }

    /// Slice result shape (`start..end` per axis).
    pub fn slice(&self, rows_start: usize, rows_end: usize) -> Result<Shape, ShapeError> {
        if rows_start > rows_end {
            return Err(ShapeError {
                code: "E-SHAPE-003",
                message: "slice start exceeds end".into(),
            });
        }
        let mut extents = self.extents.clone();
        if let Some(first) = extents.first_mut() {
            match first {
                Extent::Fixed(size) => {
                    if rows_end > *size {
                        return Err(ShapeError {
                            code: "E-SHAPE-003",
                            message: format!("slice end {rows_end} exceeds extent {size}"),
                        });
                    }
                    *first = Extent::Fixed(rows_end - rows_start);
                }
                Extent::Symbolic(_) => {}
            }
        }
        Ok(Shape { extents })
    }

    /// Canonical encoding.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "shape:v1:[{}]",
            self.extents
                .iter()
                .map(|extent| match extent {
                    Extent::Fixed(size) => size.to_string(),
                    Extent::Symbolic(name) => name.clone(),
                })
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// FNV-1a64 identity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }
}

fn is_one(extent: &Extent) -> bool {
    matches!(extent, Extent::Fixed(1))
}

/// Shape failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeError {
    /// Stable code (`E-SHAPE-001`..`E-SHAPE-003`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Sparse layout targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SparseLayout {
    /// Dense representation.
    Dense,
    /// Compressed sparse column.
    Csc,
    /// Compressed sparse row.
    Csr,
    /// Block-sparse.
    BlockSparse,
}

impl SparseLayout {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dense => "dense",
            Self::Csc => "csc",
            Self::Csr => "csr",
            Self::BlockSparse => "block-sparse",
        }
    }

    /// Whether a layout may represent a rank-2 shape (target constraint).
    #[must_use]
    pub fn accepts_rank(&self, rank: usize) -> bool {
        matches!(
            (self, rank),
            (Self::Dense, 0..=3) | (Self::Csc | Self::Csr | Self::BlockSparse, 2)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_product_checks_inner_extents() {
        let left = Shape::matrix(2, 3);
        let right = Shape::matrix(3, 4);
        let result = left.mat_mul(&right).unwrap();
        assert_eq!(result, Shape::matrix(2, 4));
        let error = Shape::matrix(2, 5)
            .mat_mul(&Shape::matrix(3, 4))
            .unwrap_err();
        assert_eq!(error.code, "E-SHAPE-002");
        let error = Shape::vector("n").mat_mul(&Shape::vector("m")).unwrap_err();
        assert_eq!(error.code, "E-SHAPE-001");
    }

    #[test]
    fn broadcasting_is_bounded_and_deterministic() {
        assert!(Shape::matrix(2, 3).broadcastable_with(&Shape::matrix(2, 3)));
        assert!(Shape::matrix(2, 3).broadcastable_with(&Shape::matrix(1, 3)));
        assert!(!Shape::matrix(2, 3).broadcastable_with(&Shape::matrix(3, 2)));
        assert!(!Shape::scalar().broadcastable_with(&Shape::matrix(2, 3)));
    }

    #[test]
    fn slices_report_oob() {
        assert_eq!(
            Shape::matrix(4, 2).slice(1, 3).unwrap(),
            Shape::matrix(2, 2)
        );
        let error = Shape::matrix(4, 2).slice(3, 9).unwrap_err();
        assert_eq!(error.code, "E-SHAPE-003");
    }

    #[test]
    fn layouts_obey_target_constraints() {
        assert!(SparseLayout::Csr.accepts_rank(2));
        assert!(!SparseLayout::Csc.accepts_rank(3));
        assert!(SparseLayout::Dense.accepts_rank(0));
    }

    #[test]
    fn shape_identity_is_stable() {
        let shape = Shape::matrix(2, 3);
        assert_eq!(shape.identity(), shape.identity());
        assert_ne!(shape.identity(), Shape::matrix(3, 2).identity());
        assert_eq!(shape.canonical(), "shape:v1:[2,3]");
    }
}
