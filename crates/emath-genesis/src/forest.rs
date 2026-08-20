//! G1: bounded parse forest and signature inference for genesis bodies.
//!
//! Moved wholesale from `emath-syntax` (world-side fence, pass 5): the
//! forest enumerates structural parses of a body expression under a
//! deterministic grammar: atoms, parenthesized groups, prefix application
//! (`op(...)`), infix composition, and postfix application for a trailing
//! operator (`a op` at a scope close), with no precedence. All enumeration
//! is bounded by [`ForestLimits`]; budget exhaustion and unparseable input
//! are reported as typed recovery holes, never panics. Every emitted
//! artifact (canonical JSON, FNV-1a64 ids) is byte-identical across runs.
//! Stable diagnostic codes are unchanged (`E-SYN-2xx`; never repurposed).
//!
//! Ranking policy (deterministic, SGK-G1-006): candidates are kept in
//! grammar-production insertion order, deduplicated by (canonical form,
//! end position), and capped by `max_alternatives` with a typed
//! `alternative-budget` hole. The forest never scores or guesses between
//! survivors: one complete parse is the answer, several are refused as
//! ambiguous (`E-SYN-211`), none as unparseable (`E-SYN-210`).
//!
//! The world-side stage consumes the genesis body string produced by the
//! G0 parser in `emath-syntax` (`genesis::parse_genesis` -> `body_text`);
//! it never touches the syntax parse tree. `emath-syntax` re-exports this
//! module at its root (`pub use emath_genesis::forest;`) for the CLI.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use emath_term::{Signature, SymbolId, Term, VariableId};
use emath_world_ir::{fnv1a64, Fixity};

/// Budget for the bounded parse forest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForestLimits {
    /// Maximum number of term nodes constructed during enumeration.
    pub max_nodes: usize,
    /// Maximum number of derivations kept per parse position.
    pub max_alternatives: usize,
    /// Maximum nesting depth (parentheses, application args, infix operands).
    pub max_depth: usize,
}

impl Default for ForestLimits {
    fn default() -> Self {
        Self {
            max_nodes: 8192,
            max_alternatives: 128,
            max_depth: 64,
        }
    }
}

/// Typed forest diagnostic with a stable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForestError {
    /// Stable code (`E-SYN-2xx`).
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

/// Bounded, deterministic parse forest over a genesis body expression.
#[derive(Debug, Clone)]
pub struct ParseForest {
    world_name: String,
    body: String,
    node_count: usize,
    ambiguity_count: usize,
    holes: Vec<(String, String)>,
    canonical_term: Option<Term>,
    hints: BTreeMap<String, Hint>,
    parse_id: u64,
}

/// Signature inferred from a unique structural term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInference {
    /// Inferred first-order signature.
    pub signature: Signature,
    /// Inferred surface fixity per symbol.
    pub fixities: BTreeMap<SymbolId, Fixity>,
    /// Canonical type-variable name per symbol (`T0`, `T1`, ...).
    pub type_variables: BTreeMap<SymbolId, String>,
    /// Free variables in first-appearance (preorder) order.
    pub variables: Vec<VariableId>,
    world_name: String,
    signature_id: u64,
}

/// Operator-position usage hints collected during enumeration.
#[derive(Debug, Clone, Copy, Default)]
struct Hint {
    prefix: bool,
    infix: bool,
    postfix: bool,
}

/// Mutable enumeration state, bounded by [`ForestLimits`].
struct ForestState<'a> {
    limits: &'a ForestLimits,
    nodes_used: usize,
    holes: Vec<(String, String)>,
    recorded: BTreeSet<&'static str>,
    hints: BTreeMap<String, Hint>,
}

impl ForestState<'_> {
    fn new(limits: &ForestLimits) -> ForestState<'_> {
        ForestState {
            limits,
            nodes_used: 0,
            holes: Vec::new(),
            recorded: BTreeSet::new(),
            hints: BTreeMap::new(),
        }
    }

    fn record_hole(&mut self, reason: &str) {
        let id = format!("H-{:04}", self.holes.len() + 1);
        self.holes.push((id, reason.to_string()));
    }

    fn record_once(&mut self, reason: &'static str) {
        if self.recorded.insert(reason) {
            self.record_hole(reason);
        }
    }

    fn claim(&mut self, count: usize) -> bool {
        if self.nodes_used + count > self.limits.max_nodes {
            self.record_once("node-budget");
            return false;
        }
        self.nodes_used += count;
        true
    }

    fn hint_postfix(&mut self, symbol: &str) {
        self.hints.entry(symbol.to_string()).or_default().postfix = true;
    }

    fn hint(&mut self, symbol: &str, prefix: bool, infix: bool) {
        let hint = self.hints.entry(symbol.to_string()).or_default();
        hint.prefix |= prefix;
        hint.infix |= infix;
    }

    /// Inserts a derivation, deduplicated by (end, canonical term), capped by
    /// `max_alternatives`.
    fn insert_candidate(&mut self, results: &mut Vec<(Term, usize)>, term: Term, end: usize) {
        if results.len() >= self.limits.max_alternatives {
            self.record_once("alternative-budget");
            return;
        }
        let canonical = term.canonical();
        if results.iter().any(|(existing, existing_end)| {
            *existing_end == end && existing.canonical() == canonical
        }) {
            return;
        }
        results.push((term, end));
    }

    /// All derivations (term, end position) from `pos`.
    fn parse_exprs(&mut self, tokens: &[String], pos: usize, depth: usize) -> Vec<(Term, usize)> {
        if depth > self.limits.max_depth {
            self.record_once("depth-budget");
            return Vec::new();
        }
        let mut results: Vec<(Term, usize)> = Vec::new();
        if let Some(token) = tokens.get(pos) {
            match token.as_str() {
                "(" => {
                    let inner = self.parse_exprs(tokens, pos + 1, depth + 1);
                    for (term, end) in inner {
                        if tokens.get(end).is_some_and(|t| t.as_str() == ")") {
                            self.insert_candidate(&mut results, term, end + 1);
                        }
                    }
                }
                ")" | "," => {}
                ident => {
                    // Atom: free variable or constant/symbol.
                    let atom = if is_free_variable(ident) {
                        Term::Variable(VariableId(ident.to_string()))
                    } else {
                        Term::Constant(SymbolId(ident.to_string()))
                    };
                    if self.claim(1) {
                        self.insert_candidate(&mut results, atom, pos + 1);
                    }
                    // Prefix/function-style application: `op(...)`.
                    if tokens.get(pos + 1).is_some_and(|t| t.as_str() == "(") {
                        let applications = self.parse_applications(tokens, pos + 1, depth + 1);
                        for (arguments, end) in applications {
                            if self.claim(1 + arguments.len()) {
                                self.hint(ident, true, false);
                                let term = Term::Apply {
                                    operator: SymbolId(ident.to_string()),
                                    arguments,
                                };
                                self.insert_candidate(&mut results, term, end);
                            }
                        }
                    }
                }
            }
        }
        // Infix composition: extend every derivation whose end position holds
        // an operator with all derivations of its right operand. When the
        // operator is trailing (its right side closes the scope), the only
        // hypothesis is postfix application `op(left)`. The postfix guard is
        // syntactic (scope close), never budget-dependent, so exhaustion can
        // not invent a postfix parse.
        let mut index = 0;
        while index < results.len() {
            let (left, end) = results[index].clone();
            if let Some(operator) = tokens
                .get(end)
                .map(String::as_str)
                .filter(|token| is_operator_token(token))
            {
                let scope_closes = matches!(
                    tokens.get(end + 1).map(String::as_str),
                    None | Some(")" | ",")
                );
                if scope_closes {
                    if self.claim(1) {
                        self.hint_postfix(operator);
                        let term = Term::Apply {
                            operator: SymbolId(operator.to_string()),
                            arguments: vec![left.clone()],
                        };
                        self.insert_candidate(&mut results, term, end + 1);
                    }
                } else {
                    let rights = self.parse_exprs(tokens, end + 1, depth + 1);
                    for (right, right_end) in rights {
                        if self.claim(1) {
                            self.hint(operator, false, true);
                            let term = Term::Apply {
                                operator: SymbolId(operator.to_string()),
                                arguments: vec![left.clone(), right],
                            };
                            self.insert_candidate(&mut results, term, right_end);
                        }
                    }
                }
            }
            index += 1;
        }
        results
    }

    /// Derivations of a parenthesized argument list (expr, comma-separated).
    /// Returns `(arguments, end_after_close_paren)`.
    fn parse_applications(
        &mut self,
        tokens: &[String],
        open_paren: usize,
        depth: usize,
    ) -> Vec<(Vec<Term>, usize)> {
        let mut results: Vec<(Vec<Term>, usize)> = Vec::new();
        let mut queue: Vec<(Vec<Term>, usize)> = Vec::new();
        for (term, end) in self.parse_exprs(tokens, open_paren + 1, depth) {
            queue.push((vec![term], end));
        }
        let mut index = 0;
        while index < queue.len() {
            let (arguments, end) = queue[index].clone();
            match tokens.get(end).map(String::as_str) {
                Some(")") => {
                    if results.len() < self.limits.max_alternatives {
                        results.push((arguments, end + 1));
                    } else {
                        self.record_once("alternative-budget");
                    }
                }
                Some(",") => {
                    for (term, next_end) in self.parse_exprs(tokens, end + 1, depth) {
                        let mut extended = arguments.clone();
                        extended.push(term);
                        if queue.len() < self.limits.max_alternatives && self.claim(extended.len())
                        {
                            queue.push((extended, next_end));
                        } else {
                            self.record_once("alternative-budget");
                        }
                    }
                }
                _ => {}
            }
            index += 1;
        }
        // Zero-argument application: `op()`.
        if tokens
            .get(open_paren + 1)
            .is_some_and(|t| t.as_str() == ")")
        {
            results.push((Vec::new(), open_paren + 2));
        }
        results
    }
}

/// Builds a parse forest over a body expression with an anonymous world name.
#[must_use]
pub fn build_forest(body: &str, limits: &ForestLimits) -> ParseForest {
    build_forest_named(body, "", limits)
}

/// Builds a parse forest over a body expression for the named world.
#[must_use]
pub fn build_forest_named(body: &str, world_name: &str, limits: &ForestLimits) -> ParseForest {
    let tokens = tokenize(body);
    let mut state = ForestState::new(limits);
    let derivations = state.parse_exprs(&tokens, 0, 0);
    let mut complete: Vec<(Term, usize)> = Vec::new();
    for (term, end) in derivations {
        if end == tokens.len() {
            let canonical = term.canonical();
            if !complete
                .iter()
                .any(|(existing, _)| existing.canonical() == canonical)
            {
                complete.push((term, end));
            }
        }
    }
    let ambiguity_count = complete.len();
    let canonical_term = match complete.len() {
        1 => complete.first().map(|(term, _)| term.clone()),
        _ => None,
    };
    if complete.is_empty() {
        state.record_hole("unparseable-body");
    }
    let mut forest = ParseForest {
        world_name: world_name.to_string(),
        body: body.to_string(),
        node_count: state.nodes_used,
        ambiguity_count,
        holes: state.holes,
        canonical_term,
        hints: state.hints,
        parse_id: 0,
    };
    forest.parse_id = fnv1a64(forest.json_without_id().as_bytes());
    forest
}

/// Infers a signature from the unique structural term of a body expression.
pub fn infer_signature(
    body: &str,
    limits: &ForestLimits,
) -> Result<SignatureInference, Vec<ForestError>> {
    infer_signature_named(body, "", limits)
}

/// Infers a signature for the named world from its unique structural term.
pub fn infer_signature_named(
    body: &str,
    world_name: &str,
    limits: &ForestLimits,
) -> Result<SignatureInference, Vec<ForestError>> {
    let forest = build_forest_named(body, world_name, limits);
    let term = forest.unique_term().map_err(|error| vec![error])?;
    let mut arities: BTreeMap<SymbolId, usize> = BTreeMap::new();
    let mut variables: Vec<VariableId> = Vec::new();
    collect_signature(&term, &mut arities, &mut variables);

    let mut signature = Signature::default();
    for (symbol, arity) in &arities {
        signature
            .insert(symbol.clone(), *arity)
            .expect("each symbol is inserted exactly once");
    }
    // Deterministic fixity-hypothesis priority: infix > prefix > postfix >
    // constant. A symbol seen in several positions resolves to the highest
    // hypothesis so re-runs can never disagree.
    let mut fixities = BTreeMap::new();
    for symbol in arities.keys() {
        let fixity = match forest.hints.get(&symbol.0) {
            Some(hint) if hint.infix => Fixity::Infix,
            Some(hint) if hint.prefix => Fixity::Prefix,
            Some(hint) if hint.postfix => Fixity::Postfix,
            _ => Fixity::Constant,
        };
        fixities.insert(symbol.clone(), fixity);
    }
    let mut type_variables = BTreeMap::new();
    for (index, symbol) in arities.keys().enumerate() {
        type_variables.insert(symbol.clone(), format!("T{index}"));
    }

    let mut inference = SignatureInference {
        signature,
        fixities,
        type_variables,
        variables,
        world_name: world_name.to_string(),
        signature_id: 0,
    };
    inference.signature_id = fnv1a64(inference.json_without_id().as_bytes());
    Ok(inference)
}

/// Collects symbol arities (max over occurrences) and free variables in
/// preorder first-appearance order.
fn collect_signature(
    term: &Term,
    arities: &mut BTreeMap<SymbolId, usize>,
    variables: &mut Vec<VariableId>,
) {
    match term {
        Term::Variable(variable) => {
            if !variables.contains(variable) {
                variables.push(variable.clone());
            }
        }
        Term::Constant(symbol) => {
            arities.entry(symbol.clone()).or_insert(0);
        }
        Term::Apply {
            operator,
            arguments,
        } => {
            arities
                .entry(operator.clone())
                .and_modify(|arity| *arity = (*arity).max(arguments.len()))
                .or_insert(arguments.len());
            for argument in arguments {
                collect_signature(argument, arities, variables);
            }
        }
    }
}

impl ParseForest {
    /// Number of distinct complete structural parses.
    #[must_use]
    pub fn ambiguity_count(&self) -> usize {
        self.ambiguity_count
    }

    /// Number of term nodes constructed during enumeration (bounded).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Returns the unique structural term, or a typed error when there is no
    /// complete parse or several structural parses survive.
    pub fn unique_term(&self) -> Result<Term, ForestError> {
        match &self.canonical_term {
            Some(term) => Ok(term.clone()),
            None => {
                if self.ambiguity_count > 1 {
                    Err(ForestError {
                        code: "E-SYN-211",
                        message: format!(
                            "body is ambiguous: {} structural parses survive",
                            self.ambiguity_count
                        ),
                    })
                } else {
                    Err(ForestError {
                        code: "E-SYN-210",
                        message: format!(
                            "no complete parse of body `{}`; holes: {:?}",
                            self.body, self.holes
                        ),
                    })
                }
            }
        }
    }

    /// Recovery holes as `(hole id, reason)` pairs in emission order.
    #[must_use]
    pub fn holes(&self) -> Vec<(String, String)> {
        self.holes.clone()
    }

    /// Deterministic `parse-forest.json` body (`emath.parse-forest`).
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"emath.parse-forest\",\"world_name\":\"{}\",\"body\":\"{}\",\"parse_id\":{},\"ambiguity_count\":{},\"node_count\":{},\"holes\":[",
            json_escape(&self.world_name),
            json_escape(&self.body),
            self.parse_id,
            self.ambiguity_count,
            self.node_count
        );
        for (index, (id, reason)) in self.holes.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"id\":\"{}\",\"reason\":\"{}\"}}",
                json_escape(id),
                json_escape(reason)
            );
        }
        out.push(']');
        if let Some(term) = &self.canonical_term {
            let _ = write!(
                out,
                ",\"canonical_term\":\"{}\"",
                json_escape(&term.canonical())
            );
        }
        out.push_str(",\"recovery\":\"bounded-holes\"}");
        out
    }

    /// FNV-1a64 identity over the canonical JSON (without the id field).
    #[must_use]
    pub fn parse_id(&self) -> u64 {
        self.parse_id
    }

    fn json_without_id(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"emath.parse-forest\",\"world_name\":\"{}\",\"body\":\"{}\",\"ambiguity_count\":{},\"node_count\":{},\"holes\":[",
            json_escape(&self.world_name),
            json_escape(&self.body),
            self.ambiguity_count,
            self.node_count
        );
        for (index, (id, reason)) in self.holes.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"id\":\"{}\",\"reason\":\"{}\"}}",
                json_escape(id),
                json_escape(reason)
            );
        }
        out.push(']');
        if let Some(term) = &self.canonical_term {
            let _ = write!(
                out,
                ",\"canonical_term\":\"{}\"",
                json_escape(&term.canonical())
            );
        }
        out.push_str(",\"recovery\":\"bounded-holes\"}");
        out
    }
}

impl SignatureInference {
    /// Deterministic `signature.json` body (`emath.signature`).
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"emath.signature\",\"world_name\":\"{}\",\"signature_id\":{},\"arities\":{{",
            json_escape(&self.world_name),
            self.signature_id
        );
        for (index, (symbol, arity)) in self.signature.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{}\":{}", json_escape(&symbol.0), arity);
        }
        out.push_str("},\"fixities\":{");
        for (index, (symbol, fixity)) in self.fixities.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "\"{}\":\"{}\"",
                json_escape(&symbol.0),
                fixity_name(*fixity)
            );
        }
        out.push_str("},\"type_variables\":{");
        for (index, (symbol, variable)) in self.type_variables.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "\"{}\":\"{}\"",
                json_escape(&symbol.0),
                json_escape(variable)
            );
        }
        out.push_str("},\"variables\":[");
        for (index, variable) in self.variables.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{}\"", json_escape(&variable.0));
        }
        out.push_str("]}");
        out
    }

    /// FNV-1a64 identity over the canonical JSON (without the id field).
    #[must_use]
    pub fn signature_id(&self) -> u64 {
        self.signature_id
    }

    fn json_without_id(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"emath.signature\",\"world_name\":\"{}\",\"arities\":{{",
            json_escape(&self.world_name)
        );
        for (index, (symbol, arity)) in self.signature.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{}\":{}", json_escape(&symbol.0), arity);
        }
        out.push_str("},\"fixities\":{");
        for (index, (symbol, fixity)) in self.fixities.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "\"{}\":\"{}\"",
                json_escape(&symbol.0),
                fixity_name(*fixity)
            );
        }
        out.push_str("},\"type_variables\":{");
        for (index, (symbol, variable)) in self.type_variables.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "\"{}\":\"{}\"",
                json_escape(&symbol.0),
                json_escape(variable)
            );
        }
        out.push_str("},\"variables\":[");
        for (index, variable) in self.variables.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{}\"", json_escape(&variable.0));
        }
        out.push_str("]}");
        out
    }
}

/// Splits a body into deterministic tokens; whitespace, `(` `)` `,` delimit.
fn tokenize(body: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in body.chars() {
        if ch.is_whitespace() || ch == '(' || ch == ')' || ch == ',' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if ch == '(' || ch == ')' || ch == ',' {
                tokens.push(ch.to_string());
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Single ASCII lowercase letter: the only free-variable form.
fn is_free_variable(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.next().is_none()
}

/// Any token that may hold operator position (not pure delimiters).
fn is_operator_token(token: &str) -> bool {
    token != "(" && token != ")" && token != ","
}

/// Deterministic JSON string escaping (no whitespace inside values).
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

fn fixity_name(fixity: Fixity) -> &'static str {
    match fixity {
        Fixity::Constant => "constant",
        Fixity::Prefix => "prefix",
        Fixity::Infix => "infix",
        Fixity::Postfix => "postfix",
        Fixity::Function => "function",
    }
}
