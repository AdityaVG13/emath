//! Sequence append must not cost per element (emath-1jp92).
//!
//! A list threaded through a tail recursion and appended onto once per
//! step must cost the same per step at every length. When the append
//! rebuilds the list element by element, the work count grows O(n^2):
//! doubling n quadruples the units. The discriminator is the
//! deterministic work count of two runs that double n — never wall
//! time, which the machine shares with everything else.

use std::collections::BTreeMap;
use std::sync::Arc;

use emath_exec_ir::constructor_layer::{prepare_module, CValue};
use emath_syntax::parse_str;

/// Threading through the authored `concat` (what search.backtrack's
/// receipt walk writes): each append goes through the library function,
/// so a per-element rebuild shows up as engine steps.
const SOURCE: &str = "\
use fold.reduce

emath function thread:
    inputs:
        acc: sequence(Int)
        i: Int
        n: Int
    outputs:
        result: sequence(Int)
    definitions:
        result = if i >= n: acc else: thread(concat(acc, [i]), i + 1, n)
";

const WORK_LIMIT: u64 = 2_000_000_000;

fn int(n: i128) -> CValue {
    CValue::Int(n.into())
}

fn run_thread(module: &emath_exec_ir::constructor_layer::PreparedModule, n: i128) -> u64 {
    let mut inputs = BTreeMap::new();
    inputs.insert("acc".to_string(), CValue::Sequence(Arc::new(Vec::new())));
    inputs.insert("i".to_string(), int(0));
    inputs.insert("n".to_string(), int(n));
    module
        .evaluate_function_budgeted_reported("thread", &inputs, WORK_LIMIT, None, "", "")
        .unwrap()
        .1
}

#[test]
fn threaded_append_work_is_linear() {
    let (tree, diagnostics) = parse_str(SOURCE);
    assert!(!diagnostics.has_errors(), "probe parse errors: {diagnostics:?}");
    let module = prepare_module(&tree, None).unwrap();

    let small = run_thread(&module, 512);
    let large = run_thread(&module, 1024);

    // Linear append: doubling n doubles the units (plus fixed overhead).
    // A per-element rebuild: doubling n quadruples them. The bound sits
    // between; HEAD's authored concat lands at ~4x.
    let ratio = large as f64 / small as f64;
    assert!(
        ratio <= 2.5,
        "threaded append work grew {small} -> {large} units ({ratio:.2}x for 2x the elements): \
         the append path costs per element instead of per append"
    );
}
