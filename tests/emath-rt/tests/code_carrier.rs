//! The artifact Code carrier: substitution binds by name (never at a
//! fixed position), an absent reference is a no-op, and evaluate is
//! the guarded executor (open code refuses `unbound_code` naming the
//! remaining constants; closed code yields the specialized closure).
//! The carrier is generic - these laws are pinned over the Rat
//! instantiation `Code<ExactRatio>`; the Int and Bool instantiations
//! are the same generic code, exercised end-to-end by the
//! program-space fixture and export acceptance.

use std::rc::Rc;

use emath_rt::code::{
    code_add, code_as_bool, code_cmp, code_div, code_eq, code_mul, code_neg, code_not, code_sub,
    evaluate, evaluate_expr, free_names, free_names_expr, open, open_expr, project_bool,
    project_i64, project_ratio, substitute, substitute_expr, Code, CodeValue,
};
use emath_rt::code_tree::{CodeTree, ModuleTable, TreeBinary};
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

/// The expression-template union: the dynamic kernels implement the
/// constructor VM's EXACT carrier rules (normative source
/// `constructor_layer/ops.rs`). Every case below is a discriminating
/// pin - a kernel that hardcodes one carrier, collapses too eagerly,
/// or truncates instead of refusing fails exactly one named demand.
#[test]
fn probe_union_kernels() {
    boot();
    let mut p = Probe::new("code union: the VM's exact carrier rules over Int/Rat/Bool");

    // 1. Int x Int keeps Int for + - * (`2 + 3` is Int 5, never Rat
    //    5/1); any Rational operand locks the Rat carrier forever
    //    (`2/1 + 3/1` is Rat 5/1, and `1/4 - 1/4` is Rat 0/1, never
    //    Int 0).
    p.case("carrier-locking", |p| {
        let sum = code_add(&CodeValue::Int(2), &CodeValue::Int(3));
        p.eq("int-plus-int-stays-int", sum, Ok(CodeValue::Int(5)));
        let rat_sum = code_add(&CodeValue::Rat((2, 1)), &CodeValue::Rat((3, 1)));
        p.eq("rat-plus-rat-stays-rat", rat_sum, Ok(CodeValue::Rat((5, 1))));
        let widened = code_add(&CodeValue::Int(2), &CodeValue::Rat((3, 1)));
        p.eq("int-plus-rat-locks-rat", widened, Ok(CodeValue::Rat((5, 1))));
        let collapse_guard = code_sub(&CodeValue::Rat((1, 4)), &CodeValue::Rat((1, 4)));
        p.eq(
            "rational-zero-never-collapses",
            collapse_guard,
            Ok(CodeValue::Rat((0, 1))),
        );
        let product = code_mul(&CodeValue::Int(4), &CodeValue::Int(2));
        p.eq("int-times-int-stays-int", product, Ok(CodeValue::Int(8)));
    });

    // 2. Division NEVER collapses to Int (`4 / 2` is Rat 2/1), and a
    //    zero denominator refuses `division_by_zero`.
    p.case("division-laws", |p| {
        let quotient = code_div(&CodeValue::Int(4), &CodeValue::Int(2));
        p.eq("division-stays-rational", quotient, Ok(CodeValue::Rat((2, 1))));
        let by_zero = code_div(&CodeValue::Int(4), &CodeValue::Int(0));
        match by_zero {
            Ok(_) => {
                p.fail("zero-denominator", "division by zero must refuse");
            }
            Err(fault) => {
                p.demand(
                    "named-refusal",
                    fault.contains("division_by_zero"),
                    &format!("refusal must be named: {fault}"),
                );
            }
        }
    });

    // 3. Equality compares VALUES (`2 == 2/1` is true), Bool equality
    //    is structural, mixed kinds are never equal; ordering
    //    cross-multiplies exactly.
    p.case("value-equality-and-ordering", |p| {
        p.eq(
            "int-equals-rat-by-value",
            code_eq(&CodeValue::Int(2), &CodeValue::Rat((2, 1))),
            Ok(true),
        );
        p.eq(
            "mixed-kinds-never-equal",
            code_eq(&CodeValue::Bool(true), &CodeValue::Int(1)),
            Ok(false),
        );
        p.eq(
            "ordering-cross-multiplies",
            code_cmp(&CodeValue::Rat((3, 2)), &CodeValue::Int(1)),
            Ok(core::cmp::Ordering::Greater),
        );
    });

    // 4. Boolean combinators admit Bool only, with the VM's message.
    p.case("boolean-admission", |p| {
        p.eq("bool-admits", code_as_bool(&CodeValue::Bool(true)), Ok(true));
        match code_as_bool(&CodeValue::Int(5)) {
            Ok(_) => {
                p.fail("int-refused", "a non-Bool operand must refuse");
            }
            Err(fault) => {
                p.demand(
                    "vm-message-shape",
                    fault.contains("boolean combinator expects Bool"),
                    &format!("refusal must match the VM's shape: {fault}"),
                );
            }
        }
        p.eq("not-bools-only", code_not(&CodeValue::Bool(false)), Ok(true));
    });

    // 5. Negation is checked on both numeric carriers and refuses Bool.
    p.case("negation", |p| {
        p.eq("neg-int", code_neg(&CodeValue::Int(5)), Ok(CodeValue::Int(-5)));
        p.eq(
            "neg-rat",
            code_neg(&CodeValue::Rat((1, 2))),
            Ok(CodeValue::Rat((-1, 2))),
        );
        p.demand(
            "neg-refuses-bool",
            code_neg(&CodeValue::Bool(true)).is_err(),
            "negating a Bool must refuse",
        );
    });

    // 6. Checked projections mirror the engine's `type_admits`: Rat
    //    widens Int exactly (`5` becomes `5/1`), Int refuses a
    //    Rational by name (never truncates), Bool admits Bool only.
    p.case("typed-boundary-projections", |p| {
        p.eq(
            "rat-widens-int-exactly",
            project_ratio(&CodeValue::Int(5), "result"),
            Ok((5, 1)),
        );
        match project_i64(&CodeValue::Rat((5, 1)), "result") {
            Ok(_) => {
                p.fail("int-refuses-rational", "a Rational must refuse an Int output");
            }
            Err(fault) => {
                p.demand(
                    "engine-message-shape",
                    fault.contains("output `result` does not have the declared type, found 5/1"),
                    &format!("refusal must match the engine's shape: {fault}"),
                );
            }
        }
        p.demand(
            "bool-refuses-numeric",
            project_bool(&CodeValue::Int(1), "result").is_err(),
            "a numeric must refuse a Bool output",
        );
    });

    // 7. The expression carrier obeys the same laws as the function
    //    carrier: by-name binding, absent no-op, guarded evaluate.
    //    The dual representation (bead emath-shared-tree-view-make-bp8nu)
    //    carries the distilled tree beside the factory; this case
    //    exercises the FACTORY lane (a static template literal's
    //    shape), with the tree lane's laws pinned in code_tree.rs.
    let expression = || {
        open_expr(
            vec!["x".to_string(), "c".to_string()],
            CodeTree::Binary {
                op: TreeBinary::Mul,
                left: Box::new(CodeTree::Path(vec!["x".to_string()])),
                right: Box::new(CodeTree::Path(vec!["c".to_string()])),
            },
            Some(Rc::new(|values: &[CodeValue]| code_mul(&values[0], &values[1]))),
            std::collections::BTreeMap::new(),
        )
    };
    p.case("expression-carrier-laws", |p| {
        // Substitute the LATER name first: by-name, not position.
        let bound_c = substitute_expr(&expression(), "c", CodeValue::Int(3));
        let closed = substitute_expr(&bound_c, "x", CodeValue::Int(5));
        p.eq(
            "closed-computes",
            evaluate_expr(&closed, &ModuleTable::EMPTY),
            Ok(CodeValue::Int(15)),
        );
        // An absent reference is a no-op.
        let untouched = substitute_expr(&expression(), "zz", CodeValue::Int(1));
        p.demand(
            "absent-no-op",
            free_names_expr(&untouched) == ["x".to_string(), "c".to_string()].as_slice(),
            "substituting an absent name must not open or close anything",
        );
        // Open code refuses `unbound_code` naming the remaining names.
        match evaluate_expr(&bound_c, &ModuleTable::EMPTY) {
            Ok(_) => {
                p.fail("open-refuses", "a one-name-open expression must refuse");
            }
            Err(fault) => {
                p.demand(
                    "names-remaining",
                    fault.ends_with("unbound name(s): x"),
                    &format!("refusal must name only the remaining name: {fault}"),
                );
            }
        }
    });

    p.finish();
}
