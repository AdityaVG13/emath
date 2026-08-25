//! Tokens with exact byte spans.

use emath_core::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Emath,
    Custom,
    As,
    Fn,
    Pub,
    Package,
    Private,
    Require,
    Ensure,
    Invariant,
    Let,
    If,
    Else,
    For,
    In,
    While,
    Match,
    True,
    False,
    And,
    Or,
    Not,
    Sum,
    Product,
    Integral,
    ForAll,
    Exists,
    Derivative,
    Solve,
    Minimize,
    Maximize,
    Use,
    Extern,
    Where,
    Wrt,
    At,
    On,
    Over,
    Against,
    With,
    Return,
    SelfKw,
}

impl Keyword {
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Emath => "emath",
            Self::Custom => "custom",
            Self::As => "as",
            Self::Fn => "fn",
            Self::Pub => "public",
            Self::Package => "package",
            Self::Private => "private",
            Self::Require => "require",
            Self::Ensure => "ensure",
            Self::Invariant => "invariant",
            Self::Let => "let",
            Self::If => "if",
            Self::Else => "else",
            Self::For => "for",
            Self::In => "in",
            Self::While => "while",
            Self::Match => "match",
            Self::True => "true",
            Self::False => "false",
            Self::And => "and",
            Self::Or => "or",
            Self::Not => "not",
            Self::Sum => "sum",
            Self::Product => "product",
            Self::Integral => "integral",
            Self::ForAll => "forall",
            Self::Exists => "exists",
            Self::Derivative => "derivative",
            Self::Solve => "solve",
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Use => "use",
            Self::Extern => "extern",
            Self::Where => "where",
            Self::Wrt => "wrt",
            Self::At => "at",
            Self::On => "on",
            Self::Over => "over",
            Self::Against => "against",
            Self::With => "with",
            Self::Return => "return",
            Self::SelfKw => "Self",
        }
    }

    #[must_use]
    pub fn from_ident(text: &str) -> Option<Self> {
        Some(match text {
            "emath" => Self::Emath,
            "custom" => Self::Custom,
            "as" => Self::As,
            "fn" => Self::Fn,
            "public" => Self::Pub,
            "package" => Self::Package,
            "private" => Self::Private,
            "require" => Self::Require,
            "ensure" => Self::Ensure,
            "invariant" => Self::Invariant,
            "let" => Self::Let,
            "if" => Self::If,
            "else" => Self::Else,
            "for" => Self::For,
            "in" => Self::In,
            "while" => Self::While,
            "match" => Self::Match,
            "true" => Self::True,
            "false" => Self::False,
            "and" => Self::And,
            "or" => Self::Or,
            "not" => Self::Not,
            "sum" => Self::Sum,
            "product" => Self::Product,
            "integral" => Self::Integral,
            "forall" => Self::ForAll,
            "exists" => Self::Exists,
            "derivative" => Self::Derivative,
            "solve" => Self::Solve,
            "minimize" => Self::Minimize,
            "maximize" => Self::Maximize,
            "use" => Self::Use,
            "extern" => Self::Extern,
            "where" => Self::Where,
            "wrt" => Self::Wrt,
            "at" => Self::At,
            "on" => Self::On,
            "over" => Self::Over,
            "against" => Self::Against,
            "with" => Self::With,
            "return" => Self::Return,
            "Self" => Self::SelfKw,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Keyword(Keyword),
    /// Integer literal spelling (underscores preserved).
    Int(String),
    /// Float literal spelling including any suffix (`1.5`, `1e-12f32`).
    Float(String),
    /// String literal value (escapes resolved).
    Str(String),
    Eq,
    EqEq,
    NotEq,
    /// `==>` — logical implication.
    Imply,
    /// `<==>` — logical biconditional.
    Iff,
    /// `~~` — asymptotic equivalence (B18).
    TildeTilde,
    Le,
    Ge,
    Lt,
    Gt,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    PathSep,
    Arrow,
    Dot,
    DotDot,
    DotDotEq,
    Question,
    Amp,
    Pipe,
    /// `@` — attribute prefix on `emath` items (`@capabilities(...)`).
    AtSign,
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl TokenKind {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("identifier `{name}`"),
            Self::Keyword(keyword) => format!("keyword `{}`", keyword.spelling()),
            Self::Int(text) => format!("integer `{text}`"),
            Self::Float(text) => format!("float `{text}`"),
            Self::Str(_) => "string literal".to_string(),
            Self::Eq => "`=`".to_string(),
            Self::EqEq => "`==`".to_string(),
            Self::NotEq => "`!=`".to_string(),
            Self::Imply => "`==>`".to_string(),
            Self::Iff => "`<==>`".to_string(),
            Self::TildeTilde => "`~~`".to_string(),
            Self::Le => "`<=`".to_string(),
            Self::Ge => "`>=`".to_string(),
            Self::Lt => "`<`".to_string(),
            Self::Gt => "`>`".to_string(),
            Self::Plus => "`+`".to_string(),
            Self::Minus => "`-`".to_string(),
            Self::Star => "`*`".to_string(),
            Self::Slash => "`/`".to_string(),
            Self::Caret => "`^`".to_string(),
            Self::Bang => "`!`".to_string(),
            Self::LParen => "`(`".to_string(),
            Self::RParen => "`)`".to_string(),
            Self::LBracket => "`[`".to_string(),
            Self::RBracket => "`]`".to_string(),
            Self::LBrace => "`{`".to_string(),
            Self::RBrace => "`}`".to_string(),
            Self::Comma => "`,`".to_string(),
            Self::Colon => "`:`".to_string(),
            Self::PathSep => "`::`".to_string(),
            Self::Arrow => "`->`".to_string(),
            Self::Dot => "`.`".to_string(),
            Self::DotDot => "`..`".to_string(),
            Self::DotDotEq => "`..=`".to_string(),
            Self::Question => "`?`".to_string(),
            Self::Amp => "`&`".to_string(),
            Self::Pipe => "`|`".to_string(),
            Self::AtSign => "`@`".to_string(),
            Self::Newline => "end of line".to_string(),
            Self::Indent => "indent".to_string(),
            Self::Dedent => "dedent".to_string(),
            Self::Eof => "end of file".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// A comment retained for lossless formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    /// Comment text including the marker (`#`, `//`, `///`) and trailing
    /// newline-adjacent whitespace trimmed.
    pub text: String,
    pub span: Span,
    /// True when the comment occupies its own line (line lead); false for
    /// trailing comments after code.
    pub own_line: bool,
}

impl Token {
    #[must_use]
    pub fn describe(&self) -> String {
        self.kind.describe()
    }
}
