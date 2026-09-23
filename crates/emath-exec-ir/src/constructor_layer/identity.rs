use super::{Expr, ExprKind, BTreeSet, FnDecl};

pub(super) fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Canonical (de Bruijn-style) expression bytes: binder introductions
/// push their name; a single-segment path resolving to a binder emits
/// its binding depth; everything else recurses with stable kind tags.
/// Alpha-equivalent expressions produce identical bytes, so the FNV-1a
/// digest over them is a deterministic canonical identity.
pub(super) fn canonical_expr_bytes(expr: &Expr, binders: &mut Vec<String>, out: &mut Vec<u8>) {
    match &expr.kind {
        ExprKind::Int(text) => {
            out.push(b'I');
            out.extend_from_slice(text.as_bytes());
        }
        ExprKind::Float(text) => {
            out.push(b'F');
            out.extend_from_slice(text.as_bytes());
        }
        ExprKind::Rational { numer, denom } => {
            out.push(b'R');
            out.extend_from_slice(numer.as_bytes());
            out.push(b'/');
            out.extend_from_slice(denom.as_bytes());
        }
        ExprKind::Str(text) => {
            out.push(b'S');
            out.extend_from_slice(text.as_bytes());
        }
        ExprKind::Bool(value) => {
            out.push(b'B');
            out.push(u8::from(*value));
        }
        ExprKind::Path { segments, .. } => {
            if segments.len() == 1 && binders.iter().any(|b| b == &segments[0]) {
                let depth = binders.iter().rposition(|b| b == &segments[0]).unwrap_or(0);
                out.push(b'#');
                out.extend_from_slice(&(depth as u16).to_le_bytes());
            } else {
                out.push(b'P');
                for segment in segments {
                    out.push(b'.');
                    out.extend_from_slice(segment.as_bytes());
                }
            }
        }
        ExprKind::Call { function, args } => {
            out.push(b'C');
            canonical_expr_bytes(function, binders, out);
            for arg in args {
                canonical_expr_bytes(arg, binders, out);
            }
        }
        ExprKind::Index { value, indices } => {
            out.push(b'N');
            canonical_expr_bytes(value, binders, out);
            for index in indices {
                canonical_expr_bytes(index, binders, out);
            }
        }
        ExprKind::Unary { op, value } => {
            out.push(b'U');
            out.extend_from_slice(format!("{op:?}").as_bytes());
            canonical_expr_bytes(value, binders, out);
        }
        ExprKind::Binary { op, left, right } => {
            out.push(b'Y');
            out.extend_from_slice(format!("{op:?}").as_bytes());
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
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
            out.push(b'[');
            for item in items {
                canonical_expr_bytes(item, binders, out);
            }
        }
        ExprKind::Record { type_path, fields } => {
            out.push(b'{');
            for segment in type_path {
                out.push(b'.');
                out.extend_from_slice(segment.as_bytes());
            }
            for (name, value) in fields {
                out.push(b':');
                out.extend_from_slice(name.as_bytes());
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
            if let Some(start) = start {
                canonical_expr_bytes(start, binders, out);
            }
            if let Some(end) = end {
                canonical_expr_bytes(end, binders, out);
            }
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            out.push(b'K');
            if let Some(subject) = subject {
                canonical_expr_bytes(subject, binders, out);
            }
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
        ExprKind::SequenceCons { head, tail } => {
            out.push(b';');
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
            if let Some(guard) = guard {
                canonical_expr_bytes(guard, binders, out);
            }
            binders.pop();
        }
        ExprKind::Approx {
            left,
            right,
            tolerance: _,
        } => {
            out.push(b'~');
            canonical_expr_bytes(left, binders, out);
            canonical_expr_bytes(right, binders, out);
        }
        ExprKind::Conditioned {
            value,
            condition,
        } => {
            out.push(b'!');
            canonical_expr_bytes(value, binders, out);
            canonical_expr_bytes(condition, binders, out);
        }
        ExprKind::UnitQuery { expr, .. } => {
            out.push(b'^');
            canonical_expr_bytes(expr, binders, out);
        }
        ExprKind::Quantity { value, .. } => {
            out.push(b'u');
            canonical_expr_bytes(value, binders, out);
        }
        ExprKind::WithSeriesPolicy { value, .. } => {
            out.push(b'w');
            canonical_expr_bytes(value, binders, out);
        }
        ExprKind::Slice { start, end } => {
            out.push(b'g');
            if let Some(start) = start {
                canonical_expr_bytes(start, binders, out);
            }
            if let Some(end) = end {
                canonical_expr_bytes(end, binders, out);
            }
        }
        ExprKind::Table { rows, .. } => {
            out.push(b't');
            for row in rows {
                for cell in row {
                    canonical_expr_bytes(cell, binders, out);
                }
            }
        }
        _ => out.push(b'X'),
    }
}

pub(super) fn code_identity(expr: &Expr) -> u64 {
    let mut out = Vec::new();
    canonical_expr_bytes(expr, &mut Vec::new(), &mut out);
    fnv64(&out)
}

/// Free path names of a code body: single-segment paths not bound by an
/// internal binder, plus every multi-segment path (module-qualified
/// callables). Binder sites: function/quote/callable binders, `recur`
/// names, and set-comprehension variables.
pub(super) fn free_path_names(expr: &Expr, binders: &mut Vec<String>, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Path { segments, .. } => {
            if segments.len() == 1 {
                if !binders.iter().any(|b| b == &segments[0]) {
                    out.insert(segments[0].clone());
                }
            } else {
                out.insert(segments.join("."));
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
        ExprKind::SequenceCons { head, tail } => {
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

/// Identity stamp of a function declaration: the FNV-1a digest over the
/// canonical bytes of its definition bodies. Alpha-equivalent
/// redefinitions keep the same stamp (they are the same dependency).
pub(super) fn decl_stamp(decl: &FnDecl) -> u64 {
    let mut out = Vec::new();
    for (name, expr) in &decl.defs {
        out.push(b'd');
        out.extend_from_slice(name.as_bytes());
        canonical_expr_bytes(expr, &mut Vec::new(), &mut out);
    }
    fnv64(&out)
}

