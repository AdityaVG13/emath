//! Numeric representation crossings must compile, compute, and refuse overflow.
use std::path::PathBuf;
use std::process::Command;

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
    let emission =
        emath_rust_backend::constructor_crate::emit_constructor_crate(&tree, &fixture).unwrap();
    assert!(emission.runnable, "{:?}", emission.unresolved);
    let out = std::env::temp_dir().join(format!("emath-native-numeric-{}", std::process::id()));
    std::fs::create_dir_all(out.join("src")).unwrap();
    std::fs::write(out.join("src/lib.rs"), emission.lib).unwrap();
    std::fs::write(
        out.join("Cargo.toml"),
        format!("{}\n[workspace]\n", emission.manifest),
    )
    .unwrap();
    let main = r#"
    use native_numeric_boundaries::*;
    fn main() {
        let pair = Pair(-10, 3).unwrap();
        assert_eq!((pair.quotient, pair.remainder), (-4, 2));
        assert_eq!(Countdown(10).unwrap(), 4);
        assert_eq!(ClosureBoundary(12).unwrap(), 5);
        assert_eq!(VectorRecord(10).unwrap().values, vec![3, 1]);
        assert_eq!(MixedVectorRecord(10).unwrap().values, vec![1, 3]);
        assert_eq!(ConsVectorRecord(10).unwrap().values, vec![1, 3]);
        assert_eq!(PrependInput(7, vec![]).unwrap(), vec![7]);
        assert_eq!(PrependInput(7, vec![1, 2, 3]).unwrap(), vec![7, 1, 2, 3]);
        assert_eq!(VectorClosure(10).unwrap(), 4);
        assert_eq!(WideThenSmall(0).unwrap().to_string(), "0");
        assert_eq!(RatRecord(10).unwrap().value, (3, 1));
        assert_eq!(LiteralRatRecord(0).unwrap().value, (3, 1));
        assert!(OverflowRecord(0).unwrap_err().contains("E-INT-002"));
        assert!(ClosureOverflow(0).unwrap_err().contains("E-INT-002"));
        assert_eq!(WideThroughFrame(0).unwrap().to_string(), "9223372036854775808");
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
