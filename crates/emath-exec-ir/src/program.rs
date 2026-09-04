//! Program-level structures: `EmirProgram`, `DomainObligation`.

use super::*;

/// Domain obligations recorded during lowering. Phase 1 semantics: the
/// obligation is emitted as an assumption (strict-f64 IEEE behavior); no
/// silent erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainObligation {
    DivisionNonZero,
    SqrtNonNegative,
    LogPositive,
    PowFiniteResult,
}

impl DomainObligation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DivisionNonZero => "division requires a non-zero denominator",
            Self::SqrtNonNegative => "sqrt requires a non-negative argument",
            Self::LogPositive => "ln requires a strictly positive argument",
            Self::PowFiniteResult => "pow result must be finite under strict-f64 policy",
        }
    }
}

/// One lowered definition: a linear op list computing the output.
#[derive(Clone, Debug, PartialEq)]
pub struct EmirProgram {
    pub ops: Vec<(EmirOp, Span)>,
    pub result: EmirValue,
    pub input_count: u16,
    pub state_count: u16,
    pub domain_obligations: Vec<DomainObligation>,
}

impl EmirProgram {
    /// Deterministic SSA dump. Distinct register operands, constant
    /// payloads, nested bodies, counts, and obligations produce distinct
    /// bytes; `op.name()`-only dumps used to collide on those.
    #[must_use]
    pub fn print(&self) -> String {
        let mut out = String::new();
        self.write_print(&mut out, 0);
        out
    }

    pub(super) fn write_print(&self, out: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        out.push_str(&pad);
        out.push_str(&format!("inputs: {}\n", self.input_count));
        out.push_str(&pad);
        out.push_str(&format!("states: {}\n", self.state_count));
        for (index, (op, _)) in self.ops.iter().enumerate() {
            out.push_str(&pad);
            out.push_str(&format!("%{index}: {}\n", op.format_ssa()));
            write_nested_programs(out, op, indent + 1);
        }
        out.push_str(&pad);
        out.push_str(&format!("result: %{}\n", self.result.0));
        for obligation in &self.domain_obligations {
            out.push_str(&pad);
            out.push_str("obligation: ");
            out.push_str(obligation.as_str());
            out.push('\n');
        }
    }
}
