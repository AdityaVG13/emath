//! Curated example sources embedded in the wasm surface.


/// ABI version carried by the `version` op.
pub const ABI_VERSION: u32 = 1;

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
        y = 3.0 * x + 7.0

    tests:
        example <test_four>:
            given x = 4.0
            expect y == 19.0
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
        y = x

    tests:
        example <origin>:
            given x = 0.0
            expect y == 0.0
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
";

/// Tutorial 6 source: diagnostics and error recovery.
pub const TUTORIAL_06_DIAGNOSTICS_DEMO: &str = "\
# Tutorial 6: Diagnostics & Error Recovery
# Notice the red indicator in the status bar and the Diagnostics tab (Alt+5).
# Fix the undefined variable below to see diagnostics clear automatically.

emath function DiagnosticsDemo:
    inputs:
        x: Float64
    outputs:
        y: Float64
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
        (
            "Tutorial 6: Diagnostics & Error Recovery",
            TUTORIAL_06_DIAGNOSTICS_DEMO,
        ),
    ]
}
