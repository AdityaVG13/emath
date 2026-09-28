// ── Mixed Int / Float64 comparison ────────────────────────────────────────

/// Exact mixed `Int` vs `Float64` compare. Widening `n as f64` is a lie
/// past 2^53: `((1<<53)+1) as f64 == (1<<53) as f64`. Returns `None` for
/// NaN (IEEE unordered). `+0`/`-0` compare equal. `i64::MAX as f64` is
/// 2^63 (outside i64), so the bound is `2^63`, not `i64::MAX as f64`.
pub fn cmp_i64_f64(n: i64, x: f64) -> Option<core::cmp::Ordering> {
    if x.is_nan() {
        return None;
    }
    if x == f64::INFINITY {
        return Some(core::cmp::Ordering::Less);
    }
    if x == f64::NEG_INFINITY {
        return Some(core::cmp::Ordering::Greater);
    }
    // First f64 integer outside i64. `i64::MAX as f64` *is* this value.
    const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
    let trunc = x.trunc();
    if trunc < i64::MIN as f64 {
        return Some(core::cmp::Ordering::Greater);
    }
    if trunc >= TWO_POW_63 {
        return Some(core::cmp::Ordering::Less);
    }
    let xi = trunc as i64;
    match n.cmp(&xi) {
        core::cmp::Ordering::Equal if x == trunc => Some(core::cmp::Ordering::Equal),
        core::cmp::Ordering::Equal if x > 0.0 => Some(core::cmp::Ordering::Less),
        core::cmp::Ordering::Equal => Some(core::cmp::Ordering::Greater),
        other => Some(other),
    }
}

/// Exact mixed equality; `false` for NaN (IEEE `==`).
pub fn eq_i64_f64(n: i64, x: f64) -> bool {
    matches!(cmp_i64_f64(n, x), Some(core::cmp::Ordering::Equal))
}

/// Row-major storage with explicit extents, including zero-row matrices.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix<T = f64> {
    rows: usize,
    cols: usize,
    data: Vec<T>,
}

impl<T> Matrix<T> {
    pub fn new(rows: usize, cols: usize, data: Vec<T>) -> Result<Self, &'static str> {
        if rows.checked_mul(cols) != Some(data.len()) {
            return Err("E-MATRIX-SHAPE: invalid dimensions or data length");
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn into_data(self) -> Vec<T> { self.data }
    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        if row >= self.rows || col >= self.cols { return None; }
        self.data.get(row * self.cols + col)
    }
}

/// Format binary64 in scientific notation without a significance policy.
/// Precision counts digits after the decimal point; allocation failure refuses.
pub fn format_scientific(value: f64, precision: i64) -> Result<String, &'static str> {
    use std::fmt::Write;
    let precision = usize::try_from(precision).map_err(|_| "E-SCALAR-CONVERT: negative precision")?;
    let capacity = if value.is_finite() {
        precision.checked_add(32).ok_or("E-SCALAR-CONVERT: precision exceeds capacity")?
    } else { 32 };
    let mut text = String::new();
    text.try_reserve_exact(capacity).map_err(|_| "E-SCALAR-CONVERT: formatting allocation failed")?;
    write!(&mut text, "{value:.precision$e}").map_err(|_| "E-SCALAR-CONVERT: formatting failed")?;
    Ok(text)
}

/// Numeric storage layout. The stored length does not certify shape validity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DenseLayout {
    Scalar,
    Vector(usize),
    Matrix { rows: usize, cols: usize, len: usize },
    Tensor { shape: Vec<usize>, len: usize },
}

impl DenseLayout {
    pub fn len(&self) -> usize {
        match self {
            Self::Scalar => 1,
            Self::Vector(len) | Self::Matrix { len, .. } | Self::Tensor { len, .. } => *len,
        }
    }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

/// Numeric callback result carrier. Float64, Int, and vector results remain distinct.
#[derive(Clone, Debug, PartialEq)]
pub enum NumericProgramResult {
    Scalar(f64),
    Integer(i64),
    Vector(Vec<f64>),
}

impl NumericProgramResult {
    pub fn into_vector(self) -> Result<Vec<f64>, String> {
        match self {
            Self::Scalar(value) => Ok(vec![value]),
            Self::Vector(values) => Ok(values),
            Self::Integer(_) => Err("E-TYPE-012: program result must be Float64 or Vector<Float64>".into()),
        }
    }

    pub fn into_real(self) -> Result<f64, String> {
        match self {
            Self::Scalar(value) => Ok(value),
            Self::Integer(value) => Ok(value as f64),
            Self::Vector(_) => Err("E-TYPE-012: program result must be a real scalar".into()),
        }
    }

    pub fn into_scalar(self) -> Result<f64, String> {
        match self {
            Self::Scalar(value) => Ok(value),
            Self::Vector(_) | Self::Integer(_) => Err("E-TYPE-012: program result must be Float64".into()),
        }
    }
}
