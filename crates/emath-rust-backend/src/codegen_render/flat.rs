//! Flat-SSA construction: register resolution and let-binding policy.

use super::{
    BackendError, EmirOp, EmirProgram, HashMap, InputKinds, ValueKind, is_total, op_expr,
    operand_registers, render_expr,
};

/// Register-inlined SSA body renderer: single-use, provably-total
/// registers inline into their consumer; multi-use/fault-capable ops stay
/// bound as lets, preserving eager fault timing — except the right arms
/// of `and`/`or`/`==>`, whose exclusively-fed registers defer into a
/// block (the engine short-circuits those arms; see
/// [`defer_boolean_rights`]).
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
                let digits_len = src[start..].bytes().take_while(u8::is_ascii_digit).count();
                if digits_len > 0 {
                    if let Ok(idx) = src[start..(start + digits_len)].parse::<u32>() {
                        if (idx as usize) < self.program.ops.len() {
                            let replacement = if self.inline_e.get(idx as usize) == Some(&true) {
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
    kinds: &[ValueKind],
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
            kinds,
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
    let deferred = defer_boolean_rights(program, &mut e_src, &inline_e);
    let mut resolver = Resolver {
        program,
        e_src: &e_src,
        inline_e: &inline_e,
        e_memo: HashMap::new(),
    };
    let mut e_lets = Vec::new();
    let result = program.result;
    for i in 0..n {
        if !inline_e[i] && !deferred.contains(&(i as u32)) {
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

/// Boolean right-operand deferral. The engine evaluates `and`/`or`/`==>`
/// right arms only when the left arm does not decide, so a register whose
/// definition feeds ONLY the right arm (an authored inline subexpression,
/// e.g. the `prev[b - cost]` read in a feasibility guard) must not fault
/// eagerly. Each such register's let moves into a block spliced over the
/// right-operand token:
/// `__e_left && { let __e_k = ..; ..; __e_right }`. Registers used
/// anywhere else stay bound eagerly: authored named definitions bind
/// eagerly in the engine (guard-first), so eager fault timing for shared
/// definitions is the faithful lowering. Inlining already defers
/// single-use total registers (they substitute inside the `&&` right
/// side), so only bound lets move.
///
/// Returns the moved registers; their lets are embedded in the spliced
/// blocks and must not be emitted again at body scope.
fn defer_boolean_rights(
    program: &EmirProgram,
    e_src: &mut [String],
    inline_e: &[bool],
) -> std::collections::HashSet<u32> {
    let n = program.ops.len();
    // Register operands per op (body registers only; other value indices
    // are frame inputs, not movable definitions).
    let mut operand_regs: Vec<Vec<u32>> = Vec::with_capacity(n);
    let mut scratch = Vec::new();
    for (op, _) in &program.ops {
        scratch.clear();
        operand_registers(op, &mut scratch);
        operand_regs.push(
            scratch
                .iter()
                .map(|value| value.0)
                .filter(|idx| (*idx as usize) < n)
                .collect(),
        );
    }
    // Ops mentioning each register; exclusivity is decided from these.
    let mut mentioners: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (k, regs) in operand_regs.iter().enumerate() {
        for j in regs {
            mentioners[*j as usize].push(k as u32);
        }
    }
    let mut moved = std::collections::HashSet::new();
    for i in 0..n {
        let (left, right) = match &program.ops[i].0 {
            EmirOp::And(l, r) | EmirOp::Or(l, r) | EmirOp::Imply(l, r) => (*l, *r),
            _ => continue,
        };
        let right_idx = right.0 as usize;
        if right_idx >= n || left == right {
            // Not a body register, or `x and x` — one token, no deferral.
            continue;
        }
        // Transitive feed closure of the right operand (the authored
        // right-arm subexpression, flattened to registers).
        let mut in_closure = vec![false; n];
        let mut stack = vec![right.0];
        while let Some(j) = stack.pop() {
            if in_closure[j as usize] {
                continue;
            }
            in_closure[j as usize] = true;
            for k in &operand_regs[j as usize] {
                if !in_closure[*k as usize] {
                    stack.push(*k);
                }
            }
        }
        // Movable members: the fixpoint of "every consumer is inside the
        // block or is this op". Candidates are the closure's bound
        // registers (not inlined, not the left operand - the condition
        // evaluates outside the block, not the result register - the
        // tail names it at body scope, not already embedded by a nested
        // boolean). Then remove any register with a consumer that is not
        // itself moving: that consumer's let stays at body scope (or in
        // another block) and would reference a moved binding. This also
        // drops shared subexpressions (consumed anywhere outside the
        // right arm), which the engine evaluates eagerly anyway.
        let left_reg = if (left.0 as usize) < n {
            Some(left.0)
        } else {
            None
        };
        let mut member_set: std::collections::HashSet<u32> = (0..n as u32)
            .filter(|j| in_closure[*j as usize])
            .filter(|j| {
                !inline_e[*j as usize]
                    && Some(*j) != left_reg
                    && *j != program.result.0
                    && !moved.contains(j)
            })
            .collect();
        loop {
            let before = member_set.len();
            let current = member_set.clone();
            member_set.retain(|j| {
                mentioners[*j as usize]
                    .iter()
                    .all(|k| *k as usize == i || current.contains(k))
            });
            if member_set.len() == before {
                break;
            }
        }
        let mut members: Vec<u32> = member_set.into_iter().collect();
        // Register order keeps dependencies above uses inside the block.
        members.sort_unstable();
        if members.is_empty() {
            continue;
        }
        // Splice the block over the right-operand token. Operands render
        // exactly once and `left == right` was excluded, so the full token
        // (maximal digit run) must appear exactly once; anything else is a
        // defensive skip, never a miscompile.
        let token = format!("__e{}", right.0);
        let token_digits = token.as_bytes()[3..].to_vec();
        let bytes = e_src[i].as_bytes();
        let mut splice: Option<(usize, usize)> = None;
        let mut count = 0usize;
        let mut p = 0usize;
        while p < bytes.len() {
            if bytes[p..].starts_with(b"__e") {
                let digits = p + 3;
                let end = bytes[digits..]
                    .iter()
                    .position(|b| !b.is_ascii_digit())
                    .map_or(bytes.len(), |offset| digits + offset);
                if end > digits && &bytes[digits..end] == token_digits.as_slice() {
                    count += 1;
                    splice = Some((p, end));
                }
                p = end;
            } else {
                p += 1;
            }
        }
        if count != 1 {
            continue;
        }
        let (start, end) = splice.expect("count == 1 implies a splice position");
        let mut block = String::from("{ ");
        for j in &members {
            block.push_str(&format!("let __e{j} = {}; ", e_src[*j as usize]));
        }
        block.push_str(&token);
        block.push_str(" }");
        e_src[i].replace_range(start..end, &block);
        moved.extend(members);
    }
    moved
}
