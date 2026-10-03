use super::{Expr, ExprKind, BTreeSet, FnDecl};

pub(super) fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A length (of text, or of a list of subterms) as fixed-width bytes.
pub(super) fn count_bytes(n: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

/// Text preceded by its length, so it cannot run into what follows.
pub(super) fn text_bytes(text: &str, out: &mut Vec<u8>) {
    count_bytes(text.len(), out);
    out.extend_from_slice(text.as_bytes());
}

fn optional_bytes(expr: Option<&Expr>, binders: &mut Vec<String>, out: &mut Vec<u8>) {
    match expr {
        Some(expr) => {
            out.push(1);
            canonical_expr_bytes(expr, binders, out);
        }
        None => out.push(0),
    }
}

fn list_bytes(items: &[Expr], binders: &mut Vec<String>, out: &mut Vec<u8>) {
    count_bytes(items.len(), out);
    for item in items {
        canonical_expr_bytes(item, binders, out);
    }
}

/// Canonical (de Bruijn-style) expression bytes: binder introductions
/// push their name; a single-segment path resolving to a binder emits
/// its binding depth; everything else recurses with stable kind tags.
/// Alpha-equivalent expressions produce identical bytes, so the FNV-1a
/// digest over them is a deterministic canonical identity.
///
/// The encoding is prefix-free: text carries its length, every list of
/// subterms its count, every optional part a presence byte, and every
/// kind is matched. Without that, `[[1], 2]` and `[[1, 2]]`, or
/// `f(g(x), y)` and `f(g(x, y))`, wrote the same bytes and shared an
/// identity whatever the hash. `emath-rt`'s `canonical_tree_bytes`
/// writes the same bytes for the subset it distills.
pub(super) fn canonical_expr_bytes(expr: &Expr, binders: &mut Vec<String>, out: &mut Vec<u8>) {
    match &expr.kind {
        ExprKind::Int(text) => {
            out.push(b'I');
            text_bytes(text, out);
        }
        ExprKind::Float(text) => {
            out.push(b'F');
            text_bytes(text, out);
        }
        ExprKind::Rational { numer, denom } => {
            out.push(b'R');
            text_bytes(numer, out);
            text_bytes(denom, out);
        }
        ExprKind::Str(text) => {
            out.push(b'S');
            text_bytes(text, out);
        }
        ExprKind::Bool(value) => {
            out.push(b'B');
            out.push(u8::from(*value));
        }
        ExprKind::Measured {
            value,
            uncertainty,
            uncertainty_digits,
            distribution,
        } => {
            out.push(b'M');
            text_bytes(value, out);
            text_bytes(uncertainty, out);
            text_bytes(uncertainty_digits, out);
            match distribution {
                Some(name) => {
                    out.push(1);
                    text_bytes(name, out);
                }
                None => out.push(0),
            }
        }
        ExprKind::Path { segments, generics } => {
            if binders.iter().any(|b| b == &segments[0]) {
                let depth = binders.iter().rposition(|b| b == &segments[0]).unwrap_or(0);
                out.push(b'#');
                count_bytes(depth, out);
                // Field reads of a bound name (`a.lo`) keep their fields.
                count_bytes(segments.len() - 1, out);
                for segment in &segments[1..] {
                    text_bytes(segment, out);
                }
            } else {
                out.push(b'P');
                count_bytes(segments.len(), out);
                for segment in segments {
                    text_bytes(segment, out);
                }
            }
            match generics {
                Some(generics) => {
                    out.push(1);
                    text_bytes(&super::carry::spanless(&format!("{generics:?}")), out);
                }
                None => out.push(0),
            }
        }
        ExprKind::Call { function, args } => {
            out.push(b'C');
            canonical_expr_bytes(function, binders, out);
            list_bytes(args, binders, out);
        }
        ExprKind::Index { value, indices } => {
            out.push(b'N');
            canonical_expr_bytes(value, binders, out);
            list_bytes(indices, binders, out);
        }
        ExprKind::Unary { op, value } => {
            out.push(b'U');
            text_bytes(&format!("{op:?}"), out);
            canonical_expr_bytes(value, binders, out);
        }
        ExprKind::Binary { op, left, right } => {
            out.push(b'Y');
            text_bytes(&format!("{op:?}"), out);
            canonical_expr_bytes(left, binders, out);
            canonical_expr_bytes(right, binders, out);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            out.push(b'?');
            canonical_expr_bytes(condition, binders, out);
            canonical_expr_bytes(then_value, binders, out);
            canonical_expr_bytes(else_value, binders, out);
        }
        // A list and a tuple both view as a `Sequence`, so they share a tag.
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            out.push(b'[');
            list_bytes(items, binders, out);
        }
        ExprKind::Set(items) => {
            out.push(b'$');
            list_bytes(items, binders, out);
        }
        ExprKind::Record { type_path, fields } => {
            out.push(b'{');
            count_bytes(type_path.len(), out);
            for segment in type_path {
                text_bytes(segment, out);
            }
            count_bytes(fields.len(), out);
            for (name, value) in fields {
                text_bytes(name, out);
                canonical_expr_bytes(value, binders, out);
            }
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            out.push(b'G');
            out.push(u8::from(*inclusive));
            optional_bytes(start.as_deref(), binders, out);
            optional_bytes(end.as_deref(), binders, out);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            out.push(b'K');
            optional_bytes(subject.as_deref(), binders, out);
            count_bytes(arms.len(), out);
            for (condition, value) in arms {
                canonical_expr_bytes(condition, binders, out);
                canonical_expr_bytes(value, binders, out);
            }
            canonical_expr_bytes(else_arm, binders, out);
        }
        ExprKind::FunctionAbs {
            param,
            domain,
            body,
        } => {
            out.push(b'L');
            canonical_expr_bytes(domain, binders, out);
            binders.push(param.clone());
            canonical_expr_bytes(body, binders, out);
            binders.pop();
        }
        ExprKind::Recur { name, ty, body } => {
            out.push(b'V');
            canonical_expr_bytes(ty, binders, out);
            binders.push(name.clone());
            canonical_expr_bytes(body, binders, out);
            binders.pop();
        }
        ExprKind::Quote { body } => {
            out.push(b'Q');
            canonical_expr_bytes(body, binders, out);
        }
        ExprKind::QuoteBind {
            param,
            domain,
            body,
        } => {
            out.push(b'D');
            canonical_expr_bytes(domain, binders, out);
            binders.push(param.clone());
            canonical_expr_bytes(body, binders, out);
            binders.pop();
        }
        ExprKind::CallableBinder {
            callee,
            param,
            domain,
            body,
        } => {
            out.push(b'A');
            canonical_expr_bytes(callee, binders, out);
            canonical_expr_bytes(domain, binders, out);
            binders.push(param.clone());
            canonical_expr_bytes(body, binders, out);
            binders.pop();
        }
        ExprKind::SequenceCons {
            head,
            tail,
            spread_head,
        } => {
            out.push(if *spread_head { b'+' } else { b';' });
            canonical_expr_bytes(head, binders, out);
            canonical_expr_bytes(tail, binders, out);
        }
        ExprKind::SetComprehension {
            element,
            var,
            domain,
            guard,
        } => {
            out.push(b'%');
            canonical_expr_bytes(domain, binders, out);
            binders.push(var.clone());
            canonical_expr_bytes(element, binders, out);
            optional_bytes(guard.as_deref(), binders, out);
            binders.pop();
        }
        ExprKind::Approx {
            left,
            right,
            tolerance,
        } => {
            out.push(b'~');
            canonical_expr_bytes(left, binders, out);
            canonical_expr_bytes(right, binders, out);
            match tolerance {
                Some(tolerance) => {
                    out.push(1);
                    optional_bytes(tolerance.rtol.as_ref(), binders, out);
                    optional_bytes(tolerance.atol.as_ref(), binders, out);
                }
                None => out.push(0),
            }
        }
        ExprKind::Conditioned {
            value,
            condition,
        } => {
            out.push(b'!');
            canonical_expr_bytes(value, binders, out);
            canonical_expr_bytes(condition, binders, out);
        }
        ExprKind::UnitQuery { kind, expr } => {
            out.push(b'^');
            text_bytes(&format!("{kind:?}"), out);
            canonical_expr_bytes(expr, binders, out);
        }
        ExprKind::Quantity { value, unit } => {
            out.push(b'u');
            canonical_expr_bytes(value, binders, out);
            text_bytes(&super::carry::spanless(&format!("{unit:?}")), out);
        }
        ExprKind::WithSeriesPolicy {
            value,
            interpolation,
            extrapolation,
        } => {
            out.push(b'w');
            canonical_expr_bytes(value, binders, out);
            text_bytes(interpolation.map_or("", |mode| mode.spelling()), out);
            text_bytes(extrapolation.map_or("", |mode| mode.spelling()), out);
        }
        ExprKind::Slice { start, end } => {
            out.push(b'g');
            optional_bytes(start.as_deref(), binders, out);
            optional_bytes(end.as_deref(), binders, out);
        }
        ExprKind::Table { headers, rows } => {
            out.push(b't');
            count_bytes(headers.len(), out);
            for header in headers {
                text_bytes(header, out);
            }
            count_bytes(rows.len(), out);
            for row in rows {
                list_bytes(row, binders, out);
            }
        }
    }
}

pub(super) fn code_identity(expr: &Expr) -> u64 {
    let mut out = Vec::new();
    canonical_expr_bytes(expr, &mut Vec::new(), &mut out);
    fnv64(&out)
}

/// Free path names of a code body: single-segment paths not bound by an
/// internal binder, plus every multi-segment path (module-qualified
/// callables) whose head is not such a binder. A bound head makes the
/// path a field read of that binder (`a.lo` under `function a`: how
/// `quote.body` renders field access on a definition), so it is bound.
/// Binder sites: function/quote/callable binders, `recur` names, and
/// set-comprehension variables.
pub(super) fn free_path_names(expr: &Expr, binders: &mut Vec<String>, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Path { segments, .. } => {
            if !binders.iter().any(|b| b == &segments[0]) {
                if segments.len() == 1 {
                    out.insert(segments[0].clone());
                } else {
                    out.insert(segments.join("."));
                }
            }
        }
        // Binder domains are TYPE expressions (`in Rat`, `in Int`): a
        // type path is not a value binding and never an unbound name.
        // Only the binder's body scope is walked.
        ExprKind::FunctionAbs {
            param,
            domain: _,
            body,
        } => {
            binders.push(param.clone());
            free_path_names(body, binders, out);
            binders.pop();
        }
        ExprKind::Recur {
            name,
            ty: _,
            body,
        } => {
            binders.push(name.clone());
            free_path_names(body, binders, out);
            binders.pop();
        }
        ExprKind::Quote { body } => free_path_names(body, binders, out),
        ExprKind::QuoteBind {
            param,
            domain: _,
            body,
        } => {
            binders.push(param.clone());
            free_path_names(body, binders, out);
            binders.pop();
        }
        ExprKind::CallableBinder {
            callee,
            param,
            domain: _,
            body,
        } => {
            free_path_names(callee, binders, out);
            binders.push(param.clone());
            free_path_names(body, binders, out);
            binders.pop();
        }
        ExprKind::SetComprehension {
            element,
            var,
            domain,
            guard,
        } => {
            free_path_names(domain, binders, out);
            binders.push(var.clone());
            free_path_names(element, binders, out);
            if let Some(guard) = guard {
                free_path_names(guard, binders, out);
            }
            binders.pop();
        }
        ExprKind::Call { function, args } => {
            free_path_names(function, binders, out);
            for arg in args {
                free_path_names(arg, binders, out);
            }
        }
        ExprKind::Index { value, indices } => {
            free_path_names(value, binders, out);
            for index in indices {
                free_path_names(index, binders, out);
            }
        }
        ExprKind::Slice { start, end } => {
            if let Some(start) = start {
                free_path_names(start, binders, out);
            }
            if let Some(end) = end {
                free_path_names(end, binders, out);
            }
        }
        ExprKind::Unary { value, .. } => free_path_names(value, binders, out),
        ExprKind::Binary { left, right, .. } => {
            free_path_names(left, binders, out);
            free_path_names(right, binders, out);
        }
        ExprKind::Approx {
            left,
            right,
            tolerance: _,
        } => {
            free_path_names(left, binders, out);
            free_path_names(right, binders, out);
        }
        ExprKind::Conditioned {
            value,
            condition,
        } => {
            free_path_names(value, binders, out);
            free_path_names(condition, binders, out);
        }
        ExprKind::UnitQuery { expr, .. } => free_path_names(expr, binders, out),
        ExprKind::Quantity { value, .. } => free_path_names(value, binders, out),
        ExprKind::WithSeriesPolicy { value, .. } => free_path_names(value, binders, out),
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            free_path_names(condition, binders, out);
            free_path_names(then_value, binders, out);
            free_path_names(else_value, binders, out);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
            for item in items {
                free_path_names(item, binders, out);
            }
        }
        ExprKind::Record { fields, .. } => {
            for (_, value) in fields {
                free_path_names(value, binders, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                free_path_names(start, binders, out);
            }
            if let Some(end) = end {
                free_path_names(end, binders, out);
            }
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                free_path_names(subject, binders, out);
            }
            for (condition, value) in arms {
                free_path_names(condition, binders, out);
                free_path_names(value, binders, out);
            }
            free_path_names(else_arm, binders, out);
        }
        ExprKind::SequenceCons { head, tail, .. } => {
            free_path_names(head, binders, out);
            free_path_names(tail, binders, out);
        }
        ExprKind::Table { rows, .. } => {
            for row in rows {
                for cell in row {
                    free_path_names(cell, binders, out);
                }
            }
        }
        _ => {}
    }
}

/// Every environment name evaluating `expr` could read: the first
/// segment of each path anywhere in it, binder domains included.
/// Binders are not subtracted, so the set only over-approximates; a
/// closure that captures just these names reads exactly what it read
/// when it captured the whole scope.
pub(super) fn mentioned_names(expr: &Expr, out: &mut BTreeSet<String>) {
    let mut walk = |e: &Expr| mentioned_names(e, out);
    match &expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Rational { .. }
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Measured { .. } => {}
        ExprKind::Path { segments, .. } => {
            if let Some(first) = segments.first() {
                out.insert(first.clone());
            }
        }
        ExprKind::Quantity { value, .. }
        | ExprKind::Unary { value, .. }
        | ExprKind::WithSeriesPolicy { value, .. }
        | ExprKind::UnitQuery { expr: value, .. }
        | ExprKind::Quote { body: value } => walk(value),
        ExprKind::Call { function, args } => {
            walk(function);
            args.iter().for_each(&mut walk);
        }
        ExprKind::Index { value, indices } => {
            walk(value);
            indices.iter().for_each(&mut walk);
        }
        ExprKind::Slice { start, end } | ExprKind::Range { start, end, .. } => {
            start.iter().chain(end).for_each(|e| walk(e));
        }
        ExprKind::Binary { left, right, .. } | ExprKind::SequenceCons { head: left, tail: right, .. } => {
            walk(left);
            walk(right);
        }
        ExprKind::Approx { left, right, tolerance } => {
            walk(left);
            walk(right);
            if let Some(tolerance) = tolerance {
                tolerance.rtol.iter().chain(&tolerance.atol).for_each(&mut walk);
            }
        }
        ExprKind::If { condition, then_value, else_value } => {
            walk(condition);
            walk(then_value);
            walk(else_value);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => items.iter().for_each(&mut walk),
        ExprKind::Table { rows, .. } => rows.iter().flatten().for_each(&mut walk),
        ExprKind::SetComprehension { element, domain, guard, .. } => {
            walk(element);
            walk(domain);
            guard.iter().for_each(|e| walk(e));
        }
        ExprKind::Record { fields, .. } => fields.iter().for_each(|(_, e)| walk(e)),
        ExprKind::Conditioned { value, condition } => {
            walk(value);
            walk(condition);
        }
        ExprKind::Cases { subject, arms, else_arm } => {
            subject.iter().for_each(|e| walk(e));
            arms.iter().for_each(|(c, v)| {
                walk(c);
                walk(v);
            });
            walk(else_arm);
        }
        ExprKind::FunctionAbs { domain, body, .. } | ExprKind::QuoteBind { domain, body, .. } => {
            walk(domain);
            walk(body);
        }
        ExprKind::Recur { ty, body, .. } => {
            walk(ty);
            walk(body);
        }
        ExprKind::CallableBinder { callee, domain, body, .. } => {
            walk(callee);
            walk(domain);
            walk(body);
        }
    }
}

/// Identity stamp of a function declaration: the FNV-1a digest over the
/// canonical bytes of its definition bodies. Alpha-equivalent
/// redefinitions keep the same stamp (they are the same dependency).
pub(super) fn decl_stamp(decl: &FnDecl) -> u64 {
    let mut out = Vec::new();
    for (name, expr) in &decl.defs {
        out.push(b'd');
        text_bytes(name, &mut out);
        canonical_expr_bytes(expr, &mut Vec::new(), &mut out);
    }
    fnv64(&out)
}

