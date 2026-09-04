//! Flat-SSA construction: register resolution and let-binding policy.

use super::*;

/// Register-inlined SSA body renderer: single-use, provably-total
/// registers inline into their consumer; multi-use/fault-capable ops stay
/// bound as lets, preserving strict eager fault timing. `var_index` also
/// renders the tangent (`__d`) space for the AD torsos.
pub(crate) struct FlatSsa {
    /// `let __eN = <src>;` lines for registers that must stay bound, in
    /// register order.
    pub e_lets: Vec<(String, String)>,
    /// `let __dN = <src>;` tangent lines (same rule, tangent space).
    pub d_lets: Vec<(String, String)>,
    /// Fully resolved primal source of the result register.
    pub e_tail: String,
    /// Fully resolved tangent source of the result register (empty when
    /// `var_index` was `None`).
    pub d_tail: String,
}

/// Scratch state for one body's flattening; resolves a register to fully
/// inlined source on demand, memoized.
pub(super) struct Resolver<'a> {
    program: &'a EmirProgram,
    e_src: &'a [String],
    d_src: &'a [String],
    inline_e: &'a [bool],
    inline_d: &'a [bool],
    e_memo: HashMap<u32, String>,
    d_memo: HashMap<u32, String>,
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

    fn d(&mut self, i: u32) -> Result<String, BackendError> {
        if let Some(s) = self.d_memo.get(&i) {
            return Ok(s.clone());
        }
        let src = self
            .d_src
            .get(i as usize)
            .ok_or_else(|| BackendError::Lowering("flat d-register out of range".into()))?
            .clone();
        let out = self.substitute(&src)?;
        self.d_memo.insert(i, out.clone());
        Ok(out)
    }

    /// Expand `__e{N}`/`__d{N}` tokens for inlined registers to their
    /// (parenthesized) defining expression; others keep their bound name.
    fn substitute(&mut self, src: &str) -> Result<String, BackendError> {
        let mut out = String::with_capacity(src.len());
        let mut i = 0usize;
        while i < src.len() {
            let token_len = if src[i..].starts_with("__e") || src[i..].starts_with("__d") {
                let (kind, start) = if src[i..].starts_with("__e") {
                    ('e', i + 3)
                } else {
                    ('d', i + 3)
                };
                let digits_len = src[start..]
                    .bytes()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                if digits_len > 0 {
                    if let Ok(idx) = src[start..(start + digits_len)].parse::<u32>() {
                        if (idx as usize) < self.program.ops.len() {
                            let replacement = match (
                                kind,
                                self.inline_e.get(idx as usize),
                                self.inline_d.get(idx as usize),
                            ) {
                                ('e', Some(true), _) => format!("({})", self.e(idx)?),
                                ('d', _, Some(true)) => format!("({})", self.d(idx)?),
                                _ => src[i..(start + digits_len)].to_string(),
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
        let name = name
            .strip_prefix("__e")
            .or_else(|| name.strip_prefix("__d"));
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
    matches!(op, EmirOp::Fold { .. })
}

/// Flatten an SSA body; see [`FlatSsa`].
pub(crate) fn flat_ssa(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
    var_index: Option<u16>,
) -> Result<FlatSsa, BackendError> {
    let n = program.ops.len();
    // Primal sources for every register.
    let mut e_src = Vec::with_capacity(n);
    for (op, _) in &program.ops {
        e_src.push(render_expr(&op_expr(
            op, program, names, states, i64_names,
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
    // Tangent programs are no longer a backend operation. They must arrive
    // as a capability artifact rather than reopening mathematical dispatch.
    if var_index.is_some() {
        return Err(BackendError::MissingArtifactContract(
            "tangent program".to_string(),
        ));
    }
    let d_src = Vec::new();
    let inline_d = vec![false; n];
    let mut resolver = Resolver {
        program,
        e_src: &e_src,
        d_src: &d_src,
        inline_e: &inline_e,
        inline_d: &inline_d,
        e_memo: HashMap::new(),
        d_memo: HashMap::new(),
    };
    let mut e_lets = Vec::new();
    let mut d_lets = Vec::new();
    let result = program.result;
    for i in 0..n {
        if !inline_e[i] {
            e_lets.push((format!("__e{i}"), resolver.e(i as u32)?));
        }
        if var_index.is_some() && !inline_d[i] {
            d_lets.push((format!("__d{i}"), resolver.d(i as u32)?));
        }
    }
    let result_idx = result.0 as usize;
    let e_tail = if result_idx < n && inline_e[result_idx] {
        resolver.e(result.0)?
    } else {
        format!("__e{}", result.0)
    };
    let d_tail = if var_index.is_some() {
        if result_idx < n && inline_d[result_idx] {
            resolver.d(result.0)?
        } else {
            format!("__d{}", result.0)
        }
    } else {
        String::new()
    };
    // e_lets must precede d_lets, then the inferred result bindings follow
    // register order within each space; fault order is unchanged because
    // d-sources never fault and e-lets keep relative order.
    Ok(FlatSsa {
        e_lets,
        d_lets,
        e_tail,
        d_tail,
    })
}
