//! Printing a closed quoted program as an `emath function` definition
//! (`emath keep`). Structural only: the program's binder, types, and
//! body are printed as written; nothing is simplified or evaluated.
//!
//! Accepted shape: `recur f in A -> B: function x in A: body`. The
//! definition takes `x: A`, returns `result: B`, and every free `f` in
//! the body becomes a call to the definition's own name.

use super::expr::{path_expr, substitute_path};
use super::{CValue, ConstructorError, ExprKind};
use emath_syntax::formatter::{format_expr, Prec};

fn shape(message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: "E-KEEP-SHAPE".into(),
        message: message.into(),
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The source text of `emath function {name}` for a kept program.
/// `note` becomes the definition's summary comment.
pub fn kept_definition(value: &CValue, name: &str, note: &str) -> Result<String, ConstructorError> {
    if !is_identifier(name) {
        return Err(shape(format!("`{name}` is not a definition name")));
    }
    let CValue::Code(code) = value else {
        return Err(shape("the kept value is not Code"));
    };
    let ExprKind::Recur { name: self_name, ty, body } = &code.expr.kind else {
        return Err(shape("expected `recur f in A -> B: function x in A: body`"));
    };
    let ExprKind::FunctionAbs { param, domain, body } = &body.kind else {
        return Err(shape("the recur body is not `function x in A: ...`"));
    };
    let output = match &ty.kind {
        ExprKind::Call { function, args }
            if args.len() == 2
                && matches!(&function.kind, ExprKind::Path { segments, .. } if segments.len() == 1 && segments[0] == "Fn") =>
        {
            &args[1]
        }
        _ => return Err(shape("the recur type is not `A -> B`")),
    };
    if param == name || param == "result" {
        return Err(shape(format!("the parameter `{param}` collides with `{name}` or `result`")));
    }
    let mut mint = 0;
    let body = substitute_path(body, self_name, &path_expr(name), &mut mint);
    let (mut dom, mut out, mut text) = (String::new(), String::new(), String::new());
    format_expr(&mut dom, domain, Prec::Root);
    format_expr(&mut out, output, Prec::Root);
    format_expr(&mut text, &body, Prec::Root);
    Ok(format!(
        "emath function {name}:\n    # {note}\n    inputs:\n        {param}: {dom}\n    outputs:\n        result: {out}\n    definitions:\n        result = {text}\n"
    ))
}
