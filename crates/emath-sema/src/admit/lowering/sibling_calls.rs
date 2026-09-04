//! Sibling user-function call admission: a call whose name
//! matches a sibling `emath function` declaration lowers by pure-inline
//! substitution — the callee's `definitions:` body is lowered into the
//! caller's arena with the parameters bound as definitions over the
//! caller-side argument subtrees, and every callee-local `Variable`
//! reference is folded in through the existing `inline_defs` pass. No
//! new IR node, no runtime callee frame, no capability registry entry.
//! Recursion (inline cycle) and arity mismatch refuse typed; the
//! callee's own admission reports its internal errors.

use super::super::Admitter;
use super::super::infer::Infer;
use emath_core::Span;
use emath_core::tree::{Expr, ExprKind};
use emath_ir::ExprId;
use std::collections::BTreeMap;

/// Maximum nested sibling inlining depth. Phase 1 surface functions are
/// shallow compositions; the cap only bounds pathological chains from
/// exponential blowup.
const INLINE_DEPTH_CAP: usize = 32;

/// The `#` separator for alpha-renamed sibling parameters. `#` is not a
/// valid identifier character (lexer: alphanumeric, `_`, alphabetic,
/// combining marks), so a renamed parameter can never collide with a
/// user-written name — the rename makes cross-function parameter/variable
/// name collisions structurally impossible.
const RENAME_SEP: char = '#';

/// The renamed parameter for `param` of the function `owner`: every
/// occurrence of the parameter inside the function's own body reads the
/// caller-bound argument through this unique name.
pub(in crate::admit) fn renamed_parameter(owner: &str, param: &str) -> String {
    format!("{param}{RENAME_SEP}{owner}")
}

/// Rewrite parameter references inside one sibling function body to the
/// renamed parameters (`u` -> `u#paraboloid`). Binder-introduced names
/// shadow parameters, so renaming stops at binders that rebind a
/// parameter (set-comprehension variables, fold binders, limit
/// variables, `wrt` targets). Called once per sibling function at
/// collection time — never per call.
pub(in crate::admit) fn rename_parameter_uses(
    expr: &Expr,
    map: &BTreeMap<String, String>,
    shadowed: &mut Vec<String>,
) -> Expr {
    let rebuild = |kind: ExprKind| Expr {
        kind,
        source: expr.source,
    };
    match &expr.kind {
        ExprKind::Path { segments, generics } => {
            if segments.len() == 1 && !shadowed.contains(&segments[0]) {
                if let Some(renamed) = map.get(&segments[0]) {
                    return rebuild(ExprKind::Path {
                        segments: vec![renamed.clone()],
                        generics: generics.clone(),
                    });
                }
            }
            expr.clone()
        }
        ExprKind::Call { function, args } => rebuild(ExprKind::Call {
            function: Box::new(rename_parameter_uses(function, map, shadowed)),
            args: args
                .iter()
                .map(|arg| rename_parameter_uses(arg, map, shadowed))
                .collect(),
        }),
        ExprKind::Unary { op, value } => rebuild(ExprKind::Unary {
            op: *op,
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
        }),
        ExprKind::Binary { op, left, right } => rebuild(ExprKind::Binary {
            op: *op,
            left: Box::new(rename_parameter_uses(left, map, shadowed)),
            right: Box::new(rename_parameter_uses(right, map, shadowed)),
        }),
        ExprKind::Index { value, indices } => rebuild(ExprKind::Index {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            indices: indices
                .iter()
                .map(|index| rename_parameter_uses(index, map, shadowed))
                .collect(),
        }),
        ExprKind::List(items) => rebuild(ExprKind::List(
            items
                .iter()
                .map(|item| rename_parameter_uses(item, map, shadowed))
                .collect(),
        )),
        ExprKind::Tuple(items) => rebuild(ExprKind::Tuple(
            items
                .iter()
                .map(|item| rename_parameter_uses(item, map, shadowed))
                .collect(),
        )),
        ExprKind::Set(items) => rebuild(ExprKind::Set(
            items
                .iter()
                .map(|item| rename_parameter_uses(item, map, shadowed))
                .collect(),
        )),
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => rebuild(ExprKind::If {
            condition: Box::new(rename_parameter_uses(condition, map, shadowed)),
            then_value: Box::new(rename_parameter_uses(then_value, map, shadowed)),
            else_value: Box::new(rename_parameter_uses(else_value, map, shadowed)),
        }),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Rational { .. }
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Measured { .. } => expr.clone(),
        ExprKind::Quantity { value, unit } => rebuild(ExprKind::Quantity {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            unit: unit.clone(),
        }),
        ExprKind::Slice { start, end } => rebuild(ExprKind::Slice {
            start: start
                .as_ref()
                .map(|start| Box::new(rename_parameter_uses(start, map, shadowed))),
            end: end
                .as_ref()
                .map(|end| Box::new(rename_parameter_uses(end, map, shadowed))),
        }),
        ExprKind::Approx {
            left,
            right,
            tolerance,
        } => rebuild(ExprKind::Approx {
            left: Box::new(rename_parameter_uses(left, map, shadowed)),
            right: Box::new(rename_parameter_uses(right, map, shadowed)),
            tolerance: tolerance.clone(),
        }),
        ExprKind::Table { headers, rows } => rebuild(ExprKind::Table {
            headers: headers.clone(),
            rows: rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| rename_parameter_uses(cell, map, shadowed))
                        .collect()
                })
                .collect(),
        }),
        ExprKind::SetComprehension {
            element,
            var,
            domain,
            guard,
        } => {
            shadowed.push(var.clone());
            let renamed = rebuild(ExprKind::SetComprehension {
                element: Box::new(rename_parameter_uses(element, map, shadowed)),
                var: var.clone(),
                domain: Box::new(rename_parameter_uses(domain, map, shadowed)),
                guard: guard
                    .as_ref()
                    .map(|guard| Box::new(rename_parameter_uses(guard, map, shadowed))),
            });
            shadowed.pop();
            renamed
        }
        ExprKind::Record { type_path, fields } => rebuild(ExprKind::Record {
            type_path: type_path.clone(),
            fields: fields
                .iter()
                .map(|(name, field)| (name.clone(), rename_parameter_uses(field, map, shadowed)))
                .collect(),
        }),
        ExprKind::WithSeriesPolicy {
            value,
            interpolation,
            extrapolation,
        } => rebuild(ExprKind::WithSeriesPolicy {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            interpolation: interpolation.clone(),
            extrapolation: extrapolation.clone(),
        }),
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => rebuild(ExprKind::Range {
            start: start
                .as_ref()
                .map(|start| Box::new(rename_parameter_uses(start, map, shadowed))),
            end: end
                .as_ref()
                .map(|end| Box::new(rename_parameter_uses(end, map, shadowed))),
            inclusive: *inclusive,
        }),
        ExprKind::Binder {
            kind,
            binders,
            body,
            guard,
        } => {
            let pushed = binders
                .iter()
                .filter(|binder| map.contains_key(&binder.name))
                .map(|binder| binder.name.clone())
                .collect::<Vec<_>>();
            shadowed.extend(pushed.iter().cloned());
            // Binder DOMAINS are expressions in the callee's scope too
            // (`product k in 1..=n`): they must be renamed like the body,
            // or the domain keeps the raw parameter name, which cannot
            // resolve inside the callee environment swap ("unknown
            // variable") for every cross-function call whose callee uses
            // a parameter in a binder range. Binder NAMES stay: they
            // shadow parameters and are already pushed to `shadowed`.
            let renamed_binders = binders
                .iter()
                .map(|binder| emath_core::tree::Binder {
                    name: binder.name.clone(),
                    domain: binder
                        .domain
                        .as_ref()
                        .map(|domain| rename_parameter_uses(domain, map, shadowed)),
                    source: binder.source,
                })
                .collect();
            let renamed = rebuild(ExprKind::Binder {
                kind: *kind,
                binders: renamed_binders,
                body: Box::new(rename_parameter_uses(body, map, shadowed)),
                guard: guard
                    .as_ref()
                    .map(|guard| Box::new(rename_parameter_uses(guard, map, shadowed))),
            });
            shadowed.truncate(shadowed.len() - pushed.len());
            renamed
        }
        ExprKind::Derivative {
            value,
            wrt,
            kind,
            holding,
        } => {
            let wrt_names = wrt.as_ref().map(|wrt| {
                wrt.iter()
                    .filter_map(|expr| match &expr.kind {
                        ExprKind::Path { segments, .. } if segments.len() == 1 => {
                            Some(segments[0].clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            if let Some(names) = &wrt_names {
                shadowed.extend(names.iter().cloned());
            }
            let renamed = rebuild(ExprKind::Derivative {
                value: Box::new(rename_parameter_uses(value, map, shadowed)),
                wrt: wrt.clone(),
                kind: *kind,
                holding: holding.clone(),
            });
            if let Some(names) = &wrt_names {
                shadowed.truncate(shadowed.len() - names.len());
            }
            renamed
        }
        ExprKind::Solve { value, wrt } => {
            let wrt_names = wrt.as_ref().map(|wrt| {
                wrt.iter()
                    .filter_map(|expr| match &expr.kind {
                        ExprKind::Path { segments, .. } if segments.len() == 1 => {
                            Some(segments[0].clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            if let Some(names) = &wrt_names {
                shadowed.extend(names.iter().cloned());
            }
            let renamed = rebuild(ExprKind::Solve {
                value: Box::new(rename_parameter_uses(value, map, shadowed)),
                wrt: wrt.clone(),
            });
            if let Some(names) = &wrt_names {
                shadowed.truncate(shadowed.len() - names.len());
            }
            renamed
        }
        ExprKind::Optimize {
            value,
            wrt,
            maximize,
        } => {
            let wrt_names = wrt.as_ref().map(|wrt| {
                wrt.iter()
                    .filter_map(|expr| match &expr.kind {
                        ExprKind::Path { segments, .. } if segments.len() == 1 => {
                            Some(segments[0].clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            if let Some(names) = &wrt_names {
                shadowed.extend(names.iter().cloned());
            }
            let renamed = rebuild(ExprKind::Optimize {
                value: Box::new(rename_parameter_uses(value, map, shadowed)),
                wrt: wrt.clone(),
                maximize: *maximize,
            });
            if let Some(names) = &wrt_names {
                shadowed.truncate(shadowed.len() - names.len());
            }
            renamed
        }
        ExprKind::At { value, location } => rebuild(ExprKind::At {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            location: Box::new(rename_parameter_uses(location, map, shadowed)),
        }),
        ExprKind::On { value, location } => rebuild(ExprKind::On {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            location: Box::new(rename_parameter_uses(location, map, shadowed)),
        }),
        ExprKind::Conditioned { value, condition } => rebuild(ExprKind::Conditioned {
            value: Box::new(rename_parameter_uses(value, map, shadowed)),
            condition: Box::new(rename_parameter_uses(condition, map, shadowed)),
        }),
        ExprKind::UnitQuery { kind, expr } => rebuild(ExprKind::UnitQuery {
            kind: *kind,
            expr: Box::new(rename_parameter_uses(expr, map, shadowed)),
        }),
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            shadowed.push(var.clone());
            let renamed = rebuild(ExprKind::Limit {
                var: var.clone(),
                target: Box::new(rename_parameter_uses(target, map, shadowed)),
                direction: *direction,
                body: Box::new(rename_parameter_uses(body, map, shadowed)),
            });
            shadowed.pop();
            renamed
        }
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            shadowed.push(var.clone());
            let renamed = rebuild(ExprKind::SampleLimit {
                var: var.clone(),
                target: Box::new(rename_parameter_uses(target, map, shadowed)),
                direction: *direction,
                body: Box::new(rename_parameter_uses(body, map, shadowed)),
            });
            shadowed.pop();
            renamed
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => rebuild(ExprKind::Cases {
            subject: subject
                .as_ref()
                .map(|subject| Box::new(rename_parameter_uses(subject, map, shadowed))),
            arms: arms
                .iter()
                .map(|(condition, value)| {
                    (
                        rename_parameter_uses(condition, map, shadowed),
                        rename_parameter_uses(value, map, shadowed),
                    )
                })
                .collect(),
            else_arm: Box::new(rename_parameter_uses(else_arm, map, shadowed)),
        }),
    }
}

impl Admitter {
    /// Lower a call to a sibling `emath function` by inline substitution.
    ///
    /// Invariant: the caller environment (`params`/`inputs`/
    /// `definitions`/`index_locals`) is swapped out for the duration, so
    /// the callee body resolves names only against its own parameters,
    /// its own definitions, and globals (builtins) — a caller local can
    /// never leak into the callee. Parameters are bound as definitions
    /// over the caller-side argument subtrees, so the runner needs no
    /// callee frame: after `inline_defs`, no callee-local `Variable`
    /// survives in the spliced tree.
    pub(super) fn lower_sibling_call(
        &mut self,
        name: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Option<(ExprId, Infer)> {
        let callee = self.sibling_functions.get(name)?.clone();
        if self.inline_stack.iter().any(|frame| frame == name) {
            self.error(
                "E-TYPE-013",
                format!("recursive call `{name}` refused (inline cycle in sibling functions)"),
                call_span,
            );
            return None;
        }
        if self.inline_stack.len() >= INLINE_DEPTH_CAP {
            self.error(
                "E-TYPE-013",
                format!("sibling-call inlining depth cap {INLINE_DEPTH_CAP} exceeded at `{name}`"),
                call_span,
            );
            return None;
        }
        if callee.params.len() != args.len() {
            self.error(
                "E-TYPE-012",
                format!(
                    "`{name}` arity mismatch: expects {} argument(s), found {}",
                    callee.params.len(),
                    args.len()
                ),
                call_span,
            );
            return None;
        }
        // Lower the arguments in the CALLER context (caller locals and
        // inputs resolve) and check each against the declared parameter
        // type. Every parameter binds as a definition under its RENAMED
        // name (see `rename_parameter_uses`): the callee body only ever
        // references the renamed form, so no caller variable can collide
        // with a parameter and no inlined definition can reference its
        // own name — `inline_defs` terminates by construction.
        let mut bound = Vec::with_capacity(args.len());
        for (arg, (param_name, param_infer)) in args.iter().zip(&callee.params) {
            let Some((arg_id, arg_infer)) = self.lower_expr(arg) else {
                return None;
            };
            if !super::super::infer::infer_conforms(&arg_infer, param_infer) {
                self.error(
                    "E-TYPE-012",
                    format!(
                        "`{name}` argument `{param_name}` type mismatch: expected {param_infer:?}, found {arg_infer:?}"
                    ),
                    arg.source,
                );
                return None;
            }
            // Inline the argument IN THE CALLER CONTEXT, before the
            // environment swap: an argument may reference caller-local
            // definitions (`sphere_field(q)`), and once the caller's
            // `definitions` are swapped out those references can no
            // longer resolve — the raw `Variable` would survive into the
            // spliced tree and the runner would reject it as an unknown
            // input. After this fold the argument subtree is
            // self-contained: free `Variable`s are caller INPUTS only,
            // which the runner binds from the declaration's input table.
            let arg_id = self.inline_defs(arg_id);
            bound.push((param_name.clone(), arg_id, arg_infer));
        }
        // Swap the caller environment out; restored on every exit path.
        let saved_definitions = std::mem::take(&mut self.definitions);
        let saved_params = std::mem::take(&mut self.params);
        let saved_inputs = std::mem::take(&mut self.inputs);
        let saved_index_locals = std::mem::take(&mut self.index_locals);
        for (param_name, arg_id, arg_infer) in &bound {
            self.definitions
                .insert(param_name.clone(), (*arg_id, arg_infer.clone()));
        }
        self.inline_stack.push(name.to_string());
        // Lower the callee's definitions in order; inline each so the
        // spliced tree is self-contained (no callee-local Variable
        // survives into the caller's arena).
        let mut failed = false;
        for stmt in &callee.definitions {
            // Definitions bind as `name = expr` (Assign with a
            // single-segment place), the same shape the definitions
            // section admits elsewhere.
            let emath_core::tree::StmtKind::Assign { target, value } = &stmt.kind else {
                continue;
            };
            if target.segments.len() != 1 || !target.indices.is_empty() {
                continue;
            }
            match self.lower_expr(value) {
                Some((id, infer)) => {
                    let inlined = self.inline_defs(id);
                    self.definitions
                        .insert(target.segments[0].clone(), (inlined, infer));
                }
                None => {
                    failed = true;
                    break;
                }
            }
        }
        let out = if failed {
            None
        } else {
            self.definitions.get(&callee.output_name).cloned()
        };
        self.inline_stack.pop();
        self.definitions = saved_definitions;
        self.params = saved_params;
        self.inputs = saved_inputs;
        self.index_locals = saved_index_locals;
        match out {
            Some(result) => Some(result),
            None => {
                if !failed {
                    self.error(
                        "E-TYPE-003",
                        format!(
                            "sibling function `{name}` defines no `{}` output binding",
                            callee.output_name
                        ),
                        call_span,
                    );
                }
                None
            }
        }
    }
}
