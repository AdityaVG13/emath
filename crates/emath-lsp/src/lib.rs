#![forbid(unsafe_code)]

//! Minimal language-server-protocol skeleton for emath.
//!
//! Implements a std-only, deterministic LSP server slice:
//!
//! - base-protocol framing (`Content-Length` headers, JSON-RPC messages);
//! - `initialize` capabilities with incremental text synchronization;
//! - `textDocument/didOpen` / `didChange` with incremental edits and
//!   publishDiagnostics computed by the real compiler session
//!   (`emath_sema::CompilerSession::check_owned`): LSP and CLI agree on
//!   diagnostics because they share the same admission path;
//! - skeleton `completion` (Phase 1 grammar keywords), `hover` (keyword
//!   documentation) and `signatureHelp` (null response);
//! - typed refusal for unknown methods (`-32601`), deterministic writes.
//!
//! No network, filesystem watch, or third-party dependencies in the default
//! build. The optional `async-runtime` feature (FrankenStack asupersync
//! cutover) adds the `lab` module, a Cx/lab-runtime entry for deterministic
//! tests, and the async `transport` lane (pass 3: stdio JSON-RPC framing on
//! Cx); the blocking run loop below is not touched by them.

pub mod json;
pub mod protocol;
pub mod server;

#[cfg(feature = "async-runtime")]
pub mod lab;
#[cfg(feature = "async-runtime")]
pub mod transport;

use std::io::{Read, Write};

use protocol::{read_message, write_error};

/// Runs the server loop over `input`/`output` until EOF.
///
/// Returns 0 if the client performed `shutdown` before `exit`, 1 otherwise
/// (the LSP exit-code contract: a client that skips shutdown is abnormal).
pub fn run(input: &mut impl Read, output: &mut impl Write) -> u8 {
    let mut state = server::ServerState::new();
    loop {
        match read_message(input) {
            Ok(Some(message)) => {
                if message.method == "exit" {
                    return u8::from(!state.shutdown);
                }
                if state.handle(&message, output).is_err() {
                    return 1;
                }
            }
            Ok(None) => return u8::from(!state.shutdown),
            Err(error) => {
                let _ = write_error(output, None, -32700, &error);
                return 1;
            }
        }
    }
}
