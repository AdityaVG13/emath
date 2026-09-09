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

use emath_ir::ExprNode;
use emath_test_harness::{Probe, Source, boot};

/// Two pure cells (the two integrator rules) plus a function that calls
/// both by bare name and consumes the vector results.
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

#[test]
fn sde_capability_cells_admit_and_lower_through_apply() {
    boot();
    let mut p = Probe::new("SDE integrator cells admit and lower through the generic declared-capability Apply path");
    p.case("cells-lower-to-apply", |p| {
        let checked = Source::from_str("sde-surface", CELLS_AND_CALLS).must_admit(p);
        let names: Vec<&str> = checked
            .package
            .capabilities
            .iter()
            .map(|capability| capability.name.0.as_str())
            .collect();
        let ito = names.iter().position(|name| *name == "std.stochastic.euler_maruyama");
        let strat = names.iter().position(|name| *name == "std.stochastic.stratonovich");
        p.demand("ito-cell", ito.is_some(), format!("euler_maruyama admitted: {names:?}"));
        p.demand("strat-cell", strat.is_some(), format!("stratonovich admitted: {names:?}"));
        let applies: Vec<usize> = checked
            .package
            .exprs
            .iter()
            .filter_map(|node| match node {
                ExprNode::Apply { capability, .. } => Some(capability.index()),
                _ => None,
            })
            .collect();
        p.demand(
            "ito-apply",
            ito.map(|index| applies.contains(&index)).unwrap_or(false),
            format!("euler_maruyama reached through Apply: {applies:?} vs {names:?}"),
        );
        p.demand(
            "strat-apply",
            strat.map(|index| applies.contains(&index)).unwrap_or(false),
            format!("stratonovich reached through Apply: {applies:?} vs {names:?}"),
        );
    });
    p.case("unknown-call", |p| {
        Source::from_str(
            "unknown-call",
            CELLS_AND_CALLS.replace(
                "ito = euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
                "ito = nonexistent_cell(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
            ),
        )
        .must_refuse(p, &["E-TYPE-003"]);
    });
    p.case("qualified-call", |p| {
        Source::from_str(
            "qualified-call",
            CELLS_AND_CALLS.replace(
                "ito = euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
                "ito = std::stochastic::euler_maruyama(drift, sigma, 1.0, 0.01, 64, 7.0, [0.0])",
            ),
        )
        .must_admit(p);
    });
    p.case("shipped-example", |p| {
        Source::from_workspace("language/examples/numerical/sde-control.emath").must_admit(p);
    });
    p.finish();
}
