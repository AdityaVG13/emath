//! Binding-provenance explanation for `emath explain --provenance`.

use std::path::Path;

use emath_artifact::JsonWriter;
use emath_sema::session::CompilerSession;

use crate::{
    json_diagnostic_entry, json_diagnostics_entries, print_diagnostics, print_json_diagnostics,
    CliExit, EXIT_REFUSED, EXIT_USAGE,
};

/// Render every admitted binding provenance edge in deterministic order.
pub fn provenance_explanation(path: &Path, json: bool) -> Result<String, CliExit> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        if json {
            print_json_diagnostics(
                "explain",
                false,
                &[json_diagnostic_entry(
                    "E-PKG-080",
                    "error",
                    &format!("cannot read {}", path.display()),
                )],
            );
        }
        return Err(EXIT_USAGE);
    };
    let result = session.check(package.file);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        if json {
            print_json_diagnostics(
                "explain",
                false,
                &json_diagnostics_entries(&result.diagnostics),
            );
        }
        return Err(EXIT_REFUSED);
    }

    let entries = result
        .package
        .binding_provenance
        .iter()
        .map(|(site, provenance)| {
            let declaration = result.package.declaration(site.declaration).map_or_else(
                || format!("declaration#{}", site.declaration.0),
                |declaration| declaration.name.leaf().to_string(),
            );
            (
                format!("{declaration}.{}", site.binding),
                provenance.variant_name(),
                provenance.explain(),
            )
        })
        .collect::<Vec<_>>();

    if json {
        let nodes = entries
            .iter()
            .map(|(binding, kind, detail)| {
                let mut node = JsonWriter::object();
                node.string("binding", binding);
                node.string("kind", kind);
                node.string("detail", detail);
                node.finish().trim_end().to_string()
            })
            .collect::<Vec<_>>();
        let mut document = JsonWriter::object();
        document.string("schema", "emath.provenance-explanation.v1");
        document.objects("nodes", &nodes);
        Ok(document.finish())
    } else if entries.is_empty() {
        Ok("provenance: no binding provenance\n".to_string())
    } else {
        let mut out = String::from("provenance DAG\n");
        for (binding, _, detail) in entries {
            out.push_str("  ");
            out.push_str(&binding);
            out.push_str(" -> ");
            out.push_str(&detail);
            out.push('\n');
        }
        Ok(out)
    }
}
