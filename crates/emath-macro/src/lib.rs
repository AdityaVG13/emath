//! `emath!` procedural macro.
//!
//! Expands an inline `.emath` source literal (parsed as tokens, never
//! concatenated) to `::emath_builder::MacroExpansion::from_literals`.
//! Security: the literal is embedded in the host binary — never paste
//! untrusted input; malformed input fails compilation with `E-CODEGEN-011`.
//! The macro performs no I/O and touches no files.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

/// Expands an inline `.emath` source literal into a
/// `::emath_builder::MacroExpansion` value.
#[proc_macro]
pub fn emath(input: TokenStream) -> TokenStream {
    let text = input.to_string();
    match emath_builder::macro_expand(&text) {
        Ok(expansion) => {
            // Compile-time honesty: the literal must be valid `.emath`
            // source, not just a quoted string (rustdoc: malformed input
            // fails compilation with E-CODEGEN-011).
            let (_, parse_diagnostics) = emath_syntax::parse_str(&expansion.source);
            if parse_diagnostics.has_errors() {
                let first = parse_diagnostics.errors().next().map_or_else(
                    || "source does not parse".to_string(),
                    |d| d.message.clone(),
                );
                return compile_error(&format!("E-CODEGEN-011: {first}"));
            }
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
