//! Emitted-Rust operand parenthesization: atomic registers inline bare
//! and method calls render as chains, not stacked paren groups.
//!
//! Regression fence for over-wrapped emission: register inlining wrapped
//! every substituted token and each method call wrapped itself, so
//! `int_quot(a + b, c)` emitted
//! `(((a).checked_add((b))).ok_or(..))` where canonical Rust is
//! `a.checked_add(b).ok_or(..)`. The pins below demand
//! the canonical chain and refuse any paren-wrapped bare atom. Field
//! projection off a `quote.evaluate` result is pinned to the node-view
//! chain for the same reason: the render shape decides whether the crate
//! compiles.

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

const EVALUATED_FIELD: &str = "emath function probe:
    inputs:
        cert: Code
    outputs:
        result: Bool
    definitions:
        c = quote.evaluate(cert)(0)
        result = c.start < 0
";

const CONFLICT_CYCLE: &str = r#"# Fixture for the cannot-monomorphize guard (bead emath-qhl1i): the
# generic cycle f<S> <-> g<S> is reached at two different instantiations,
# S = Int and S = sequence(Int), so no single concrete entry exists.
# The calling function must refuse by name, and the emitted crate must
# never call an entry that was not rendered.
emath function f<S>:
    inputs:
        x: S
    outputs:
        result: S
    definitions:
        result = g(x)

emath function g<S>:
    inputs:
        x: S
    outputs:
        result: S
    definitions:
        result = f(x)

emath function conflict_caller:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = f(n) + length(f([n, n]))
"#;

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
            "a.checked_add(b).ok_or(\"E-INT-002: exact integer result exceeds the i64 lane\")",
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

    // A field off a `quote.evaluate` result projects through the node view,
    // the same law a `Node` receiver follows. A bare struct access against
    // the `EvaluatedValue` union does not compile, yet emission still
    // reported the crate runnable.
    p.case("evaluated-field-projects-through-node", |p| {
        let mut session =
            emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
        let checked = session.check_owned("evaluated_field.emath", EVALUATED_FIELD);
        let Some(prepared) = checked.prepared.as_ref() else {
            p.fail("check", format!("{:?}", checked.diagnostics));
            return;
        };
        let emission = match emath_rust_backend::constructor_crate::emit_constructor_crate(
            prepared,
            std::path::Path::new("evaluated_field.emath"),
            EVALUATED_FIELD,
        ) {
            Ok(emission) => emission,
            Err(err) => {
                p.fail("emit", format!("{err:?}"));
                return;
            }
        };
        p.demand(
            "runnable",
            emission.runnable,
            format!("unresolved {:?}", emission.unresolved),
        );
        // The crate embeds the authored source (`__EMATH_SOURCE`); only the
        // rendered code region may be searched for `.start`.
        let code = emission.lib.split("static __EMATH_SOURCE").next().unwrap_or_default();
        let body = code.split("pub fn probe").nth(1).unwrap_or(code);
        p.contains("node projection", body, ".to_node()?).field(\"start\")");
        p.demand(
            "no bare field access",
            !body.contains(".start"),
            format!("bare field access off the union enum:\n{body}"),
        );
    });

    // The generic recursion cycle (`replay` ⇄ `replay_step` ⇄
    // `replay_checked` over `S`) in the exhaustion receipt rechecker:
    // every member is a type-parameterized function, so no single-
    // carrier entry exists until the emitter monomorphizes one from
    // the call-site instantiation (`S = sequence(Int)` in the claims
    // fixture). The emitted artifact must (a) render the concrete
    // entry under the plain mangled name its sites already call,
    // (b) declare every instantiated record the entry touches
    // (`Receipt<Vector<Int>>` and friends), and (c) never call an
    // entry that was not rendered — a member with no resolvable
    // instantiation refuses the calling function by name instead.
    p.case("generic-cycle-monomorphizes", |p| {
        let source = include_str!("../../../tests/fixtures/verify/claims.emath");
        let mut session =
            emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
        let checked = session.check_owned("claims.emath", source);
        let Some(prepared) = checked.prepared.as_ref() else {
            p.fail("check", format!("{:?}", checked.diagnostics));
            return;
        };
        let emission = match emath_rust_backend::constructor_crate::emit_constructor_crate(
            prepared,
            std::path::Path::new("claims.emath"),
            source,
        ) {
            Ok(emission) => emission,
            Err(err) => {
                p.fail("emit", format!("{err:?}"));
                return;
            }
        };
        // The crate is not blanket-runnable while a required producer
        // refuses (the bound claim's certificate template, its own
        // bead) - that is the vm-only law, not a defect. The cycle
        // itself must monomorphize: one concrete entry under the
        // plain mangled name its sites already call, the exhaustion
        // checker lowering against it, every instantiated record the
        // pair touches declared, and no call refused for a member the
        // emitter did render.
        let lib = emission.lib.as_str();
        p.contains(
            "monomorphic entry rendered",
            lib,
            "pub fn replay__search_receipts(",
        );
        p.contains("exhaustion checker lowered", lib, "pub fn exhaustion_checker(");
        p.contains(
            "receipt instantiation declared",
            lib,
            "pub struct EmathRecord_Receipt_Vector_ExactInt",
        );
        // The checker's certificate decode crosses the node lane: the
        // scalar union cannot carry the receipt record, so the frame
        // binding re-decodes the quoted tree through evaluate_node and
        // builds the declared record from its fields (the same bridge
        // the declared application lane uses).
        p.contains(
            "certificate decodes through the node lane",
            lib,
            "evaluate_node(&__EMATH_MODULE_TABLE)?).field(\"steps\")",
        );
        p.demand(
            "no missing-entry refusal",
            !lib.contains("no monomorphic entry"),
            "an entry call refused for a member the emitter monomorphized",
        );
        p.contains(
            "bound template refusal stays",
            lib,
            "quote template body must compute its declared `Int` carrier",
        );
    });

    // The cannot-monomorphize guard itself: a generic cycle the emitter
    // CANNOT resolve to one instantiation makes the calling function
    // refuse BY NAME, and the crate never calls an entry it did not
    // render (ruling 1691; the guard at the `CallSibling` arm). The
    // cycle `f<S>` <-> `g<S>` here is reached at S = Int and at
    // S = sequence(Int), so no single concrete entry exists.
    p.case("unresolvable-cycle-refuses-by-name", |p| {
        let mut session =
            emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
        let checked = session.check_owned("conflict_cycle.emath", CONFLICT_CYCLE);
        let Some(prepared) = checked.prepared.as_ref() else {
            p.fail("check", format!("{:?}", checked.diagnostics));
            return;
        };
        let emission = match emath_rust_backend::constructor_crate::emit_constructor_crate(
            prepared,
            std::path::Path::new("conflict_cycle.emath"),
            CONFLICT_CYCLE,
        ) {
            Ok(emission) => emission,
            Err(err) => {
                p.fail("emit", format!("{err:?}"));
                return;
            }
        };
        p.contains(
            "caller refuses by name",
            &emission.unresolved.join("; "),
            "has no monomorphic entry",
        );
        // The crate never calls a missing entry: every `crate::<name>(`
        // call token in the rendered code must name a rendered `pub fn`.
        // The embedded authored source is excluded (prose, not code).
        let code = emission
            .lib
            .split("static __EMATH_SOURCE")
            .next()
            .unwrap_or_default();
        let defined: std::collections::BTreeSet<String> = code
            .match_indices("pub fn ")
            .filter_map(|(at, _)| {
                let rest = &code[at + 7..];
                let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
                Some(rest[..end].to_string())
            })
            .collect();
        let mut missing: Vec<String> = Vec::new();
        let mut rest = code;
        while let Some(at) = rest.find("crate::") {
            let after = &rest[at + 7..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            let name = &after[..end];
            if after[end..].starts_with('(') && !defined.contains(name) {
                missing.push(name.to_string());
            }
            rest = &after[end..];
        }
        p.demand(
            "no call to an unrendered entry",
            missing.is_empty(),
            format!("calls without a `pub fn`: {missing:?}"),
        );
    });

    p.finish();
}
