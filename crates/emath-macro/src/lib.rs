//! `emath!` procedural macro.
//!
//! Small-source convenience: `emath! { "... .emath source ..." }` lowers an
//! inline source literal to the *same* compiler path. The macro parses its
//! input as tokens (never concatenates strings), validates that it is a
//! single string literal, and expands to
//! `::emath_builder::MacroExpansion::from_literals(source, identity)`,
//! which hosts pass to `emath_builder::build_from_source` (the exact
//! `emath-build` artifact pipeline).
//!
//! Security documentation:
//! - The expansion embeds the literal source into the host binary. Treat
//!   `.emath` source as code: never paste untrusted input into `emath!`.
//! - Malformed input (non-literal, unescaped quotes) fails compilation
//!   with a typed `E-CODEGEN-011` message; nothing is generated.
//! - The macro performs no I/O and touches no files.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

/// Expands an inline `.emath` source literal into a
/// `::emath_builder::MacroExpansion` value.
#[proc_macro]
pub fn emath(input: TokenStream) -> TokenStream {
    let text = input.to_string();
    match emath_builder::macro_expand(&text) {
        Ok(expansion) => {
            let source = proc_macro::Literal::string(&expansion.source);
            let identity = proc_macro::Literal::string(&expansion.identity);
            format!("::emath_builder::MacroExpansion::from_literals({source}, {identity})")
                .parse()
                .unwrap_or_else(|_| {
                    compile_error("E-CODEGEN-011: internal expansion parse failure")
                })
        }
        Err(error) => compile_error(&format!("{}: {}", error.code, error.message)),
    }
}

/// Emits a compile-time configuration error with a stable code.
fn compile_error(message: &str) -> TokenStream {
    let literal = proc_macro::Literal::string(message);
    format!("::core::compile_error!({literal})")
        .parse()
        .unwrap_or_else(|_| TokenStream::new())
}
