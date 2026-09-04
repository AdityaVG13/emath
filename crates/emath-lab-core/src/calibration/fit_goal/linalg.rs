//! Small dense linear algebra: Jacobi eigenvalues, inversion.

use super::*;

/// Eigenvalues of a real symmetric matrix via the classic Jacobi
/// rotation sweep (small dense matrices; deterministic, no
/// dependencies).
pub(super) fn jacobi_eigenvalues(matrix: &[f64], n: usize, max_sweeps: usize) -> Vec<f64> {
    let mut a = matrix.to_vec();
    for _ in 0..max_sweeps {
        let mut p = 0;
        let mut q = 1;
        let mut largest = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                let value = a[i * n + j].abs();
                if value > largest {
                    largest = value;
                    p = i;
                    q = j;
                }
            }
        }
        if largest < 1e-300 {
            break;
        }
        let app = a[p * n + p];
        let aqq = a[q * n + q];
        let apq = a[p * n + q];
        let theta = 0.5 * (aqq - app) / apq;
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        // Two-sided similarity transform A' = G^T A G: rotate the
        // off-block rows/columns from SAVED values (a later k iteration
        // must never read an entry an earlier one already overwrote),
        // then apply the closed-form 2x2 block update. The rotation
        // annihilates a[p][q]; reading the diagonal off `a` is then the
        // eigenvalue set.
        for k in 0..n {
            if k == p || k == q {
                continue;
            }
            let akp = a[k * n + p];
            let akq = a[k * n + q];
            a[k * n + p] = c * akp - s * akq;
            a[p * n + k] = a[k * n + p];
            a[k * n + q] = s * akp + c * akq;
            a[q * n + k] = a[k * n + q];
        }
        a[p * n + p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q * n + q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p * n + q] = 0.0;
        a[q * n + p] = 0.0;
    }
    (0..n).map(|i| a[i * n + i]).collect()
}

/// Inverse of a real symmetric matrix by Gauss-Jordan elimination with
/// partial pivoting; `None` when singular.
pub(super) fn invert_symmetric(matrix: &[f64], n: usize) -> Option<Vec<f64>> {
    let stride = 2 * n;
    let mut augmented = vec![0.0; n * stride];
    for i in 0..n {
        for j in 0..n {
            augmented[i * stride + j] = matrix[i * n + j];
        }
        augmented[i * stride + n + i] = 1.0;
    }
    for column in 0..n {
        let mut best = column;
        for row in column..n {
            if augmented[row * stride + column].abs()
                > augmented[best * stride + column].abs()
            {
                best = row;
            }
        }
        if augmented[best * stride + column].abs() < f64::EPSILON {
            return None;
        }
        augmented.swap(column, best);
        let pivot = augmented[column * stride + column];
        for k in 0..stride {
            augmented[column * stride + k] /= pivot;
        }
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = augmented[row * stride + column];
            if factor == 0.0 {
                continue;
            }
            for k in 0..stride {
                augmented[row * stride + k] -= factor * augmented[column * stride + k];
            }
        }
    }
    let mut inverse = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            inverse[i * n + j] = augmented[i * stride + n + j];
        }
    }
    Some(inverse)
}
