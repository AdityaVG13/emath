//! Deterministic Rust renderer with byte-range anchors.

use crate::ast::{
    escape_ident, BinOp, Block, Expr, Item, Module, Param, Stmt, Ty, UnOp, Visibility,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub label: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderResult {
    pub code: String,
    pub anchors: Vec<Anchor>,
}

struct Code {
    buf: String,
    indent: usize,
    anchors: Vec<Anchor>,
}

impl Code {
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.buf.push_str("    ");
        }
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    fn raw(&mut self, text: &str) {
        self.buf.push_str(text);
    }

    fn blank(&mut self) {
        self.buf.push('\n');
    }

    fn anchor(&mut self, label: &str, start: usize) {
        self.anchors.push(Anchor {
            label: label.to_string(),
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end: u32::try_from(self.buf.len()).unwrap_or(u32::MAX),
        });
    }
}

/// Render a module to deterministic Rust source.
#[must_use]
pub fn render_module(module: &Module) -> RenderResult {
    let mut code = Code {
        buf: String::new(),
        indent: 0,
        anchors: Vec::new(),
    };
    // Doc comments attach to their target item and raw attributes (crate
    // headers) stay adjacent, so the output is rustfmt-stable.
    for (index, item) in module.items.iter().enumerate() {
        if index > 0
            && !matches!(
                module.items[index - 1],
                Item::DocComment(_) | Item::RawAttribute(_)
            )
        {
            code.blank();
        }
        render_item(&mut code, item);
    }
    RenderResult {
        code: code.buf,
        anchors: code.anchors,
    }
}

fn render_doc(code: &mut Code, doc: &[String]) {
    for line in doc {
        code.line(&format!("/// {line}"));
    }
}

fn render_item(code: &mut Code, item: &Item) {
    match item {
        Item::RawAttribute(attribute) => code.line(attribute),
        Item::DocComment(line) => {
            code.line(&format!("/// {line}"));
        }
        Item::Struct(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            let derives = if def.derives.is_empty() {
                String::new()
            } else {
                format!("#[derive({})]", def.derives.join(", "))
            };
            if !derives.is_empty() {
                code.line(&derives);
            }
            let visibility = match def.visibility {
                Visibility::Public => "pub ",
                Visibility::Private => "",
            };
            code.line(&format!("{visibility}struct {} {{", def.name));
            code.indent += 1;
            for (name, ty) in &def.fields {
                code.line(&format!("{}: {},", escape_ident(name), render_ty(ty)));
            }
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("struct {}", def.name), start);
        }
        Item::Enum(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            let derives = if def.derives.is_empty() {
                String::new()
            } else {
                format!("#[derive({})]", def.derives.join(", "))
            };
            if !derives.is_empty() {
                code.line(&derives);
            }
            let visibility = match def.visibility {
                Visibility::Public => "pub ",
                Visibility::Private => "",
            };
            code.line(&format!("{visibility}enum {} {{", def.name));
            code.indent += 1;
            for variant in &def.variants {
                render_doc(code, &variant.doc);
                code.line(&format!("{},", variant.name));
            }
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("enum {}", def.name), start);
        }
        Item::Fn(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            for attribute in &def.attrs {
                code.line(attribute);
            }
            let visibility = match def.visibility {
                Visibility::Public => "pub ",
                Visibility::Private => "",
            };
            let params = def
                .params
                .iter()
                .map(render_param)
                .collect::<Vec<_>>()
                .join(", ");
            code.line(&format!(
                "{visibility}fn {}({params}) -> {} {{",
                escape_ident(&def.name),
                render_ty(&def.ret)
            ));
            code.indent += 1;
            render_stmt(code, &def.body, true);
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("fn {}", def.name), start);
        }
        Item::Test(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            code.line("#[test]");
            code.line(&format!("fn {}() {{", escape_ident(&def.name)));
            code.indent += 1;
            render_stmt(code, &def.body, true);
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("test {}", def.name), start);
        }
        Item::Impl(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            code.line(&format!("impl {} {{", def.target));
            code.indent += 1;
            for method in &def.methods {
                render_doc(code, &method.doc);
                for attribute in &method.attrs {
                    code.line(attribute);
                }
                let visibility = match method.visibility {
                    Visibility::Public => "pub ",
                    Visibility::Private => "",
                };
                let params = method
                    .params
                    .iter()
                    .map(render_param)
                    .collect::<Vec<_>>()
                    .join(", ");
                code.line(&format!(
                    "{visibility}fn {}({params}) -> {} {{",
                    escape_ident(&method.name),
                    render_ty(&method.ret)
                ));
                code.indent += 1;
                render_stmt(code, &method.body, true);
                code.indent -= 1;
                code.line("}");
            }
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("impl {}", def.target), start);
        }
    }
}

/// Block-valued expressions are indented line-by-line instead of flattened,
/// so generated code matches rustfmt output byte-for-byte. `Un(Not, Block)`
/// is the common constructed shape (`!{ ... }` guards).
fn block_value(expr: &Expr) -> Option<(String, &Block)> {
    match expr {
        Expr::Block(stmt) => match &**stmt {
            Stmt::Block(block) => Some((String::new(), block)),
            _ => None,
        },
        Expr::Un {
            op: UnOp::Not,
            value,
        } => match &**value {
            Expr::Block(stmt) => match &**stmt {
                Stmt::Block(block) => Some(("!".to_string(), block)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Render `prefix{ ... }suffix` with statements indented one level inside.
fn render_block_expr(code: &mut Code, block: &Block, prefix: &str, suffix: &str) {
    code.line(&format!("{prefix}{{"));
    code.indent += 1;
    let count = block.statements.len();
    for (index, stmt) in block.statements.iter().enumerate() {
        render_stmt(code, stmt, index + 1 == count);
    }
    code.indent -= 1;
    code.line(&format!("}}{suffix}"));
}

/// Render a statement; `tail` marks the final statement (expression without
/// semicolon when it is the block tail).
fn render_stmt(code: &mut Code, stmt: &Stmt, tail: bool) {
    match stmt {
        Stmt::Block(block) => render_block(code, block),
        Stmt::Let { pattern, value } => {
            if let Some((prefix, block)) = block_value(value) {
                render_block_expr(
                    code,
                    block,
                    &format!("let {} = {prefix}", escape_ident(pattern)),
                    ";",
                );
            } else {
                code.line(&format!(
                    "let {} = {};",
                    escape_ident(pattern),
                    render_expr(value)
                ));
            }
        }
        Stmt::Return(value) => {
            code.line(&format!("return {};", render_expr(value)));
        }
        Stmt::Expr(expr) => {
            // `if` statements never take a trailing semicolon and their
            // blocks are indented line-by-line (rustfmt-stable output).
            if let Expr::IfElse {
                condition,
                then,
                else_value,
            } = expr
            {
                code.line(&format!("if {} {{", render_expr(condition)));
                code.indent += 1;
                render_stmt(code, then, true);
                code.indent -= 1;
                let has_else = match &**else_value {
                    Stmt::Block(block) => !block.statements.is_empty(),
                    _ => true,
                };
                if has_else {
                    code.line("} else {");
                    code.indent += 1;
                    render_stmt(code, else_value, true);
                    code.indent -= 1;
                }
                code.line("}");
                return;
            }
            if tail {
                if let Some((prefix, block)) = block_value(expr) {
                    render_block_expr(code, block, &prefix, "");
                } else {
                    code.line(&render_expr(expr));
                }
            } else {
                code.line(&format!("{};", render_expr(expr)));
            }
        }
    }
}

fn render_block(code: &mut Code, block: &Block) {
    code.line("{");
    code.indent += 1;
    let count = block.statements.len();
    for (index, stmt) in block.statements.iter().enumerate() {
        render_stmt(code, stmt, index + 1 == count);
    }
    code.indent -= 1;
    code.line("}");
}

fn render_param(param: &Param) -> String {
    if param.name == "self" {
        if matches!(param.ty, Ty::Ref(_)) {
            "&self".to_string()
        } else {
            "self".to_string()
        }
    } else {
        format!("{}: {}", escape_ident(&param.name), render_ty(&param.ty))
    }
}

#[must_use]
pub fn render_ty(ty: &Ty) -> String {
    match ty {
        Ty::F64 => "f64".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::SelfType => "Self".to_string(),
        Ty::Named(name) => name.clone(),
        Ty::Result { ok, error } => format!("Result<{}, {}>", render_ty(ok), render_ty(error)),
        Ty::Ref(inner) => format!("&{}", render_ty(inner)),
        Ty::Unit => "()".to_string(),
    }
}

#[must_use]
pub fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::F64(bits) => format!("{:?}", f64::from_bits(*bits)),
        Expr::Int(value) => format!("{value}"),
        Expr::Bool(value) => format!("{value}"),
        Expr::Var(name) => escape_ident(name),
        Expr::SelfValue => "self".to_string(),
        Expr::StructLiteral { name, fields } => {
            let fields = fields
                .iter()
                .map(|(field, value)| match value {
                    Expr::Var(var) if var == field => escape_ident(field),
                    _ => format!("{}: {}", escape_ident(field), render_expr(value)),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{ {fields} }}")
        }
        Expr::Path(segments) => segments
            .iter()
            .map(|segment| escape_ident(segment))
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Str(text) => {
            let mut out = String::with_capacity(text.len() + 2);
            out.push('"');
            for ch in text.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    ch => {
                        if (ch as u32) < 0x20 {
                            let escape = format!("\\u{{{:x}}}", ch as u32);
                            out.push_str(&escape);
                        } else {
                            out.push(ch);
                        }
                    }
                }
            }
            out.push('"');
            out
        }
        Expr::Field { receiver, field } => {
            format!("{}.{}", render_expr(receiver), escape_ident(field))
        }
        Expr::Call { path, args } => {
            let path = path
                .iter()
                .map(|s| escape_ident(s))
                .collect::<Vec<_>>()
                .join("::");
            let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{path}({args})")
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => {
            let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{}.{}({args})", render_expr(receiver), escape_ident(method))
        }
        Expr::Bin { op, left, right } => {
            if *op == BinOp::Pow {
                // lowered to a method call for strict powf semantics
                return format!("{}.powf({})", render_expr(left), render_expr(right));
            }
            format!(
                "{} {} {}",
                render_expr(left),
                op.symbol(),
                render_expr(right)
            )
        }
        Expr::Un { op, value } => {
            let text = render_expr(value);
            match op {
                UnOp::Neg => format!("-{text}"),
                UnOp::Not => format!("!{text}"),
                UnOp::Method(method) => format!("{text}.{}()", escape_ident(method)),
            }
        }
        Expr::Block(stmt) => {
            let mut code = Code {
                buf: String::new(),
                indent: 0,
                anchors: Vec::new(),
            };
            render_stmt(&mut code, stmt, true);
            code.buf
        }
        Expr::IfElse {
            condition,
            then,
            else_value,
        } => {
            let mut code = Code {
                buf: String::new(),
                indent: 0,
                anchors: Vec::new(),
            };
            code.raw(&format!("if {} ", render_expr(condition)));
            render_stmt(&mut code, then, true);
            if let Stmt::Block(block) = &**else_value {
                if !block.statements.is_empty() {
                    code.raw(" else ");
                    render_stmt(&mut code, else_value, true);
                }
            } else {
                code.raw(" else ");
                render_stmt(&mut code, else_value, true);
            }
            code.buf
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{snake_case, FnDef, StructDef};

    #[test]
    fn render_is_deterministic() {
        let module = Module {
            items: vec![
                Item::Struct(StructDef {
                    name: "AffinePolicy".into(),
                    fields: vec![("scale".into(), Ty::F64), ("bias".into(), Ty::F64)],
                    derives: vec!["Clone".into(), "Debug".into()],
                    doc: vec!["An affine policy.".into()],
                    visibility: Visibility::Public,
                }),
                Item::Fn(FnDef {
                    name: "score".into(),
                    params: vec![Param {
                        name: "x".into(),
                        ty: Ty::F64,
                    }],
                    ret: Ty::F64,
                    body: Stmt::Block(Block {
                        statements: vec![Stmt::Let {
                            pattern: "__e0".into(),
                            value: Box::new(Expr::Bin {
                                op: BinOp::Add,
                                left: Box::new(Expr::Field {
                                    receiver: Box::new(Expr::SelfValue),
                                    field: "scale".into(),
                                }),
                                right: Box::new(Expr::Var("x".into())),
                            }),
                        }],
                    }),
                    doc: vec![],
                    visibility: Visibility::Public,
                    attrs: vec![],
                }),
            ],
        };
        let first = render_module(&module);
        let second = render_module(&module);
        assert_eq!(first.code, second.code);
        assert_eq!(first.anchors, second.anchors);
        assert!(first.code.contains("pub struct AffinePolicy"));
        assert!(first.code.contains("__e0 = self.scale + x;"));
    }

    #[test]
    fn keywords_are_escaped() {
        assert_eq!(escape_ident("type"), "type_");
        assert_eq!(escape_ident("score"), "score");
        assert_eq!(snake_case("AffinePolicy"), "affine_policy");
    }

    #[test]
    fn pow_renders_as_method_call() {
        let rendered = render_expr(&Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(Expr::Var("x".into())),
            right: Box::new(Expr::F64(0x4000_0000_0000_0000)),
        });
        assert_eq!(rendered, "x.powf(2.0)");
    }
}
