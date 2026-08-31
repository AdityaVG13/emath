#![forbid(unsafe_code)]

//! Minimal language-server-protocol skeleton for emath.
//!
//! Deterministic, std-only LSP slice: base framing, incremental text sync, and
//! diagnostics via the real compiler session (`emath_sema::CompilerSession`).
//! The optional `async-runtime` feature adds the `lab` entry and the async
//! `transport` lane; the blocking run loop is untouched.

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
