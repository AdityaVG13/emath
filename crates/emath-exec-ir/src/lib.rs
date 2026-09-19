//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

mod builtin;
mod emitter;
pub mod growth;
pub mod image;
pub mod install;
pub mod interp;
pub mod language_image;
pub mod language_tables;
pub mod lazy;
pub mod native_kernel;
pub mod progress;
pub mod optimize;
pub mod reference_views;
pub mod exact_int;
pub mod constructor_layer;
pub mod constructor_emir;
pub mod runner;
pub mod shake;
pub mod term_compile;

pub use builtin::BuiltinId;
pub use runner::{
    CONSTRUCTOR_SIMULATE_GONE, Continuation, DAEDisposition, DAEIndex, InitializationVerdict,
    SimulateOptions, StepMethod, Trajectory, TrajectorySample, definition_order,
    simulate_continuous, simulate_continuous_dispositioned, simulate_continuous_with,
    step_continuous, step_continuous_values,
};

use emath_core::Span;
pub use emath_ir::CellClass;
use emath_ir::SemanticPackage;
mod format;
mod op;
mod ops_impl;
mod program;
mod types;

pub use op::*;
pub use program::*;
pub use types::*;

use format::*;

/// SIR requirement lowering is leftover. Constructor emission uses
/// `constructor_emir::lower_constructor_function`.
#[allow(unreachable_code, unused_variables)]
pub fn lower_requirement(
    package: &SemanticPackage,
    expr: EmirExprRef,
    param_names: &[String],
) -> Result<EmirProgram, String> {
    let _ = (package, expr, param_names);
    return Err(
        "E-KIND-GONE: SIR requirement lowering is not constructor surface. Use constructor_emir::lower_constructor_function.".into(),
    );
    let mut program = emitter::lower(package, expr, param_names, &[])?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

/// SIR definition lowering is leftover. Constructor emission uses
/// `constructor_emir::lower_constructor_function`.
#[allow(unreachable_code, unused_variables)]
pub fn lower_definition(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    let _ = (package, expr, inputs, states);
    return Err(
        "E-KIND-GONE: SIR definition lowering is not constructor surface. Use constructor_emir::lower_constructor_function.".into(),
    );
    let mut program = emitter::lower(package, expr, inputs, states)?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

pub type EmirExprRef = emath_ir::ExprId;
