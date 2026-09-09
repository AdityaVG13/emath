//! Rat exact-rational capability cells: exact canonical arithmetic, typed refusal on zero denominator.

use std::collections::BTreeMap;

use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{TestVerdict, eval_definitions_values};
use emath_test_harness::{boot, Probe, Source};

/// Evaluate the first declaration of an admitted source over `bindings`.
fn eval_first(
    p: &mut Probe,
    name: &str,
    source: &str,
    bindings: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        p.fail(
            format!("{name}:eval"),
            "cannot evaluate a source that did not admit",
        );
        return BTreeMap::new();
    }
    match eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &bindings,
        &BTreeMap::new(),
    ) {
        Ok(values) => values,
        Err(fault) => {
            p.fail(format!("{name}:eval"), format!("rat cell must evaluate: {fault:?}"));
            BTreeMap::new()
        }
    }
}

#[test]
fn rat_cells_are_exact_and_refuse_typed() {
    boot();
    let mut p = Probe::new("Rat cells evaluate exact canonical rationals and refuse zero denominators typed");
    p.case("add", |p| {
        let values = eval_first(
            p,
            "add",
            "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_add(rat(1, n), rat(1, 6))\n",
            BTreeMap::from([("n".to_string(), Value::I64(3))]),
        );
        p.eq("halves", values.get("c"), Some(&Value::Rat { num: 1, den: 2 }));
    });
    p.case("norm", |p| {
        let values = eval_first(
            p,
            "norm",
            "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_norm(rat(n, 4))\n",
            BTreeMap::from([("n".to_string(), Value::I64(6))]),
        );
        p.eq("reduced", values.get("c"), Some(&Value::Rat { num: 3, den: 2 }));
    });
    p.case("large-denominator", |p| {
        let values = eval_first(
            p,
            "large-denom",
            "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, n)\n",
            BTreeMap::from([("n".to_string(), Value::I64(1_000_000_000_000_000_007))]),
        );
        p.eq(
            "exact",
            values.get("c"),
            Some(&Value::Rat {
                num: 1,
                den: 1_000_000_000_000_000_007,
            }),
        );
    });
    p.case("zero-denominator", |p| {
        let result = Source::from_str(
            "zero-denom",
            "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, n)\n",
        )
        .must_admit(p);
        let err = eval_definitions_values(
            &result.package,
            &result.package.declarations[0],
            &BTreeMap::from([("n".to_string(), Value::I64(0))]),
            &BTreeMap::new(),
        )
        .err();
        match err {
            Some(TestVerdict::Fault {
                fault: EvalFault::CapabilityRefused { code, .. },
            }) => {
                p.contains("names-denominator", &code, "denominator");
            }
            Some(other) => {
                p.fail(
                    "fault-kind",
                    format!("expected a typed capability refusal, got {other:?}"),
                );
            }
            None => {
                p.fail("refuses", "rat(1, 0) must refuse, but it evaluated");
            }
        }
    });
    p.finish();
}
