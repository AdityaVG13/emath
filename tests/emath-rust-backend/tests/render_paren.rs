//! Emitted-Rust operand parenthesization: atomic registers inline bare
//! and method calls render as chains, not stacked paren groups.
//!
//! Regression fence for over-wrapped emission: register inlining wrapped
//! every substituted token and each method call wrapped itself, so
//! `int_quot(a + b, c)` emitted
//! `(((a).checked_add((b))).expect("i64 overflow"))` where canonical Rust
//! is `a.checked_add(b).expect("i64 overflow")`. The pins below demand
//! the canonical chain and refuse any paren-wrapped bare atom.

use emath_rust_backend::emit_constructor_program;
use emath_test_harness::{boot, Probe};

/// Parse, lower, and render one constructor function; the emitted
/// `entry` body text, or the refusal that stopped the pipeline.
fn emitted(name: &str, source: &str) -> Result<String, String> {
    let (tree, parse) = emath_syntax::parse_str(source);
    if parse.has_errors() {
        return Err(format!("{name}: parse refused"));
    }
    let lowered = emath_exec_ir::constructor_emir::lower_constructor_function(&tree, name)?;
    if !lowered.runnable {
        return Err(format!(
            "{name}: not runnable, unresolved {:?}",
            lowered.unresolved
        ));
    }
    emit_constructor_program(&lowered.program, &lowered.inputs).map_err(|err| err.to_string())
}

/// Refuse a stacked paren layer directly around a bare operand: the
/// over-wrap signature is `((x` (call/substitution layer + atom layer).
/// A single paren around an atom is the argument list of a canonical
/// call (`checked_add(b)`), not a defect.
fn no_wrapped_atoms(probe: &mut Probe, body: &str, atoms: &[&str]) {
    for atom in atoms {
        let stacked = format!("({atom}");
        probe.demand(
            format!("no stacked paren around {atom}"),
            !body.contains(&stacked),
            format!("emitted body stacks a paren layer around {atom}"),
        );
    }
}

const QUOT_SUM: &str = "emath function quot_sum:
    inputs:
        a: Int
        b: Int
        c: Int
    outputs:
        result: Int
    definitions:
        result = int_quot(a + b, c)
    tests:
        example <five_over_three>:
            given a = 2
            given b = 3
            given c = 3
            expect result == 1
";

const REM_PLAIN: &str = "emath function rem_plain:
    inputs:
        a: Int
        c: Int
    outputs:
        result: Int
    definitions:
        result = int_rem(a, c)
    tests:
        example <two_over_three>:
            given a = 2
            given c = 3
            expect result == 2
";

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("emitted Rust inlines atomic operands bare and chains method calls");

    p.case("checked-add-chain", |p| {
        let body = match emitted("quot_sum", QUOT_SUM) {
            Ok(body) => body,
            Err(err) => {
                p.fail("render", err);
                return;
            }
        };
        p.contains(
            "canonical checked chain",
            &body,
            "a.checked_add(b).expect(\"i64 overflow\")",
        );
        p.contains(
            "quot operand bare",
            &body,
            ").quot(&emath_rt::ExactInt::from(c))",
        );
        no_wrapped_atoms(p, &body, &["(a)", "(b)", "(c)"]);
    });

    p.case("exact-int-bare", |p| {
        let body = match emitted("rem_plain", REM_PLAIN) {
            Ok(body) => body,
            Err(err) => {
                p.fail("render", err);
                return;
            }
        };
        p.contains(
            "canonical rem chain",
            &body,
            "emath_rt::ExactInt::from(a).rem_euclid(&emath_rt::ExactInt::from(c))",
        );
        no_wrapped_atoms(p, &body, &["(a)", "(c)"]);
    });

    p.finish();
}
