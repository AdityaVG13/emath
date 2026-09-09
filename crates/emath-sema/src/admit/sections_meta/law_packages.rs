//! Embedded curated law-package imports.
//!
//! `use physics::classical::{NewtonSecond}` is a compiler import of an
//! authored `.emath` source already in `language/stdlib/laws/`. No math
//! is invented here: the file is parsed and admitted like any other source.

use std::collections::BTreeSet;

use emath_core::Diagnostics;
use emath_ir::{ImportEntry, ImportSelection};

use super::{CheckResult, SemanticTrace};

pub(super) fn embedded_law_package(path: &[String]) -> Option<&'static str> {
    match path {
        [domain, pack] if domain == "physics" && pack == "classical" => Some(include_str!(
            "../../../../../language/stdlib/laws/physics-classical.emath"
        )),
        [domain, pack] if domain == "physics" && pack == "relativity" => Some(include_str!(
            "../../../../../language/stdlib/laws/physics-relativity.emath"
        )),
        [domain, pack] if domain == "cs" && pack == "laws" => Some(include_str!(
            "../../../../../language/stdlib/laws/computer-science.emath"
        )),
        [domain, pack] if domain == "probability" && pack == "laws" => Some(include_str!(
            "../../../../../language/stdlib/laws/probability-statistics.emath"
        )),
        [domain, pack] if domain == "analysis" && pack == "laws" => Some(include_str!(
            "../../../../../language/stdlib/laws/analysis.emath"
        )),
        [domain, pack] if domain == "number_theory" && pack == "laws" => Some(include_str!(
            "../../../../../language/stdlib/laws/algebra-number-theory.emath"
        )),
        [domain, pack] if domain == "optimization_control" && pack == "laws" => Some(include_str!(
            "../../../../../language/stdlib/laws/optimization-control.emath"
        )),
        _ => None,
    }
}

pub(super) fn resolve_embedded_law_import(
    imports: &[ImportEntry],
    source: emath_core::Span,
) -> Option<CheckResult> {
    let [import] = imports else {
        if imports.len() > 1
            && imports
                .iter()
                .all(|import| embedded_law_package(&import.path).is_some())
        {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error(
                "E-PKG-053",
                "import one embedded law package per source; multi-package imports are not admitted yet",
                source,
            );
            let mut package = emath_ir::SemanticPackage::new();
            package.imports = imports.to_vec();
            return Some(CheckResult {
                package,
                diagnostics,
                trace: SemanticTrace::default(),
                units_profiles: Vec::new(),
            });
        }
        return None;
    };
    let package_source = embedded_law_package(&import.path)?;
    let Some(parser) = emath_core::source_parser() else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-SYN-120",
            "no source parser installed for embedded law package",
            source,
        );
        return Some(CheckResult {
            package: emath_ir::SemanticPackage::new(),
            diagnostics,
            trace: SemanticTrace::default(),
            units_profiles: Vec::new(),
        });
    };
    let (tree, parse_diagnostics) = parser.parse(
        package_source,
        source.file,
        &emath_core::limits::Limits::default(),
        emath_core::Edition::Ed2026,
    );
    let mut result = super::check_tree(&tree);
    result.diagnostics.extend_from(&parse_diagnostics);

    if let ImportSelection::Named(names) = &import.selection {
        let selected: BTreeSet<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
        for (name, alias) in names {
            if alias.is_some() {
                result.diagnostics.error(
                    "E-PKG-053",
                    "aliases on embedded law imports are not admitted yet",
                    import.source,
                );
            }
            if !result
                .package
                .declarations
                .iter()
                .any(|declaration| declaration.name.leaf() == name)
            {
                result.diagnostics.error(
                    "E-PKG-053",
                    format!(
                        "law symbol `{name}` is not exported by `{}`",
                        import.path.join("::")
                    ),
                    import.source,
                );
            }
        }
        let retained: BTreeSet<emath_ir::DeclarationId> = result
            .package
            .declarations
            .iter()
            .filter(|declaration| selected.contains(declaration.name.leaf()))
            .map(|declaration| declaration.id)
            .collect();
        result
            .package
            .declarations
            .retain(|declaration| retained.contains(&declaration.id));
        result
            .package
            .law_metadata
            .retain(|declaration, _| retained.contains(declaration));
    }
    result.package.imports = imports.to_vec();
    if !result.diagnostics.has_errors() {
        result.package.seal();
    }
    Some(result)
}
