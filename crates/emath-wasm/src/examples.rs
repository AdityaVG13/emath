//! Curated example sources embedded in the wasm surface.

use super::*;

/// ABI version carried by the `version` op.
pub const ABI_VERSION: u32 = 1;

/// Classic `hello-square` example served by the `examples` op.
pub const HELLO_SQUARE: &str = include_str!("../../../language/examples/intro/hello-square.emath");
/// Stateful affine-scorer tutorial served by the `examples` op.
pub const AFFINE_SCORER: &str =
    include_str!("../../../tests/fixtures/language/intro/stateful-affine-scorer.emath");
/// Sum-1-to-5 example served by the `examples` op.
pub const SUM_ONE_TO_FIVE: &str =
    include_str!("../../../tests/fixtures/language/intro/sum-one-to-five.emath");
/// Tensor-face fixture served by the `examples` op.
pub const TENSOR_FACE: &str =
    include_str!("../../../tests/fixtures/language/intro/tensor-face.emath");
/// Vector `given`/`expect` example served by the `examples` op.
pub const VECTOR_GIVEN: &str =
    include_str!("../../../tests/fixtures/language/intro/vector-given.emath");
/// Factorial fixture served by the `examples` op.
pub const FACTORIAL: &str = include_str!("../../../tests/fixtures/language/intro/factorial.emath");
/// Range-sum fixture served by the `examples` op.
pub const RANGE_SUM: &str = include_str!("../../../tests/fixtures/language/intro/range-sum.emath");
/// Quantifier fixture served by the `examples` op.
pub const FORALL_EXISTS: &str =
    include_str!("../../../tests/fixtures/language/intro/forall-exists.emath");
/// Integral fixture served by the `examples` op.
pub const INTEGRAL: &str = include_str!("../../../tests/fixtures/language/intro/integral.emath");
/// Autodiff example served by the `examples` op.
pub const AUTODIFF: &str = include_str!("../../../language/examples/intro/autodiff.emath");
/// Equation-solving example served by the `examples` op.
pub const SOLVE: &str = include_str!("../../../language/examples/intro/solve.emath");
/// Optimization example served by the `examples` op.
pub const OPTIMIZE: &str = include_str!("../../../language/examples/intro/optimize.emath");
/// Constrained-optimization fixture served by the `examples` op.
pub const CONSTRAINED_OPT: &str =
    include_str!("../../../tests/fixtures/language/intro/constrained-optimization.emath");

/// Tutorial 1 source: quickstart and scratchpad.
pub const TUTORIAL_01_QUICKSTART: &str = "\
# Tutorial 1: Quickstart & Scratchpad
# Declarative mathematical function with test assertions.
# Press Ctrl+R (or Cmd+Enter) to evaluate the interpreter.

emath function Quickstart:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = 3 * x + 7

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <test_four>:
            given x = 4
            expect y == 19
";

/// Tutorial 2 source: 2D curve plotter with parameters.
pub const TUTORIAL_02_PLOTTER: &str = "\
# Tutorial 2: 2D Curve Plotter & Parameters
# Switch to the 'Plot 2D' tab (Alt+2) to visualize this oscillator curve.
# Adjust the 'x' slider live while viewing the canvas.

emath function DampedOscillator:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = exp(-0.1 * x) * sin(x)

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <origin>:
            given x = 0
            expect y == 0
";

/// Tutorial 3 source: math intent and typography.
pub const TUTORIAL_03_MATH_INTENT: &str = "\
# Tutorial 3: Math Intent & Typography
# Press Shift+Cmd+Y to toggle Unicode math symbols.
# Switch to the 'Math Intent' tab (Alt+3) to view LaTeX rendering and export formulas.

emath function AerodynamicDrag:
    inputs:
        rho: Float64
        v: Float64
        cd: Float64
        area: Float64

    outputs:
        drag_force: Float64

    definitions:
        drag_force = 0.5 * rho * (v * v) * cd * area

    goals:
        evaluate <drag_force>:
            produce rust.library
";

/// Tutorial 6 source: diagnostics and error recovery.
pub const TUTORIAL_06_DIAGNOSTICS_DEMO: &str = "\
# Tutorial 6: Diagnostics & Error Recovery
# Notice the red indicator in the status bar and the Diagnostics tab (Alt+5).
# Fix the undefined variable below to see diagnostics clear automatically.

emath function DiagnosticsDemo:
    inputs:
        x: Float64

    definitions:
        y = missing_variable
";

/// Curated examples served by the `examples` op.
pub fn curated_examples() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Tutorial 1: Quickstart & Scratchpad",
            TUTORIAL_01_QUICKSTART,
        ),
        (
            "Tutorial 2: 2D Curve Plotter & Parameters",
            TUTORIAL_02_PLOTTER,
        ),
        (
            "Tutorial 3: Math Intent & Typography",
            TUTORIAL_03_MATH_INTENT,
        ),
        ("Tutorial 4: Stateful Scorer & Assertions", AFFINE_SCORER),
        (
            "Tutorial 6: Diagnostics & Error Recovery",
            TUTORIAL_06_DIAGNOSTICS_DEMO,
        ),
        ("Hello Square (Classic)", HELLO_SQUARE),
        ("Sum 1 to 5", SUM_ONE_TO_FIVE),
        ("Tensor Face", TENSOR_FACE),
        ("Vector Given", VECTOR_GIVEN),
        ("Factorial (inclusive 1..=n)", FACTORIAL),
        ("Range Sum (variable-bound fold)", RANGE_SUM),
        ("Forall / Exists (quantifier binders)", FORALL_EXISTS),
        ("Integral (numerical integration)", INTEGRAL),
        ("Autodiff (forward-mode derivative)", AUTODIFF),
        ("Solve (Newton's method root-finding)", SOLVE),
        ("Optimize (Newton on ∇f = 0)", OPTIMIZE),
        ("Constrained optimization (penalty method)", CONSTRAINED_OPT),
    ]
}
