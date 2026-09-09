//! LaTeX token and AST types plus the tokenizer.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct Ast {
    pub(super) kind: AstKind,
    pub(super) span: (usize, usize),
}

#[derive(Debug, Clone)]
pub(super) enum AstKind {
    Glyph(String),
    Infix {
        op: String,
        left: Box<Ast>,
        right: Box<Ast>,
    },
    Pow {
        base: Box<Ast>,
        exp: Box<Ast>,
    },
    Sub {
        base: Box<Ast>,
        sub: Box<Ast>,
    },
    Frac {
        num: Box<Ast>,
        den: Box<Ast>,
    },
    Sqrt(Box<Ast>),
    BigOp {
        name: String,
        bound: Option<String>,
        lower: Option<Box<Ast>>,
        upper: Option<Box<Ast>>,
        body: Option<Box<Ast>>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct Token {
    pub(super) kind: TokKind,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokKind {
    Letter(char),
    Number(String),
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Caret,
    Underscore,
    Command(String),
}

pub(super) fn parse_math_str(source: &str, base: usize) -> Result<Ast, LayoutError> {
    let tokens = tokenize(source, base)?;
    if tokens.is_empty() {
        return Err(LayoutError::UnexpectedToken {
            token: "EOF".to_string(),
            offset: base,
        });
    }
    let mut parser = Parser {
        tokens,
        index: 0,
        end: base + source.len(),
    };
    let ast = parser.parse_equality()?;
    if let Some(extra) = parser.peek() {
        return Err(LayoutError::UnexpectedToken {
            token: token_text(&extra.kind),
            offset: extra.start,
        });
    }
    Ok(ast)
}

pub(super) fn tokenize(source: &str, base: usize) -> Result<Vec<Token>, LayoutError> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let bytes = source.as_bytes();
    while pos < bytes.len() {
        let Some(ch) = source.get(pos..).and_then(|rest| rest.chars().next()) else {
            return Err(LayoutError::UnexpectedToken {
                token: "invalid utf-8 boundary".to_string(),
                offset: base + pos,
            });
        };
        if ch.is_ascii_whitespace() {
            pos += ch.len_utf8();
            continue;
        }
        let start = base + pos;
        if ch == '\\' {
            let cmd_start = pos;
            pos += 1;
            let rest = &source[pos..];
            let name_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .map(char::len_utf8)
                .sum::<usize>();
            let name = if name_len == 0 {
                let Some(next) = rest.chars().next() else {
                    return Err(LayoutError::UnexpectedToken {
                        token: "\\".to_string(),
                        offset: start,
                    });
                };
                pos += next.len_utf8();
                next.to_string()
            } else {
                pos += name_len;
                rest[..name_len].to_string()
            };
            if !KNOWN_COMMANDS.contains(&name.as_str()) && !GREEK.contains(&name.as_str()) {
                return Err(LayoutError::UnknownMacro {
                    name,
                    offset: base + cmd_start,
                });
            }
            tokens.push(Token {
                kind: TokKind::Command(name),
                start,
                end: base + pos,
            });
            continue;
        }
        let kind = match ch {
            '+' => TokKind::Plus,
            '-' => TokKind::Minus,
            '*' => TokKind::Star,
            '/' => TokKind::Slash,
            '=' => TokKind::Eq,
            '(' => TokKind::LParen,
            ')' => TokKind::RParen,
            '{' => TokKind::LBrace,
            '}' => TokKind::RBrace,
            '^' => TokKind::Caret,
            '_' => TokKind::Underscore,
            '0'..='9' => {
                let rest = &source[pos..];
                let len = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .map(char::len_utf8)
                    .sum::<usize>();
                let number = rest[..len].to_string();
                pos += len;
                tokens.push(Token {
                    kind: TokKind::Number(number),
                    start,
                    end: base + pos,
                });
                continue;
            }
            'a'..='z' | 'A'..='Z' => TokKind::Letter(ch),
            _ => {
                return Err(LayoutError::UnexpectedToken {
                    token: ch.to_string(),
                    offset: start,
                });
            }
        };
        pos += ch.len_utf8();
        tokens.push(Token {
            kind,
            start,
            end: base + pos,
        });
    }
    Ok(tokens)
}
