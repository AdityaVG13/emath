//! Rust target AST.

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Struct(StructDef),
    Enum(EnumDef),
    Fn(FnDef),
    Test(TestDef),
    /// Inherent implementation block.
    Impl(ImplDef),
    /// Trait definition.
    Trait(TraitDef),
    /// Raw attribute-only lines such as `#![forbid(unsafe_code)]`.
    RawAttribute(String),
    /// Doc comment lines.
    DocComment(String),
}

#[derive(Clone, Debug)]
pub struct ImplDef {
    pub target: String,
    pub generics: Vec<String>,
    pub methods: Vec<FnDef>,
    pub doc: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TraitDef {
    pub name: String,
    pub generics: Vec<String>,
    /// Method signatures (name, params, return type) without bodies.
    pub methods: Vec<(String, Vec<Param>, Ty)>,
    pub doc: Vec<String>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<(String, Ty)>,
    pub derives: Vec<String>,
    pub doc: Vec<String>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub derives: Vec<String>,
    pub doc: Vec<String>,
    pub visibility: Visibility,
}

#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub doc: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FnDef {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub body: Stmt,
    pub doc: Vec<String>,
    pub visibility: Visibility,
    pub attrs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TestDef {
    pub name: String,
    pub body: Stmt,
    pub doc: Vec<String>,
    pub attrs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    F64,
    /// Exact `i64` (`Int` / `Nat` in the source language).
    I64,
    Bool,
    SelfType,
    Named(String),
    Result {
        ok: Box<Ty>,
        error: Box<Ty>,
    },
    Ref(Box<Ty>),
    Unit,
}

/// Statement either single or a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Block(Block),
    Let { pattern: String, value: Box<Expr> },
    Return(Expr),
    Expr(Expr),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    F64(u64),
    Int(i64),
    Bool(bool),
    Var(String),
    SelfValue,
    /// `Name { field: value, ... }`.
    StructLiteral {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// `A::B` path expression (enum unit variants, constants).
    Path(Vec<String>),
    /// `name!(args)` macro invocation.
    Macro {
        name: String,
        args: Vec<Expr>,
    },
    Str(String),
    Field {
        receiver: Box<Expr>,
        field: String,
    },
    Raw(String),
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Cast {
        value: Box<Expr>,
        target: Ty,
    },
    Call {
        path: Vec<String>,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    Bin {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Un {
        op: UnOp,
        value: Box<Expr>,
    },
    Block(Box<Stmt>),
    IfElse {
        condition: Box<Expr>,
        then: Box<Stmt>,
        else_value: Box<Stmt>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `powf` strict semantics; rendered as `.powf(...)`.
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Pow => ".powf(",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Method(String),
}

/// Identifier hygiene: escape Rust keywords and normalize presentation
/// names to deterministic `snake_case`.
pub fn escape_ident(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

pub fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut previous_upper = false;
    for ch in name.chars() {
        if out.is_empty() {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_uppercase() && !previous_upper {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_uppercase() && previous_upper {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
        previous_upper = ch.is_ascii_uppercase();
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

pub const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn",
];
