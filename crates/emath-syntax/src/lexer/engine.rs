//! The Lexer state machine (moved verbatim).

use super::*;

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_char(&self) -> Option<char> {
        self.peek_char_at(self.pos)
    }

    fn peek_char_at(&self, pos: usize) -> Option<char> {
        self.source.get(pos..).and_then(|rest| rest.chars().next())
    }

    fn peek2(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn peek3(&self) -> Option<u8> {
        self.bytes.get(self.pos + 2).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn span(&self, start: usize) -> Span {
        Span::new(
            self.file,
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(self.pos).unwrap_or(u32::MAX),
        )
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, start: usize) {
        self.diagnostics.error(code, message, self.span(start));
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        if self.tokens.len() >= self.limits.max_tokens {
            if self.tokens.len() == self.limits.max_tokens {
                self.error(
                    "E-SYN-108",
                    "token limit exceeded; source too complex",
                    start,
                );
            }
            return;
        }
        self.tokens.push(Token {
            kind,
            span: self.span(start),
        });
    }

    pub(super) fn lex_lines(&mut self) {
        // Layout analysis runs once per real line: after a newline (outside
        // parens) the next token starts a fresh indentation context; further
        // tokens on the same line continue the current context.
        let mut at_line_start = true;
        if self.bytes.len() >= 3
            && self.bytes[0] == 0xEF
            && self.bytes[1] == 0xBB
            && self.bytes[2] == 0xBF
        {
            self.error(
                "E-SYN-113",
                "UTF-8 BOM is rejected at the start of a source file",
                0,
            );
            self.pos = 3;
        }

        while self.pos < self.bytes.len() {
            let line_start = self.pos;
            if self.peek() == Some(b'\n') {
                self.pos += 1;
                if self.paren_depth == 0 && !at_line_start {
                    self.push(TokenKind::Newline, line_start);
                }
                at_line_start = true;
                continue;
            }
            if self.peek() == Some(b'\r') {
                self.pos += 1;
                continue;
            }
            if at_line_start {
                // Leading spaces only. Tabs are rejected as content so they
                // cannot silently widen or shrink indent.
                while self.peek() == Some(b' ') {
                    self.pos += 1;
                }
                // Comment-only or blank line: consume and emit no layout
                // tokens; `at_line_start` stays true for the next line.
                // Ordinary comments are `#`; documentation is `///`.
                // `//` is the rational separator, not a comment.
                if self.peek() == Some(b'#') {
                    self.skip_line_comment();
                    continue;
                }
                if self.peek() == Some(b'/')
                    && self.peek2() == Some(b'/')
                    && self.peek3() == Some(b'/')
                {
                    self.skip_line_comment();
                    continue;
                }
                if matches!(self.peek(), None | Some(b'\n')) {
                    continue;
                }
                let content_start = self.pos;
                if self.paren_depth == 0 {
                    let indent = content_start - line_start;
                    if indent > *self.indent_stack.last().unwrap_or(&0) {
                        // Do not grow the stack past the token budget: a
                        // rejected Indent would otherwise force unbounded
                        // trailing Dedents at EOF that bypass `push`.
                        if self.tokens.len() < self.limits.max_tokens {
                            self.indent_stack.push(indent);
                        }
                        self.push(TokenKind::Indent, line_start);
                    } else {
                        while self.indent_stack.len() > 1
                            && indent < *self.indent_stack.last().unwrap_or(&0)
                        {
                            self.indent_stack.pop();
                            self.push(TokenKind::Dedent, line_start);
                        }
                        if indent != *self.indent_stack.last().unwrap_or(&0) {
                            self.error(
                                "E-SYN-100",
                                "inconsistent indentation: dedent does not match an enclosing block",
                                line_start,
                            );
                            self.indent_stack.pop();
                        }
                    }
                }
                at_line_start = false;
            }
            self.lex_token();
        }
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.push(TokenKind::Dedent, self.bytes.len());
        }
    }

    fn skip_line_comment(&mut self) {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            self.pos += 1;
        }
        self.record_comment(start, self.pos);
    }

    fn record_comment(&mut self, start: usize, end: usize) {
        if !self.keep_comments {
            return;
        }
        let text = self.source[start..end].trim_end().to_string();
        if text.is_empty() {
            return;
        }
        self.comments.push(Comment {
            text,
            span: Span::new(
                self.file,
                u32::try_from(start).unwrap_or(u32::MAX),
                u32::try_from(end).unwrap_or(u32::MAX),
            ),
            own_line: self.at_line_start(),
        });
    }

    fn at_line_start(&self) -> bool {
        // The lexer has consumed indentation before real content; comments
        // reached through the at_line_start branch are line leads. We track
        // this implicitly by inspecting the last emitted token: a comment is
        // own_line when the previous significant token was Newline (or none).
        match self.tokens.last() {
            None => true,
            Some(token) => matches!(token.kind, TokenKind::Newline),
        }
    }

    fn lex_token(&mut self) {
        let start = self.pos;
        let Some(byte) = self.peek() else {
            return;
        };
        match byte {
            b'0'..=b'9' => self.lex_number(),
            b'"' => self.lex_string(),
            b'#' => {
                self.skip_line_comment();
            }
            b'/' if self.peek2() == Some(b'/') && self.peek3() == Some(b'/') => {
                self.skip_line_comment();
            }
            b'/' if self.peek2() == Some(b'/') => {
                self.pos += 2;
                self.push(TokenKind::SlashSlash, start);
            }
            _ if byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80 => {
                // `±` (U+00B1, UTF-8 C2 B1) is the measurement-literal
                // separator (spec 04 section 1.5 / X6), not an identifier
                // glyph — it must be claimed before the non-ASCII ident
                // path sees it.
                if byte == 0xc2 && self.peek2() == Some(0xb1) {
                    self.pos += 2;
                    self.push(TokenKind::PlusMinus, start);
                } else if byte == 0xE2
                    && self.bytes.get(self.pos + 1) == Some(&0x89)
                    && self.bytes.get(self.pos + 2) == Some(&0x88)
                {
                    // `≈` (U+2248, UTF-8 E2 89 88) — approximation
                    // labeling operator (04 §6.4). Claimed before the
                    // non-ASCII ident path so the glyph never glues into
                    // one unknown name.
                    self.pos += 3;
                    self.push(TokenKind::TildeEq, start);
                } else if byte == 0xE2
                    && self.bytes.get(self.pos + 1) == Some(&0x88)
                    && self.bytes.get(self.pos + 2) == Some(&0x87)
                {
                    // `∇` (U+2207, UTF-8 E2 88 87) — nabla family (pack). The follower byte decides the form: `·`
                    // (C2 B7) is divergence, `×` (C3 97) is curl, `²`
                    // (C2 B2) is the stencil Laplacian; a bare ∇ is the
                    // gradient glyph. Claimed before the non-ASCII ident
                    // path so the glyph never glues into one unknown name.
                    self.pos += 3;
                    let form = match (self.bytes.get(self.pos), self.bytes.get(self.pos + 1)) {
                        (Some(&0xC2), Some(&0xB7)) => {
                            self.pos += 2;
                            NablaForm::Div
                        }
                        (Some(&0xC3), Some(&0x97)) => {
                            self.pos += 2;
                            NablaForm::Curl
                        }
                        (Some(&0xC2), Some(&0xB2)) => {
                            self.pos += 2;
                            NablaForm::Lap
                        }
                        _ => NablaForm::Grad,
                    };
                    self.push(TokenKind::Nabla(form), start);
                } else if byte == 0xE2
                    && self.bytes.get(self.pos + 1) == Some(&0x9F)
                    && matches!(self.bytes.get(self.pos + 2), Some(&0xA8) | Some(&0xA9))
                {
                    // `⟨` (U+27E8, E2 9F A8) / `⟩` (U+27E9, E2 9F A9) —
                    // braket pack angles. Claimed before the
                    // non-ASCII ident path so the glyphs never glue into
                    // one unknown name; only the mounted braket pack
                    // admits forms built from them.
                    self.pos += 3;
                    let kind = if self.bytes.get(self.pos - 1) == Some(&0xA8) {
                        TokenKind::LAngle
                    } else {
                        TokenKind::RAngle
                    };
                    self.push(kind, start);
                } else if byte == 0xE2
                    && self.bytes.get(self.pos + 1) == Some(&0x88)
                    && self.bytes.get(self.pos + 2) == Some(&0x85)
                {
                    // `∅` (U+2205, UTF-8 E2 88 85) — the declared sink
                    // (04 §4.1): a
                    // reaction endpoint that is deliberately nothing
                    // (degradation/elimination), never a magic empty
                    // side. Claimed before the non-ASCII ident path so
                    // the glyph never glues into one unknown name.
                    self.pos += 3;
                    self.push(TokenKind::EmptySet, start);
                } else {
                    self.lex_ident(start);
                }
            }
            b'=' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    // `==>` — logical implication (B12)
                    if self.peek() == Some(b'>') {
                        self.pos += 1;
                        self.push(TokenKind::Imply, start);
                    } else {
                        self.push(TokenKind::EqEq, start);
                    }
                } else if self.peek() == Some(b'>') {
                    self.pos += 1;
                    self.push(TokenKind::Arrow, start);
                } else {
                    self.push(TokenKind::Eq, start);
                }
            }
            b'!' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    self.push(TokenKind::NotEq, start);
                } else {
                    self.push(TokenKind::Bang, start);
                }
            }
            b'<' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    // `<==>` — logical biconditional (B12)
                    if self.peek() == Some(b'=') && self.peek2() == Some(b'>') {
                        self.pos += 2;
                        self.push(TokenKind::Iff, start);
                    } else {
                        self.push(TokenKind::Le, start);
                    }
                } else {
                    self.push(TokenKind::Lt, start);
                }
            }
            b'>' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    self.push(TokenKind::Ge, start);
                } else {
                    self.push(TokenKind::Gt, start);
                }
            }
            b'+' => {
                self.pos += 1;
                self.push(TokenKind::Plus, start);
            }
            b'-' => {
                self.pos += 1;
                if self.peek() == Some(b'>') {
                    self.pos += 1;
                    self.push(TokenKind::Arrow, start);
                } else if self.peek() == Some(b'-') && self.peek2() == Some(b'>') {
                    // `-->` — directed graph edge arrow (B23).
                    // Glued so it beats `->`; `x--y` (no `>`) stays two
                    // Minus tokens, so outside-graph arithmetic is
                    // untouched (G4).
                    self.pos += 2;
                    self.push(TokenKind::EdgeArrow, start);
                } else {
                    self.push(TokenKind::Minus, start);
                }
            }
            b'*' => {
                self.pos += 1;
                self.push(TokenKind::Star, start);
            }
            b'/' => {
                self.pos += 1;
                self.push(TokenKind::Slash, start);
            }
            b'^' => {
                self.pos += 1;
                self.push(TokenKind::Caret, start);
            }
            b'~' => {
                self.pos += 1;
                if self.peek() == Some(b'~') {
                    self.pos += 1;
                    self.push(TokenKind::TildeTilde, start);
                } else if self.peek() == Some(b'=') {
                    // `~=` — ASCII spelling of the approximation
                    // labeling operator (04 §6.4); one token, same as `≈`.
                    self.pos += 1;
                    self.push(TokenKind::TildeEq, start);
                } else {
                    // `~ name` distribution tag on measurement literals
                    // (spec 04 section 1.5); the parser refuses a bare
                    // `~` where no tag can appear.
                    self.push(TokenKind::Tilde, start);
                }
            }
            b'(' => {
                self.pos += 1;
                self.paren_depth += 1;
                self.nesting += 1;
                if self.nesting > self.limits.max_nesting {
                    self.error("E-SYN-106", "nesting limit exceeded", start);
                }
                self.push(TokenKind::LParen, start);
            }
            b')' => {
                self.pos += 1;
                self.paren_depth = self.paren_depth.saturating_sub(1_u32);
                self.nesting = self.nesting.saturating_sub(1_usize);
                self.push(TokenKind::RParen, start);
            }
            b'[' => {
                self.pos += 1;
                self.paren_depth += 1;
                self.nesting += 1;
                if self.nesting > self.limits.max_nesting {
                    self.error("E-SYN-106", "nesting limit exceeded", start);
                }
                self.push(TokenKind::LBracket, start);
            }
            b']' => {
                self.pos += 1;
                self.paren_depth = self.paren_depth.saturating_sub(1_u32);
                self.nesting = self.nesting.saturating_sub(1_usize);
                self.push(TokenKind::RBracket, start);
            }
            b'{' => {
                self.pos += 1;
                self.paren_depth += 1;
                self.nesting = self.nesting.saturating_add(1_usize);
                if self.nesting > self.limits.max_nesting {
                    self.error("E-SYN-106", "nesting limit exceeded", start);
                }
                self.push(TokenKind::LBrace, start);
            }
            b'}' => {
                self.pos += 1;
                self.paren_depth = self.paren_depth.saturating_sub(1_u32);
                self.nesting = self.nesting.saturating_sub(1_usize);
                self.push(TokenKind::RBrace, start);
            }
            b',' => {
                self.pos += 1;
                self.push(TokenKind::Comma, start);
            }
            b':' => {
                self.pos += 1;
                if self.peek() == Some(b':') {
                    self.pos += 1;
                    self.push(TokenKind::PathSep, start);
                } else {
                    self.push(TokenKind::Colon, start);
                }
            }
            b'.' => {
                self.pos += 1;
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    if self.peek() == Some(b'=') {
                        self.pos += 1;
                        self.push(TokenKind::DotDotEq, start);
                    } else {
                        self.push(TokenKind::DotDot, start);
                    }
                } else {
                    self.push(TokenKind::Dot, start);
                }
            }
            b'?' => {
                self.pos += 1;
                self.push(TokenKind::Question, start);
            }
            b'&' => {
                self.pos += 1;
                self.push(TokenKind::Amp, start);
            }
            b'@' => {
                self.pos += 1;
                self.push(TokenKind::AtSign, start);
            }
            b'|' => {
                self.pos += 1;
                self.push(TokenKind::Pipe, start);
            }
            b';' => {
                // U9: row separator inside list literals only; the parser
                // refuses it anywhere else, so lexing stays additive.
                self.pos += 1;
                self.push(TokenKind::Semicolon, start);
            }
            b' ' => {
                self.pos += 1;
            }
            b'\t' => {
                self.pos += 1;
                self.error(
                    "E-SYN-101",
                    "tab is rejected in canonical source; indent with four spaces",
                    start,
                );
            }
            other => {
                self.pos += 1;
                self.error(
                    "E-SYN-101",
                    format!("unexpected character `{}`", char::from(other)),
                    start,
                );
            }
        }
    }

    /// Letter-idents (`x`, `αβ`, `Δx`) stay one token; math-symbol idents
    /// (`⊕`, `√`, `¬`) do not glue to adjacent letters, so `x⊕y` and `√a`
    /// tokenize as operator uses rather than a single unknown name.
    fn lex_ident(&mut self, start: usize) {
        let Some(first) = self.peek_char() else {
            return;
        };
        self.pos += first.len_utf8();
        if is_letter_ident_start(first) {
            while let Some(ch) = self.peek_char() {
                if is_letter_ident_continue(ch) {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
        } else {
            while let Some(ch) = self.peek_char() {
                if is_symbol_ident_continue(ch) {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
        let text = &self.source[start..self.pos];
        if !text.is_ascii() {
            // NFC identity (spec `01_LEXICAL_LAYOUT_AND_SOURCE`):
            // an identifier built from combining diacritic marks
            // is canonically non-NFC by construction and cannot
            // be re-normalized without a Unicode table. Refuse
            // it (E-SYN-115) instead of admitting an identity
            // the pipeline cannot verify.
            if text.chars().any(is_combining_mark) {
                self.error(
                    "E-SYN-115",
                    "identifier contains a combining mark; source must be NFC",
                    start,
                );
            } else {
                self.diagnostics.warning(
                    "E-SYN-114",
                    "identifier contains non-ASCII characters; confusable Unicode lookalikes are a quality hazard",
                    self.span(start),
                );
            }
        }

        if let Some(keyword) = Keyword::from_ident(text) {
            self.push(TokenKind::Keyword(keyword), start);
        } else {
            self.push(TokenKind::Ident(text.to_string()), start);
        }
    }

    fn lex_number(&mut self) {
        let start = self.pos;
        let mut is_float = false;
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') && self.peek2().is_some_and(|b| b.is_ascii_digit()) {
            is_float = true;
            self.pos += 1;
            while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mut look = self.pos + 1;
            if matches!(self.bytes.get(look), Some(b'+' | b'-')) {
                look += 1;
            }
            if self.bytes.get(look).is_some_and(u8::is_ascii_digit) {
                is_float = true;
                self.pos = look;
                while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
                    self.pos += 1;
                }
            }
        }
        if is_float {
            let rest = &self.source[self.pos..];
            for suffix in ["bf16", "f16", "f32", "f64", "f128"] {
                if rest.starts_with(suffix) {
                    self.pos += suffix.len();
                    break;
                }
            }
        }
        // B14: Complex literal suffix `Ni` (e.g., `2i`, `3.5i`).
        // Only when `i` is not followed by a letter-ident continue
        // (`2image` stays Int("2") + Ident("image"); `2i⊕3` is still
        // complex `2i` then the math-symbol token `⊕`).
        if self.peek() == Some(b'i')
            && !self
                .peek_char_at(self.pos + 1)
                .is_some_and(is_letter_ident_continue)
        {
            self.pos += 1; // consume `i`
            is_float = true; // complex literals use the Float channel
        }
        let text = &self.source[start..self.pos];
        // Attached parenthetical uncertainty (spec 04 section 1.5 /
        // CODATA): `0.5012(3)` = 0.5012 ± 0.0003. Attachment is lexical:
        // immediately after the number (no space), digits only. A space
        // before `(` or a non-digit inside leaves ordinary tokenization —
        // `f(2)` is a call (leading token is an Ident, never scanned
        // here), and `1.50 (2)` lexes as Float + group, which the parser
        // refuses where an uncertainty cannot appear.
        if is_float
            && self.peek() == Some(b'(')
            && self
                .peek_char_at(self.pos + 1)
                .is_some_and(|b| b.is_ascii_digit())
        {
            let scan = self.pos + 1;
            let mut cursor = scan;
            while self
                .peek_char_at(cursor)
                .is_some_and(|b| b.is_ascii_digit())
            {
                cursor += 1;
            }
            if self.peek_char_at(cursor) == Some(')') {
                let digits = self.source[scan..cursor].to_string();
                // CODATA: the exponent may follow the uncertainty
                // parenthetical (`6.67430(15)e-11`); it stays part of the
                // number spelling so admission scales ±digits by 10^exp.
                let mut end = cursor + 1;
                if matches!(self.bytes.get(end), Some(b'e' | b'E')) {
                    let mut look = end + 1;
                    if matches!(self.bytes.get(look), Some(b'+' | b'-')) {
                        look += 1;
                    }
                    if self.bytes.get(look).is_some_and(u8::is_ascii_digit) {
                        end = look;
                        while self.bytes.get(end).is_some_and(|b| b.is_ascii_digit()) {
                            end += 1;
                        }
                    }
                }
                // Build owned spellings first: `text`/`digits` borrow
                // `self.source`, and the cursor advance mutates `self`.
                let number = if end > cursor + 1 {
                    format!("{text}{}", &self.source[cursor + 1..end])
                } else {
                    text.to_string()
                };
                self.pos = end;
                self.push(TokenKind::FloatUncertainty { number, digits }, start);
                return;
            }
        }
        if is_float {
            self.push(TokenKind::Float(text.to_string()), start);
        } else {
            self.push(TokenKind::Int(text.to_string()), start);
        }
    }

    fn lex_string(&mut self) {
        let start = self.pos;
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            let Some(byte) = self.peek() else {
                self.error("E-SYN-105", "unterminated string literal", start);
                return;
            };
            if byte == b'"' {
                self.pos += 1;
                break;
            }
            if byte == b'\n' {
                self.error("E-SYN-105", "unterminated string literal", start);
                return;
            }
            if byte == b'\\' {
                self.pos += 1;
                let Some(escaped) = self.peek() else {
                    self.error("E-SYN-105", "unterminated string literal", start);
                    return;
                };
                self.pos += 1;
                match escaped {
                    b'n' => value.push('\n'),
                    b't' => value.push('\t'),
                    b'r' => value.push('\r'),
                    b'"' => value.push('"'),
                    b'\\' => value.push('\\'),
                    b'u' => {
                        // Invalid `\u` forms must stay inside the string so the
                        // closing quote is still found; `break` would exit the
                        // lex loop, emit a truncated Str, and retokenize the tail.
                        if !self.eat(b'{') {
                            self.error("E-SYN-109", "invalid string escape", start);
                            continue;
                        }
                        let hex_start = self.pos;
                        while self.peek().is_some_and(|b| b.is_ascii_hexdigit()) {
                            self.pos += 1;
                        }
                        let hex = &self.source[hex_start..self.pos];
                        if !self.eat(b'}') || hex.is_empty() {
                            self.error("E-SYN-109", "invalid string escape", start);
                            continue;
                        }
                        match u32::from_str_radix(hex, 16) {
                            Ok(code) => {
                                if let Some(ch) = char::from_u32(code) {
                                    value.push(ch);
                                } else {
                                    self.error("E-SYN-109", "invalid unicode escape", start);
                                }
                            }
                            Err(_) => {
                                self.error("E-SYN-109", "invalid unicode escape", start);
                            }
                        }
                    }
                    other => {
                        self.error(
                            "E-SYN-109",
                            format!("invalid string escape `\\{}`", char::from(other)),
                            start,
                        );
                        value.push(char::from(other));
                    }
                }
                continue;
            }
            let ch = self.source[self.pos..].chars().next().unwrap_or('\u{fffd}');
            self.pos += ch.len_utf8();
            value.push(ch);
        }
        self.push(TokenKind::Str(value), start);
    }
}

pub(super) fn is_combining_mark(ch: char) -> bool {
    matches!(ch, '\u{0300}'..='\u{036F}')
}

pub(super) fn is_letter_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

pub(super) fn is_letter_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch.is_alphabetic() || is_combining_mark(ch)
}

pub(super) fn is_symbol_ident_continue(ch: char) -> bool {
    !ch.is_ascii() && !ch.is_alphabetic()
}
