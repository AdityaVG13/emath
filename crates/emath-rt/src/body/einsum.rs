// ── Dense operand flattening ──────────────────────────────────────────────

/// Flatten a vector, nested matrix, or tensor into `(shape, row-major data)` so
/// generated tensor indexing and slicing can pass mixed operands without the backend knowing
/// the Rust type of each register.
pub trait EinsumIn {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>);
}

impl EinsumIn for Vec<f64> {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (vec![self.len()], self.clone())
    }
}

impl EinsumIn for [f64] {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (vec![self.len()], self.to_vec())
    }
}

impl EinsumIn for Vec<Vec<f64>> {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        let rows = self.len();
        let cols = self.first().map(Vec::len).unwrap_or(0);
        let mut data = Vec::with_capacity(rows.saturating_mul(cols));
        for row in self {
            data.extend_from_slice(row);
        }
        (vec![rows, cols], data)
    }
}

impl EinsumIn for Tensor {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (self.shape.clone(), self.data.clone())
    }
}

