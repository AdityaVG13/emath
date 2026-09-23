//! User-facing expression surface for capsule reference bodies.
//!
//! Canonical `apply(...)` text remains valid. The surface is the same
//! operators a method author already thinks in: calls, infix arithmetic
//! and comparisons, `if`/`let`, and indexing. Pretty spelling is not
//! identity; [`Term::canonical`] is.

use crate::{CanonicalError, SymbolId, Term, VariableId};

pub(crate) fn parse_body(text: &str) -> Result<Term, CanonicalError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(CanonicalError::Malformed {
            text: text.to_string(),
        });
    }
    if let Ok(term) = Term::parse_canonical(trimmed) {
        return Ok(term);
    }
    let mut parser = SurfaceParser {
        bytes: trimmed.as_bytes(),
        pos: 0,
    };
    let term = parser.parse_expr()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(CanonicalError::Trailing {
            text: trimmed.to_string(),
        });
    }
    Ok(term)
}

struct SurfaceParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl SurfaceParser<'_> {
    fn remaining(&self) -> &str {
        std::str::from_utf8(&self.bytes[self.pos..]).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn eat_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_str(&mut self, expected: &str) -> bool {
        if self.remaining().starts_with(expected) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn keyword_boundary(rest: &str, keyword: &str) -> bool {
        rest.starts_with(keyword)
            && rest[keyword.len()..]
                .chars()
                .next()
                .is_none_or(|ch| !is_ident_continue(ch))
    }

    fn peek_keyword(&self, keyword: &str) -> bool {
        Self::keyword_boundary(self.remaining(), keyword)
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.peek_keyword(keyword) {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }

    fn malformed(&self) -> CanonicalError {
        CanonicalError::Malformed {
            text: self.remaining().to_string(),
        }
    }

    fn parse_expr(&mut self) -> Result<Term, CanonicalError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_and()?;
        loop {
            self.skip_ws();
            if !self.eat_keyword("or") {
                break;
            }
            let rhs = self.parse_and()?;
            term = apply("or", vec![term, rhs]);
        }
        Ok(term)
    }

    fn parse_and(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_not()?;
        loop {
            self.skip_ws();
            if !self.eat_keyword("and") {
                break;
            }
            let rhs = self.parse_not()?;
            term = apply("and", vec![term, rhs]);
        }
        Ok(term)
    }

    fn parse_not(&mut self) -> Result<Term, CanonicalError> {
        self.skip_ws();
        if self.eat_keyword("not") {
            return Ok(apply("not", vec![self.parse_not()?]));
        }
        self.parse_compare()
    }

    fn parse_compare(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_add()?;
        loop {
            self.skip_ws();
            let op = if self.eat_str("<==>") {
                Some("iff")
            } else if self.eat_str("==>") {
                Some("imply")
            } else if self.eat_str("==") {
                Some("eq")
            } else if self.eat_str("!=") {
                Some("ne")
            } else if self.eat_str("<=") {
                Some("le")
            } else if self.eat_str(">=") {
                Some("ge")
            } else if self.eat_char('<') {
                Some("lt")
            } else if self.eat_char('>') {
                Some("gt")
            } else {
                None
            };
            let Some(op) = op else {
                break;
            };
            let rhs = self.parse_add()?;
            term = apply(op, vec![term, rhs]);
        }
        Ok(term)
    }

    fn parse_add(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_mul()?;
        loop {
            self.skip_ws();
            let op = if self.eat_char('+') {
                Some("add")
            } else if self.eat_char('-') {
                Some("sub")
            } else {
                None
            };
            let Some(op) = op else {
                break;
            };
            let rhs = self.parse_mul()?;
            term = apply(op, vec![term, rhs]);
        }
        Ok(term)
    }

    fn parse_mul(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_pow()?;
        loop {
            self.skip_ws();
            let op = if self.eat_char('*') {
                Some("mul")
            } else if self.eat_char('/') {
                Some("div")
            } else {
                None
            };
            let Some(op) = op else {
                break;
            };
            let rhs = self.parse_pow()?;
            term = apply(op, vec![term, rhs]);
        }
        Ok(term)
    }

    fn parse_pow(&mut self) -> Result<Term, CanonicalError> {
        let term = self.parse_prefix()?;
        self.skip_ws();
        if self.eat_char('^') {
            let rhs = self.parse_pow()?;
            return Ok(apply("pow", vec![term, rhs]));
        }
        Ok(term)
    }

    fn parse_prefix(&mut self) -> Result<Term, CanonicalError> {
        self.skip_ws();
        if self.peek() == Some('-') {
            let rest = self.remaining();
            if rest.len() > 1 && rest.as_bytes()[1].is_ascii_digit() {
                return self.parse_postfix();
            }
            self.bump();
            return Ok(apply("neg", vec![self.parse_prefix()?]));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Term, CanonicalError> {
        let mut term = self.parse_primary()?;
        loop {
            self.skip_ws();
            if self.eat_char('(') {
                let args = self.parse_arg_list(')')?;
                term = match term {
                    Term::Variable(VariableId(name)) | Term::Constant(SymbolId(name)) => {
                        apply(&name, args)
                    }
                    Term::Apply {
                        operator,
                        arguments,
                    } if arguments.is_empty() => Term::Apply {
                        operator,
                        arguments: args,
                    },
                    other => apply_on(other, args),
                };
                continue;
            }
            if self.eat_char('[') {
                let index = self.parse_expr()?;
                self.skip_ws();
                if !self.eat_char(']') {
                    return Err(self.malformed());
                }
                term = apply("index", vec![term, index]);
                continue;
            }
            break;
        }
        Ok(term)
    }

    fn parse_primary(&mut self) -> Result<Term, CanonicalError> {
        self.skip_ws();
        if self.eat_char('(') {
            let term = self.parse_expr()?;
            self.skip_ws();
            if !self.eat_char(')') {
                return Err(self.malformed());
            }
            return Ok(term);
        }
        if self.peek_keyword("if") {
            return self.parse_if();
        }
        if self.peek_keyword("let") {
            return self.parse_let();
        }
        if self.peek_keyword("var") && self.remaining().get(3..4) == Some("(") {
            return self.parse_special_atom("var");
        }
        if self.peek_keyword("const") && self.remaining().get(5..6) == Some("(") {
            return self.parse_special_atom("const");
        }
        if self.peek_keyword("apply") && self.remaining().get(5..6) == Some("(") {
            return self.parse_apply_form();
        }
        if self.peek_keyword("true") {
            self.eat_keyword("true");
            return Ok(constant("true"));
        }
        if self.peek_keyword("false") {
            self.eat_keyword("false");
            return Ok(constant("false"));
        }
        if let Some(number) = self.parse_number() {
            return Ok(constant(&number));
        }
        if let Some(name) = self.parse_ident() {
            if name.contains(':') {
                return Ok(constant(&name));
            }
            return Ok(variable(&name));
        }
        Err(self.malformed())
    }

    fn parse_if(&mut self) -> Result<Term, CanonicalError> {
        self.eat_keyword("if");
        self.skip_ws();
        if self.peek() == Some('(') {
            self.bump();
            let args = self.parse_arg_list(')')?;
            return Ok(apply("if", args));
        }
        let cond = self.parse_expr()?;
        self.skip_ws();
        if !self.eat_keyword("then") && !self.eat_char(':') {
            return Err(self.malformed());
        }
        let then_term = self.parse_expr()?;
        self.skip_ws();
        if !self.eat_keyword("else") {
            return Err(self.malformed());
        }
        self.skip_ws();
        let _ = self.eat_char(':');
        let else_term = self.parse_expr()?;
        Ok(apply("if", vec![cond, then_term, else_term]))
    }

    fn parse_let(&mut self) -> Result<Term, CanonicalError> {
        self.eat_keyword("let");
        self.skip_ws();
        if self.peek() == Some('(') {
            self.bump();
            let args = self.parse_arg_list(')')?;
            return Ok(apply("let", args));
        }
        let Some(name) = self.parse_ident() else {
            return Err(self.malformed());
        };
        self.skip_ws();
        if !self.eat_char('=') {
            return Err(self.malformed());
        }
        let value = self.parse_expr()?;
        self.skip_ws();
        if !self.eat_keyword("in") {
            return Err(self.malformed());
        }
        let body = self.parse_expr()?;
        Ok(apply("let", vec![variable(&name), value, body]))
    }

    fn parse_special_atom(&mut self, kind: &str) -> Result<Term, CanonicalError> {
        if !self.eat_keyword(kind) || !self.eat_char('(') {
            return Err(self.malformed());
        }
        let name = self.parse_raw_name()?;
        if !self.eat_char(')') {
            return Err(self.malformed());
        }
        match kind {
            "var" => Ok(variable(&name)),
            _ => Ok(constant(&name)),
        }
    }

    fn parse_apply_form(&mut self) -> Result<Term, CanonicalError> {
        if !self.eat_keyword("apply") || !self.eat_char('(') {
            return Err(self.malformed());
        }
        self.skip_ws();
        let Some(operator) = self.parse_ident() else {
            return Err(self.malformed());
        };
        let mut arguments = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_char(')') {
                return Ok(apply(&operator, arguments));
            }
            if !self.eat_char(',') {
                return Err(self.malformed());
            }
            arguments.push(self.parse_expr()?);
        }
    }

    fn parse_arg_list(&mut self, closer: char) -> Result<Vec<Term>, CanonicalError> {
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_char(closer) {
                return Ok(args);
            }
            if !args.is_empty() && !self.eat_char(',') {
                return Err(self.malformed());
            }
            self.skip_ws();
            if self.eat_char(closer) {
                return Ok(args);
            }
            args.push(self.parse_expr()?);
        }
    }

    fn parse_number(&mut self) -> Option<String> {
        let start = self.pos;
        let rest = self.remaining();
        let mut chars = rest.char_indices().peekable();
        if chars.peek().map(|(_, ch)| *ch) == Some('-') {
            let next = rest[1..].chars().next()?;
            if !next.is_ascii_digit() {
                return None;
            }
            chars.next();
        }
        let Some((_, first)) = chars.peek() else {
            return None;
        };
        if !first.is_ascii_digit() {
            return None;
        }
        while chars.peek().is_some_and(|(_, ch)| ch.is_ascii_digit()) {
            chars.next();
        }
        if chars.peek().map(|(_, ch)| *ch) == Some('.') {
            let after_dot = rest[chars.peek().unwrap().0 + 1..].chars().next();
            if after_dot.is_some_and(|ch| ch.is_ascii_digit()) {
                chars.next();
                while chars.peek().is_some_and(|(_, ch)| ch.is_ascii_digit()) {
                    chars.next();
                }
            }
        }
        if chars
            .peek()
            .is_some_and(|(_, ch)| *ch == 'e' || *ch == 'E')
        {
            let saved = chars.clone();
            chars.next();
            if chars.peek().is_some_and(|(_, ch)| *ch == '+' || *ch == '-') {
                chars.next();
            }
            if chars.peek().is_some_and(|(_, ch)| ch.is_ascii_digit()) {
                while chars.peek().is_some_and(|(_, ch)| ch.is_ascii_digit()) {
                    chars.next();
                }
            } else {
                chars = saved;
            }
        }
        let end = chars.peek().map_or(start + rest.len(), |(index, _)| start + *index);
        if end == start {
            return None;
        }
        let lexeme = std::str::from_utf8(&self.bytes[start..end]).ok()?.to_string();
        self.pos = end;
        Some(lexeme)
    }

    fn parse_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let rest = self.remaining();
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !is_ident_start(first) {
            return None;
        }
        let mut len = first.len_utf8();
        for ch in chars {
            if ch == '-' {
                let next = rest[len + 1..].chars().next();
                if next.is_some_and(|next| next.is_ascii_alphanumeric() || next == '_') {
                    len += 1;
                    continue;
                }
                break;
            }
            if is_ident_continue(ch) {
                len += ch.len_utf8();
                continue;
            }
            break;
        }
        let name = rest[..len].to_string();
        if is_reserved(&name) {
            return None;
        }
        self.pos += len;
        Some(name)
    }

    fn parse_raw_name(&mut self) -> Result<String, CanonicalError> {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == ')' {
                break;
            }
            self.bump();
        }
        if start == self.pos {
            return Err(self.malformed());
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map(str::to_string)
            .map_err(|_| self.malformed())
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':')
}

fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "if" | "then" | "else" | "let" | "in" | "and" | "or" | "not" | "true" | "false"
    )
}

fn apply(operator: &str, arguments: Vec<Term>) -> Term {
    Term::Apply {
        operator: SymbolId(operator.to_string()),
        arguments,
    }
}

fn apply_on(callee: Term, arguments: Vec<Term>) -> Term {
    match callee {
        Term::Variable(VariableId(name)) | Term::Constant(SymbolId(name)) => apply(&name, arguments),
        other => {
            let mut args = vec![other];
            args.extend(arguments);
            apply("apply", args)
        }
    }
}

fn variable(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn constant(name: &str) -> Term {
    Term::Constant(SymbolId(name.to_string()))
}
