#![forbid(unsafe_code)]
#![allow(dead_code)]
/// `AffinePolicy`: a `policy` declaration generated from `.emath`.
/// Generated deterministically by eMath Phase 1; do not edit.
#[derive(Clone, Debug)]
pub struct AffinePolicy {
    scale: f64,
    bias: f64,
}

/// Configuration error type returned by failed constructors.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigError {
    /// A constructor `require` invariant did not hold.
    FailedPrecondition,
}

impl AffinePolicy {
    /// Construct a `AffinePolicy`; every `require` invariant is checked.
    pub fn new(scale: f64, bias: f64) -> Result<Self, ConfigError> {
        {
            let __ok0 = !{
                let __e0 = scale;
                let __e1 = 0.0;
                __e0 >= __e1
            };
            if __ok0 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok1 = !{
                let __e0 = scale;
                __e0.is_finite()
            };
            if __ok1 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok2 = !{
                let __e0 = bias;
                __e0.is_finite()
            };
            if __ok2 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            Ok(Self { scale, bias })
        }
    }
    /// Evaluate `score` (strict-f64, Phase 1).
    pub fn score(&self, x: f64) -> f64 {
        {
            {
                let __e0 = self.scale;
                let __e1 = x;
                let __e2 = __e0 * __e1;
                let __e3 = self.bias;
                __e2 + __e3
            }
        }
    }
}
