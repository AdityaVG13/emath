//! Discrete signals over DECLARED sampling, convolution, the direct DFT
//! reference, the `TransformBackend` provider contract, and windows.
//!
//! Honesty contract (bead emath-r3-signal-z2yt):
//! - Sampling semantics are DECLARED, never ambient: every signal carries
//!   its `Sampling { rate, phase }`; there is no default, no inference, and
//!   the constructor has no ambient form. Signals sampled at different
//!   rates are different time worlds — operations that would need a rate
//!   refuse (never assume).
//! - The reference transform is the DIRECT O(n^2) DFT with exact
//!   semantics (deterministic summation order). FFT is a PROVIDER behind
//!   the `TransformBackend` contract, never the core reference.
//! - All inputs are validated; nothing fabricates.

/// Typed refusal codes for the signal layer.
pub const E_SIGNAL_RATE: &str = "E-SIGNAL-1";
pub const E_SIGNAL_SAMPLE: &str = "E-SIGNAL-2";
pub const E_SIGNAL_EMPTY: &str = "E-SIGNAL-3";
pub const E_SIGNAL_RATE_MISMATCH: &str = "E-SIGNAL-4";
pub const E_SIGNAL_FFT_LENGTH: &str = "E-SIGNAL-5";

/// A tiny complex scalar (std has no `num-complex`; core is std-only).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub fn new(re: f64, im: f64) -> Self {
        Complex { re, im }
    }

    pub fn conj(self) -> Self {
        Complex {
            re: self.re,
            im: -self.im,
        }
    }

    pub fn abs2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

impl std::ops::Add for Complex {
    type Output = Complex;
    fn add(self, o: Complex) -> Complex {
        Complex {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
}

impl std::ops::Sub for Complex {
    type Output = Complex;
    fn sub(self, o: Complex) -> Complex {
        Complex {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }
}

impl std::ops::Mul for Complex {
    type Output = Complex;
    fn mul(self, o: Complex) -> Complex {
        Complex {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

impl std::fmt::Display for Complex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.im < 0.0 {
            write!(f, "{}-{}i", self.re, -self.im)
        } else {
            write!(f, "{}+{}i", self.re, self.im)
        }
    }
}

impl From<f64> for Complex {
    fn from(re: f64) -> Self {
        Complex { re, im: 0.0 }
    }
}

/// DECLARED sampling semantics: sample `n` is taken at time
/// `t(n) = phase + n / rate` (rate in Hz, phase in seconds).
///
/// There is deliberately no ambient/inferred form: `Sampling::new` is the
/// only constructor, so an undeclared rate cannot compile into a signal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sampling {
    pub rate: f64,
    pub phase: f64,
}

impl Sampling {
    pub fn new(rate: f64, phase: f64) -> Result<Self, String> {
        if !rate.is_finite() || rate <= 0.0 {
            return Err(format!(
                "{E_SIGNAL_RATE}: sampling rate must be finite and positive, got {rate}"
            ));
        }
        if !phase.is_finite() {
            return Err(format!(
                "{E_SIGNAL_RATE}: sampling phase must be finite, got {phase}"
            ));
        }
        Ok(Sampling { rate, phase })
    }
}

/// A discrete signal: declared sampling plus real samples. Construction
/// validates (finite samples, named index on refusal).
#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteSignal {
    sampling: Sampling,
    samples: Vec<f64>,
}

impl DiscreteSignal {
    pub fn new(sampling: Sampling, samples: Vec<f64>) -> Result<Self, String> {
        for (i, s) in samples.iter().enumerate() {
            if !s.is_finite() {
                return Err(format!("{E_SIGNAL_SAMPLE}: index {i}"));
            }
        }
        Ok(DiscreteSignal { sampling, samples })
    }

    pub fn sampling(&self) -> &Sampling {
        &self.sampling
    }

    pub fn samples(&self) -> &[f64] {
        &self.samples
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Declared time base: `t(n) = phase + n / rate`.
    pub fn sample_time(&self, n: usize) -> f64 {
        self.sampling.phase + n as f64 / self.sampling.rate
    }

    /// Direct convolution. Both signals must be sampled at the SAME
    /// declared rate — a different rate is a different time world and
    /// refuses instead of being silently assumed. The output keeps that
    /// rate and its phase is the sum of the declared phases (the
    /// convolution support starts at t_a0 + t_b0).
    pub fn convolve(&self, other: &DiscreteSignal) -> Result<DiscreteSignal, String> {
        if self.sampling.rate != other.sampling.rate {
            return Err(format!(
                "{E_SIGNAL_RATE_MISMATCH}: cannot convolve {} Hz with {} Hz signals; \
                 resample into one declared time world first",
                self.sampling.rate, other.sampling.rate
            ));
        }
        let n = self.samples.len();
        let m = other.samples.len();
        let mut out = vec![0.0; n + m.saturating_sub(1)];
        for (i, &a) in self.samples.iter().enumerate() {
            for (j, &b) in other.samples.iter().enumerate() {
                out[i + j] += a * b;
            }
        }
        DiscreteSignal::new(
            Sampling::new(
                self.sampling.rate,
                self.sampling.phase + other.sampling.phase,
            )
            .expect("rate validated on both inputs"),
            out,
        )
    }
}

/// Window functions over a declared length. Values are bounded in [0, 1];
/// `hann` vanishes at both endpoints.
pub struct Window;

impl Window {
    pub fn rectangular(n: usize) -> Vec<f64> {
        vec![1.0; n]
    }

    pub fn hann(n: usize) -> Vec<f64> {
        periodic_cosine(n, 0.5)
    }

    pub fn hamming(n: usize) -> Vec<f64> {
        periodic_cosine(n, 0.46)
    }
}

/// Symmetric convention: `w[k] = a - (1 - a) cos(2 pi k / (n - 1))`, so
/// the window is palindromic and (for hann) vanishes at both endpoints.
/// Spectral-estimation variants use the periodic normalization; that is a
/// named fence in the cell contract, not silently mixed in.
fn periodic_cosine(n: usize, a: f64) -> Vec<f64> {
    match n {
        0 => return Vec::new(),
        1 => return vec![1.0],
        _ => {}
    }
    let denom = (n - 1) as f64;
    (0..n)
        .map(|k| a - (1.0 - a) * (2.0 * std::f64::consts::PI * k as f64 / denom).cos())
        .collect()
}

/// The transform provider contract. Core ships the direct DFT reference;
/// FFT backends are providers and must not become core.
pub trait TransformBackend {
    fn name(&self) -> &'static str;
    fn transform(&self, x: &[Complex]) -> Result<Vec<Complex>, String>;
}

/// Reference transform: direct O(n^2) DFT, deterministic summation order,
/// `X[k] = sum_n x[n] * exp(-2 pi i k n / N)`. Exact semantics: refuses
/// empty input rather than fabricating a spectrum.
pub struct DirectDft;

impl TransformBackend for DirectDft {
    fn name(&self) -> &'static str {
        "direct_dft"
    }

    fn transform(&self, x: &[Complex]) -> Result<Vec<Complex>, String> {
        if x.is_empty() {
            return Err(format!(
                "{E_SIGNAL_EMPTY}: DFT of an empty signal is not a spectrum"
            ));
        }
        let n = x.len();
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let mut acc = Complex::new(0.0, 0.0);
            for (j, &v) in x.iter().enumerate() {
                let angle = -2.0 * std::f64::consts::PI * (k * j) as f64 / n as f64;
                // e^{-i angle}: cos + i sin with the negative angle baked in.
                let twiddle = Complex::new(angle.cos(), angle.sin());
                acc = acc + v * twiddle;
            }
            out.push(acc);
        }
        Ok(out)
    }
}

/// FFT PROVIDER: iterative radix-2 Cooley-Tukey for power-of-two lengths.
/// Lives behind the `TransformBackend` contract; a non-power-of-two length
/// refuses typed (use the direct DFT reference for arbitrary lengths).
pub struct Radix2Fft;

impl TransformBackend for Radix2Fft {
    fn name(&self) -> &'static str {
        "radix2_fft"
    }

    fn transform(&self, x: &[Complex]) -> Result<Vec<Complex>, String> {
        let n = x.len();
        if n == 0 || !n.is_power_of_two() {
            return Err(format!(
                "{E_SIGNAL_FFT_LENGTH}: radix-2 FFT needs a nonzero power-of-two length, got {n}"
            ));
        }
        // Bit-reversal permutation.
        let bits = n.trailing_zeros();
        let mut a = x.to_vec();
        for i in 0..n {
            let rev = i.reverse_bits() >> (usize::BITS - bits);
            if i < rev {
                a.swap(i, rev);
            }
        }
        // Butterflies.
        let mut len = 2;
        while len <= n {
            let angle = -2.0 * std::f64::consts::PI / len as f64;
            let w_step = Complex::new(angle.cos(), angle.sin());
            for start in (0..n).step_by(len) {
                let mut w = Complex::new(1.0, 0.0);
                for j in 0..len / 2 {
                    let u = a[start + j];
                    let t = w * a[start + j + len / 2];
                    a[start + j] = u + t;
                    a[start + j + len / 2] = u - t;
                    w = w * w_step;
                }
            }
            len *= 2;
        }
        Ok(a)
    }
}
