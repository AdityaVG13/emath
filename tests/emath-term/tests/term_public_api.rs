//! Public-API integration tests for the emath-term crate.

use emath_term::{CanonicalError, Signature, SymbolId, Term, TermError, VariableId};
use emath_test_harness::Probe;

fn sample_term() -> Term {
    Term::Apply {
        operator: SymbolId("mul".to_string()),
        arguments: vec![
            Term::Constant(SymbolId("a".to_string())),
            Term::Apply {
                operator: SymbolId("add".to_string()),
                arguments: vec![Term::Variable(VariableId("x".to_string())), Term::Constant(SymbolId("b".to_string()))],
            },
        ],
    }
}

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn constant(name: &str) -> Term {
    Term::Constant(SymbolId(name.to_string()))
}

fn apply(op: &str, arguments: Vec<Term>) -> Term {
    Term::Apply { operator: SymbolId(op.to_string()), arguments }
}

#[test]
fn term_public_api() {
    let mut p = Probe::new("Term canonical round-trips and typed refusals hold on public API");
    p.case("round-trip", |p| {
        for (name, term) in [
            ("sample", sample_term()),
            ("unicode-const", constant("ζ")),
            ("empty-apply", apply("ζ", vec![])),
            ("paren-var", var("(")),
            ("comma-var", var("a,b")),
            ("escape-var", var("\\n")),
            ("op-name", apply("f(g,h)", vec![constant("ζ")])),
            ("nested-unicode", apply("⊛", vec![apply("⧖", vec![apply("⋈", vec![var("a"), var("b")])]), constant("ζ")])),
            ("shadow", apply("apply", vec![apply("f", vec![]), apply("const", vec![constant("a")])])),
            ("honest-nested", apply("f", vec![constant("ζ")])),
            ("escaped-op", apply("const(ζ", vec![])),
        ] {
            let canonical = term.canonical();
            match Term::parse_canonical(&canonical) {
                Ok(parsed) => {
                    p.eq(format!("{name}/parse"), parsed.clone(), term.clone());
                    p.eq(format!("{name}/stable"), parsed.canonical(), canonical.clone());
                    p.ne(format!("{name}/nonempty"), canonical.clone(), String::new());
                }
                Err(e) => { p.fail(format!("{name}/parse"), format!("must re-parse {canonical:?}: {e:?}")); },
            }
        }
        let padded = format!("{}  \n\t ", sample_term().canonical());
        match Term::parse_canonical(&padded) {
            Ok(parsed) => {
                p.eq("padded/parse", parsed.clone(), sample_term());
                p.eq("padded/stable", parsed.canonical(), sample_term().canonical());
            }
            Err(e) => { p.fail("padded/parse", format!("trailing whitespace tolerated: {e:?}")); },
        }
    });
    p.case("refuse", |p| {
        for bad in ["apply(const(ζ)", "apply(var(x)", "var(\\n)", "const(\\ζ)", "var(a,b)", "apply("] {
            p.demand(format!("malformed/{bad}"), matches!(Term::parse_canonical(bad), Err(CanonicalError::Malformed { .. })), "must refuse Malformed");
        }
        p.demand("trailing", matches!(Term::parse_canonical("const(a) trailing"), Err(CanonicalError::Trailing { .. })), "trailing refuses");
    });
    p.case("surface", |p| {
        let expected = apply("add", vec![var("lhs"), var("rhs")]);
        for (name, source) in [
            ("infix", "lhs + rhs"),
            ("call", "add(lhs, rhs)"),
            ("canonical", "apply(add,var(lhs),var(rhs))"),
            ("spaced-canonical", "apply(add, lhs, rhs)"),
        ] {
            match Term::parse_body(source) {
                Ok(term) => p.eq(name, term, expected.clone()),
                Err(error) => p.fail(name, format!("{source}: {error:?}")),
            }
        }
        match Term::parse_body("if not is_finite(x) then refuse(text:E-POLY-002) else acc") {
            Ok(term) => p.eq(
                "if-then",
                term.canonical(),
                "apply(if,apply(not,apply(is_finite,var(x))),apply(refuse,const(text:E-POLY-002)),var(acc))".to_string(),
            ),
            Err(error) => p.fail("if-then", format!("{error:?}")),
        }
        match Term::parse_body("let acc = 0 in coeffs[i] + acc * x") {
            Ok(term) => p.eq(
                "let-index",
                term.canonical(),
                "apply(let,var(acc),const(0),apply(add,apply(index,var(coeffs),var(i)),apply(mul,var(acc),var(x))))".to_string(),
            ),
            Err(error) => p.fail("let-index", format!("{error:?}")),
        }
        p.demand(
            "refuse-trailing",
            matches!(Term::parse_body("lhs + rhs leftover"), Err(CanonicalError::Trailing { .. })),
            "trailing surface refuses",
        );
    });
    p.case("signature", |p| {
        let mut sig = Signature::default();
        p.demand("insert", sig.insert(SymbolId("f".to_string()), 2).is_ok(), "fresh inserts");
        p.eq("arity", sig.arity(&SymbolId("f".to_string())), Some(2));
        p.demand("conflict", matches!(sig.insert(SymbolId("f".to_string()), 3), Err(TermError::ConflictingArity { .. })), "conflict refuses");
        let wrong = Term::Apply { operator: SymbolId("f".to_string()), arguments: vec![Term::Constant(SymbolId("a".to_string()))] };
        p.demand("validate", matches!(sig.validate(&wrong), Err(TermError::ArityMismatch { .. })), "arity mismatch refuses");
    });
    p.finish();
}
