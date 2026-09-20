//! The artifact Code carrier: substitution binds by name (never at a
//! fixed position), an absent reference is a no-op, and evaluate is
//! the guarded executor (open code refuses `unbound_code` naming the
//! remaining constants; closed code yields the specialized closure).
//! The carrier is generic - these laws are pinned over the Rat
//! instantiation `Code<ExactRatio>`; the Int and Bool instantiations
//! are the same generic code, exercised end-to-end by the
//! program-space fixture and export acceptance.

use std::rc::Rc;

use emath_rt::code::{evaluate, open, substitute, free_names, Code};
use emath_rt::{ratio_add, ratio_mul, ExactRatio};
use emath_test_harness::{Probe, boot};

/// The two-constant template `a * x + b` as a compiled factory: the
/// free slice is (a, b) in declaration order.
fn template() -> Code<ExactRatio> {
    open(
        vec!["a".to_string(), "b".to_string()],
        Rc::new(|values: &[ExactRatio]| {
            let a = values[0];
            let b = values[1];
            Ok(Rc::new(move |x: ExactRatio| {
                ratio_add(ratio_mul(a, x)?, b)
            }))
        }),
    )
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("code carrier: name-bound substitution, guarded evaluation");

    // 1. Binding is by name: substituting the LATER constant first
    //    still splices each value at its own slot. A position-blind
    //    splice would swap a and b (5*3+2 = 17, not 11).
    p.case("substitute-binds-by-name", |p| {
        let bound_b = substitute(&template(), "b", (5, 1));
        let closed = substitute(&bound_b, "a", (2, 1));
        match evaluate(&closed).and_then(|f| f((3, 1))) {
            Ok(value) => {
                p.eq("two-constants-in-reverse-order", value, (11, 1));
            }
            Err(fault) => {
                p.fail(
                    "two-constants-in-reverse-order",
                    &format!("closed code must compute: {fault}"),
                );
            }
        }
    });

    // 2. An absent reference is a no-op (tree-substitution parity):
    //    the code is unchanged, so it stays open under its own names.
    p.case("absent-reference-is-no-op", |p| {
        let untouched = substitute(&template(), "zz", (5, 1));
        p.demand(
            "free-names-unchanged",
            free_names(&untouched) == ["a".to_string(), "b".to_string()].as_slice(),
            "substituting an absent name must not open or close anything",
        );
        p.demand(
            "still-refuses",
            evaluate(&untouched).is_err(),
            "an untouched two-constant template is still open",
        );
    });

    // 3. The guarded executor: open code refuses `unbound_code`
    //    naming the remaining constants in binding order; each
    //    substitution narrows the refusal to what is still open.
    p.case("evaluate-refuses-open-code", |p| {
        match evaluate(&template()) {
            Ok(_) => {
                p.fail("both-open", "a two-constant template must refuse");
            }
            Err(fault) => {
                p.demand(
                    "names-both",
                    fault.contains("unbound_code") && fault.contains("a, b"),
                    &format!("refusal must name both constants: {fault}"),
                );
            }
        }
        let bound_a = substitute(&template(), "a", (2, 1));
        match evaluate(&bound_a) {
            Ok(_) => {
                p.fail("one-open", "a one-constant template must refuse");
            }
            Err(fault) => {
                p.demand(
                    "names-remaining",
                    fault.contains("unbound_code") && fault.contains("b") && !fault.contains("a,"),
                    &format!("refusal must name only the remaining constant: {fault}"),
                );
            }
        }
    });

    p.finish();
}
