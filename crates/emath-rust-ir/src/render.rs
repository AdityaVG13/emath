//! Deterministic Rust renderer with byte-range anchors, deterministic file
//! partitioning and content-based identity (independent of absolute paths).

use crate::ast::{
    escape_ident, BinOp, Block, Expr, Item, Module, Param, Stmt, Ty, UnOp, Visibility,
};
use emath_core::{fnv1a64_bytes, ContentId};
use std::collections::BTreeMap;

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

/// A generated file set: the crate root plus deterministic submodules.
/// File names are derived from module names, never from host paths.
#[derive(Clone, Debug, Default)]
pub struct FileSet {
    pub root: Module,
    /// Submodules keyed by module name (sorted at render time).
    pub modules: BTreeMap<String, Module>,
}

impl FileSet {
    /// A file set with only the crate root.
    #[must_use]
    pub fn root_only(root: Module) -> Self {
        Self {
            root,
            modules: BTreeMap::new(),
        }
    }

    /// Content identity of one generated file: path-independent (only the
    /// relative path and bytes contribute, never absolute host paths).
    #[must_use]
    pub fn file_identity(relative_path: &str, contents: &str) -> ContentId {
        let mut payload = String::new();
        payload.push_str("file:v1:");
        payload.push_str(relative_path);
        payload.push('\n');
        payload.push_str(contents);
        ContentId(format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(payload.as_bytes())
        ))
    }
}

/// Renders the file set partitioned into deterministic per-file contents:
/// `src/lib.rs` (module declarations + crate-root items) plus
/// `src/<module>.rs` per submodule. Both the byte content and the relative
/// file name (never absolute host paths) determine the identity.
#[must_use]
pub fn render_file_set_partitioned(file_set: &FileSet) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    // Root: crate items plus `pub mod` declarations in sorted order.
    let mut code = Code {
        buf: String::new(),
        indent: 0,
        anchors: Vec::new(),
    };
    for (index, name) in file_set.modules.keys().enumerate() {
        if index > 0 {
            code.blank();
        }
        code.line(&format!("pub mod {name};"));
    }
    if !file_set.modules.is_empty() && !file_set.root.items.is_empty() {
        code.blank();
    }
    for (index, item) in file_set.root.items.iter().enumerate() {
        if index > 0
            && !matches!(
                file_set.root.items[index - 1],
                Item::DocComment(_) | Item::RawAttribute(_)
            )
        {
            code.blank();
        }
        render_item(&mut code, item);
    }
    files.insert("src/lib.rs".to_string(), code.buf);
    for (name, module) in &file_set.modules {
        let mut module_code = Code {
            buf: String::new(),
            indent: 0,
            anchors: Vec::new(),
        };
        for (index, item) in module.items.iter().enumerate() {
            if index > 0
                && !matches!(
                    module.items[index - 1],
                    Item::DocComment(_) | Item::RawAttribute(_)
                )
            {
                module_code.blank();
            }
            render_item(&mut module_code, item);
        }
        files.insert(format!("src/{name}.rs"), module_code.buf);
    }
    files
}

/// Renders a file set into a single `RenderResult`; anchors carry the
/// relative file name so the source map is complete across partitions
/// (`lib.rs:fn x`, `src/solver.rs:struct Y`).
#[must_use]
pub fn render_file_set(file_set: &FileSet) -> RenderResult {
    let partitioned = render_file_set_partitioned(file_set);
    let mut out = String::new();
    let mut anchors = Vec::new();
    for (relative_path, contents) in &partitioned {
        let mut offset = 0u64;
        let prefix = format!("%F {relative_path} %\n");
        out.push_str(&prefix);
        offset += u64::try_from(prefix.len()).unwrap_or(u64::MAX);
        out.push_str(contents);
        // Reconstruct anchors for this file by rendering it standalone.
        let standalone = if relative_path == "src/lib.rs" {
            render_module(&file_set.root)
        } else {
            let name = relative_path
                .strip_prefix("src/")
                .and_then(|path| path.strip_suffix(".rs"))
                .unwrap_or("module");
            let module = file_set.modules.get(name).cloned().unwrap_or_default();
            render_module(&module)
        };
        for anchor in &standalone.anchors {
            anchors.push(Anchor {
                label: format!("{relative_path}:{}", anchor.label),
                start: u32::try_from(offset + u64::from(anchor.start)).unwrap_or(u32::MAX),
                end: u32::try_from(offset + u64::from(anchor.end)).unwrap_or(u32::MAX),
            });
        }
    }
    RenderResult { code: out, anchors }
}

/// Anchor coverage: every public function/struct/enum/trait must carry a
/// byte-range anchor. Returns the labels of items missing anchors
/// (`E-CODEGEN-004` source-map gap).
#[must_use]
pub fn coverage_gaps(module: &Module) -> Vec<String> {
    let mut missing = Vec::new();
    for item in &module.items {
        match item {
            Item::Struct(def) => {
                if def.visibility == Visibility::Public {
                    missing.push(format!("struct {}", def.name));
                }
            }
            Item::Enum(def) => {
                if def.visibility == Visibility::Public {
                    missing.push(format!("enum {}", def.name));
                }
            }
            Item::Fn(def) => {
                if def.visibility == Visibility::Public {
                    missing.push(format!("fn {}", def.name));
                }
            }
            Item::Trait(def) => {
                if def.visibility == Visibility::Public {
                    missing.push(format!("trait {}", def.name));
                }
            }
            Item::Impl(_) | Item::Test(_) | Item::RawAttribute(_) | Item::DocComment(_) => {}
        }
    }
    // Subtract covered anchors.
    let result = render_module(module);
    for anchor in &result.anchors {
        missing.retain(|label| label != &anchor.label);
    }
    missing
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
            let generics = render_generics(&def.generics);
            code.line(&format!("{visibility}struct {}{generics} {{", def.name));
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
            let generics = render_generics(&def.generics);
            code.line(&format!(
                "{visibility}fn {}{generics}({params}) -> {} {{",
                escape_ident(&def.name),
                render_ty(&def.ret)
            ));
            code.indent += 1;
            render_stmt(code, &def.body, true);
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("fn {}", def.name), start);
        }
        Item::Trait(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            let visibility = match def.visibility {
                Visibility::Public => "pub ",
                Visibility::Private => "",
            };
            let generics = render_generics(&def.generics);
            code.line(&format!("{visibility}trait {}{generics} {{", def.name));
            code.indent += 1;
            for (name, params, ret) in &def.methods {
                let params = params
                    .iter()
                    .map(render_param)
                    .collect::<Vec<_>>()
                    .join(", ");
                code.line(&format!(
                    "fn {}({params}) -> {};",
                    escape_ident(name),
                    render_ty(ret)
                ));
            }
            code.indent -= 1;
            code.line("}");
            code.anchor(&format!("trait {}", def.name), start);
        }
        Item::Test(def) => {
            let start = code.buf.len();
            render_doc(code, &def.doc);
            for attribute in &def.attrs {
                code.line(attribute);
            }
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
            let generics = render_generics(&def.generics);
            code.line(&format!("impl{generics} {} {{", def.target));
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
                let method_generics = render_generics(&method.generics);
                code.line(&format!(
                    "{visibility}fn {}{method_generics}({params}) -> {} {{",
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

/// Renders a generic parameter list `<A, B>` (empty list renders nothing).
#[must_use]
pub fn render_generics(generics: &[String]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
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
            // A method chain on a call receiver (e.g. `AffinePolicy::new(..)`
            // followed by `.expect(..)`) is laid out the way rustfmt lays it
            // out: receiver on the `let` line, chain head on the next line.
            if let Expr::MethodCall {
                receiver,
                method,
                args,
            } = &**value
            {
                if let Expr::Call { .. } = &**receiver {
                    let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
                    code.line(&format!(
                        "let {} = {}",
                        escape_ident(pattern),
                        render_expr(receiver)
                    ));
                    code.indent += 1;
                    code.line(&format!(".{}({});", escape_ident(method), args));
                    code.indent -= 1;
                    return;
                }
            }
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
            // Macro invocations are always statements (e.g. `assert!(...)`)
            // and keep their semicolon even as the block tail.
            if let Expr::Macro { name, args } = expr {
                // A macro carrying a single block argument (an `assert!` on a block expression) is laid out under the macro parens exactly as rustfmt does.
                if let [Expr::Block(stmt)] = args.as_slice() {
                    if let Stmt::Block(block) = &**stmt {
                        render_block_expr(code, block, &format!("{name}!("), ");");
                        return;
                    }
                }
                let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
                code.line(&format!("{name}!({args});"));
                return;
            }
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
        Expr::Macro { name, args } => {
            let args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{name}!({args})")
        }
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
