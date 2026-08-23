//! Structured LaTeX math frontend (SG-11).

use emath_genesis::{BinderDomain, BinderFamily, BinderKind, BinderTerm, ScopedBinder};
use emath_term::{SymbolId, Term, VariableId};

use crate::graph::{
    GraphBuilder, LayoutContent, LayoutError, MathLayoutGraph, NodeId, SpatialRelation,
};

const KNOWN_COMMANDS: &[&str] = &[
    "frac", "sqrt", "sum", "prod", "int", "lim", "to",
];

const GREEK: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "zeta",
    "eta",
    "theta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "omicron",
    "pi",
    "rho",
    "sigma",
    "tau",
    "upsilon",
    "phi",
    "chi",
    "psi",
    "omega",
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Upsilon",
    "Phi",
    "Psi",
    "Omega",
    "varepsilon",
    "vartheta",
    "varpi",
    "varrho",
    "varsigma",
    "varphi",
];

/// Import structured LaTeX (or a mixed document with `$...$` / `\[...\]`)
/// into a layout graph, preserving the original source byte-exactly.
pub fn parse_latex(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    if has_formula_delimiters(source) {
        parse_document(source)
    } else {
        parse_bare_math(source)
    }
}

/// Lower a layout graph to a binder term. Extraction never fabricates a
/// term: structured subset only, otherwise [`LayoutError::Unlowered`].
pub fn to_binder_term(graph: &MathLayoutGraph) -> Result<BinderTerm, LayoutError> {
    let root = graph
        .formula_regions()
        .next()
        .map(|node| node.id)
        .or_else(|| graph.nodes().first().map(|node| node.id))
        .ok_or_else(|| LayoutError::Unlowered {
            reason: "empty layout graph".to_string(),
        })?;
    lower_id(graph, root)
}

fn has_formula_delimiters(source: &str) -> bool {
    source.contains('$') || source.contains("\\[")
}

fn parse_bare_math(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    let ast = parse_math_str(source, 0)?;
    let mut builder = GraphBuilder::new(source.to_string());
    let region = builder.add_node(LayoutContent::FormulaRegion, (0, source.len()));
    let root = emit(&mut builder, &ast);
    builder.add_edge(region, root, SpatialRelation::Contains);
    Ok(builder.finish())
}

fn parse_document(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    let mut builder = GraphBuilder::new(source.to_string());
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes.get(pos) == Some(&b'$') {
            let open = pos;
            let inner_start = open + 1;
            let Some(close) = find_unescaped_dollar(bytes, inner_start) else {
                return Err(LayoutError::UnterminatedDollar { offset: open });
            };
            let Some(inner) = source.get(inner_start..close) else {
                return Err(LayoutError::UnterminatedDollar { offset: open });
            };
            let ast = parse_math_str(inner, inner_start)?;
            let region = builder.add_node(LayoutContent::FormulaRegion, (open, close + 1));
            let root = emit(&mut builder, &ast);
            builder.add_edge(region, root, SpatialRelation::Contains);
            pos = close + 1;
        } else if source.get(pos..).is_some_and(|rest| rest.starts_with("\\[")) {
            let open = pos;
            let inner_start = open + 2;
            let Some(rel) = source.get(inner_start..).and_then(|rest| rest.find("\\]")) else {
                return Err(LayoutError::UnterminatedDisplay { offset: open });
            };
            let close = inner_start + rel;
            let Some(inner) = source.get(inner_start..close) else {
                return Err(LayoutError::UnterminatedDisplay { offset: open });
            };
            let ast = parse_math_str(inner, inner_start)?;
            let region = builder.add_node(LayoutContent::FormulaRegion, (open, close + 2));
            let root = emit(&mut builder, &ast);
            builder.add_edge(region, root, SpatialRelation::Contains);
            pos = close + 2;
        } else {
            pos += source
                .get(pos..)
                .and_then(|rest| rest.chars().next())
                .map_or(1, char::len_utf8);
        }
    }
    Ok(builder.finish())
}

fn find_unescaped_dollar(bytes: &[u8], start: usize) -> Option<usize> {
    bytes.get(start..).and_then(|rest| {
        rest.iter()
            .position(|byte| *byte == b'$')
            .map(|rel| start + rel)
    })
}

#[derive(Debug, Clone)]
struct Ast {
    kind: AstKind,
    span: (usize, usize),
}

#[derive(Debug, Clone)]
enum AstKind {
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
struct Token {
    kind: TokKind,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokKind {
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

fn parse_math_str(source: &str, base: usize) -> Result<Ast, LayoutError> {
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

fn tokenize(source: &str, base: usize) -> Result<Vec<Token>, LayoutError> {
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

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn eat(&mut self, kind: &TokKind) -> bool {
        if self.peek().is_some_and(|token| token.kind == *kind) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokKind) -> Result<Token, LayoutError> {
        match self.bump() {
            Some(token) if token.kind == kind => Ok(token),
            Some(token) => Err(LayoutError::UnexpectedToken {
                token: token_text(&token.kind),
                offset: token.start,
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    fn parse_equality(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_add()?;
        while self.eat(&TokKind::Eq) {
            let right = self.parse_add()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: "=".to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().map(|token| &token.kind) {
                Some(TokKind::Plus) => "+",
                Some(TokKind::Minus) => "-",
                _ => break,
            };
            self.index += 1;
            let right = self.parse_mul()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_postfix()?;
        loop {
            let op = if self.eat(&TokKind::Star) {
                "*"
            } else if self.eat(&TokKind::Slash) {
                "/"
            } else if self.peek().is_some_and(|token| starts_atom(&token.kind)) {
                "*"
            } else {
                break;
            };
            let right = self.parse_postfix()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<Ast, LayoutError> {
        let mut atom = self.parse_atom()?;
        loop {
            if self.eat(&TokKind::Caret) {
                let exp = self.parse_script()?;
                let span = (atom.span.0, exp.span.1);
                atom = Ast {
                    kind: AstKind::Pow {
                        base: Box::new(atom),
                        exp: Box::new(exp),
                    },
                    span,
                };
            } else if self.eat(&TokKind::Underscore) {
                let sub = self.parse_script()?;
                let span = (atom.span.0, sub.span.1);
                atom = Ast {
                    kind: AstKind::Sub {
                        base: Box::new(atom),
                        sub: Box::new(sub),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(atom)
    }

    fn parse_script(&mut self) -> Result<Ast, LayoutError> {
        if self.eat(&TokKind::LBrace) {
            let inner = self.parse_equality()?;
            self.expect(TokKind::RBrace)?;
            return Ok(inner);
        }
        match self.peek().map(|token| &token.kind) {
            Some(TokKind::Letter(_) | TokKind::Number(_) | TokKind::Command(_)) => {
                self.parse_atom()
            }
            Some(kind) => Err(LayoutError::UnexpectedToken {
                token: token_text(kind),
                offset: self.peek().map_or(self.end, |token| token.start),
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    fn parse_atom(&mut self) -> Result<Ast, LayoutError> {
        let token = self.bump().ok_or_else(|| LayoutError::UnexpectedToken {
            token: "EOF".to_string(),
            offset: self.end,
        })?;
        match token.kind {
            TokKind::Letter(ch) => Ok(Ast {
                kind: AstKind::Glyph(ch.to_string()),
                span: (token.start, token.end),
            }),
            TokKind::Number(text) => Ok(Ast {
                kind: AstKind::Glyph(text),
                span: (token.start, token.end),
            }),
            TokKind::LParen => {
                let inner = self.parse_equality()?;
                self.expect(TokKind::RParen)?;
                Ok(inner)
            }
            TokKind::LBrace => {
                let inner = self.parse_equality()?;
                self.expect(TokKind::RBrace)?;
                Ok(inner)
            }
            TokKind::Command(name) => self.parse_command(name, token.start, token.end),
            other => Err(LayoutError::UnexpectedToken {
                token: token_text(&other),
                offset: token.start,
            }),
        }
    }

    fn parse_command(
        &mut self,
        name: String,
        start: usize,
        end: usize,
    ) -> Result<Ast, LayoutError> {
        if GREEK.contains(&name.as_str()) || name == "to" {
            return Ok(Ast {
                kind: AstKind::Glyph(name),
                span: (start, end),
            });
        }
        match name.as_str() {
            "frac" => {
                let num = self.parse_braced()?;
                let den = self.parse_braced()?;
                Ok(Ast {
                    span: (start, den.span.1),
                    kind: AstKind::Frac {
                        num: Box::new(num),
                        den: Box::new(den),
                    },
                })
            }
            "sqrt" => {
                let inner = self.parse_braced()?;
                Ok(Ast {
                    span: (start, inner.span.1),
                    kind: AstKind::Sqrt(Box::new(inner)),
                })
            }
            "sum" | "prod" => self.parse_sum_like(name, start),
            "int" => self.parse_integral(start),
            "lim" => self.parse_limit(start),
            _ => Err(LayoutError::UnknownMacro {
                name,
                offset: start,
            }),
        }
    }

    fn parse_braced(&mut self) -> Result<Ast, LayoutError> {
        self.expect(TokKind::LBrace)?;
        let inner = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        Ok(inner)
    }

    fn parse_sum_like(&mut self, name: String, start: usize) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        self.expect(TokKind::LBrace)?;
        let bound = self.expect_ident()?;
        self.expect(TokKind::Eq)?;
        let lower = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        self.expect(TokKind::Caret)?;
        let upper = self.parse_script()?;
        let body = self.parse_optional_body()?;
        let end = body
            .as_ref()
            .map_or(upper.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name,
                bound: Some(bound),
                lower: Some(Box::new(lower)),
                upper: Some(Box::new(upper)),
                body: body.map(Box::new),
            },
        })
    }

    fn parse_integral(&mut self, start: usize) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        let lower = self.parse_script()?;
        self.expect(TokKind::Caret)?;
        let upper = self.parse_script()?;
        let body = self.parse_optional_body()?;
        let end = body
            .as_ref()
            .map_or(upper.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name: "int".to_string(),
                bound: None,
                lower: Some(Box::new(lower)),
                upper: Some(Box::new(upper)),
                body: body.map(Box::new),
            },
        })
    }

    fn parse_limit(&mut self, start: usize) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        self.expect(TokKind::LBrace)?;
        let bound = self.expect_ident()?;
        match self.bump() {
            Some(token) if matches!(token.kind, TokKind::Command(ref name) if name == "to") => {}
            Some(token) => {
                return Err(LayoutError::UnexpectedToken {
                    token: token_text(&token.kind),
                    offset: token.start,
                });
            }
            None => {
                return Err(LayoutError::UnexpectedToken {
                    token: "EOF".to_string(),
                    offset: self.end,
                });
            }
        }
        let to = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        let body = self.parse_optional_body()?;
        let end = body.as_ref().map_or(to.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name: "lim".to_string(),
                bound: Some(bound),
                lower: Some(Box::new(to)),
                upper: None,
                body: body.map(Box::new),
            },
        })
    }

    fn expect_ident(&mut self) -> Result<String, LayoutError> {
        match self.bump() {
            Some(Token {
                kind: TokKind::Letter(ch),
                ..
            }) => Ok(ch.to_string()),
            Some(Token {
                kind: TokKind::Command(name),
                ..
            }) if GREEK.contains(&name.as_str()) => Ok(name),
            Some(token) => Err(LayoutError::UnexpectedToken {
                token: token_text(&token.kind),
                offset: token.start,
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    fn parse_optional_body(&mut self) -> Result<Option<Ast>, LayoutError> {
        if self.peek().is_some_and(|token| starts_atom(&token.kind)) {
            Ok(Some(self.parse_postfix()?))
        } else {
            Ok(None)
        }
    }
}

fn starts_atom(kind: &TokKind) -> bool {
    matches!(
        kind,
        TokKind::Letter(_)
            | TokKind::Number(_)
            | TokKind::LParen
            | TokKind::LBrace
            | TokKind::Command(_)
    )
}

fn token_text(kind: &TokKind) -> String {
    match kind {
        TokKind::Letter(ch) => ch.to_string(),
        TokKind::Number(text) => text.clone(),
        TokKind::Plus => "+".to_string(),
        TokKind::Minus => "-".to_string(),
        TokKind::Star => "*".to_string(),
        TokKind::Slash => "/".to_string(),
        TokKind::Eq => "=".to_string(),
        TokKind::LParen => "(".to_string(),
        TokKind::RParen => ")".to_string(),
        TokKind::LBrace => "{".to_string(),
        TokKind::RBrace => "}".to_string(),
        TokKind::Caret => "^".to_string(),
        TokKind::Underscore => "_".to_string(),
        TokKind::Command(name) => format!("\\{name}"),
    }
}

fn emit(builder: &mut GraphBuilder, ast: &Ast) -> NodeId {
    match &ast.kind {
        AstKind::Glyph(text) => builder.add_node(LayoutContent::Glyph(text.clone()), ast.span),
        AstKind::Infix { op, left, right } => {
            let row = builder.add_node(LayoutContent::Row, ast.span);
            let left_id = emit(builder, left);
            builder.add_edge(row, left_id, SpatialRelation::Contains);
            let op_span = (left.span.1, right.span.0);
            let op_id = builder.add_node(LayoutContent::Glyph(op.clone()), op_span);
            builder.add_edge(row, op_id, SpatialRelation::Contains);
            builder.add_edge(left_id, op_id, SpatialRelation::RightOf);
            let right_id = emit(builder, right);
            builder.add_edge(row, right_id, SpatialRelation::Contains);
            builder.add_edge(op_id, right_id, SpatialRelation::RightOf);
            row
        }
        AstKind::Pow { base, exp } => {
            let wrapper = builder.add_node(LayoutContent::Superscript, ast.span);
            let base_id = emit(builder, base);
            builder.add_edge(wrapper, base_id, SpatialRelation::Contains);
            let exp_id = emit(builder, exp);
            builder.add_edge(wrapper, exp_id, SpatialRelation::Contains);
            builder.add_edge(base_id, exp_id, SpatialRelation::SuperscriptOf);
            wrapper
        }
        AstKind::Sub { base, sub } => {
            let wrapper = builder.add_node(LayoutContent::Subscript, ast.span);
            let base_id = emit(builder, base);
            builder.add_edge(wrapper, base_id, SpatialRelation::Contains);
            let sub_id = emit(builder, sub);
            builder.add_edge(wrapper, sub_id, SpatialRelation::Contains);
            builder.add_edge(base_id, sub_id, SpatialRelation::SubscriptOf);
            wrapper
        }
        AstKind::Frac { num, den } => {
            let wrapper = builder.add_node(LayoutContent::Fraction, ast.span);
            let num_id = emit(builder, num);
            builder.add_edge(wrapper, num_id, SpatialRelation::Contains);
            builder.add_edge(wrapper, num_id, SpatialRelation::Above);
            let den_id = emit(builder, den);
            builder.add_edge(wrapper, den_id, SpatialRelation::Contains);
            builder.add_edge(wrapper, den_id, SpatialRelation::Below);
            wrapper
        }
        AstKind::Sqrt(inner) => {
            let wrapper = builder.add_node(LayoutContent::Radical, ast.span);
            let inner_id = emit(builder, inner);
            builder.add_edge(wrapper, inner_id, SpatialRelation::Contains);
            wrapper
        }
        AstKind::BigOp {
            name,
            bound,
            lower,
            upper,
            body,
        } => {
            let kind_name = match name.as_str() {
                "sum" => "sum",
                "prod" => "product",
                "int" => "integral",
                "lim" => "limit",
                other => other,
            };
            let op = builder.add_node(LayoutContent::BigOp(kind_name.to_string()), ast.span);
            if let Some(lower) = lower {
                if name != "lim" {
                    if let Some(bound) = bound {
                        let origin = lower.span.0;
                        let bound_id = builder.add_node(
                            LayoutContent::Glyph(bound.clone()),
                            (origin, origin),
                        );
                        let eq_id =
                            builder.add_node(LayoutContent::Glyph("=".to_string()), (origin, origin));
                        let lower_id = emit(builder, lower);
                        for child in [bound_id, eq_id, lower_id] {
                            builder.add_edge(op, child, SpatialRelation::Contains);
                            builder.add_edge(op, child, SpatialRelation::SubscriptOf);
                        }
                        builder.add_edge(bound_id, eq_id, SpatialRelation::RightOf);
                        builder.add_edge(eq_id, lower_id, SpatialRelation::RightOf);
                    } else {
                        let lower_id = emit(builder, lower);
                        builder.add_edge(op, lower_id, SpatialRelation::Contains);
                        builder.add_edge(op, lower_id, SpatialRelation::SubscriptOf);
                    }
                } else {
                    let lower_id = emit(builder, lower);
                    builder.add_edge(op, lower_id, SpatialRelation::Contains);
                    builder.add_edge(op, lower_id, SpatialRelation::SubscriptOf);
                }
            }
            if let Some(upper) = upper {
                let upper_id = emit(builder, upper);
                builder.add_edge(op, upper_id, SpatialRelation::Contains);
                builder.add_edge(op, upper_id, SpatialRelation::SuperscriptOf);
            }
            if name == "lim" {
                if let Some(bound) = bound {
                    let bound_id = builder.add_node(
                        LayoutContent::Glyph(bound.clone()),
                        ast.span,
                    );
                    builder.add_edge(op, bound_id, SpatialRelation::Contains);
                    builder.add_edge(op, bound_id, SpatialRelation::SubscriptOf);
                }
            }
            if let Some(body) = body {
                let body_id = emit(builder, body);
                builder.add_edge(op, body_id, SpatialRelation::Contains);
            }
            op
        }
    }
}

fn lower_id(graph: &MathLayoutGraph, id: NodeId) -> Result<BinderTerm, LayoutError> {
    let node = graph.node(id).ok_or_else(|| LayoutError::Unlowered {
        reason: format!("missing node {}", id.0),
    })?;
    match &node.content {
        LayoutContent::FormulaRegion | LayoutContent::Row => {
            lower_sequence(graph, &contained_terms(graph, id)?)
        }
        LayoutContent::Superscript => {
            let kids = contained_terms(graph, id)?;
            let base = kids.first().copied().ok_or_else(|| LayoutError::Unlowered {
                reason: "superscript missing base".to_string(),
            })?;
            let exp = graph
                .related(base, SpatialRelation::SuperscriptOf)
                .into_iter()
                .next()
                .or_else(|| kids.get(1).copied())
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "superscript missing exponent".to_string(),
                })?;
            apply2("pow", lower_id(graph, base)?, lower_id(graph, exp)?)
        }
        LayoutContent::Subscript => {
            let kids = contained_terms(graph, id)?;
            let base = kids.first().copied().ok_or_else(|| LayoutError::Unlowered {
                reason: "subscript missing base".to_string(),
            })?;
            let sub = graph
                .related(base, SpatialRelation::SubscriptOf)
                .into_iter()
                .next()
                .or_else(|| kids.get(1).copied())
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "subscript missing script".to_string(),
                })?;
            apply2("index", lower_id(graph, base)?, lower_id(graph, sub)?)
        }
        LayoutContent::Fraction => {
            let above = graph
                .related(id, SpatialRelation::Above)
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "fraction missing numerator".to_string(),
                })?;
            let below = graph
                .related(id, SpatialRelation::Below)
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "fraction missing denominator".to_string(),
                })?;
            apply2("/", lower_id(graph, above)?, lower_id(graph, below)?)
        }
        LayoutContent::Radical => {
            let inner = contained_terms(graph, id)?
                .into_iter()
                .next()
                .ok_or_else(|| LayoutError::Unlowered {
                    reason: "radical missing radicand".to_string(),
                })?;
            match lower_id(graph, inner)? {
                BinderTerm::Leaf(term) => Ok(BinderTerm::Leaf(Term::Apply {
                    operator: SymbolId("sqrt".to_string()),
                    arguments: vec![term],
                })),
                BinderTerm::Bind(_) => Err(LayoutError::Unlowered {
                    reason: "radical over binder".to_string(),
                }),
            }
        }
        LayoutContent::BigOp(name) => lower_bigop(graph, id, name),
        LayoutContent::Glyph(text) => lower_glyph(graph, id, text),
    }
}

fn contained_terms(graph: &MathLayoutGraph, id: NodeId) -> Result<Vec<NodeId>, LayoutError> {
    Ok(graph
        .related(id, SpatialRelation::Contains)
        .into_iter()
        .filter(|child| !graph.is_script_target(*child))
        .collect())
}

fn lower_glyph(
    graph: &MathLayoutGraph,
    id: NodeId,
    text: &str,
) -> Result<BinderTerm, LayoutError> {
    if is_infix_op(text) {
        return Err(LayoutError::Unlowered {
            reason: format!("operator {text:?} is not a term"),
        });
    }
    let mut term = BinderTerm::Leaf(glyph_term(text));
    if let Some(exp) = graph
        .related(id, SpatialRelation::SuperscriptOf)
        .into_iter()
        .next()
    {
        term = apply2("pow", term, lower_id(graph, exp)?)?;
    }
    if let Some(sub) = graph
        .related(id, SpatialRelation::SubscriptOf)
        .into_iter()
        .next()
    {
        term = apply2("index", term, lower_id(graph, sub)?)?;
    }
    Ok(term)
}

fn lower_bigop(
    graph: &MathLayoutGraph,
    id: NodeId,
    name: &str,
) -> Result<BinderTerm, LayoutError> {
    let subs = graph.related(id, SpatialRelation::SubscriptOf);
    let supers = graph.related(id, SpatialRelation::SuperscriptOf);
    let bodies: Vec<NodeId> = graph
        .related(id, SpatialRelation::Contains)
        .into_iter()
        .filter(|child| !graph.is_script_target(*child) && !subs.contains(child) && !supers.contains(child))
        .collect();

    let (kind, family, default_bound) = match name {
        "sum" => (BinderKind::Sum, BinderFamily::Structural, "i"),
        "product" => (BinderKind::Product, BinderFamily::Structural, "i"),
        "integral" => (BinderKind::Integral, BinderFamily::FiniteAnalogue, "x"),
        "limit" => (BinderKind::Limit, BinderFamily::Conventional, "x"),
        other => {
            return Err(LayoutError::Unlowered {
                reason: format!("unknown bigop {other}"),
            });
        }
    };

    let (bound, domain) = if name == "limit" {
        let glyphs = flatten_glyphs(graph, &subs);
        let bound_name = glyphs
            .first()
            .cloned()
            .unwrap_or_else(|| default_bound.to_string());
        let anchor = glyphs
            .iter()
            .rev()
            .find(|glyph| *glyph != "to" && *glyph != "→")
            .cloned()
            .or_else(|| glyphs.last().cloned())
            .unwrap_or_else(|| "0".to_string());
        (
            VariableId(bound_name),
            BinderDomain::Symbolic { anchor },
        )
    } else {
        let glyphs = flatten_glyphs(graph, &subs);
        let (bound_name, lower_int) = if let Some(eq) = glyphs.iter().position(|glyph| glyph == "=")
        {
            let name = glyphs
                .first()
                .cloned()
                .filter(|glyph| glyph != "=")
                .unwrap_or_else(|| default_bound.to_string());
            let rhs = glyphs[eq + 1..].join("");
            (name, rhs.parse().ok())
        } else {
            let lower_term = if subs.is_empty() {
                None
            } else {
                Some(lower_related(graph, &subs)?)
            };
            match &lower_term {
                Some(other) => (default_bound.to_string(), as_int_binder(other)),
                None => (default_bound.to_string(), None),
            }
        };
        let upper_term = if supers.is_empty() {
            None
        } else {
            Some(lower_related(graph, &supers)?)
        };
        let upper_int = upper_term.as_ref().and_then(as_int_binder);
        let domain = match (lower_int, upper_int) {
            (Some(lower), Some(upper)) => BinderDomain::FiniteRange { lower, upper },
            _ => BinderDomain::Symbolic {
                anchor: format!(
                    "{}..{}",
                    if glyphs.is_empty() {
                        "_".to_string()
                    } else {
                        glyphs.join("")
                    },
                    upper_term.as_ref().map_or_else(
                        || "_".to_string(),
                        |term| match term {
                            BinderTerm::Leaf(leaf) => leaf.canonical(),
                            BinderTerm::Bind(binder) => binder.canonical(),
                        }
                    )
                ),
            },
        };
        (VariableId(bound_name), domain)
    };

    let body = if let Some(body_id) = bodies.first() {
        lower_id(graph, *body_id)?
    } else {
        BinderTerm::Leaf(Term::Variable(bound.clone()))
    };

    Ok(BinderTerm::Bind(Box::new(ScopedBinder {
        kind,
        family,
        domain,
        bound,
        body,
    })))
}

fn lower_related(graph: &MathLayoutGraph, ids: &[NodeId]) -> Result<BinderTerm, LayoutError> {
    if ids.len() == 1 {
        lower_id(graph, ids[0])
    } else {
        lower_sequence(graph, ids)
    }
}

fn flatten_glyphs(graph: &MathLayoutGraph, ids: &[NodeId]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        flatten_glyphs_into(graph, *id, &mut out);
    }
    out
}

fn flatten_glyphs_into(graph: &MathLayoutGraph, id: NodeId, out: &mut Vec<String>) {
    let Some(node) = graph.node(id) else {
        return;
    };
    match &node.content {
        LayoutContent::Glyph(text) => out.push(text.clone()),
        _ => {
            for child in graph.related(id, SpatialRelation::Contains) {
                flatten_glyphs_into(graph, child, out);
            }
        }
    }
}

fn lower_sequence(graph: &MathLayoutGraph, ids: &[NodeId]) -> Result<BinderTerm, LayoutError> {
    if ids.is_empty() {
        return Err(LayoutError::Unlowered {
            reason: "empty formula".to_string(),
        });
    }
    let mut items: Vec<SeqItem> = Vec::new();
    for id in ids {
        let node = graph.node(*id).ok_or_else(|| LayoutError::Unlowered {
            reason: format!("missing node {}", id.0),
        })?;
        if let LayoutContent::Glyph(text) = &node.content {
            if is_infix_op(text) && !graph.is_script_target(*id) {
                items.push(SeqItem::Op(text.clone()));
                continue;
            }
        }
        items.push(SeqItem::Term(lower_id(graph, *id)?));
    }
    climb_eq(&items, 0).and_then(|(term, end)| {
        if end == items.len() {
            Ok(term)
        } else {
            Err(LayoutError::Unlowered {
                reason: "trailing tokens in formula sequence".to_string(),
            })
        }
    })
}

enum SeqItem {
    Term(BinderTerm),
    Op(String),
}

fn climb_eq(items: &[SeqItem], start: usize) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_add(items, start)?;
    while matches!(items.get(index), Some(SeqItem::Op(op)) if op == "=") {
        index += 1;
        let (right, next) = climb_add(items, index)?;
        left = apply2("=", left, right)?;
        index = next;
    }
    Ok((left, index))
}

fn climb_add(items: &[SeqItem], start: usize) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_mul(items, start)?;
    while let Some(SeqItem::Op(op)) = items.get(index) {
        if op != "+" && op != "-" {
            break;
        }
        let op = op.clone();
        index += 1;
        let (right, next) = climb_mul(items, index)?;
        left = apply2(&op, left, right)?;
        index = next;
    }
    Ok((left, index))
}

fn climb_mul(items: &[SeqItem], start: usize) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_atom(items, start)?;
    loop {
        match items.get(index) {
            Some(SeqItem::Op(op)) if op == "*" || op == "/" => {
                let op = op.clone();
                index += 1;
                let (right, next) = climb_atom(items, index)?;
                left = apply2(&op, left, right)?;
                index = next;
            }
            Some(SeqItem::Term(_)) => {
                let (right, next) = climb_atom(items, index)?;
                left = apply2("*", left, right)?;
                index = next;
            }
            _ => break,
        }
    }
    Ok((left, index))
}

fn climb_atom(items: &[SeqItem], start: usize) -> Result<(BinderTerm, usize), LayoutError> {
    match items.get(start) {
        Some(SeqItem::Term(term)) => Ok((clone_term(term), start + 1)),
        Some(SeqItem::Op(op)) => Err(LayoutError::Unlowered {
            reason: format!("expected term, found operator {op}"),
        }),
        None => Err(LayoutError::Unlowered {
            reason: "expected term, found end of sequence".to_string(),
        }),
    }
}

fn clone_term(term: &BinderTerm) -> BinderTerm {
    term.clone()
}

fn apply2(op: &str, left: BinderTerm, right: BinderTerm) -> Result<BinderTerm, LayoutError> {
    match (left, right) {
        (BinderTerm::Leaf(left), BinderTerm::Leaf(right)) => Ok(BinderTerm::Leaf(Term::Apply {
            operator: SymbolId(op.to_string()),
            arguments: vec![left, right],
        })),
        (BinderTerm::Leaf(_), BinderTerm::Bind(binder)) if op == "=" => {
            Ok(BinderTerm::Bind(binder))
        }
        (BinderTerm::Bind(binder), _) => Ok(BinderTerm::Bind(binder)),
        _ => Err(LayoutError::Unlowered {
            reason: format!("cannot apply {op:?} across a binder"),
        }),
    }
}

fn glyph_term(text: &str) -> Term {
    if text.chars().all(|ch| ch.is_ascii_digit()) && !text.is_empty() {
        Term::Constant(SymbolId(text.to_string()))
    } else {
        Term::Variable(VariableId(text.to_string()))
    }
}

fn is_infix_op(text: &str) -> bool {
    matches!(text, "+" | "-" | "*" | "/" | "=")
}

fn as_int(term: &Term) -> Option<i64> {
    match term {
        Term::Constant(symbol) => symbol.0.parse().ok(),
        _ => None,
    }
}

fn as_int_binder(term: &BinderTerm) -> Option<i64> {
    match term {
        BinderTerm::Leaf(leaf) => as_int(leaf),
        BinderTerm::Bind(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use emath_genesis::{BinderBudget, BinderDomain, BinderFamily, BinderKind, BinderTerm};
    use emath_term::SymbolId;

    use super::{parse_latex, to_binder_term};
    use crate::graph::LayoutError;

    #[test]
    fn latex_source_preserved_byte_exact() {
        let source = r"\sum_{i=1}^{3} i";
        let graph = parse_latex(source).expect("parse");
        assert_eq!(graph.source(), source);
        assert_eq!(graph.source().as_bytes(), source.as_bytes());
    }

    #[test]
    fn latex_sum_lowers_to_structural_finite_range_and_expands() {
        let graph = parse_latex(r"\sum_{i=1}^{3} i").expect("parse");
        let term = to_binder_term(&graph).expect("lower");
        let BinderTerm::Bind(binder) = term else {
            panic!("expected a sum binder, got {term:?}");
        };
        assert_eq!(binder.kind, BinderKind::Sum);
        assert_eq!(binder.family, BinderFamily::Structural);
        assert_eq!(
            binder.domain,
            BinderDomain::FiniteRange {
                lower: 1,
                upper: 3
            }
        );
        let expanded = binder
            .expand(&SymbolId("+".to_string()), BinderBudget::default())
            .expect("expand");
        assert_eq!(
            expanded.canonical(),
            "apply(+,apply(+,const(1),const(2)),const(3))"
        );
    }

    #[test]
    fn latex_unknown_macro_refused_with_offset() {
        let error = parse_latex(r"x+\foo").expect_err("unknown macro");
        assert_eq!(
            error,
            LayoutError::UnknownMacro {
                name: "foo".to_string(),
                offset: 2,
            }
        );
    }

    #[test]
    fn latex_unterminated_dollar_refused() {
        let error = parse_latex("hello $foo").expect_err("unterminated");
        assert_eq!(error, LayoutError::UnterminatedDollar { offset: 6 });
    }

    #[test]
    fn latex_formula_region_spans_byte_exact() {
        let source = r"see $\sum_{i=1}^{3} i$ please";
        let graph = parse_latex(source).expect("parse");
        let region = graph
            .formula_regions()
            .next()
            .expect("one formula region");
        let (start, end) = region.source_span;
        assert_eq!(&source[start..end], r"$\sum_{i=1}^{3} i$");
    }
}
