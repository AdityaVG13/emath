#![forbid(unsafe_code)]

//! Provider-neutral first-order term representation.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

/// Stable symbol identity within a signature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub String);

/// Stable free-variable identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub String);

/// A finite first-order term.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    /// A free variable.
    Variable(VariableId),
    /// A nullary symbol.
    Constant(SymbolId),
    /// An operator applied to ordered arguments.
    Apply {
        /// Operator identity.
        operator: SymbolId,
        /// Ordered argument terms.
        arguments: Vec<Term>,
    },
}

/// A finite operator signature mapping symbols to arities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signature {
    arities: BTreeMap<SymbolId, usize>,
}

/// Structural validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermError {
    /// A symbol was used without an arity declaration.
    UnknownSymbol(SymbolId),
    /// A symbol application used the wrong number of arguments.
    ArityMismatch {
        /// Symbol identity.
        symbol: SymbolId,
        /// Declared arity.
        expected: usize,
        /// Observed arity.
        actual: usize,
    },
    /// One symbol was declared with conflicting arities.
    ConflictingArity {
        /// Symbol identity.
        symbol: SymbolId,
        /// Earlier declaration.
        first: usize,
        /// Conflicting declaration.
        second: usize,
    },
}

impl Signature {
    /// Inserts a symbol arity, rejecting conflicts.
    pub fn insert(&mut self, symbol: SymbolId, arity: usize) -> Result<(), TermError> {
        if let Some(first) = self.arities.get(&symbol).copied() {
            if first != arity {
                return Err(TermError::ConflictingArity {
                    symbol,
                    first,
                    second: arity,
                });
            }
            return Ok(());
        }
        self.arities.insert(symbol, arity);
        Ok(())
    }

    /// Returns the declared arity.
    #[must_use]
    pub fn arity(&self, symbol: &SymbolId) -> Option<usize> {
        self.arities.get(symbol).copied()
    }

    /// Iterates over declarations in canonical symbol order.
    pub fn iter(&self) -> impl Iterator<Item = (&SymbolId, &usize)> {
        self.arities.iter()
    }

    /// Validates a term recursively.
    pub fn validate(&self, term: &Term) -> Result<(), TermError> {
        match term {
            Term::Variable(_) => Ok(()),
            Term::Constant(symbol) => self.validate_application(symbol, 0),
            Term::Apply {
                operator,
                arguments,
            } => {
                self.validate_application(operator, arguments.len())?;
                for argument in arguments {
                    self.validate(argument)?;
                }
                Ok(())
            }
        }
    }

    fn validate_application(&self, symbol: &SymbolId, actual: usize) -> Result<(), TermError> {
        let expected = self
            .arity(symbol)
            .ok_or_else(|| TermError::UnknownSymbol(symbol.clone()))?;
        if expected != actual {
            return Err(TermError::ArityMismatch {
                symbol: symbol.clone(),
                expected,
                actual,
            });
        }
        Ok(())
    }
}

impl Term {
    /// Renders a deterministic structural form independent of glyph fixity.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut output = String::new();
        self.write_canonical(&mut output)
            .expect("writing into String cannot fail");
        output
    }

    fn write_canonical(&self, output: &mut String) -> fmt::Result {
        match self {
            Self::Variable(variable) => write!(output, "var({})", escape(&variable.0)),
            Self::Constant(symbol) => write!(output, "const({})", escape(&symbol.0)),
            Self::Apply {
                operator,
                arguments,
            } => {
                write!(output, "apply({}", escape(&operator.0))?;
                for argument in arguments {
                    output.push(',');
                    argument.write_canonical(output)?;
                }
                output.push(')');
                Ok(())
            }
        }
    }
}

impl Term {
    /// Parses the canonical form produced by [`Term::canonical`] back into a
    /// term, preserving glyph byte-exactness.
    pub fn parse_canonical(text: &str) -> Result<Self, CanonicalError> {
        let mut parser = CanonicalParser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let term = parser.parse_term()?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err(CanonicalError::Trailing {
                text: text.to_string(),
            });
        }
        Ok(term)
    }
}

/// Error from [`Term::parse_canonical`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// Unterminated or structurally invalid canonical form.
    Malformed { text: String },
    /// Non-whitespace content after the canonical term.
    Trailing { text: String },
}

struct CanonicalParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl CanonicalParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
        {
            self.pos += 1;
        }
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self.bytes[self.pos..].starts_with(expected.as_bytes()) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn parse_term(&mut self) -> Result<Term, CanonicalError> {
        if self.eat("var") {
            if !self.eat("(") {
                return Err(self.malformed());
            }
            let name = self.parse_name(false)?;
            if !self.eat(")") {
                return Err(self.malformed());
            }
            return Ok(Term::Variable(VariableId(name)));
        }
        if self.eat("const") {
            if !self.eat("(") {
                return Err(self.malformed());
            }
            let name = self.parse_name(false)?;
            if !self.eat(")") {
                return Err(self.malformed());
            }
            return Ok(Term::Constant(SymbolId(name)));
        }
        if !self.eat("apply") {
            return Err(self.malformed());
        }
        if !self.eat("(") {
            return Err(self.malformed());
        }
        let operator = self.parse_name(true)?;
        let mut arguments = Vec::new();
        loop {
            if self.eat(")") {
                return Ok(Term::Apply {
                    operator: SymbolId(operator),
                    arguments,
                });
            }
            if !self.eat(",") {
                return Err(self.malformed());
            }
            arguments.push(self.parse_term()?);
        }
    }

    /// Reads an escaped name up to its terminator: `)` for `var`/`const`
    /// names, `,` or `)` for operator names.
    fn parse_name(&mut self, stop_at_comma: bool) -> Result<String, CanonicalError> {
        let mut name = String::new();
        while let Some(byte) = self.peek() {
            if byte == b'\\' {
                self.pos += 1;
                let Some(escaped) = self.peek() else {
                    return Err(self.malformed());
                };
                name.push(char::from(escaped));
                self.pos += 1;
                continue;
            }
            if byte == b')' || (stop_at_comma && byte == b',') {
                return Ok(name);
            }
            let Ok(rest) = std::str::from_utf8(&self.bytes[self.pos..]) else {
                return Err(self.malformed());
            };
            let Some(ch) = rest.chars().next() else {
                return Err(self.malformed());
            };
            name.push(ch);
            self.pos += ch.len_utf8();
        }
        Err(self.malformed())
    }

    fn malformed(&self) -> CanonicalError {
        CanonicalError::Malformed {
            text: String::from_utf8_lossy(&self.bytes[self.pos..]).into_owned(),
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}
