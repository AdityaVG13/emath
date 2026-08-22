//! Shapes and layouts.
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

impl std::fmt::Display for Extent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(size) => write!(f, "{size}"),
            Self::Symbolic(name) => f.write_str(name),
        }
    }
}

impl Extent {
    /// Parse a surface type argument into an extent.
    ///
    /// Numeric spellings become `Fixed`; everything else is `Symbolic`.
    #[must_use]
    pub fn from_surface(name: &str) -> Self {
        if let Ok(size) = name.parse::<usize>() {
            Self::Fixed(size)
        } else {
            Self::Symbolic(name.to_string())
        }
    }
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

    /// Declared shape; zero extents and empty tensor ranks are typed refusals.
    pub fn declare(extents: Vec<Extent>) -> Result<Self, ShapeError> {
        if extents.is_empty() {
            return Err(ShapeError {
                code: "E-SHAPE-004",
                message: "declared tensor/vector shape must have rank >= 1".into(),
            });
        }
        for extent in &extents {
            match extent {
                Extent::Fixed(0) => {
                    return Err(ShapeError {
                        code: "E-SHAPE-004",
                        message: "declared extent 0 is not a well-formed shape".into(),
                    });
                }
                Extent::Symbolic(name)
                    if name == "0" || name.eq_ignore_ascii_case("zero") =>
                {
                    return Err(ShapeError {
                        code: "E-SHAPE-004",
                        message: format!("declared extent `{name}` is not a well-formed shape"),
                    });
                }
                _ => {}
            }
        }
        Ok(Self { extents })
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
        // Rank-0 operands never broadcast implicitly (documented policy).
        if self.rank() == 0 || other.rank() == 0 {
            return false;
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
            "shape:[{}]",
            self.extents
                .iter()
                .map(|extent| match extent {
                    Extent::Fixed(size) => format!("f{size}"),
                    Extent::Symbolic(name) => format!("s{name}"),
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
    /// Stable code (`E-SHAPE-001`..`E-SHAPE-004`).
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
