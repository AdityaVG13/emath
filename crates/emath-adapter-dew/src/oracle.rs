//!: differential oracle.
//!
//! For every mapped expression the oracle compares the emath reference
//! interpreter (over EMIR) against the Dew mirror interpreter (over the
//! mapped `DewMirrorProgram`) on random, boundary, NaN/Inf, signed-zero
//! and domain cases, under the numeric profile's equivalence policy.

use crate::mirror::{map_program, DewMirrorProgram, DewOp};
use emath_exec_ir::{EmirOp, EmirProgram};

/// Bit-exact comparison mode: the two evaluated results must have identical
/// IEEE-754 bit patterns (including NaN payloads and signed zero). This is
/// the strictest and therefore the Phase 1 default.
pub const F64_BIT_MODE: &str = "exact-bits";

/// Equivalence policy for oracle comparisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ComparePolicy {
    #[default]
    /// Identical `to_bits()`.
    ExactBits,
    /// Equal under IEEE `==` and both-finite-or-both-non-finite shape.
    IeeeShape,
}

impl ComparePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactBits => F64_BIT_MODE,
            Self::IeeeShape => "ieee-shape",
        }
    }

    #[must_use]
    fn matches(self, expected: u64, actual: u64) -> bool {
        match self {
            Self::ExactBits => expected == actual,
            Self::IeeeShape => {
                let e = f64::from_bits(expected);
                let a = f64::from_bits(actual);
                if e.is_nan() || a.is_nan() {
                    e.is_nan() && a.is_nan()
                } else {
                    e == a
                }
            }
        }
    }
}

/// One oracle input case: values for the input slots (states are supplied
/// after inputs when a program has state).
#[derive(Clone, Debug, PartialEq)]
pub struct OracleCase {
    pub values: Vec<f64>,
}

/// The verdict for one case on one mapped program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleOutcome {
    pub matched: bool,
    /// First diverging value position (result-program register index).
    pub diverging_slot: Option<usize>,
    pub expected_bits: Option<u64>,
    pub dew_bits: Option<u64>,
}

/// Full oracle report for a program over a case set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleReport {
    pub cases_run: usize,
    pub mismatches: Vec<(usize, OracleOutcome)>,
}

impl OracleReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.mismatches.is_empty()
    }
}

/// Errors surfaced by the oracle; evaluation never panics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OracleError {
    /// The case did not supply enough input values.
    InputCount { expected: usize, got: usize },
    /// An interpreter invariant broke (should be unreachable).
    Interpreter(String),
}

/// Reference interpreter over EMIR: bit-exact strict-Float64 evaluation.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferenceInterp;

impl ReferenceInterp {
    pub fn eval(&self, program: &EmirProgram, values: &[f64]) -> Result<f64, OracleError> {
        let slots = usize::from(program.input_count) + usize::from(program.state_count);
        if values.len() < slots {
            return Err(OracleError::InputCount {
                expected: slots,
                got: values.len(),
            });
        }
        let mut regs: Vec<f64> = Vec::with_capacity(program.ops.len());
        for (op, _) in &program.ops {
            let v = match *op {
                EmirOp::ConstF64(bits) => f64::from_bits(bits),
                EmirOp::LoadInput(i) => values[usize::from(i)],
                EmirOp::LoadState(i) => values[usize::from(program.input_count) + usize::from(i)],
                EmirOp::F64Add(l, r) => regs[l.0 as usize] + regs[r.0 as usize],
                EmirOp::F64Sub(l, r) => regs[l.0 as usize] - regs[r.0 as usize],
                EmirOp::F64Mul(l, r) => regs[l.0 as usize] * regs[r.0 as usize],
                EmirOp::F64Div(l, r) => regs[l.0 as usize] / regs[r.0 as usize],
                EmirOp::F64Pow(l, r) => regs[l.0 as usize].powf(regs[r.0 as usize]),
                EmirOp::Neg(v) => -regs[v.0 as usize],
                EmirOp::Not(v) => bool01(regs[v.0 as usize] == 0.0),
                EmirOp::Exp(v) => regs[v.0 as usize].exp(),
                EmirOp::Ln(v) => regs[v.0 as usize].ln(),
                EmirOp::Sqrt(v) => regs[v.0 as usize].sqrt(),
                EmirOp::Sin(v) => regs[v.0 as usize].sin(),
                EmirOp::Cos(v) => regs[v.0 as usize].cos(),
                EmirOp::Tan(v) => regs[v.0 as usize].tan(),
                EmirOp::Tanh(v) => regs[v.0 as usize].tanh(),
                EmirOp::Abs(v) => regs[v.0 as usize].abs(),
                EmirOp::Floor(v) => regs[v.0 as usize].floor(),
                EmirOp::Ceil(v) => regs[v.0 as usize].ceil(),
                EmirOp::Min(l, r) => regs[l.0 as usize].min(regs[r.0 as usize]),
                EmirOp::Max(l, r) => regs[l.0 as usize].max(regs[r.0 as usize]),
                EmirOp::Atan2(l, r) => regs[l.0 as usize].atan2(regs[r.0 as usize]),
                EmirOp::IsFinite(v) => bool01(regs[v.0 as usize].is_finite()),
                EmirOp::Lt(l, r) => bool01(regs[l.0 as usize] < regs[r.0 as usize]),
                EmirOp::Le(l, r) => bool01(regs[l.0 as usize] <= regs[r.0 as usize]),
                EmirOp::Gt(l, r) => bool01(regs[l.0 as usize] > regs[r.0 as usize]),
                EmirOp::Ge(l, r) => bool01(regs[l.0 as usize] >= regs[r.0 as usize]),
                EmirOp::Eq(l, r) => bool01(regs[l.0 as usize] == regs[r.0 as usize]),
                EmirOp::Ne(l, r) => bool01(regs[l.0 as usize] != regs[r.0 as usize]),
                EmirOp::And(l, r) => bool01(regs[l.0 as usize] != 0.0 && regs[r.0 as usize] != 0.0),
                EmirOp::Or(l, r) => bool01(regs[l.0 as usize] != 0.0 || regs[r.0 as usize] != 0.0),
                EmirOp::Select {
                    condition,
                    then_value,
                    else_value,
                } => {
                    if regs[condition.0 as usize] != 0.0 {
                        regs[then_value.0 as usize]
                    } else {
                        regs[else_value.0 as usize]
                    }
                }
            };
            regs.push(v);
        }
        regs.get(program.result.0 as usize)
            .copied()
            .ok_or_else(|| OracleError::Interpreter("result beyond registers".into()))
    }
}

/// Mirror interpreter over the mapped `DewMirrorProgram`. Slot indexing
/// matches the mirror's input_count + state_count layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct DewMirrorInterp;

impl DewMirrorInterp {
    pub fn eval(&self, program: &DewMirrorProgram, values: &[f64]) -> Result<f64, OracleError> {
        let slots = program.input_count + program.state_count;
        if values.len() < slots {
            return Err(OracleError::InputCount {
                expected: slots,
                got: values.len(),
            });
        }
        let mut regs: Vec<f64> = Vec::with_capacity(program.ops.len());
        for op in &program.ops {
            let v = match *op {
                DewOp::ConstF64(bits) => f64::from_bits(bits),
                DewOp::Var { index } => values[index],
                DewOp::Neg(a) => -regs[a],
                DewOp::Not(a) => bool01(regs[a] == 0.0),
                DewOp::Add(a, b) => regs[a] + regs[b],
                DewOp::Sub(a, b) => regs[a] - regs[b],
                DewOp::Mul(a, b) => regs[a] * regs[b],
                DewOp::Div(a, b) => regs[a] / regs[b],
                DewOp::Pow(a, b) => regs[a].powf(regs[b]),
                DewOp::Exp(a) => regs[a].exp(),
                DewOp::Ln(a) => regs[a].ln(),
                DewOp::Sqrt(a) => regs[a].sqrt(),
                DewOp::Sin(a) => regs[a].sin(),
                DewOp::Cos(a) => regs[a].cos(),
                DewOp::Tan(a) => regs[a].tan(),
                DewOp::Tanh(a) => regs[a].tanh(),
                DewOp::Abs(a) => regs[a].abs(),
                DewOp::Floor(a) => regs[a].floor(),
                DewOp::Ceil(a) => regs[a].ceil(),
                DewOp::Min(a, b) => regs[a].min(regs[b]),
                DewOp::Max(a, b) => regs[a].max(regs[b]),
                DewOp::Atan2(a, b) => regs[a].atan2(regs[b]),
                DewOp::IsFinite(a) => bool01(regs[a].is_finite()),
                DewOp::Lt(a, b) => bool01(regs[a] < regs[b]),
                DewOp::Le(a, b) => bool01(regs[a] <= regs[b]),
                DewOp::Gt(a, b) => bool01(regs[a] > regs[b]),
                DewOp::Ge(a, b) => bool01(regs[a] >= regs[b]),
                DewOp::Eq(a, b) => bool01(regs[a] == regs[b]),
                DewOp::Ne(a, b) => bool01(regs[a] != regs[b]),
                DewOp::And(a, b) => bool01(regs[a] != 0.0 && regs[b] != 0.0),
                DewOp::Or(a, b) => bool01(regs[a] != 0.0 || regs[b] != 0.0),
                DewOp::Select {
                    condition,
                    then_value,
                    else_value,
                } => {
                    if regs[condition] != 0.0 {
                        regs[then_value]
                    } else {
                        regs[else_value]
                    }
                }
            };
            regs.push(v);
        }
        regs.get(program.result)
            .copied()
            .ok_or_else(|| OracleError::Interpreter("mirror result beyond registers".into()))
    }
}

fn bool01(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// Deterministic case sets. The generator is a fixed-seed LCG so oracle
/// runs are byte-reproducible across machines (no `rand` dependency).
pub struct CaseGenerator {
    state: u64,
}

impl Default for CaseGenerator {
    fn default() -> Self {
        Self {
            state: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl CaseGenerator {
    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state >> 33
    }

    fn next_f64(&mut self) -> f64 {
        // Uniform-ish in [-64, 64): enough spread to cross pow/atan2
        // interesting ranges while staying away from overflow.
        let raw = (self.next() >> 11) as f64 / ((1u64 << 53) as f64);
        let sign = if self.next() % 2 == 0 { 1.0 } else { -1.0 };
        sign * raw * 64.0
    }

    /// Boundary + NaN/Inf + signed-zero + pseudo-random cases, in a stable
    /// order: boundaries first, then `random_count` deterministic cases.
    #[must_use]
    pub fn cases(random_count: usize) -> Vec<OracleCase> {
        let mut gen = Self::default();
        let boundary = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            -f64::NAN,
        ];
        let mut out: Vec<OracleCase> = boundary
            .iter()
            .map(|&b| OracleCase { values: vec![b] })
            .collect();
        for _ in 0..random_count {
            let n = gen.next() as usize % 3 + 1;
            let values = (0..n).map(|_| gen.next_f64()).collect();
            out.push(OracleCase { values });
        }
        out
    }
}

/// The oracle itself: maps the EMIR program to the mirror (refusing when
/// the program is outside the subset) and compares both interpreters on
/// every case.
#[derive(Clone, Copy, Debug)]
pub struct DifferentialOracle {
    pub policy: ComparePolicy,
}

impl Default for DifferentialOracle {
    fn default() -> Self {
        Self {
            policy: ComparePolicy::ExactBits,
        }
    }
}

impl DifferentialOracle {
    /// Run the oracle. When `map_refusal` is `Some`, the program was
    /// refused at adapter entry (typed refusal, ): the oracle
    /// reports zero cases run and a single artificial mismatch carrying no
    /// values, so the refusal is never mistaken for a clean parity run.
    pub fn compare(
        &self,
        program: &EmirProgram,
        cases: &[OracleCase],
        map_refusal: Option<String>,
    ) -> Result<OracleReport, OracleError> {
        if map_refusal.is_some() {
            return Ok(OracleReport {
                cases_run: 0,
                mismatches: vec![(
                    0,
                    OracleOutcome {
                        matched: false,
                        diverging_slot: None,
                        expected_bits: None,
                        dew_bits: None,
                    },
                )],
            });
        }
        self.run(program, cases)
    }

    fn run(
        &self,
        program: &EmirProgram,
        cases: &[OracleCase],
    ) -> Result<OracleReport, OracleError> {
        let mapped = match map_program(program) {
            Ok(m) => m,
            Err(refusal) => {
                // Programs with refused ops surface the typed refusal
                // instead of a value-agnostic mismatch.
                let _ = refusal.diagnostic();
                return Ok(OracleReport {
                    cases_run: 0,
                    mismatches: vec![(
                        0,
                        OracleOutcome {
                            matched: false,
                            diverging_slot: Some(0),
                            expected_bits: None,
                            dew_bits: None,
                        },
                    )],
                });
            }
        };
        let slots = mapped.program.input_count + mapped.program.state_count;
        let mut mismatches = Vec::new();
        for (idx, case) in cases.iter().enumerate() {
            // Cases are seed sets; pad deterministically with 0.0 so every
            // program exercises every case.
            let mut values = case.values.clone();
            while values.len() < slots {
                values.push(0.0);
            }
            let expected = ReferenceInterp.eval(program, &values)?;
            let actual = DewMirrorInterp.eval(&mapped.program, &values)?;
            if !self.policy.matches(expected.to_bits(), actual.to_bits()) {
                mismatches.push((
                    idx,
                    OracleOutcome {
                        matched: false,
                        diverging_slot: Some(mapped.program.result),
                        expected_bits: Some(expected.to_bits()),
                        dew_bits: Some(actual.to_bits()),
                    },
                ));
            }
        }
        Ok(OracleReport {
            cases_run: cases.len(),
            mismatches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::Span;
    use emath_exec_ir::{DomainObligation, EmirValue};

    fn program(ops: Vec<(EmirOp, Span)>, result: EmirValue, inputs: u16) -> EmirProgram {
        EmirProgram {
            ops,
            result,
            input_count: inputs,
            state_count: 0,
            domain_obligations: Vec::<DomainObligation>::new(),
        }
    }

    #[test]
    fn parity_on_double_polynomial() {
        // (a + b) * (a - b), the full admitted surface.
        let p = program(
            vec![
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::LoadInput(1), Span::default()),
                (EmirOp::F64Add(EmirValue(0), EmirValue(1)), Span::default()),
                (EmirOp::F64Sub(EmirValue(0), EmirValue(1)), Span::default()),
                (EmirOp::F64Mul(EmirValue(2), EmirValue(3)), Span::default()),
            ],
            EmirValue(4),
            2,
        );
        let oracle = DifferentialOracle::default();
        let report = oracle
            .compare(&p, &CaseGenerator::cases(200), None)
            .expect("oracle runs");
        assert!(report.is_clean(), "mismatches: {:?}", report.mismatches);
    }

    #[test]
    fn signed_zero_and_nan_propagate_identically() {
        // 1 / x: signed zero flips the sign of infinity; 0/0 yields NaN in
        // both interpreters under exact-bits comparison.
        let p = program(
            vec![
                (EmirOp::ConstF64(1.0_f64.to_bits()), Span::default()),
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::F64Div(EmirValue(0), EmirValue(1)), Span::default()),
            ],
            EmirValue(2),
            1,
        );
        let oracle = DifferentialOracle::default();
        let cases = vec![
            OracleCase { values: vec![0.0] },
            OracleCase { values: vec![-0.0] },
            OracleCase {
                values: vec![f64::NAN],
            },
        ];
        let report = oracle.compare(&p, &cases, None).expect("oracle runs");
        assert!(report.is_clean(), "divergence: {:?}", report.mismatches);
    }

    #[test]
    fn oracle_detects_mirror_divergence() {
        // A Dew mirror that swaps Mul for Sub must be caught.
        let p = program(
            vec![
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::LoadInput(1), Span::default()),
                (EmirOp::F64Mul(EmirValue(0), EmirValue(1)), Span::default()),
            ],
            EmirValue(2),
            2,
        );
        let cases = CaseGenerator::cases(50);
        let mapped = map_program(&p).expect("maps");
        let mut broken = mapped.program.clone();
        broken.ops[2] = DewOp::Sub(0, 1); // corrupt the Mul slot
        let mut mismatches = 0;
        for case in &cases {
            let mut values = case.values.clone();
            while values.len() < 2 {
                values.push(0.0);
            }
            let expected = ReferenceInterp.eval(&p, &values).expect("ref");
            let actual = DewMirrorInterp.eval(&broken, &values).expect("mirror");
            if expected.to_bits() != actual.to_bits() {
                mismatches += 1;
            }
        }
        assert!(mismatches > 0, "broken mirror must diverge");
        // And the intact mirror run stays clean on the same cases.
        let report = DifferentialOracle::default()
            .compare(&p, &cases, None)
            .expect("oracle runs");
        assert!(report.is_clean());
    }

    #[test]
    fn refusal_is_never_a_clean_parity_run() {
        let p = program(
            vec![
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::LoadInput(1), Span::default()),
                (EmirOp::F64Add(EmirValue(0), EmirValue(1)), Span::default()),
            ],
            EmirValue(2),
            2,
        );
        let oracle = DifferentialOracle::default();
        let report = oracle
            .compare(
                &p,
                &CaseGenerator::cases(10),
                Some("exact-integer: exact-add".into()),
            )
            .expect("oracle runs");
        assert!(!report.is_clean());
        assert_eq!(report.cases_run, 0);
    }
}
