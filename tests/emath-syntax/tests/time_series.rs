//! Time-series literals.
//!
//! Contracts (literal-first phase):
//! - `Series<T in tunit, U in vunit>` is an admitted TYPE; the literal
//!   `[(<time quantity>, <value quantity>), ...]` is admitted DATA;
//! - the policy suffix `with interpolation: <mode>[, extrapolation:
//!   <mode>]` is required for interpolation (no silent default — the
//!   mode changes every downstream number); extrapolation defaults to
//!   `refuse`;
//! - the policy is identity-bearing: two series differing only in
//!   interpolation mode hash to different meanings;
//! - a non-increasing time axis refuses (every mode orders by time);
//! - interpolation and declared extrapolation execute in the reference VM;
//!   `refuse` produces a typed out-of-support fault.
//!
//! Failure-first evidence: before this slice, the same probe files
//! refused with `E-TYPE-001 unknown type Series` + `E-TYPE-010 tuple
//! outside the Phase 1 subset` (live probes recorded in the pack).

use emath_core::limits::Limits;
use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const WIND_SERIES: &str = "\
emath function WindTunnel:
    inputs:
        t: Float64 in s

    outputs:
        v: Float64

    definitions:
        wind_data = [
            (0.0 s, 0.0 [unit m/s]),
            (0.1 s, 1.0 [unit m/s]),
            (0.2 s, 1.8 [unit m/s]),
        ] with interpolation: linear, extrapolation: refuse

        v = series_at(wind_data, t)

    tests:
        example <interior>:
            given t = 0.05
            expect v == 0.5
";

#[test]
fn series_literal_with_policy_admits() {
    let errors = check(WIND_SERIES, "ts-admit");
    assert!(
        errors.is_empty(),
        "a series data literal with a declared policy must admit; got {errors:?}"
    );
}

#[test]
fn linear_series_evaluates_between_support_points() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("ts-evaluate", WIND_SERIES);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("v"),
        Some(&Value::F64(0.5))
    );
}

#[test]
fn refuse_extrapolation_names_time_and_support() {
    install_source_parser();
    let source = "\
emath function RefuseOutside:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0)] with interpolation: linear
        sampled = series_at(data, 2.0)
    tests:
        example <outside>:
            expect sampled == 0.0
";
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("ts-refuse-outside", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(
        run_package(&checked.package).declarations[0].tests[0].verdict,
        TestVerdict::Fault {
            fault: EvalFault::SeriesOutOfSupport {
                time_bits: 2.0_f64.to_bits(),
                start_bits: 0.0_f64.to_bits(),
                end_bits: 1.0_f64.to_bits(),
            }
        }
    );
}

#[test]
fn missing_interpolation_refuses() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "\
emath function NoInterp:
    inputs:
        t: Float64 in s

    outputs:
        v: Float64 in s

    definitions:
        wind_data = [(0.0 s, 0.0 [unit m/s]), (0.1 s, 1.0 [unit m/s])] with extrapolation: refuse

        v = t * 2.0
";
    let result = session.check_owned("ts-nointerp", text);
    let rendered: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        rendered
            .iter()
            .any(|m| m.contains("E-SYN-101") && m.contains("interpolation")),
        "a series without a declared interpolation mode must refuse naming the key; got {rendered:?}"
    );
}

#[test]
fn unordered_time_axis_refuses() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "\
emath function Unordered:
    inputs:
        t: Float64 in s

    outputs:
        v: Float64 in s

    definitions:
        wind_data = [
            (0.2 s, 1.8 [unit m/s]),
            (0.1 s, 1.0 [unit m/s]),
        ] with interpolation: linear

        v = t * 2.0
";
    let result = session.check_owned("ts-unordered", text);
    let rendered: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        rendered.iter().any(|m| m.contains("strictly increasing")),
        "a non-increasing time axis must refuse; got {rendered:?}"
    );
}

#[test]
fn interpolation_mode_hashes_into_identity() {
    install_source_parser();
    let base = "\
emath function SeriesId:
    inputs:
        t: Float64 in s

    outputs:
        v: Float64 in s

    definitions:
        wind_data = [(0.0 s, 0.0 [unit m/s]), (0.1 s, 1.0 [unit m/s])] with interpolation: MODE

        v = t * 2.0
";
    let mut session_linear = CompilerSession::new(Limits::default());
    let linear = session_linear.check_owned("ts-id-linear", &base.replace("MODE", "linear"));
    let mut session_previous = CompilerSession::new(Limits::default());
    let previous =
        session_previous.check_owned("ts-id-previous", &base.replace("MODE", "previous"));
    assert!(
        !linear.diagnostics.has_errors() && !previous.diagnostics.has_errors(),
        "both policy variants must admit"
    );
    let linear_id = linear.package.meaning_id(&[]).expect("linear meaning id");
    let previous_id = previous
        .package
        .meaning_id(&[])
        .expect("previous meaning id");
    assert_ne!(
        linear_id, previous_id,
        "two series differing only in interpolation mode are different artifacts"
    );
}

#[test]
fn series_type_admits_in_outputs_and_conforms() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let text = "\
emath function Typed:
    inputs:
        t: Float64 in s

    outputs:
        wind: Series<Float64 in s, Float64 in m/s>

    definitions:
        wind = [(0.0 s, 0.0 [unit m/s]), (0.1 s, 1.0 [unit m/s])] with interpolation: linear
";
    let result = session.check_owned("ts-typed", text);
    let rendered: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        !result.diagnostics.has_errors(),
        "a Series<...> output type with a conforming data binding must admit; got {rendered:?}"
    );
}

#[test]
fn csv_series_import_maps_named_columns_and_evaluates() {
    install_source_parser();
    let source = "\
emath function CsvWind:
    definitions:
        data = series_from_csv(\"sample,time (s),wind (m/s)\\nA,0.0,0.0\\nB,0.1,1.0\\nC,0.2,1.8\", \"time\", \"wind\", \"linear\", \"refuse\")
        midpoint = series_at(data, 0.05)

    tests:
        example <mapped_columns>:
            expect midpoint == 0.5
";
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("csv-series", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("midpoint"),
        Some(&Value::F64(0.5))
    );
}

#[test]
fn csv_series_import_refuses_unknown_column_mapping() {
    install_source_parser();
    let source = "\
emath function CsvWind:
    definitions:
        data = series_from_csv(\"time,wind\\n0.0,0.0\\n0.1,1.0\", \"timestamp\", \"wind\", \"linear\", \"refuse\")
";
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("csv-series-missing-column", source);
    assert!(
        checked.diagnostics.errors().any(|diagnostic| {
            diagnostic.code == "E-CSV-001"
                && diagnostic.message.contains("timestamp")
                && diagnostic.message.contains("time")
        }),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}

// ---- previous / pwc (piecewise-constant) semantics ---
// `previous` is the left-continuous step interpolation: at time t the
// value is the sample at the greatest support time <= t; on the last
// support point the value is that point's sample (the endpoint is
// always evaluated, never the step below it). `pwc` is the canonical
// alias of `previous` — the two modes must agree at every time.

const STEP_SERIES: &str = "\
emath function StepWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: MODE, extrapolation: refuse
        at_half = series_at(data, 0.5)
        at_one = series_at(data, 1.0)
        at_one_half = series_at(data, 1.5)
        at_two = series_at(data, 2.0)
    tests:
        example <steps>:
            expect at_half == 0.0
            expect at_one == 2.0
            expect at_one_half == 2.0
            expect at_two == 4.0
";

fn run_definitions_value(name: &str, source: &str, binding: &str) -> Value {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(name, source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{name}: {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    report.declarations[0].tests[0]
        .definitions
        .get(binding)
        .cloned()
        .expect(binding)
}

#[test]
fn previous_interpolation_is_left_continuous_step() {
    for mode in ["previous", "pwc"] {
        let mut session = CompilerSession::new(Limits::default());
        let source = STEP_SERIES.replace("MODE", mode);
        let checked = session.check_owned(&format!("step-{mode}"), &source);
        assert!(
            !checked.diagnostics.has_errors(),
            "{mode}: {:?}",
            checked.diagnostics.errors().collect::<Vec<_>>()
        );
        let report = run_package(&checked.package);
        let values = &report.declarations[0].tests[0].definitions;
        assert_eq!(values.get("at_half"), Some(&Value::F64(0.0)), "{mode}");
        assert_eq!(values.get("at_one"), Some(&Value::F64(2.0)), "{mode}");
        assert_eq!(values.get("at_one_half"), Some(&Value::F64(2.0)), "{mode}");
        assert_eq!(values.get("at_two"), Some(&Value::F64(4.0)), "{mode}");
    }
}

// Failure-first (BLOCKED driver): `extrapolation: extend` continues the
// OUTER interval's interpolation. For a step mode the step is the last
// sample — `series_at(data, 3.0)` must be the LAST value (4.0). The
// current `sample_series` pins the second-to-last interval for
// `time >= end`, so `previous`/`pwc` return the SECOND-TO-LAST sample
// (2.0) at alpha >= 1. This test fails today; it flips green with the
// outer-interval fix in crates/emath-exec-ir/src/interp.rs.
#[test]
fn step_modes_with_extend_past_end_return_last_sample() {
    let source = "\
emath function StepExtend:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: MODE, extrapolation: extend
        past_end = series_at(data, 3.0)
    tests:
        example <beyond>:
            expect past_end == 4.0
";
    for mode in ["previous", "pwc"] {
        let value = run_definitions_value(
            &format!("step-extend-{mode}"),
            &source.replace("MODE", mode),
            "past_end",
        );
        assert_eq!(
            value,
            Value::F64(4.0),
            "{mode} + extend must hold the last sample beyond the support"
        );
    }
}

// ---- linear semantics --------------------------------
// Linear interpolation pieces the support with straight segments:
// between samples the value is the time-proportional blend of the two
// bracketing samples; at every support point the value is that point's
// sample (mode-independent, pinned by the pass-7 law too).
#[test]
fn linear_interpolation_evaluates_interior_and_endpoints() {
    let source = "\
emath function LinearWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: linear, extrapolation: refuse
        at_quarter = series_at(data, 0.25)
        at_half = series_at(data, 0.5)
        at_one = series_at(data, 1.0)
        at_one_half = series_at(data, 1.5)
        at_two = series_at(data, 2.0)
    tests:
        example <lin>:
            expect at_quarter == 0.5
            expect at_half == 1.0
            expect at_one == 2.0
            expect at_one_half == 3.0
            expect at_two == 4.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("linear-wind", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    for (binding, expected) in [
        ("at_quarter", 0.5),
        ("at_half", 1.0),
        ("at_one", 2.0),
        ("at_one_half", 3.0),
        ("at_two", 4.0),
    ] {
        assert_eq!(
            report.declarations[0].tests[0].definitions.get(binding),
            Some(&Value::F64(expected)),
            "{binding}"
        );
    }
}

// ---- monotone-cubic semantics and shape refusals -----
// `monotone_cubic` is the Fritsch–Carlson monotone cubic: per-interval
// slopes are the sign-matched average of adjacent secants (0 where
// adjacent secants disagree), end slopes are the outer secant, and the
// segment is the cubic Hermite basis over that slope data — shape-
// preserving: the interpolant never overshoots the bracketing samples.
#[test]
fn monotone_cubic_is_shape_preserving_fritsch_carlson() {
    // Slopes: bracket [0,1] secant 1: left end = 1, right end = 0 (next
    // secant -1 disagrees in sign). Bracket [1,2] secant -1: left = 0,
    // right end = -1. Hermite values pinned below; every interior value
    // stays inside the sample min/max [0, 1].
    let source = "\
emath function CubicPeak:
    definitions:
        data = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)] with interpolation: monotone_cubic, extrapolation: refuse
        v_quarter = series_at(data, 0.25)
        v_half = series_at(data, 0.5)
        v_three_quarters = series_at(data, 0.75)
        v_second_half = series_at(data, 1.5)
    tests:
        example <cubic>:
            expect v_quarter == 0.296875
            expect v_half == 0.625
            expect v_three_quarters == 0.890625
            expect v_second_half == 0.625
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("cubic-peak", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("v_quarter"), Some(&Value::F64(0.296875)));
    assert_eq!(values.get("v_half"), Some(&Value::F64(0.625)));
    assert_eq!(values.get("v_three_quarters"), Some(&Value::F64(0.890625)));
    assert_eq!(values.get("v_second_half"), Some(&Value::F64(0.625)));
}

#[test]
fn monotone_cubic_two_points_reduces_to_secant_line() {
    // With exactly two support points both end slopes are the single
    // secant and the Hermite basis collapses to a plain line.
    let source = "\
emath function CubicRise:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0)] with interpolation: monotone_cubic, extrapolation: refuse
        v_quarter = series_at(data, 0.25)
        v_half = series_at(data, 0.5)
        v_three_quarters = series_at(data, 0.75)
    tests:
        example <cubic_line>:
            expect v_quarter == 0.5
            expect v_half == 1.0
            expect v_three_quarters == 1.5
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("cubic-rise", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    for (binding, expected) in [
        ("v_quarter", 0.5),
        ("v_half", 1.0),
        ("v_three_quarters", 1.5),
    ] {
        assert_eq!(
            report.declarations[0].tests[0].definitions.get(binding),
            Some(&Value::F64(expected)),
            "{binding}"
        );
    }
}

#[test]
fn monotone_cubic_refuses_non_increasing_and_malformed_support() {
    let malformed: Vec<(&str, &str)> = vec![
        // Unordered time axis — every mode, cubic included, refuses.
        (
            "cmb-unordered",
            "\
emath function BadAxis:
    definitions:
        data = [(1.0, 1.0), (0.0, 0.0)] with interpolation: monotone_cubic
",
        ),
        // Equal consecutive times are not strictly increasing.
        (
            "cmb-equal-times",
            "\
emath function EqualTimes:
    definitions:
        data = [(0.0, 0.0), (0.0, 1.0)] with interpolation: monotone_cubic
",
        ),
        // An empty series has no support to interpolate.
        (
            "cmb-empty",
            "\
emath function EmptySeries:
    definitions:
        data = [] with interpolation: monotone_cubic
",
        ),
        // Rows are exactly `(time, value)` pairs.
        (
            "cmb-not-pair",
            "\
emath function NotPairs:
    definitions:
        data = [(0.0, 0.0, 1.0)] with interpolation: monotone_cubic
",
        ),
    ];
    for (name, source) in malformed {
        let errors = check(source, name);
        assert!(
            errors.iter().any(|message| message.contains("E-SYN-101")),
            "{name} must refuse with E-SYN-101 naming the malformed shape; got {errors:?}"
        );
    }
}

#[test]
fn monotone_cubic_single_point_evaluates_to_that_value() {
    // A one-point support is degenerate but well-defined: every sample
    // request evaluates to the single value (the endpoint path).
    let source = "\
emath function SinglePoint:
    definitions:
        data = [(1.0, 7.0)] with interpolation: monotone_cubic, extrapolation: refuse
        sampled = series_at(data, 1.0)
    tests:
        example <single>:
            expect sampled == 7.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("cubic-single", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("sampled"),
        Some(&Value::F64(7.0))
    );
}
// `nearest` picks the closer sample; on an exact binary midpoint
// (alpha == 0.5) the tie resolves to the LATER sample (round-half-up).
// The choice is deterministic: the same series and time always produce
// the same value.
#[test]
fn nearest_rounds_half_to_later_sample_and_is_deterministic() {
    let source = "\
emath function NearestWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0)] with interpolation: nearest, extrapolation: refuse
        at_tie = series_at(data, 0.5)
        at_just_before_tie = series_at(data, 0.49)
        at_just_after_tie = series_at(data, 0.51)
    tests:
        example <nearest>:
            expect at_tie == 2.0
            expect at_just_before_tie == 0.0
            expect at_just_after_tie == 2.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("nearest-wind", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    for run in 0..3 {
        let report = run_package(&checked.package);
        let values = &report.declarations[0].tests[0].definitions;
        assert_eq!(values.get("at_tie"), Some(&Value::F64(2.0)), "run {run}");
        assert_eq!(
            values.get("at_just_before_tie"),
            Some(&Value::F64(0.0)),
            "run {run}"
        );
        assert_eq!(
            values.get("at_just_after_tie"),
            Some(&Value::F64(2.0)),
            "run {run}"
        );
    }
}

// ---- extrapolation refuse/clamp/extend ---------------
// `refuse` (the default) turns any sample outside the support into a
// typed `SeriesOutOfSupport` fault naming time, start, and end. `clamp`
// pins to the nearest endpoint value. `extend` continues the OUTER
// interval's interpolation: linear keeps its slope, nearest/step modes
// hold the last sample, monotone_cubic continues its Hermite segment.
#[test]
fn refuse_before_start_faults_with_time_and_support_bits() {
    install_source_parser();
    let source = "\
emath function RefuseBefore:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0)] with interpolation: linear
        sampled = series_at(data, -1.0)
    tests:
        example <before>:
            expect sampled == 0.0
";
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("ts-refuse-before", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].verdict,
        TestVerdict::Fault {
            fault: EvalFault::SeriesOutOfSupport {
                time_bits: (-1.0_f64).to_bits(),
                start_bits: 0.0_f64.to_bits(),
                end_bits: 1.0_f64.to_bits(),
            }
        }
    );
}

#[test]
fn clamp_pins_to_nearest_endpoint_for_every_interpolation_mode() {
    for mode in ["previous", "linear", "nearest", "monotone_cubic"] {
        let source = format!(
            "\
emath function ClampWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0)] with interpolation: {mode}, extrapolation: clamp
        before = series_at(data, -1.0)
        after = series_at(data, 3.0)
    tests:
        example <clamp>:
            expect before == 0.0
            expect after == 2.0
"
        );
        let mut session = CompilerSession::new(Limits::default());
        let checked = session.check_owned(&format!("clamp-{mode}"), &source);
        assert!(
            !checked.diagnostics.has_errors(),
            "{mode}: {:?}",
            checked.diagnostics.errors().collect::<Vec<_>>()
        );
        let report = run_package(&checked.package);
        let values = &report.declarations[0].tests[0].definitions;
        assert_eq!(values.get("before"), Some(&Value::F64(0.0)), "{mode}");
        assert_eq!(values.get("after"), Some(&Value::F64(2.0)), "{mode}");
    }
}

#[test]
fn extend_linear_continues_the_outer_slope() {
    let source = "\
emath function LinearExtend:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: linear, extrapolation: extend
        past_end = series_at(data, 3.0)
        before_start = series_at(data, -1.0)
    tests:
        example <extend_linear>:
            expect past_end == 6.0
            expect before_start == -2.0
";
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("extend-linear", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("past_end"), Some(&Value::F64(6.0)));
    assert_eq!(values.get("before_start"), Some(&Value::F64(-2.0)));
}

#[test]
fn extend_nearest_and_cubic_continue_the_outer_interval() {
    // nearest: beyond the end the outer interval's nearest rule holds
    // the LAST sample. monotone_cubic: the Hermite segment continues;
    // with equal adjacent secants the continuation is the straight
    // slope (4 + (3.0 - 2.0) * 2 == 6.0).
    for (mode, expected) in [("nearest", 4.0), ("monotone_cubic", 6.0)] {
        let source = format!(
            "\
emath function ExtendWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: {mode}, extrapolation: extend
        past_end = series_at(data, 3.0)
    tests:
        example <extend>:
            expect past_end == {expected:.1}
"
        );
        let value = run_definitions_value(&format!("extend-{mode}"), &source, "past_end");
        assert_eq!(value, Value::F64(expected), "{mode}");
    }
}

// ---- semantic-identity / metamorphic laws ------------
// Law 1 (interpolation): at every support point every interpolation
// mode evaluates to that point's sample — linear, step, nearest, and
// cubic Hermite all interpolate the data exactly at the knots.
// Law 2 (extrapolation does not leak inward): the declared
// extrapolation mode (refuse/clamp/extend) never changes an interior
// value. Together: the policy triple (interpolation, extrapolation)
// partitions behavior at the support and only the interpolation mode
// owns interior shape (identity separation already pinned by
// `interpolation_mode_hashes_into_identity`).
#[test]
fn every_mode_agrees_at_support_points_across_all_extrapolation_policies() {
    let base = "\
emath function LawWind:
    definitions:
        data = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0)] with interpolation: MODE, extrapolation: XMODE
        at_zero = series_at(data, 0.0)
        at_one = series_at(data, 1.0)
        at_two = series_at(data, 2.0)
    tests:
        example <law>:
            expect at_zero == 0.0
            expect at_one == 2.0
            expect at_two == 4.0
";
    for mode in ["previous", "linear", "nearest", "pwc", "monotone_cubic"] {
        for extrapolation in ["refuse", "clamp", "extend"] {
            let source = base.replace("XMODE", extrapolation).replace("MODE", mode);
            let mut session = CompilerSession::new(Limits::default());
            let checked = session.check_owned(&format!("law-{mode}-{extrapolation}"), &source);
            assert!(
                !checked.diagnostics.has_errors(),
                "{mode}/{extrapolation}: {:?}",
                checked.diagnostics.errors().collect::<Vec<_>>()
            );
            let report = run_package(&checked.package);
            let values = &report.declarations[0].tests[0].definitions;
            assert_eq!(
                values.get("at_zero"),
                Some(&Value::F64(0.0)),
                "{mode}/{extrapolation}"
            );
            assert_eq!(
                values.get("at_one"),
                Some(&Value::F64(2.0)),
                "{mode}/{extrapolation}"
            );
            assert_eq!(
                values.get("at_two"),
                Some(&Value::F64(4.0)),
                "{mode}/{extrapolation}"
            );
        }
    }
}

// Deliberate mutation record: `step_modes_with_extend_past_end_
// return_last_sample` above is the failure-first driver — it FAILS against
// the current `sample_series` (outer-interval index pins the second-to-last
// bracket for `time >= end`, so step modes return the second-to-last
// sample past the end) and must flip GREEN with the outer-interval fix in
// crates/emath-exec-ir/src/interp.rs. After the fix, re-neutering the fix
// (returning the PRE-fix index math) must fail the test again: that flip is
// the mutation kill proving the test discriminates correct from incorrect
// step+extend evaluation.
