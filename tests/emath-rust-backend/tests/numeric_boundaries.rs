//! Numeric representation crossings must compile, compute, and refuse overflow.
use std::path::PathBuf;
use std::process::Command;

#[test]
fn named_imports_retain_private_and_later_selected_callees() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/constructor/named-polynomial-import.emath");
    let source = "use algebra.polynomials.{poly_mul}
use algebra.polynomials.{poly_add as plus}

emath function Product:
    inputs:
        unused: Int
    outputs:
        result: sequence(Rat)
    definitions:
        result = plus(poly_mul([-1 / 1, 1 / 1], [1 / 1, 1 / 1]), [1 / 1])
";
    emath_syntax::install_source_parser();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let checked = session.check_owned(&fixture.display().to_string(), source);
    assert!(!checked.diagnostics.has_errors(), "{:?}", checked.diagnostics);
    let prepared = checked.prepared.as_ref().unwrap();
    let value = prepared.evaluate_function(
        "Product",
        &std::collections::BTreeMap::from([(
            "unused".into(),
            emath_exec_ir::constructor_layer::CValue::Int(0.into()),
        )]),
        100_000,
    ).unwrap();
    assert_eq!(value.to_string(), "[0/1, 0/1, 1/1]");
    let emission = emath_rust_backend::constructor_crate::emit_constructor_crate(
        prepared, &fixture, source,
    ).unwrap();
    assert!(emission.runnable, "a named import must carry its implementation: {:?}", emission.unresolved);
}

#[test]
fn native_numeric_boundaries() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/constructor/native_numeric_boundaries.emath");
    let source = std::fs::read_to_string(&fixture).unwrap();
    let (tree, diagnostics) = emath_syntax::parse_str(&source);
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let report = emath_exec_ir::constructor_layer::evaluate_tree_at(&tree, Some(&fixture)).unwrap();
    assert!(!report.tests.is_empty());
    for case in report.tests {
        assert!(case.passed, "{}: {}", case.label, case.detail);
    }
    emath_syntax::install_source_parser();
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let checked = session.check_owned(&fixture.display().to_string(), &source);
    assert!(!checked.diagnostics.has_errors(), "{:?}", checked.diagnostics);
    let emission = emath_rust_backend::constructor_crate::emit_constructor_crate(
        checked.prepared.as_ref().unwrap(), &fixture, &source,
    ).unwrap();
    assert!(emission.runnable, "{:?}", emission.unresolved);
    // The public single-program emitter is also a callable entry, not a
    // body that requires its host to install an execution frame first.
    let (square_tree, diagnostics) = emath_syntax::parse_str(
        "emath function Square:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x * x
",
    );
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let square =
        emath_exec_ir::constructor_emir::lower_constructor_function(&square_tree, "Square").unwrap();
    let standalone =
        emath_rust_backend::emit_constructor_program(&square.program, &square.inputs).unwrap();
    let (recursive_tree, diagnostics) = emath_syntax::parse_str(
        "emath function Count:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = if x <= 0: 0 else: Count(x - 1) + 1
",
    );
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let recursive =
        emath_exec_ir::constructor_emir::lower_constructor_function(&recursive_tree, "Count").unwrap();
    let recursive_entry =
        emath_rust_backend::emit_constructor_program(&recursive.program, &recursive.inputs).unwrap();
    let out = std::env::temp_dir().join(format!("emath-native-numeric-{}", std::process::id()));
    std::fs::create_dir_all(out.join("src")).unwrap();
    std::fs::write(
        out.join("src/lib.rs"),
        format!("{}\n{standalone}\npub mod standalone_recursive {{ use super::emath_rt; {recursive_entry} }}", emission.lib),
    ).unwrap();
    std::fs::write(
        out.join("Cargo.toml"),
        &emission.manifest,
    )
    .unwrap();
    let main = r#"
    use native_numeric_boundaries::*;
    fn main() {
        use emath_rt::ExactInt;
        assert_eq!(format!("{:?}", entry(7).unwrap()), "49");
        assert_eq!(format!("{:?}", standalone_recursive::entry(4).unwrap()), "4");
        assert!(emath_rt::execution::with_budget(0, 10, || entry(7)).unwrap_err().starts_with("E-NATIVE-WORK"));
        assert!(emath_rt::execution::with_budget(100, 2, || standalone_recursive::entry(4)).unwrap_err().starts_with("E-NATIVE-DEPTH"));
        assert_eq!(format!("{:?}", entry(7).unwrap()), "49", "refusal must restore the host execution context");
        let ints = |values: &[i64]| values.iter().copied().map(ExactInt::from).collect::<Vec<_>>();
        let pair = Pair(-10, 3).unwrap();
        assert_eq!((pair.quotient, pair.remainder), (ExactInt::from(-4), ExactInt::from(2)));
        assert_eq!(Countdown(ExactInt::from(10)).unwrap(), 4);
        assert_eq!(ClosureBoundary(12).unwrap(), ExactInt::from(5));
        assert_eq!(HigherOrderClosure(std::rc::Rc::new(|x| Ok(x + 1))).unwrap(), ExactInt::from(4));
        assert_eq!(TinyRatio(262143).unwrap(), 0.0);
        assert_eq!(VectorRecord(10).unwrap().values, ints(&[3, 1]));
        assert_eq!(MixedVectorRecord(10).unwrap().values, ints(&[1, 3]));
        assert_eq!(ConsVectorRecord(10).unwrap().values, ints(&[1, 3]));
        assert_eq!(PrependInput(7, vec![]).unwrap(), vec![7]);
        assert_eq!(PrependInput(7, vec![1, 2, 3]).unwrap(), vec![7, 1, 2, 3]);
        assert_eq!(VectorClosure(10).unwrap(), ExactInt::from(4));
        assert_eq!(WideThenSmall(0).unwrap().to_string(), "0");
        assert_eq!(RatRecord(10).unwrap().value, (3, 1));
        assert_eq!(LiteralRatRecord(0).unwrap().value, (3, 1));
        let wide = OverflowRecord(0).unwrap().value;
        assert_eq!(wide.to_string(), "9223372036854775808");
        assert_eq!(wide.to_i64(), None);
        assert_eq!(ClosureOverflow(0).unwrap().to_string(), "9223372036854775808");
        assert_eq!(WideThroughFrame(0).unwrap().to_string(), "9223372036854775808");
        assert_eq!(WideTimesFloat(0).unwrap(), 4611686018427387904.0);
        let equal = MixedEquality(1).unwrap();
        assert!(equal.rat_equal && !equal.rat_different && equal.float_equal && !equal.float_different);
        let different = MixedEquality(2).unwrap();
        assert!(!different.rat_equal && different.rat_different && !different.float_equal && different.float_different);
        let precise = MixedEquality(9007199254740992).unwrap();
        assert!(precise.float_precision_equal && !precise.float_precision_greater && !precise.float_precision_reverse_less);
        let rounded = MixedEquality(9007199254740993).unwrap();
        assert!(!rounded.float_precision_equal && rounded.float_precision_greater && rounded.float_precision_reverse_less);
        assert!(MixedEquality(0).unwrap().float_large_equal);
        assert!(!equal.float_large_equal);
        assert_eq!(RecordIteration(3).unwrap().to_string(), "3");
        assert_eq!(RecordIteration(0).unwrap().to_string(), "0");
        assert!(RecordIteration(-1).unwrap_err().starts_with("E-SCALAR-CONVERT"));
        assert_eq!(
            (
                AlternatingRecurrence(2).unwrap().to_string(),
                ConstantRightSubtraction(2).unwrap().to_string(),
                ReversedGuard(2).unwrap().to_string(),
                DependentBase(2).unwrap().to_string(),
            ),
            ("1".into(), "0".into(), "9".into(), "2".into()),
            "native recurrence must preserve subtraction order, guard orientation, and terminal base"
        );
        println!("native numeric boundaries passed");
    }
    "#;
    std::fs::write(out.join("src/main.rs"), main).unwrap();
    let target_base = std::env::var_os("RCH_TARGET_BASE")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let output = Command::new("cargo")
        .args([
            "run",
            "--offline",
            "--jobs",
            "1",
            "--quiet",
            "-p",
            &emission.package_name,
        ])
        .current_dir(&out)
        .env(
            "CARGO_TARGET_DIR",
            target_base.join("rch_target_emath_test/native_numeric_boundaries"),
        )
        .env("CARGO_BUILD_JOBS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native build/run failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "native numeric boundaries passed"
    );
}
