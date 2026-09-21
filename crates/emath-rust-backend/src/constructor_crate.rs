//! Constructor-file crate emission (the `emath build` core, shared
//! with the native epoch export, bead emath-8k3zw).
//!
//! One emitter, two callers: the CLI's `emath build` and the loop
//! host's `export-native` must produce byte-identical artifact crates,
//! so the orchestration lives here once. This module is pure: it takes
//! source text and the spec path (for `use` resolution) and returns
//! the lib text, the manifest, and the admission facts; callers own
//! IO, verification, and presentation.
//!
//! The emission is the self-containment law from `emath build`: the
//! runtime ships inside the artifact as `mod emath_rt`, entries carry
//! their authored function names, and authored records map through the
//! backend's carrier-signature interchange (an unmappable field leaves
//! the record unregistered; referencing it refuses by name).

use std::path::Path;

use emath_core::tree::SyntaxTree;

use crate::{emit_constructor_entry, emit_record_definitions, AuthoredRecord};

/// What one constructor-file emission produced.
#[derive(Clone, Debug)]
pub struct ConstructorCrateEmission {
    /// `src/lib.rs` body (self-contained: `mod emath_rt` embedded when
    /// the lowering used the runtime).
    pub lib: String,
    /// `Cargo.toml` body (empty dependency list).
    pub manifest: String,
    /// The crate name inside the manifest (the sanitized file stem).
    pub package_name: String,
    /// False when any entry lowered to unresolved symbolic code.
    pub runnable: bool,
    /// The main file's function entries, in declaration order.
    pub functions: Vec<String>,
    /// Why entries are not runnable, when they are not.
    pub unresolved: Vec<String>,
    /// The authored records the emission registered, with their
    /// AUTHORED (emath) field names and carrier signatures. The
    /// emitted Rust escapes keywords (`move` becomes `move_` via
    /// `escape_ident`), and that escape is not injective, so
    /// consumers that must recover the authored names (the native
    /// epoch export's scratch mirror) map back through THIS list,
    /// never by heuristic.
    pub records: Vec<AuthoredRecord>,
}

/// A typed refusal from crate emission; callers keep their own
/// presentation (the CLI's E-codes, the loop host's `loop_export_*`).
#[derive(Clone, Debug)]
pub enum ConstructorEmitRefusal {
    /// The source is not a constructor program (or has no function
    /// entries to emit); the text is the refusal message.
    NotConstructor(&'static str),
    /// Merging `use` imports refused; `e_code` is the admission-code
    /// mapping of the engine's own code.
    ImportsRefused { e_code: String, detail: String },
}

/// The gate message for non-constructor source.
const NOT_CONSTRUCTOR: &str =
    "`emath build` emits constructor functions. Write `emath object`, `emath function`, or `emath query`.";

/// The gate message for function-less source.
const NO_FUNCTIONS: &str =
    "`emath build` emits lowered Rust for `emath function` entries. Symbolic query-only files are not marked runnable.";

/// Generated-artifact lint policy: entry names follow authored
/// spellings (PascalCase), the flattener parenthesizes substituted
/// expressions defensively, and authored-but-unused parameters stay
/// in signatures - all style-only in machine-generated code.
const EMITTED_HEADER: &str = "#![forbid(unsafe_code)]\n#![allow(nonstandard_style, unused_parens, unused_braces, unused_mut, unused_variables)]\n\n";

/// Emit the constructor crate for a parsed constructor file (the
/// MAIN tree; parse diagnostics are the caller's to report), merging
/// `use` imports relative to `spec_path`.
///
/// # Errors
/// A typed [`ConstructorEmitRefusal`]; never a partial crate.
pub fn emit_constructor_crate(
    main: &SyntaxTree,
    spec_path: &Path,
) -> Result<ConstructorCrateEmission, ConstructorEmitRefusal> {
    let constructor_only = main.items.iter().all(|item| match item {
        emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => {
            matches!(decl.as_kind.as_str(), "object" | "function" | "query")
        }
        _ => true,
    });
    if !constructor_only {
        return Err(ConstructorEmitRefusal::NotConstructor(NOT_CONSTRUCTOR));
    }
    // Merge `use` imports into one whole-program tree (the engine's
    // install order) so imported objects and functions lower here too.
    // The MAIN file's functions are the crate's entries; imported
    // declarations join as lowering context (siblings, record
    // layouts), never as re-emitted entries.
    let tree = match emath_exec_ir::constructor_layer::merged_tree_with_imports(
        main,
        Some(spec_path),
    ) {
        Ok(tree) => tree,
        Err(error) => {
            return Err(ConstructorEmitRefusal::ImportsRefused {
                e_code: emath_exec_ir::constructor_layer::constructor_admit_code(&error.code)
                    .to_string(),
                detail: format!("{}: {}", error.code, error.message),
            });
        }
    };
    let functions: Vec<String> = main
        .items
        .iter()
        .filter_map(|item| match item {
            emath_core::tree::Item::Declaration(decl) if decl.as_kind == "function" => {
                Some(decl.name.clone())
            }
            _ => None,
        })
        .collect();
    if functions.is_empty() {
        return Err(ConstructorEmitRefusal::NotConstructor(NO_FUNCTIONS));
    }
    // Authored records: every object's representation fields in the
    // backend's carrier-signature interchange. An object with a field
    // the interchange cannot map is left unregistered - referencing it
    // then refuses with the named no-layout error instead of guessing.
    use std::collections::BTreeSet;
    let objects: BTreeSet<String> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            emath_core::tree::Item::Declaration(decl) if decl.as_kind == "object" => {
                Some(decl.name.clone())
            }
            _ => None,
        })
        .collect();
    let mut records = Vec::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind == "object" {
            let mut fields = Vec::new();
            let mut mappable = true;
            for (field, ty) in
                emath_exec_ir::constructor_emir::section_typed_fields(decl, "representation")
            {
                match emath_exec_ir::constructor_emir::constructor_type_signature(&ty, &objects) {
                    Some(signature) => fields.push((field, signature)),
                    None => mappable = false,
                }
            }
            if mappable {
                records.push(AuthoredRecord {
                    name: decl.name.clone(),
                    fields,
                });
            }
        } else if decl.as_kind == "function" {
            // Multi-output functions pack their outputs as a record
            // named after the function (the lowerer's output packing),
            // so the entry's result carrier needs that layout too.
            let outputs = emath_exec_ir::constructor_emir::section_typed_fields(decl, "outputs");
            if outputs.len() > 1 {
                let mut fields = Vec::new();
                let mut mappable = true;
                for (output, ty) in outputs {
                    match emath_exec_ir::constructor_emir::constructor_type_signature(
                        &ty, &objects,
                    ) {
                        Some(signature) => fields.push((output, signature)),
                        None => mappable = false,
                    }
                }
                if mappable {
                    records.push(AuthoredRecord {
                        name: decl.name.clone(),
                        fields,
                    });
                }
            }
        }
    }
    let mut rust = String::from(EMITTED_HEADER);
    let mut runnable = true;
    let mut unresolved = Vec::new();
    // The module callable table for the shared-tree lane (view's
    // Global layouts, dependency stamps): emitted as one crate-level
    // static when any lowered program carries the tree lane. The
    // stamps are the same FNV-1a `decl_stamp` digests the VM's quote
    // capture computes, so an artifact's stamped dependencies are
    // byte-identical to the VM's.
    let module_table = emath_exec_ir::constructor_layer::module_callable_table(&tree);
    let mut needs_module_table = false;
    let mut needs_definition_table = false;
    match emit_record_definitions(&records) {
        Ok(definitions) => rust.push_str(&definitions),
        Err(error) => {
            runnable = false;
            unresolved.push(error.to_string());
        }
    }
    // One named entry per runnable function: sibling calls (pilot p8)
    // need both functions in the same crate, so entries carry their
    // function names instead of a shared `entry` symbol.
    for name in &functions {
        match emath_exec_ir::constructor_emir::lower_constructor_function(&tree, name) {
            Ok(lowered) => {
                if !lowered.runnable {
                    runnable = false;
                    unresolved.extend(lowered.unresolved);
                    rust.push_str(&format!(
                        "// `{name}` is not marked runnable: unresolved symbolic code\n"
                    ));
                    continue;
                }
                needs_module_table |= crate::contains_tree_ops(&lowered.program);
                needs_definition_table |= crate::contains_body_ops(&lowered.program);
                // Declared carriers ground the entry ABI; an untyped
                // input falls back to the numeric lane's Int.
                let declared = main
                    .items
                    .iter()
                    .find_map(|item| match item {
                        emath_core::tree::Item::Declaration(decl) if &decl.name == name => Some(
                            emath_exec_ir::constructor_emir::section_typed_fields(decl, "inputs"),
                        ),
                        _ => None,
                    })
                    .unwrap_or_default();
                let inputs: Vec<(String, Option<String>)> = lowered
                    .inputs
                    .iter()
                    .map(|input| {
                        let signature = declared
                            .iter()
                            .find(|(declared, _)| declared == input)
                            .and_then(|(_, ty)| {
                                emath_exec_ir::constructor_emir::constructor_type_signature(
                                    ty, &objects,
                                )
                            });
                        (input.clone(), signature)
                    })
                    .collect();
                // The single-output declaration is the carrier
                // authority at the expression-template boundary (the
                // checked projection site); multi-output functions
                // pack a record and never carry a raw union result.
                let output = main
                    .items
                    .iter()
                    .find_map(|item| match item {
                        emath_core::tree::Item::Declaration(decl) if &decl.name == name => {
                            let outputs =
                                emath_exec_ir::constructor_emir::section_typed_fields(decl, "outputs");
                            let (output, ty) = outputs.first()?;
                            let signature =
                                emath_exec_ir::constructor_emir::constructor_type_signature(
                                    ty, &objects,
                                )?;
                            Some((output.clone(), signature))
                        }
                        _ => None,
                    })
                    .map(|(name, signature)| (name, signature));
                match emit_constructor_entry(&lowered.program, name, &inputs, output.as_ref().map(|(name, signature)| (name.as_str(), signature.as_str())), &records) {
                    Ok(body) => {
                        rust.push_str(&format!("// function `{name}`\n"));
                        rust.push_str(&body);
                        rust.push('\n');
                    }
                    Err(error) => {
                        runnable = false;
                        unresolved.push(error.to_string());
                        rust.push_str(&format!("// `{name}` is not marked runnable: {error}\n"));
                    }
                }
            }
            Err(error) => {
                runnable = false;
                unresolved.push(error);
            }
        }
    }
    if needs_module_table {
        let globals = module_table
            .iter()
            .map(|(name, opaque, _)| format!("({name:?}, {opaque})"))
            .collect::<Vec<_>>()
            .join(", ");
        let stamps = module_table
            .iter()
            .map(|(name, _, stamp)| format!("({name:?}, {stamp})"))
            .collect::<Vec<_>>()
            .join(", ");
        rust.push_str(&format!(
            "#[allow(dead_code)]\nstatic __EMATH_MODULE_TABLE: emath_rt::code_tree::ModuleTable = emath_rt::code_tree::ModuleTable {{ globals: &[{globals}], stamps: &[{stamps}] }};\n\n"
        ));
    }
    // The definition table (bead emath-quote-body-defs-trto7): the
    // module's function bodies as DATA - the same bodies the
    // compiled entries come from, distilled by the same lowering, so
    // a `quote.body` unfold and a compiled call cannot disagree (the
    // dual-representation law). Built once at first use; a
    // transparent body outside the distilled subset stays `None` and
    // the runtime unfold refuses by name.
    if needs_definition_table {
        let rows = emath_exec_ir::constructor_layer::module_definition_table(&tree);
        let rows = rows
            .iter()
            .map(|(name, opaque, body)| {
                let body = match body {
                    Some(tree) => format!("Some({})", crate::codegen_render::tree_expr(tree)),
                    None => "None".to_string(),
                };
                format!(
                    "emath_rt::code_tree::DefinitionRow {{ name: String::from({name:?}), opaque: {opaque}, body: {body} }}"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        rust.push_str(&format!(
            "#[allow(dead_code)]\nstatic __EMATH_DEFINITIONS: std::sync::LazyLock<emath_rt::code_tree::DefinitionTable> = std::sync::LazyLock::new(|| emath_rt::code_tree::DefinitionTable {{ rows: vec![{rows}] }});\n\n"
        ));
    }
    if rust.contains("emath_rt::") {
        // Self-containment law (emath-rt's `SOURCE` embed, the same
        // law the SIR lane followed): the runtime ships inside the
        // artifact as `mod emath_rt`, so the emitted crate builds
        // with zero external dependencies.
        rust.insert_str(
            EMITTED_HEADER.len(),
            &format!(
                "#[allow(dead_code)]\npub mod emath_rt {{\n{}\n}}\n\n",
                emath_rt::SOURCE
            ),
        );
    }
    // A buildable crate needs its manifest; the runtime is embedded,
    // so the dependency list stays empty and the artifact compiles
    // offline, outside this repository.
    let stem = spec_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("emath_build");
    let mut package_name: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if package_name.is_empty() || package_name.starts_with(|c: char| c.is_ascii_digit()) {
        package_name = format!("emath_{package_name}");
    }
    let manifest = format!(
        "# Generated by `emath build`; the emath runtime is embedded\n# as `mod emath_rt` in src/lib.rs - no dependencies.\n[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n"
    );
    Ok(ConstructorCrateEmission {
        lib: rust,
        manifest,
        package_name,
        runnable,
        functions,
        unresolved,
        records,
    })
}
