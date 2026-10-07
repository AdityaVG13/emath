//! Mutable buffer carrier:
//! `buffer(size, fill)` constructs an
//! in-place, bounds-checked indexed carrier; `buffer_set(buf, i, v)`
//! writes through shared references; `buf[i]` reads; `buf.length`
//! projects. Equality on buffers refuses (mutable state has no total
//! value equality); checkpoints ENCODE buffers with aliasing fidelity
//! (user decision 2026-09-17). Failure-first: every case here faults
//! against the pre-carrier engine (`buffer` is an unbound name).

use emath_exec_ir::constructor_layer::{
    admit_tree_at, evaluate_function_budgeted_at, evaluate_tree_at, CValue,
};
use emath_syntax::parse_str;
use emath_test_harness::{Probe, boot};

const WRITE_READ: &str = "\
emath function WriteThrough:
    inputs:
        flags: buffer
        i: Int
    outputs:
        result: Int
    definitions:
        wrote = buffer_set(flags, i, true)
        result = 0

emath function Aliased:
    inputs:
        size: Int
    outputs:
        result: Bool
    definitions:
        flags = buffer(size, false)
        wrote = WriteThrough(flags, 3)
        result = flags[3]
";

const BOUNDS_FAULT: &str = "\
emath function OutOfBounds:
    inputs:
        size: Int
    outputs:
        result: Int
    definitions:
        flags = buffer(size, false)
        wrote = buffer_set(flags, 99, true)
        result = 0
    tests:
        example <writes_refuse>:
            given size = 4
            expect diagnostic.code == invalid_index
";

const EQUALITY_REFUSED: &str = "\
emath function ComparesBuffers:
    inputs:
        size: Int
    outputs:
        result: Bool
    definitions:
        flags = buffer(size, false)
        result = flags == flags
";

// Two function values written alike: equality between functions is not
// decidable, and the VM's structural compare saw source positions.
const FUNCTION_EQUALITY_REFUSED: &str = "\
emath function ComparesFunctions:
    inputs:
        k: Int
    outputs:
        result: Bool
    definitions:
        f = function a in Int: a + k
        g = function a in Int: a + k
        result = f == g
";

// Non-tail call chain so the suspended state holds the SAME buffer in
// several live frames: Outer's env binds `flags`, Inner's guard frame
// binds it again, Step's write frame binds it a third time. The write
// lives in Step (below the guard), so no level ever writes at
// i == limit; an unguarded `buffer_set` would fault `invalid_index`
// by contract. Suspension lands inside the write loop.
const SUSPEND_ALIASED: &str = "\
emath function Step:
    inputs:
        flags: buffer
        i: Int
        limit: Int
    outputs:
        result: Int
    definitions:
        wrote = buffer_set(flags, i, true)
        result = Inner(flags, i + 1, limit)

emath function Inner:
    inputs:
        flags: buffer
        i: Int
        limit: Int
    outputs:
        result: Int
    definitions:
        result = if i >= limit: 42 else: Step(flags, i, limit)

emath function Outer:
    inputs:
        limit: Int
    outputs:
        result: Int
    definitions:
        flags = buffer(limit, false)
        result = 1 + Inner(flags, 0, limit)
";

fn tree(source: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diagnostics) = parse_str(source);
    assert!(!diagnostics.has_errors(), "parse errors: {diagnostics:?}");
    tree
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("buffer carrier: shared writes, checked bounds, refused buffer and function equality, alias-faithful checkpoints");

    p.case("write_reads_through_alias", |p| {
        let t = tree(WRITE_READ);
        admit_tree_at(&t, None).expect("admits");
        let value = emath_exec_ir::constructor_layer::evaluate_function_at(
            &t,
            "Aliased",
            &[("size".to_string(), CValue::Int(4_i128.into()))]
                .into_iter()
                .collect(),
            None,
        )
        .expect("Aliased evaluates");
        p.eq("aliased-write-visible", value, CValue::Bool(true));
    });

    p.case("fresh_mutable_results_never_alias_through_memo", |p| {
        let t = tree("emath function Fresh:
    inputs:
        n: Int
    outputs:
        result: buffer
    definitions:
        result = buffer(n, false)
emath function fact_fresh:
    inputs:
        n: Int
    outputs:
        result: buffer
    definitions:
        result = Fresh(n)
emath function FreshNested:
    inputs:
        n: Int
    outputs:
        result: sequence(buffer)
    definitions:
        result = [Fresh(n)]
emath function Independent:
    inputs:
        unused: Int
    outputs:
        named: Bool
        fact: Bool
        nested: Bool
        recursive: Bool
    definitions:
        a = Fresh(1)
        b = Fresh(1)
        wrote = buffer_set(a, 0, true)
        named = b[0]
        fa = fact_fresh(1)
        fb = fact_fresh(1)
        fact_write = buffer_set(fa, 0, true)
        fact = fb[0]
        na = FreshNested(1)
        nb = FreshNested(1)
        nested_write = buffer_set(na[0], 0, true)
        nested = nb[0][0]
        factory = recur fresh in Int -> buffer: function n in Int: buffer(n, false)
        ca = factory(1)
        cb = factory(1)
        closure_write = buffer_set(ca, 0, true)
        recursive = cb[0]
");
        let inputs = [("unused".to_string(), CValue::Int(0_i128.into()))].into_iter().collect();
        let expected = ["named", "fact", "nested", "recursive"];
        for budgeted in [false, true] {
            let value = if budgeted {
                evaluate_function_budgeted_at(&t, "Independent", &inputs, 10_000, None, "fresh-results", None, "").unwrap()
            } else {
                emath_exec_ir::constructor_layer::evaluate_function(&t, "Independent", &inputs).unwrap()
            };
            let CValue::Record { fields, .. } = value else { panic!("multi-output record") };
            for field in expected {
                p.eq(&format!("{budgeted}-{field}"), fields.get(field).unwrap(), &CValue::Bool(false));
            }
        }
        let module = emath_exec_ir::constructor_layer::prepare_module(&t, None).unwrap();
        let mut carried = std::collections::BTreeMap::new();
        for _ in 0..2 {
            let result = emath_exec_ir::constructor_layer::evaluate_function_carrying(
                &module, "Independent", &inputs, &mut carried, 10_000,
            ).unwrap();
            let CValue::Record { fields, .. } = result.value.unwrap() else { panic!("record") };
            p.eq("carried-fresh", fields.get("fact").unwrap(), &CValue::Bool(false));
            p.demand("mutable-facts-not-carried", result.fresh.is_empty(), format!("{:?}", result.fresh));
        }
    });

    p.case("bounds_refuse_by_name", |p| {
        let t = tree(BOUNDS_FAULT);
        let report = evaluate_tree_at(&t, None).expect("fault-row module evaluates");
        p.demand(
            "invalid_index-demanded",
            report.tests.len() == 1
                && report.tests[0].passed
                && report.tests[0].detail.contains("invalid_index"),
            format!("fault row outcome: {:?}", report.tests),
        );
    });

    p.case("invalid_primitive_prefix_does_not_run_later_effects", |p| {
        let t = tree("emath function Bump:
    inputs:
        flags: buffer
    outputs:
        result: Int
    definitions:
        wrote = buffer_set(flags, 0, flags[0] + 1)
        result = flags[0]
emath function Bad:
    inputs:
        flags: buffer
    outputs:
        result: buffer
    definitions:
        result = buffer(false, Bump(flags))
");
        let cell = std::sync::Arc::new(std::sync::Mutex::new(vec![CValue::Int(0.into())]));
        let inputs = [("flags".to_string(), CValue::Buffer(cell.clone()))].into_iter().collect();
        let fault = emath_exec_ir::constructor_layer::evaluate_function(
            &t, "Bad", &inputs,
        ).expect_err("invalid size must refuse before evaluating the fill");
        p.eq("type-refusal", fault.code, "type".to_string());
        p.eq("later-write-not-executed", cell.lock().unwrap()[0].clone(), CValue::Int(0.into()));
    });

    p.case("equality_refuses_at_admission", |p| {
        let t = tree(EQUALITY_REFUSED);
        let err = admit_tree_at(&t, None).expect_err("buffer equality must refuse");
        p.eq("named-refusal", err.code, "buffer_equality_refused");
        let t = tree(FUNCTION_EQUALITY_REFUSED);
        let err = admit_tree_at(&t, None).expect_err("function equality must refuse");
        p.eq("function-named-refusal", err.code, "function_equality_refused");
    });

    p.case("checkpoint_encodes_aliasing", |p| {
        let t = tree(SUSPEND_ALIASED);
        let inputs: std::collections::BTreeMap<String, CValue> =
            [("limit".to_string(), CValue::Int(8_i128.into()))]
                .into_iter()
                .collect();
        // Tiny budget: the run must suspend inside Inner's write loop.
        let suspended = evaluate_function_budgeted_at(
            &t,
            "Outer",
            &inputs,
            40,
            None,
            "buffer-pin",
            None,
            "",
        );
        let Err((err, checkpoint)) = suspended else {
            p.demand("suspends", false, "tiny budget must suspend inside the write loop");
            return;
        };
        p.eq("suspension-reason", err.code, "budget_exhausted");
        // Round-trip: encode then decode; aliasing must survive (the
        // user's encode decision, and the honesty floor: decoding two
        // independent buffers would silently diverge writes).
        let decoded = emath_exec_ir::constructor_layer::Checkpoint::decode(&checkpoint.encode())
            .expect("checkpoint decodes");
        let mut buffer_ptrs = std::collections::BTreeSet::new();
        for frame in &decoded.frames {
            for (name, value) in &frame.env {
                if let CValue::Buffer(cell) = value {
                    buffer_ptrs.insert(std::sync::Arc::as_ptr(cell) as usize);
                    p.demand(
                        "frames-bind-the-buffer",
                        name == "flags",
                        format!("unexpected buffer binding `{name}`"),
                    );
                }
            }
        }
        p.demand(
            "two-live-frames",
            decoded.frames.len() >= 2,
            format!("suspension must hold two live frames, got {}", decoded.frames.len()),
        );
        p.demand(
            "aliasing-survives-roundtrip",
            buffer_ptrs.len() == 1,
            format!("all frame bindings must share one buffer, got {}", buffer_ptrs.len()),
        );
        // Resume with a real budget: the answer is exact.
        let resumed = evaluate_function_budgeted_at(
            &t,
            "Outer",
            &inputs,
            1_000_000,
            Some(&decoded),
            "buffer-pin",
            None,
            "",
        );
        p.eq(
            "resumed-exact",
            resumed.expect("resume completes"),
            CValue::Int(43_i128.into()),
        );
    });

    p.finish();
}
