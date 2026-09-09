//! TOTAL refusal matrix for bare `Real`
//! at type sites. Every context where bare `Real` appears must produce ONE
//! deterministic E-NUM-004 diagnostic naming the three sanctioned spellings:
//! `Float64` (strict-f64 profile), `Interval<Float64>` (certified-interval
//! surrogate), or the `representation Real => Float64` directive. No
//! shape-dependent behavior: bare input vs Vector element → same code, same
//! message.

use emath_test_harness::{Probe, Source, boot};

const CANONICAL_E_NUM_004: &str = "E-NUM-004: bare `Real` at a type site requires profile evidence; write `Float64` (strict-f64), `Interval<Float64>` (certified interval), or a `representation Real => Float64` directive";

const MATRIX: &[(&str, &str)] = &[
    ("bare-input", "emath function F:\n    inputs:\n        x: Real\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n"),
    ("vector-element", "emath function G:\n    inputs:\n        v: Vector[Real, 3]\n    outputs:\n        y: Float64\n    definitions:\n        y = v[0]\n"),
    ("output-field", "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Real\n    definitions:\n        y = x\n"),
    ("state-field", "emath model M:\n    inputs:\n        x: Float64\n    state:\n        s: Real\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n"),
    ("matrix-element", "emath function F:\n    inputs:\n        m: Matrix[Real, 2, 2]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("tensor-element", "emath function F:\n    inputs:\n        t: Tensor[Real, 2, 2, 2]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("nested-vector-element", "emath function F:\n    inputs:\n        v: Vector[Vector[Real, 2], 3]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("option-element", "emath function F:\n    inputs:\n        o: Option<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("result-ok-arm", "emath function F:\n    inputs:\n        r: Result<Real, Float64>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("result-err-arm", "emath function F:\n    inputs:\n        r: Result<Float64, Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("set-element", "emath function F:\n    inputs:\n        s: Set<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("interval-element", "emath function F:\n    inputs:\n        i: Interval<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("refinement-element", "emath function F:\n    inputs:\n        q: NonNegative<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("domain-base", "emath function F:\n    inputs:\n        x: Real in [0, 1]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("unit-base", "emath function F:\n    inputs:\n        x: Real in m\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n"),
    ("event-parameter", "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    events:\n        event Tick(x: Real)\n"),
    ("constructor-parameter", "emath policy P:\n    inputs:\n        x: Float64\n    state:\n        v: Float64\n    constructors:\n        public fn new(x: Real) -> Float64:\n            require x == x\n            Self:\n                v = x\n"),
    ("constructor-return", "emath policy P:\n    inputs:\n        x: Float64\n    state:\n        v: Float64\n    constructors:\n        public fn new(x: Float64) -> Real:\n            require x == x\n            Self:\n                v = x\n"),
    ("observation-annotation", "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    observations:\n        obs r: Real = 1.0\n"),
];

fn enum004_of(name: &str, text: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .filter(|d| d.code == "E-NUM-004")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn bare_real_refused_everywhere_with_one_canonical_message() {
    boot();
    let mut p = Probe::new("bare Real is refused with one canonical E-NUM-004 at every type site");
    let mut bare = Vec::new();
    let mut vector = Vec::new();
    for (name, text) in MATRIX {
        let messages = enum004_of(*name, *text);
        if *name == "bare-input" {
            bare = messages.clone();
        }
        if *name == "vector-element" {
            vector = messages.clone();
        }
        p.case(name, |p| {
            p.eq("canonical", messages, vec![CANONICAL_E_NUM_004.to_string()]);
        });
    }
    p.case("shape-independence", |p| {
        p.eq("bare-equals-vector", bare.clone(), vector.clone());
    });
    p.case("spellings-named", |p| {
        let joined = bare.join("\n");
        p.contains("float64", &joined, "Float64");
        p.contains("interval", &joined, "Interval<Float64>");
        p.contains("directive", &joined, "representation Real => Float64");
    });
    Source::from_str(
        "float64-control",
        "emath function H:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n",
    )
    .must_admit(&mut p);
    Source::from_str(
        "rat-control",
        "emath function K:\n    inputs:\n        a: Rat\n        b: Rational\n    outputs:\n        r: Rat\n    definitions:\n        r = a * b\n",
    )
    .must_admit(&mut p);
    p.finish();
}
