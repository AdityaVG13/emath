//! — the `.emath` SDE capability-cell
//! surface (sema tier).
//!
//! The surface is the GENERIC declared-capability call path: cells are
//! declared with the standard capability surface (`class:`/`version:`/
//! `migration:` + `inputs`/`outputs`) and called by NAME. There is NO
//! `sde` builtin and NO `ito`/`stratonovich` keyword mapping anywhere
//! in sema — the two rules are two cells (`std.stochastic.euler_maruyama`,
//! `std.stochastic.stratonovich`); a call lowers to `ExprNode::Apply`
//! and the executor resolves it through `ApplyCapability` (compiled-cell
//! data first, then the native-kernel registry — the shared
//! builtin-miss seam the geometry lane reuses).
//!
//! Hard assertions: admission is clean, the lowered IR names the cells,
//! the declared output type types the call result, and unknown names
//! still refuse with the typed unknown-function diagnostic.

use emath_core::limits::Limits;
use emath_ir::ExprNode;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// Two pure cells (the two integrator rules) plus a function that calls
/// both by bare name and consumes the vector results. The cell names
/// match the native-kernel registry keys (`std.stochastic.*`).
const CELLS_AND_CALLS: &str = r#"
package std.stochastic
use std.kinds.capability

emath capability euler_maruyama:
    class: pure
    version: "1.0.0"
    migration: frozen
    inputs:
        drift: Vector[Float64]
        diffusion: Vector[Float64]
        x0: Float64
        h: Float64
        steps: Float64
        seed: Float64
        stream: Vector[Float64]
    outputs:
        trajectory: Vector[Float64]

emath capability stratonovich:
    class: pure
    version: "1.0.0"
    migration: frozen
    inputs:
        drift: Vector[Float64]
        diffusion: Vector[Float64]
        x0: Float64
        h: Float64
        steps: Float64
        seed: Float64
        stream: Vector[Float64]
    outputs:
        trajectory: Vector[Float64]

emath function sde_paths:
    definitions:
        drift = [0.0, 0.25]
        sigma = [0.0, 0.35]
        ito = euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])
        corrected = stratonovich(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])
        spread = ito[64] - corrected[64]
"#;

/// (severity, code, message) diagnostics for one source.
fn check(source: &str) -> Vec<(String, String, String)> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("sde-surface.emath", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
                diagnostic.message.clone(),
            )
        })
        .collect()
}

fn error_codes(out: &[(String, String, String)]) -> Vec<String> {
    out.iter()
        .filter(|(severity, _, _)| severity == "Error")
        .map(|(_, code, _)| code.clone())
        .collect()
}

/// Both cells admit cleanly, land in the package capability arena under
/// their canonical names, and BOTH call sites lower to `ExprNode::Apply`
/// targeting those cells — the generic declared-capability call path,
/// with no domain keyword anywhere in the pipeline. The declared output
/// type (`Vector[Float64]`) types the call result: indexing `ito[64]`
/// into a Float64 subtraction admits without a type error.
#[test]
fn sde_capability_cells_admit_and_lower_to_apply() {
    let mut session = CompilerSession::new(Limits::default());
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let checked = session.check_owned("sde-surface.emath", CELLS_AND_CALLS);
    let codes: Vec<String> = checked
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.is_empty(),
        "capability cells and their calls must admit; got {codes:?}"
    );
    // The cells are in the package capability arena under the canonical
    // names (package path + declaration name = the registry keys).
    let names: Vec<&str> = checked
        .package
        .capabilities
        .iter()
        .map(|capability| capability.name.0.as_str())
        .collect();
    let ito_index = names_index(&names, "std.stochastic.euler_maruyama");
    let strat_index = names_index(&names, "std.stochastic.stratonovich");
    assert!(
        ito_index.is_some(),
        "euler_maruyama cell admitted: {names:?}"
    );
    assert!(
        strat_index.is_some(),
        "stratonovich cell admitted: {names:?}"
    );
    // Both call sites are Apply nodes targeting the declared cells.
    let applies: Vec<usize> = checked
        .package
        .exprs
        .iter()
        .filter_map(|node| match node {
            ExprNode::Apply { capability, .. } => Some(capability.index()),
            _ => None,
        })
        .collect();
    assert!(
        applies.contains(&ito_index.unwrap_or(usize::MAX))
            && applies.contains(&strat_index.unwrap_or(usize::MAX)),
        "both cells must be reached through ExprNode::Apply; applies = {applies:?}, cells = {names:?}"
    );
}

/// The typed-refusal contract is preserved: a call to a name that is
/// neither a builtin nor a declared capability cell still refuses with
/// the unknown-function diagnostic — the capability path never
/// silently admits unknown names.
#[test]
fn unknown_call_still_refuses_typed() {
    let source = CELLS_AND_CALLS.replace(
        "ito = euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
        "ito = nonexistent_cell(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
    );
    let out = check(&source);
    let codes = error_codes(&out);
    assert!(
        codes.iter().any(|code| code == "E-TYPE-003"),
        "unknown call must refuse E-TYPE-003 (unknown function); got {codes:?}"
    );
}

/// The qualified spelling reaches the same cell through the canonical
/// dotted key — the call name is resolved against the package's
/// capability arena either way (one seam, two spellings).
#[test]
fn qualified_call_form_resolves_the_same_cell() {
    let source = CELLS_AND_CALLS.replace(
        "ito = euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
        "ito = std::stochastic::euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
    );
    let out = check(&source);
    let codes = error_codes(&out);
    assert!(
        codes.is_empty(),
        "qualified capability call must admit through the same seam; got {codes:?}"
    );
}

fn names_index(names: &[&str], needle: &str) -> Option<usize> {
    names.iter().position(|name| *name == needle)
}

/// The on-disk runnable example admits through the same path (the
/// biform fixture pattern): the shipped `sde-control.emath` is the
/// user-facing contract, so it is checked as-is, not paraphrased.
#[test]
fn shipped_example_admits() {
    let source = include_str!("../../../language/examples/numerical/sde-control.emath");
    let out = check(source);
    let codes = error_codes(&out);
    assert!(
        codes.is_empty(),
        "the shipped sde-control example must admit; got {codes:?}"
    );
}
