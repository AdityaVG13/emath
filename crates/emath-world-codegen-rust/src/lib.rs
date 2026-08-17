#![forbid(unsafe_code)]

//! Deterministic parametric Rust world artifact generation (Semantic Genesis G3).
//!
//! Emits a self-contained, zero-dependency generated crate evaluating a fixed
//! first-order term under free-symbolic, Boolean, and modular-17 worlds, plus
//! a negative-control world whose `⋈`/`⊛` semantics are swapped.

use emath_term::{Signature, Term};
use std::collections::BTreeMap;
use std::path::Path;

/// A world whose implementation is generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSpec {
    /// Stable label: `free_symbolic`, `boolean_algebra`, or `modular_numeric`.
    pub label: String,
    /// Declared operator semantics (symbol, meaning) actually interpreted
    /// by the genesis analysis for this world (declared expressions from
    /// the builtin world maps; structural constructors of the free world
    /// are carried as an empty map). Codegen emits a fixed per-label
    /// interpretation; a declared meaning codegen does not hardcode
    /// would be silently dropped (SURF-0008), so it is refused with
    /// `E-GEN-094` instead of ignored.
    pub operators: Vec<(String, String)>,
}

/// Typed refusal for codegen inputs outside the label-based subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenRefusal {
    /// Stable diagnostic code (`E-GEN-094`).
    pub code: &'static str,
    /// Human-readable refusal.
    pub message: String,
}

/// The fixed operator semantics the label-based emission hardcodes.
/// These must stay in lockstep with the generated world `apply`
/// implementations inside `LIB_TEMPLATE`.
fn default_operator_semantics(label: &str) -> &'static [(&'static str, &'static str)] {
    match label {
        // `free_symbolic` interprets every operator structurally (no
        // hardcoded meaning), so its declared map must be empty.
        "boolean_algebra" => &[("ζ", "true"), ("⋈", "xor"), ("⧖", "not"), ("⊛", "and")],
        "modular_numeric" => &[
            ("ζ", "3"),
            ("⋈", "(x+y) mod 17"),
            ("⧖", "(x*x) mod 17"),
            ("⊛", "(x*y) mod 17"),
        ],
        _ => &[],
    }
}

/// Deterministic generated package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedPackage {
    /// Crate name.
    pub crate_name: String,
    /// Relative path → file content.
    pub files: BTreeMap<String, String>,
}

impl GeneratedPackage {
    /// Writes every file under `dir`, replacing existing files.
    pub fn write_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for (relative, body) in &self.files {
            let target = dir.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, body)?;
        }
        Ok(())
    }
}

/// Emits the parametric crate for `term` under `signature`.
///
/// Refuses (typed refusal, `E-GEN-094`) any world whose declared operator
/// semantics differ from the fixed per-label interpretation the emitted
/// code hardcodes: a difference would be a silent drop of analyzed
/// `WorldIr` (SURF-0008), so the generator exits nonzero instead of
/// emitting a crate that disagrees with the analysis.
pub fn generate(
    term: &Term,
    signature: &Signature,
    worlds: &[WorldSpec],
) -> Result<GeneratedPackage, CodegenRefusal> {
    for world in worlds {
        let mut expected = default_operator_semantics(&world.label)
            .iter()
            .map(|&(symbol, meaning)| (symbol.to_string(), meaning.to_string()))
            .collect::<Vec<_>>();
        expected.sort();
        let mut declared = world.operators.clone();
        declared.sort();
        if declared != expected {
            let first = declared
                .iter()
                .find(|probe| !expected.contains(probe))
                .map_or_else(
                    || "extra/missing operator".to_string(),
                    |(symbol, meaning)| format!("`{symbol}` = `{meaning}`"),
                );
            return Err(CodegenRefusal {
                code: "E-GEN-094",
                message: format!(
                    "world `{}` declares operator semantics codegen cannot honor ({first}); \
                     label-based emission hardcodes a fixed interpretation (SURF-0008), so the \
                     unused WorldIr is refused instead of silently dropped",
                    world.label
                ),
            });
        }
    }
    let labels = worlds
        .iter()
        .map(|spec| spec.label.clone())
        .collect::<Vec<_>>();
    let mut files = BTreeMap::new();
    files.insert("Cargo.toml".into(), render_manifest());
    files.insert("src/lib.rs".into(), render_lib(term, signature, &labels));
    files.insert("src/main.rs".into(), render_main(&labels));
    Ok(GeneratedPackage {
        crate_name: "semantic-genesis-worlds".into(),
        files,
    })
}

fn render_manifest() -> String {
    r#"# Generated by emath Semantic Genesis (deterministic; do not edit).
[package]
name = "semantic-genesis-worlds"
version = "0.1.0"
edition = "2024"
description = "Generated parametric worlds for the reference alien signature."
license = "Apache-2.0"

[lib]
path = "src/lib.rs"

[[bin]]
name = "semantic-genesis-worlds"
path = "src/main.rs"

[dependencies]
"#
    .to_string()
}

const LIB_TEMPLATE: &str = r#"#![forbid(unsafe_code)]

//! Generated by emath Semantic Genesis (deterministic; do not edit).
//!
//! Source signature: @@ARITIES@@.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;

/// Canonical first-order term (self-contained model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Free variable.
    Variable(String),
    /// Nullary symbol.
    Constant(String),
    /// Operator applied to ordered arguments.
    Apply {
        /// Operator name.
        operator: String,
        /// Argument terms.
        arguments: Vec<Term>,
    },
}

impl Term {
    /// Renders the deterministic structural canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut output = String::new();
        self.write_canonical(&mut output)
            .expect("writing into String cannot fail");
        output
    }

    fn write_canonical(&self, output: &mut String) -> fmt::Result {
        match self {
            Self::Variable(name) => write!(output, "var({})", escape(name)),
            Self::Constant(name) => write!(output, "const({})", escape(name)),
            Self::Apply {
                operator,
                arguments,
            } => {
                write!(output, "apply({}", escape(operator))?;
                for argument in arguments {
                    output.push(',');
                    argument.write_canonical(output)?;
                }
                output.push(')');
                Ok(())
            }
        }
    }

    /// Parses the canonical form produced by [`Term::canonical`].
    pub fn parse_canonical(text: &str) -> Result<Self, String> {
        let mut parser = CanonicalParser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let term = parser.parse_term()?;
        parser.skip_whitespace();
        if parser.pos != parser.bytes.len() {
            return Err(format!("trailing content at {}", parser.pos));
        }
        Ok(term)
    }
}

fn escape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            ',' => result.push_str("\\,"),
            _ => result.push(ch),
        }
    }
    result
}

struct CanonicalParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl CanonicalParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self.bytes[self.pos..].starts_with(expected.as_bytes()) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn malformed(&self) -> String {
        format!("malformed canonical at {}", self.pos)
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
        {
            self.pos += 1;
        }
    }

    fn parse_term(&mut self) -> Result<Term, String> {
        if self.eat("var") {
            if !self.eat("(") {
                return Err(self.malformed());
            }
            let name = self.parse_name(false)?;
            if !self.eat(")") {
                return Err(self.malformed());
            }
            return Ok(Term::Variable(name));
        }
        if self.eat("const") {
            if !self.eat("(") {
                return Err(self.malformed());
            }
            let name = self.parse_name(false)?;
            if !self.eat(")") {
                return Err(self.malformed());
            }
            return Ok(Term::Constant(name));
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
                    operator,
                    arguments,
                });
            }
            if !self.eat(",") {
                return Err(self.malformed());
            }
            arguments.push(self.parse_term()?);
        }
    }

    fn parse_name(&mut self, stop_at_comma: bool) -> Result<String, String> {
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
            let rest =
                std::str::from_utf8(&self.bytes[self.pos..]).map_err(|_| self.malformed())?;
            let Some(ch) = rest.chars().next() else {
                return Err(self.malformed());
            };
            name.push(ch);
            self.pos += ch.len_utf8();
        }
        Err(self.malformed())
    }
}

/// Generic first-order world implementation.
pub trait World {
    /// Runtime value.
    type Value: Clone + fmt::Display;
    /// Evaluation error.
    type Error: fmt::Display + fmt::Debug;

    /// Resolves a nullary symbol.
    fn constant(&self, symbol: &str) -> Result<Self::Value, Self::Error>;

    /// Applies an operator to evaluated arguments.
    fn apply(
        &self,
        operator: &str,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}

/// Evaluation error shared by the generated worlds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Missing free-variable valuation.
    MissingVariable(String),
    /// Unknown symbol.
    UnknownSymbol(String),
    /// Incorrect runtime arity.
    Arity {
        /// Symbol.
        symbol: String,
        /// Expected arity.
        expected: usize,
        /// Actual arity.
        actual: usize,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EvalError {}

/// Environment for free variables.
pub type Environment<V> = BTreeMap<String, V>;

/// Evaluates a term in any world implementing its symbols.
pub fn evaluate<W: World>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    match term {
        Term::Variable(name) => environment
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::MissingVariable(name.clone()).into()),
        Term::Constant(symbol) => world.constant(symbol),
        Term::Apply {
            operator,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| evaluate(argument, world, environment))
                .collect::<Result<Vec<_>, _>>()?;
            world.apply(operator, values)
        }
    }
}

/// The reference term this crate evaluates.
///
/// Canonical: @@REFERENCE_CANONICAL@@
#[must_use]
pub fn reference_term() -> Term {
    Term::parse_canonical(@@REFERENCE_CANONICAL@@)
        .expect("reference canonical parses")
}

/// Free symbolic world: values remain canonical term strings.
#[derive(Debug, Default, Clone, Copy)]
pub struct FreeSymbolicWorld;

impl World for FreeSymbolicWorld {
    type Value = String;
    type Error = EvalError;

    fn constant(&self, symbol: &str) -> Result<Self::Value, Self::Error> {
        Ok(format!("const({})", escape(symbol)))
    }

    fn apply(
        &self,
        operator: &str,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let mut result = String::from("apply(");
        result.push_str(&escape(operator));
        for argument in arguments {
            result.push(',');
            result.push_str(&argument);
        }
        result.push(')');
        Ok(result)
    }
}

/// Boolean interpretation of the reference signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct BooleanWorld;

impl World for BooleanWorld {
    type Value = bool;
    type Error = EvalError;

    fn constant(&self, symbol: &str) -> Result<Self::Value, Self::Error> {
        match symbol {
            "ζ" => Ok(true),
            _ => Err(EvalError::UnknownSymbol(symbol.into())),
        }
    }

    fn apply(
        &self,
        operator: &str,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        match (operator, arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(*left ^ *right),
            ("⧖", [value]) => Ok(!*value),
            ("⊛", [left, right]) => Ok(*left && *right),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.into())),
        }
    }
}

/// Modular-17 interpretation of the reference signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularWorld;

impl World for ModularWorld {
    type Value = i64;
    type Error = EvalError;

    fn constant(&self, symbol: &str) -> Result<Self::Value, Self::Error> {
        match symbol {
            "ζ" => Ok(3),
            _ => Err(EvalError::UnknownSymbol(symbol.into())),
        }
    }

    fn apply(
        &self,
        operator: &str,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let modulo = |value: i64| value.rem_euclid(17);
        match (operator, arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(modulo(*left + *right)),
            ("⧖", [value]) => Ok(modulo(*value * *value)),
            ("⊛", [left, right]) => Ok(modulo(*left * *right)),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.into())),
        }
    }
}

/// Negative control: modular semantics with `⋈` and `⊛` swapped.
/// Differential evaluation must reject this world.
#[derive(Debug, Default, Clone, Copy)]
pub struct SwappedModularWorld;

impl World for SwappedModularWorld {
    type Value = i64;
    type Error = EvalError;

    fn constant(&self, symbol: &str) -> Result<Self::Value, Self::Error> {
        match symbol {
            "ζ" => Ok(3),
            _ => Err(EvalError::UnknownSymbol(symbol.into())),
        }
    }

    fn apply(
        &self,
        operator: &str,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let modulo = |value: i64| value.rem_euclid(17);
        match (operator, arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(modulo(*left * *right)),
            ("⊛", [left, right]) => Ok(modulo(*left + *right)),
            ("⧖", [value]) => Ok(modulo(*value * *value)),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.into(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.into())),
        }
    }
}

/// Free-symbolic fixture: variables remain canonical strings.
#[must_use]
pub fn fixture_free() -> Environment<String> {
    let mut environment = Environment::new();
    environment.insert("a".into(), "var(a)".into());
    environment.insert("b".into(), "var(b)".into());
    environment
}

/// Boolean fixture: a=true, b=false.
#[must_use]
pub fn fixture_boolean() -> Environment<bool> {
    let mut environment = Environment::new();
    environment.insert("a".into(), true);
    environment.insert("b".into(), false);
    environment
}

/// Modular-17 fixture: a=4, b=7.
#[must_use]
pub fn fixture_modular() -> Environment<i64> {
    let mut environment = Environment::new();
    environment.insert("a".into(), 4);
    environment.insert("b".into(), 7);
    environment
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    /// The swap transform is not a no-op mutation. The demo term
    /// `⊛(⧖(⋈(a, b)), ζ)` evaluates to 6 under the modular world and to
    /// 5 under the swapped world (⋈ becomes `*`, ⊛ becomes `+`, ζ = 3,
    /// a = 4, b = 7). A mutant that delegates the swapped world to the
    /// modular world returns 6 here and is killed.
    #[test]
    fn swapped_world_is_not_a_noop_mutation() {
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses");
        let env = fixture_modular();
        let modular = evaluate(&term, &ModularWorld, &env).expect("modular evaluates");
        let swapped = evaluate(&term, &SwappedModularWorld, &env).expect("swapped evaluates");
        assert_eq!(modular, 6, "⋈ adds, ⧖ squares, ⊛ multiplies (mod 17)");
        assert_eq!(
            swapped, 5,
            "⋈ multiplies, ⊛ adds after the swap — no-op mutants return 6"
        );
        assert_ne!(modular, swapped);
    }

    /// Nested-shape kill: the swap must hold on every operator path, not
    /// just the demo shape. `⋈(⧖(a), ⧖(b))` is 14 modular (16 + 15) vs 2
    /// swapped (16 * 15 mod 17).
    #[test]
    fn swap_mutation_is_killed_on_other_operator_paths() {
        let term = Term::parse_canonical("apply(⋈,apply(⧖,var(a)),apply(⧖,var(b)))")
            .expect("canonical parses");
        let env = fixture_modular();
        let modular = evaluate(&term, &ModularWorld, &env).expect("modular evaluates");
        let swapped = evaluate(&term, &SwappedModularWorld, &env).expect("swapped evaluates");
        assert_eq!(modular, 14);
        assert_eq!(swapped, 2);
        assert_ne!(modular, swapped);
    }

    /// Metamorphic determinism: the dual-run comparison is seed-free and
    /// deterministic (the seed contract records `consumes_rng: false`),
    /// so repeated evaluation must agree exactly.
    #[test]
    fn dual_run_is_deterministic() {
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses");
        let env = fixture_modular();
        let first = evaluate(&term, &SwappedModularWorld, &env).expect("evaluates");
        let second = evaluate(&term, &SwappedModularWorld, &env).expect("evaluates");
        assert_eq!(first, second, "dual-run evaluation must be deterministic");
    }
}

"#;

fn render_lib(term: &Term, signature: &Signature, labels: &[String]) -> String {
    let _ = labels;
    let body = LIB_TEMPLATE
        .replace(
            "@@ARITIES@@",
            &signature
                .iter()
                .map(|(symbol, arity)| format!("{}:{}", symbol.0, arity))
                .collect::<Vec<_>>()
                .join(", "),
        )
        .replace("@@REFERENCE_CANONICAL@@", &rust_string(&term.canonical()));
    // Emit exactly one trailing newline so generated crates stay
    // rustfmt-clean (determinism + `cargo fmt --check` both depend on it).
    format!("{}\n", body.trim_end())
}

/// Rust string literal escaping (default settings; Unicode glyphs inline).
fn rust_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

const MAIN_TEMPLATE: &str = r#"#![forbid(unsafe_code)]

// Generated by emath Semantic Genesis (deterministic; do not edit).
@@USES@@
fn main() {
    let term = reference_term();
    let free = evaluate(&term, &FreeSymbolicWorld, &fixture_free()).expect("free evaluates");
    println!("free: {free}");
@@BOOLEAN_BODY@@
@@MODULAR_BODY@@
}
"#;

fn render_main(labels: &[String]) -> String {
    // Statement order is significant: rustfmt (edition 2024) sorts the
    // block of `use` lines, and the committed golden is rustfmt-clean, so
    // emission must follow the same alphabetical order (B < F < M).
    let mut uses = String::new();
    if labels.iter().any(|label| label == "boolean_algebra") {
        uses.push_str("use semantic_genesis_worlds::{BooleanWorld, fixture_boolean};\n");
    }
    uses.push_str(
        "use semantic_genesis_worlds::{FreeSymbolicWorld, evaluate, fixture_free, reference_term};\n",
    );
    if labels.iter().any(|label| label == "modular_numeric") {
        uses.push_str(
            "use semantic_genesis_worlds::{ModularWorld, SwappedModularWorld, fixture_modular};\n",
        );
    }
    let mut boolean_body = String::new();
    if labels.iter().any(|label| label == "boolean_algebra") {
        boolean_body
            .push_str("    let boolean = evaluate(&term, &BooleanWorld, &fixture_boolean()).expect(\"boolean evaluates\");\n    println!(\"boolean: {boolean}\");\n");
    }
    let mut modular_body = String::new();
    if labels.iter().any(|label| label == "modular_numeric") {
        modular_body.push_str(
            "    let modular = evaluate(&term, &ModularWorld, &fixture_modular()).expect(\"modular evaluates\");\n    println!(\"modular-17: {modular}\");\n    let swapped =\n        evaluate(&term, &SwappedModularWorld, &fixture_modular()).expect(\"swapped evaluates\");\n    println!(\"swapped-modular-17: {swapped}\");\n",
        );
    }
    MAIN_TEMPLATE
        .replace("@@USES@@", uses.trim_end())
        .replace("@@BOOLEAN_BODY@@", boolean_body.trim_end())
        .replace("@@MODULAR_BODY@@", modular_body.trim_end())
}

/// Seed contract of the genesis dual-run transform (modular → swapped
/// modular world): the swap is deterministic and consumes no RNG, so the
/// differential comparison is a true metamorphic pin rather than a
/// randomized experiment. The generated `contract_tests` module (inside
/// `LIB_TEMPLATE`) kills any mutant that turns the swap into a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedContract {
    /// Transform family name.
    pub transform: &'static str,
    /// Whether the transform consumes RNG (false: fully deterministic).
    pub consumes_rng: bool,
}

/// Seed contract of the `genesis-world-swap` transform.
pub const SWAP_SEED_CONTRACT: SeedContract = SeedContract {
    transform: "genesis-world-swap",
    consumes_rng: false,
};

#[cfg(test)]
mod seed_contract_tests {
    use super::*;

    /// Template honesty: the emitted runtime contract tests must be part
    /// of the generated lib (a mutant deleting `contract_tests` silently
    /// weakens the differential pin). The seed contract itself
    /// (`consumes_rng: false`) is enforced by the generated runtime
    /// tests and the xtask oracle pin; restating its literal value here
    /// would be a test that passes by construction.
    #[test]
    fn generated_lib_carries_the_contract_tests() {
        assert!(LIB_TEMPLATE.contains("mod contract_tests"));
        assert!(LIB_TEMPLATE.contains("swapped_world_is_not_a_noop_mutation"));
        assert!(LIB_TEMPLATE.contains("dual_run_is_deterministic"));
    }
}

#[cfg(test)]
mod unused_worldir_tests {
    use super::*;

    fn reference_signature() -> Signature {
        let mut signature = Signature::default();
        for (symbol, arity) in [("ζ", 0usize), ("⋈", 2), ("⧖", 1), ("⊛", 2)] {
            signature
                .insert(emath_term::SymbolId(symbol.to_string()), arity)
                .expect("fresh symbol inserts");
        }
        signature
    }

    fn reference_term() -> Term {
        Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses")
    }

    fn modular_spec(operators: &[(&str, &str)]) -> WorldSpec {
        WorldSpec {
            label: "modular_numeric".to_string(),
            operators: operators
                .iter()
                .map(|&(symbol, meaning)| (symbol.to_string(), meaning.to_string()))
                .collect(),
        }
    }

    /// A non-default operator map (SURF-0008: analyzed `WorldIr` codegen
    /// does not consult) must be refused: emitting the crate anyway
    /// would silently disagree with the genesis analysis.
    #[test]
    fn unused_worldir_with_a_non_default_operator_map_is_refused() {
        let refusal = generate(
            &reference_term(),
            &reference_signature(),
            &[modular_spec(&[
                ("ζ", "3"),
                ("⋈", "(x-y) mod 17"),
                ("⧖", "(x*x) mod 17"),
                ("⊛", "(x*y) mod 17"),
            ])],
        )
        .expect_err("non-default ⋈ semantics must be refused");
        assert_eq!(refusal.code, "E-GEN-094");
        assert!(refusal.message.contains("⋈"), "{}", refusal.message);
        assert!(refusal.message.contains("modular_numeric"));
    }

    /// An operator outside the hardcoded per-label set is a silent-drop
    /// candidate and must be refused as well.
    #[test]
    fn unused_worldir_with_an_extra_operator_is_refused() {
        let refusal = generate(
            &reference_term(),
            &reference_signature(),
            &[modular_spec(&[
                ("ζ", "3"),
                ("⋈", "(x+y) mod 17"),
                ("⧖", "(x*x) mod 17"),
                ("⊛", "(x*y) mod 17"),
                ("✳", "(x*y) mod 19"),
            ])],
        )
        .expect_err("extra undeclared operator must be refused");
        assert_eq!(refusal.code, "E-GEN-094");
        assert!(refusal.message.contains("✳"), "{}", refusal.message);
    }

    /// The empty/default operator map keeps today's label-only success
    /// path byte-identical: the refusal only fires on divergence.
    #[test]
    fn unused_worldir_default_operator_maps_still_generate() {
        let package = generate(
            &reference_term(),
            &reference_signature(),
            &[
                WorldSpec {
                    label: "free_symbolic".to_string(),
                    operators: vec![],
                },
                WorldSpec {
                    label: "boolean_algebra".to_string(),
                    operators: vec![
                        ("ζ".to_string(), "true".to_string()),
                        ("⋈".to_string(), "xor".to_string()),
                        ("⧖".to_string(), "not".to_string()),
                        ("⊛".to_string(), "and".to_string()),
                    ],
                },
                modular_spec(&[
                    ("ζ", "3"),
                    ("⋈", "(x+y) mod 17"),
                    ("⧖", "(x*x) mod 17"),
                    ("⊛", "(x*y) mod 17"),
                ]),
            ],
        )
        .expect("default operator maps must keep generating");
        assert!(package.files["src/lib.rs"].contains("reference_term"));
        assert!(package.files["Cargo.toml"].contains("semantic-genesis-worlds"));
    }
}
