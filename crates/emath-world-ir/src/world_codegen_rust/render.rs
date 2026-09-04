//! Specialized library/main rendering and the seed contract.

use super::*;

pub(super) fn render_lib(term: &Term, signature: &Signature, labels: &[String]) -> String {
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
        .replace("@@ABI_VERSION@@", &WORLD_ABI_VERSION.to_string())
        .replace("@@SPECIALIZED@@", render_specialized(signature).trim_end())
        .replace("@@REFERENCE_CANONICAL@@", &rust_string(&term.canonical()));
    // Emit exactly one trailing newline so generated crates stay
    // rustfmt-clean (determinism + `cargo fmt --check` both depend on it).
    format!("{}\n", body.trim_end())
}

/// Renders the declaration-specific ABI: a `SpecializedWorld` trait with one
/// method per declared symbol (named `sym_<index>`, wrong-arity calls are
/// compile errors), delegating from the generic `World` trait.
pub(super) fn render_specialized(signature: &Signature) -> String {
    let entries: Vec<(String, usize)> = signature
        .iter()
        .map(|(symbol, arity)| (symbol.0.clone(), *arity))
        .collect();
    let params = |arity: usize| -> String {
        (0..arity).fold(String::new(), |mut text, position| {
            let _ = write!(text, ", a{position}: Self::Value");
            text
        })
    };
    let mut out = String::new();
    out.push_str(
        "/// Declaration-specific evaluator ABI derived from the source signature:\n\
         /// one method per declared symbol at its exact arity, so a wrong-arity\n\
         /// direct call is a compile error instead of a runtime refusal.\n\
         pub trait SpecializedWorld {\n    \
         /// Runtime value.\n    \
         type Value: Clone + fmt::Display;\n    \
         /// Evaluation error.\n    \
         type Error: fmt::Display + fmt::Debug;\n",
    );
    for (index, (symbol, arity)) in entries.iter().enumerate() {
        let _ = writeln!(
            out,
            "\n    /// `{symbol}` (arity {arity}).\n    \
             fn sym_{index}(&self{}) -> Result<Self::Value, Self::Error>;",
            params(*arity)
        );
    }
    out.push_str("}\n\n");

    out.push_str(
        "/// Every generic world satisfies the specialized ABI by delegation, so\n\
         /// the two surfaces can never disagree on semantics.\n\
         impl<W: World> SpecializedWorld for W {\n    \
         type Value = W::Value;\n    \
         type Error = W::Error;\n",
    );
    for (index, (symbol, arity)) in entries.iter().enumerate() {
        let call = if *arity == 0 {
            format!("self.constant(\"{symbol}\")")
        } else {
            let arguments = (0..*arity)
                .map(|position| format!("a{position}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("self.apply(\"{symbol}\", vec![{arguments}])")
        };
        let _ = writeln!(
            out,
            "\n    fn sym_{index}(&self{}) -> Result<Self::Value, Self::Error> {{\n        \
             {call}\n    }}",
            params(*arity)
        );
    }
    out.push_str("}\n\n");

    out.push_str(
        "/// Evaluates a term through the declaration-specific ABI. Symbols\n\
         /// outside the declared signature and wrong runtime arities are typed\n\
         /// refusals, never fallthroughs.\n\
         pub fn evaluate_specialized<W: SpecializedWorld>(\n    \
         term: &Term,\n    \
         world: &W,\n    \
         environment: &Environment<W::Value>,\n\
         ) -> Result<W::Value, W::Error>\n\
         where\n    \
         W::Error: From<EvalError>,\n\
         {\n    \
         match term {\n        \
         Term::Variable(name) => environment\n            \
         .get(name)\n            \
         .cloned()\n            \
         .ok_or_else(|| EvalError::MissingVariable(name.clone()).into()),\n        \
         Term::Constant(symbol) => dispatch_specialized(world, symbol, Vec::new()),\n        \
         Term::Apply {\n            \
         operator,\n            \
         arguments,\n        \
         } => {\n            \
         let mut values = Vec::with_capacity(arguments.len());\n            \
         for argument in arguments {\n                \
         values.push(evaluate_specialized(argument, world, environment)?);\n            \
         }\n            \
         dispatch_specialized(world, operator, values)\n        \
         }\n    \
         }\n\
         }\n\n",
    );

    out.push_str(
        "/// Maps a declared symbol to its specialized method, checking runtime\n\
         /// arity before the typed call.\n\
         fn dispatch_specialized<W: SpecializedWorld>(\n    \
         world: &W,\n    \
         symbol: &str,\n    \
         mut values: Vec<W::Value>,\n\
         ) -> Result<W::Value, W::Error>\n\
         where\n    \
         W::Error: From<EvalError>,\n\
         {\n    \
         match symbol {\n",
    );
    for (index, (symbol, arity)) in entries.iter().enumerate() {
        let mut arm = format!(
            "        \"{symbol}\" => {{\n            if values.len() != {arity} {{\n                \
             return Err(EvalError::Arity {{\n                    \
             symbol: symbol.into(),\n                    \
             expected: {arity},\n                    \
             actual: values.len(),\n                }}\n                .into());\n            }}\n"
        );
        let mut arguments = Vec::new();
        for position in (0..*arity).rev() {
            let _ = writeln!(
                arm,
                "            let a{position} = values.pop().expect(\"arity checked\");"
            );
            arguments.push(format!("a{position}"));
        }
        arguments.reverse();
        let _ = writeln!(
            arm,
            "            world.sym_{index}({})\n        }}",
            arguments.join(", ")
        );
        out.push_str(&arm);
    }
    out.push_str(
        "        _ => Err(EvalError::UnknownSymbol(symbol.into()).into()),\n    \
         }\n\
         }\n",
    );
    out
}

/// Rust string literal escaping (default settings; Unicode glyphs inline).
pub(super) fn rust_string(text: &str) -> String {
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

pub(super) const MAIN_TEMPLATE: &str = r#"#![forbid(unsafe_code)]

// Generated by emath Semantic Genesis (deterministic; do not edit).
@@USES@@
pub(super) fn main() {
    let term = reference_term();
    let free = evaluate(&term, &FreeSymbolicWorld, &fixture_free()).expect("free evaluates");
    println!("free: {free}");
@@BOOLEAN_BODY@@
@@MODULAR_BODY@@
}
"#;

pub(super) fn render_main(labels: &[String]) -> String {
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

/// Seed contract of the dual-run transform (modular → swapped): the swap is
/// deterministic and RNG-free, so the differential comparison is a true
/// metamorphic pin that kills no-op mutants.
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
