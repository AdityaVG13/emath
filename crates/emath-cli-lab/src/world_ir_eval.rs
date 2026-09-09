//! World-IR builtin execution: the 8 `emath_world_ir::builtin_worlds()`
//! candidate classes run on the reference VM through one adapter.
//!
//! The world stays pure declaration data (`WorldIr`); the adapter is the
//! World ABI seam — a new builtin class needs no evaluator change (the
//! trait contract). Each operator's declared `OperatorSemantics` is
//! interpreted at VM time:
//!
//! - `FiniteTable` cells (`"0,1→1"`, `"true,false→false"`) look up by
//!   canonical carrier keys;
//! - `DeclaredExpression` evaluates over the integer carrier with the
//!   positional formals (`x` is argument 0, `y` argument 1);
//! - `StructuralConstructor` symbols rebuild terms (free term mode: no
//!   reduction laws);
//! - prose expressions, provider bindings, and meaning holes keep the
//!   term's structural residue: the world discloses what it cannot
//!   execute instead of fabricating carrier values.
//!
//! Total by construction: every symbol's interpretation either computes
//! or keeps structure — the adapter never invents carrier elements the
//! declaration did not write.

use std::collections::BTreeMap;

use emath_genesis::{EvalError, FirstOrderWorld, WorldEvidence};
use emath_term::{SymbolId, Term};
use emath_world_ir::{OperatorSemantics, WorldIr};

/// A World-IR builtin runtime value: an integer carrier element, a
/// boolean carrier element, or the structural residue of a subterm the
/// declaration does not define.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldIrValue {
    /// Integer carrier element (`Int`, `Nat`, `Fin3`, `Z3`).
    Int(i64),
    /// Boolean carrier element (the lattice's `true`/`false`).
    Bool(bool),
    /// The structural residue of an uninterpreted subterm.
    Structural(Term),
}

impl WorldIrValue {
    /// Canonical single-line carrier form for the receipt answer.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            Self::Structural(term) => term.canonical(),
        }
    }

    /// The finite-table key for a carrier value (`"true"`, `"false"`,
    /// decimal digits); structural residues carry no key.
    fn cell_key(&self) -> Option<String> {
        match self {
            Self::Int(value) => Some(value.to_string()),
            Self::Bool(true) => Some("true".to_string()),
            Self::Bool(false) => Some("false".to_string()),
            Self::Structural(_) => None,
        }
    }

    /// The structural form of an evaluated value: carrier elements
    /// become their own constant symbols, so partially evaluated terms
    /// stay canonical.
    fn into_term(self) -> Term {
        match self {
            Self::Int(value) => Term::Constant(SymbolId(value.to_string())),
            Self::Bool(true) => Term::Constant(SymbolId("true".to_string())),
            Self::Bool(false) => Term::Constant(SymbolId("false".to_string())),
            Self::Structural(term) => term,
        }
    }
}

/// Parses one declared carrier element: the booleans and decimal
/// integers the builtin declarations write; anything else has no
/// carrier value here.
fn parse_carrier_element(text: &str) -> Option<WorldIrValue> {
    match text.trim() {
        "true" => Some(WorldIrValue::Bool(true)),
        "false" => Some(WorldIrValue::Bool(false)),
        trimmed => trimmed.parse::<i64>().ok().map(WorldIrValue::Int),
    }
}

/// One declared-expression token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExprToken {
    /// A decimal literal.
    Literal(i64),
    /// A positional formal (`x` = argument 0, `y` = argument 1).
    Formal(usize),
    /// `+`
    Add,
    /// `-` (binary subtraction or unary negation, by position).
    Sub,
    /// `*`
    Mul,
    /// `(`
    Open,
    /// `)`
    Close,
}

/// Tokenizes a declared expression: decimal literals, the positional
/// formals `x`/`y`, `+`, `-`, `*`, and parentheses. Any other shape
/// refuses, so the operator evaluates structurally instead.
fn tokenize_declared_expression(expression: &str) -> Option<Vec<ExprToken>> {
    let mut tokens = Vec::new();
    let mut characters = expression.chars().peekable();
    while let Some(&character) = characters.peek() {
        match character {
            ' ' | '\t' | '\n' | '\r' => {
                characters.next();
            }
            '+' => {
                tokens.push(ExprToken::Add);
                characters.next();
            }
            '-' => {
                tokens.push(ExprToken::Sub);
                characters.next();
            }
            '*' => {
                tokens.push(ExprToken::Mul);
                characters.next();
            }
            '(' => {
                tokens.push(ExprToken::Open);
                characters.next();
            }
            ')' => {
                tokens.push(ExprToken::Close);
                characters.next();
            }
            '0'..='9' => {
                let mut digits = String::new();
                while let Some(&digit) = characters.peek() {
                    if digit.is_ascii_digit() {
                        digits.push(digit);
                        characters.next();
                    } else {
                        break;
                    }
                }
                tokens.push(ExprToken::Literal(digits.parse().ok()?));
            }
            'x' | 'y' => {
                characters.next();
                tokens.push(ExprToken::Formal(usize::from(character == 'y')));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

/// Recursive-descent evaluator for declared integer expressions:
///
/// ```text
/// sum     := product (('+' | '-') product)*
/// product := unary ('*' unary)*
/// unary   := '-' unary | primary
/// primary := LITERAL | FORMAL | '(' sum ')'
/// ```
///
/// Standard precedence, left associativity, bounded paren depth, checked
/// integer arithmetic. The builtin declarations use `x + y`, `x * y`,
/// and unary `-x`; a form outside this grammar evaluates to `None` and
/// the operator keeps its structural form instead.
struct DeclaredExpression<'a> {
    tokens: &'a [ExprToken],
    formals: &'a [i64],
    position: usize,
    depth: u32,
}

impl DeclaredExpression<'_> {
    /// Parenthesis nesting bound for declared expressions.
    const MAX_DEPTH: u32 = 64;

    fn peek(&self) -> Option<ExprToken> {
        self.tokens.get(self.position).copied()
    }

    fn sum(&mut self) -> Option<i64> {
        let mut value = self.product()?;
        loop {
            match self.peek() {
                Some(ExprToken::Add) => {
                    self.position += 1;
                    value = value.checked_add(self.product()?)?;
                }
                Some(ExprToken::Sub) => {
                    self.position += 1;
                    value = value.checked_sub(self.product()?)?;
                }
                _ => return Some(value),
            }
        }
    }

    fn product(&mut self) -> Option<i64> {
        let mut value = self.unary()?;
        while matches!(self.peek(), Some(ExprToken::Mul)) {
            self.position += 1;
            value = value.checked_mul(self.unary()?)?;
        }
        Some(value)
    }

    fn unary(&mut self) -> Option<i64> {
        if matches!(self.peek(), Some(ExprToken::Sub)) {
            self.position += 1;
            return self.unary()?.checked_neg();
        }
        self.primary()
    }

    fn primary(&mut self) -> Option<i64> {
        match self.peek() {
            Some(ExprToken::Literal(value)) => {
                self.position += 1;
                Some(value)
            }
            Some(ExprToken::Formal(index)) => {
                self.position += 1;
                self.formals.get(index).copied()
            }
            Some(ExprToken::Open) => {
                self.position += 1;
                self.depth += 1;
                if self.depth > Self::MAX_DEPTH {
                    return None;
                }
                let value = self.sum()?;
                self.depth -= 1;
                if self.peek() == Some(ExprToken::Close) {
                    self.position += 1;
                    Some(value)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Evaluates one declared integer expression against the operator's
/// positional arguments; `None` on any unsupported form or overflow.
fn eval_declared_expression(expression: &str, formals: &[i64]) -> Option<i64> {
    let tokens = tokenize_declared_expression(expression)?;
    let mut parser = DeclaredExpression {
        tokens: &tokens,
        formals,
        position: 0,
        depth: 0,
    };
    let value = parser.sum()?;
    // Trailing tokens mean the declaration wrote something this grammar
    // does not cover; refuse rather than guess.
    if parser.position == tokens.len() {
        Some(value)
    } else {
        None
    }
}

/// A `WorldIr` builtin executed on the reference VM: the adapter answers
/// the VM's constant/apply questions from the declaration's own operator
/// semantics and declared arities.
#[derive(Clone, Debug)]
pub struct WorldIrWorld {
    name: String,
    laws: Vec<String>,
    operators: BTreeMap<SymbolId, OperatorSemantics>,
    arities: BTreeMap<SymbolId, usize>,
}

impl WorldIrWorld {
    /// Adapts one builtin world declaration.
    #[must_use]
    pub fn new(world: &WorldIr) -> Self {
        let mut operators = BTreeMap::new();
        let mut arities = BTreeMap::new();
        for operator in &world.operators {
            operators.insert(operator.symbol.clone(), operator.semantics.clone());
        }
        for (symbol, arity) in world.signature.iter() {
            arities.insert(symbol.clone(), *arity);
        }
        Self {
            name: world.name.clone(),
            laws: world.laws.clone(),
            operators,
            arities,
        }
    }

    /// Splits one finite-table cell (`"0,1→1"`) into its canonical keys
    /// and result; a malformed cell refuses.
    fn split_cell(cell: &str) -> Option<(Vec<String>, &str)> {
        let arrow = cell.find('→')?;
        let keys = cell[..arrow]
            .split(',')
            .map(str::trim)
            .map(str::to_string)
            .collect();
        Some((keys, cell[arrow + '→'.len_utf8()..].trim()))
    }

    /// The structural rebuild of an application: evaluated arguments
    /// stay as canonical carrier constants; the operator itself is kept
    /// unreduced (partial evaluation, disclosed in the answer).
    fn structural(&self, operator: &SymbolId, arguments: &[WorldIrValue]) -> WorldIrValue {
        WorldIrValue::Structural(Term::Apply {
            operator: operator.clone(),
            arguments: arguments
                .iter()
                .cloned()
                .map(WorldIrValue::into_term)
                .collect(),
        })
    }

    /// The declared arity of a symbol, from the world's own signature.
    fn declared_arity(&self, symbol: &SymbolId) -> Option<usize> {
        self.arities.get(symbol).copied()
    }
}

impl FirstOrderWorld for WorldIrWorld {
    type Value = WorldIrValue;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        let Some(semantics) = self.operators.get(symbol) else {
            return Ok(WorldIrValue::Structural(Term::Constant(symbol.clone())));
        };
        match semantics {
            OperatorSemantics::DeclaredExpression(expression) => {
                Ok(parse_carrier_element(expression)
                    .unwrap_or_else(|| WorldIrValue::Structural(Term::Constant(symbol.clone()))))
            }
            OperatorSemantics::FiniteTable(_) => {
                // A nullary table cell (empty key) would be a constant;
                // the builtin declarations declare constants as
                // expressions, so an uninterpreted nullary stays
                // structural.
                Ok(WorldIrValue::Structural(Term::Constant(symbol.clone())))
            }
            // Structural constructors and non-executable declarations
            // keep the symbol itself.
            _ => Ok(self.structural(symbol, &[])),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let Some(expected) = self.declared_arity(operator) else {
            return Ok(self.structural(operator, &arguments));
        };
        if expected != arguments.len() {
            return Ok(self.structural(operator, &arguments));
        }
        let Some(semantics) = self.operators.get(operator) else {
            return Ok(self.structural(operator, &arguments));
        };
        match semantics {
            OperatorSemantics::StructuralConstructor => Ok(self.structural(operator, &arguments)),
            OperatorSemantics::FiniteTable(cells) => {
                let mut keys = Vec::with_capacity(arguments.len());
                for argument in &arguments {
                    match argument.cell_key() {
                        Some(key) => keys.push(key),
                        None => return Ok(self.structural(operator, &arguments)),
                    }
                }
                for cell in cells {
                    if let Some((declared, value)) = Self::split_cell(cell) {
                        if declared == keys {
                            if let Some(parsed) = parse_carrier_element(value) {
                                return Ok(parsed);
                            }
                        }
                    }
                }
                // A value outside the declared carrier (e.g. an integer
                // environment element against a finite table) has no
                // row: the structural residue is the honest answer.
                Ok(self.structural(operator, &arguments))
            }
            OperatorSemantics::DeclaredExpression(expression) => {
                let mut values = Vec::with_capacity(arguments.len());
                for argument in &arguments {
                    match argument {
                        WorldIrValue::Int(value) => values.push(*value),
                        _ => return Ok(self.structural(operator, &arguments)),
                    }
                }
                Ok(match eval_declared_expression(expression, &values) {
                    Some(result) => WorldIrValue::Int(result),
                    None => self.structural(operator, &arguments),
                })
            }
            // Provider bindings, synthesized programs, and meaning
            // holes are declarations without executable meaning here.
            _ => Ok(self.structural(operator, &arguments)),
        }
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence {
            world: self.name.clone(),
            origin: "declared".to_string(),
            laws: self.laws.clone(),
        }
    }
}
