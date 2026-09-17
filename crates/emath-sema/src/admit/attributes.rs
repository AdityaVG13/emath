//! Item-attribute admission gates (ELP governance), restored from the
//! pre-contraction recognition layer.
//!
//! `@experimental` / `@capabilities` lane gating, `@units_profile`
//! ladders, and `@significant_figures` precision contracts. Experimental
//! syntax must never compile silently in a stable package: every gate
//! below is a typed refusal or a declared admit.

use emath_core::tree::Item;
use emath_core::Diagnostics;
use std::collections::BTreeSet;

// ---- item attributes and the experimental lane ----------------------------

/// `@experimental` items require the source file to declare the
/// `experimental-syntax` capability (ELP experimental lane; see
/// `elps/README.md`).
const E_EXPERIMENTAL_CAPABILITY: &str = "E-PKG-064";
/// Unknown capability key in `@capabilities(...)` (nothing is dropped).
const E_UNKNOWN_CAPABILITY: &str = "E-PKG-065";
/// Constitution: the `experimental` attribute takes no arguments.
const E_ATTRIBUTE_ARG: &str = "E-SYN-117";
/// An attribute the front-end does not understand is refused, never
/// silently ignored.
const E_UNKNOWN_ATTRIBUTE: &str = "E-SYN-118";

/// Post-pass over item attributes (ELP governance, file scope).
///
/// Capability keys: `experimental-syntax` is the only declared key today.
/// `@experimental` on any item requires the file to declare it via
/// `@capabilities(experimental-syntax)` on any item; unknown attributes
/// and unknown capability keys are typed refusals so no syntax is ever
/// silently dropped.
pub fn admit_capability_gates(tree: &emath_core::tree::SyntaxTree, diagnostics: &mut Diagnostics) {
    let mut capabilities: BTreeSet<String> = BTreeSet::new();
    let mut experimental: Vec<(&str, emath_core::Span)> = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        for attribute in &decl.attributes {
            if attribute.name == "capabilities" {
                for arg in &attribute.args {
                    let key = unquote_attribute_arg(arg);
                    if key == "experimental-syntax" {
                        capabilities.insert(key);
                    } else {
                        diagnostics.error(
                            E_UNKNOWN_CAPABILITY,
                            format!("unknown capability `{key}` in `@capabilities` (declared: experimental-syntax)"),
                            attribute.source,
                        );
                    }
                }
            } else if attribute.name == "experimental" {
                if !attribute.args.is_empty() {
                    diagnostics.error(
                        E_ATTRIBUTE_ARG,
                        "the `experimental` attribute takes no arguments",
                        attribute.source,
                    );
                }
                experimental.push((&decl.name, decl.source));
            } else if attribute.name == "significant_figures" {
                diagnostics.error(
                    "E-KIND-GONE",
                    "significant-figure contracts are not constructor surface; write an ordinary rounding function",
                    attribute.source,
                );
            } else if attribute.name == "units_profile" {
                diagnostics.error(
                    "E-KIND-GONE",
                    "units-profile catalogs are not constructor surface; write an ordinary module if you need units",
                    attribute.source,
                );
            } else {
                diagnostics.error(
                    E_UNKNOWN_ATTRIBUTE,
                    format!("unknown attribute `@{}`", attribute.name),
                    attribute.source,
                );
            }
        }
    }
    if !capabilities.contains("experimental-syntax") {
        for (name, source) in experimental {
            diagnostics.error(
                E_EXPERIMENTAL_CAPABILITY,
                format!(
                    "`@experimental` on `{name}` requires the `experimental-syntax` capability; \
                     declare `@capabilities(experimental-syntax)` on an item in this file"
                ),
                source,
            );
        }
    }
}

/// Attribute args are stored as canonical source text; string literals
/// carry their quotes. Return the bare value.
fn unquote_attribute_arg(arg: &str) -> String {
    if arg.len() >= 2 && arg.starts_with('"') && arg.ends_with('"') {
        arg[1..arg.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        arg.to_string()
    }
}
