//! Flat-SSA construction: register resolution and let-binding policy.

use super::*;

/// Register-inlined SSA body renderer: single-use, provably-total
/// registers inline into their consumer; multi-use/fault-capable ops stay
/// bound as lets, preserving strict eager fault timing.
pub(crate) struct FlatSsa {
    /// `let __eN = <src>;` lines for registers that must stay bound, in
    /// register order.
    pub e_lets: Vec<(String, String)>,
    /// Fully resolved primal source of the result register.
    pub e_tail: String,
}

/// Scratch state for one body's flattening; resolves a register to fully
/// inlined source on demand, memoized.
pub(super) struct Resolver<'a> {
    program: &'a EmirProgram,
    e_src: &'a [String],
    inline_e: &'a [bool],
    e_memo: HashMap<u32, String>,
}

/// Inline a register definition at its use site. A single Rust token
/// (identifier or numeric literal) is safe bare wherever the register
/// token stood; every other shape keeps one paren layer so the
/// substitution cannot change how the surrounding expression parses.
fn inline_token(definition: String) -> String {
    let bare = !definition.is_empty()
        && definition
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if bare {
        definition
    } else {
        format!("({definition})")
    }
}

impl Resolver<'_> {
    fn e(&mut self, i: u32) -> Result<String, BackendError> {
        if let Some(s) = self.e_memo.get(&i) {
            return Ok(s.clone());
        }
        let src = self
            .e_src
            .get(i as usize)
            .ok_or_else(|| BackendError::Lowering("flat e-register out of range".into()))?
            .clone();
        let out = self.substitute(&src)?;
        self.e_memo.insert(i, out.clone());
        Ok(out)
    }

    /// Expand `__e{N}` tokens for inlined registers to their
    /// (parenthesized) defining expression; others keep their bound name.
    fn substitute(&mut self, src: &str) -> Result<String, BackendError> {
        let mut out = String::with_capacity(src.len());
        let mut i = 0usize;
        while i < src.len() {
            let token_len = if src[i..].starts_with("__e") {
                let start = i + 3;
                let digits_len = src[start..]
                    .bytes()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                if digits_len > 0 {
                    if let Ok(idx) = src[start..(start + digits_len)].parse::<u32>() {
                        if (idx as usize) < self.program.ops.len() {
                            let replacement =
                                if self.inline_e.get(idx as usize) == Some(&true) {
                                    inline_token(self.e(idx)?)
                                } else {
                                    src[i..(start + digits_len)].to_string()
                                };
                            out.push_str(&replacement);
                            start + digits_len - i
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0
            };
            if token_len > 0 {
                i += token_len;
                continue;
            }
            let ch = src[i..]
                .chars()
                .next()
                .expect("index i always lands on a char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
        Ok(out)
    }
}

/// Whether a user-facing name could collide with an internal `__e\d+`
/// register token (the flattening scanner rewrites those tokens in the
/// rendered source). Such programs fall back to non-flat rendering.
pub(super) fn reg_token_collision(names: &[String], states: &[String]) -> bool {
    let is_like = |name: &str| {
        let name = name.strip_prefix("__e");
        matches!(name, Some(rest) if rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    };
    names.iter().any(|n| is_like(n)) || states.iter().any(|n| is_like(n))
}

/// SSA use count: each operand mention plus the program result. Token
/// scans of rendered source over-count (`sign` mentions its arg twice)
/// and under-count operands that do not appear as `__eN` (nested bodies),
/// which made multi-use registers look single-use.
pub(super) fn count_ssa_uses(program: &EmirProgram) -> Vec<u32> {
    let n = program.ops.len();
    let mut uses = vec![0u32; n];
    let mut operands = Vec::new();
    for (op, _) in &program.ops {
        operands.clear();
        operand_registers(op, &mut operands);
        for v in &operands {
            if (v.0 as usize) < n {
                uses[v.0 as usize] += 1;
            }
        }
    }
    if (program.result.0 as usize) < n {
        uses[program.result.0 as usize] += 1;
    }
    uses
}

pub(super) fn has_nested_body(op: &EmirOp) -> bool {
    matches!(
        op,
        EmirOp::Fold { .. }
            | EmirOp::ApplyCapability { .. }
            | EmirOp::Branch { .. }
            | EmirOp::Iterate { .. }
            | EmirOp::Collect { .. }
            | EmirOp::ProgramLiteral { .. }
            | EmirOp::CallFrame { .. }
    )
}

/// Flatten an SSA body; see [`FlatSsa`].
pub(crate) fn flat_ssa(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<FlatSsa, BackendError> {
    let n = program.ops.len();
    // Primal sources for every register.
    let mut e_src = Vec::with_capacity(n);
    for (op, _) in &program.ops {
        e_src.push(render_expr(&op_expr(
            op,
            program,
            names,
            states,
            input_kinds,
        )?));
    }
    let e_direct = count_ssa_uses(program);
    let collision = reg_token_collision(names, states);
    // Nested bodies already flatten with the same `__eN` namespace. Outer
    // token substitution would rewrite those inner names as outer
    // registers, so inlining is disabled for any body that embeds one.
    let nested = program.ops.iter().any(|(op, _)| has_nested_body(op));
    let mut inline_e = vec![false; n];
    for i in 0..n {
        inline_e[i] =
            !collision && !nested && e_direct[i] <= 1 && is_total(&program.ops[i].0, program);
    }
    let mut resolver = Resolver {
        program,
        e_src: &e_src,
        inline_e: &inline_e,
        e_memo: HashMap::new(),
    };
    let mut e_lets = Vec::new();
    let result = program.result;
    for i in 0..n {
        if !inline_e[i] {
            e_lets.push((format!("__e{i}"), resolver.e(i as u32)?));
        }
    }
    let result_idx = result.0 as usize;
    let e_tail = if result_idx < n && inline_e[result_idx] {
        resolver.e(result.0)?
    } else {
        format!("__e{}", result.0)
    };
    Ok(FlatSsa { e_lets, e_tail })
}
