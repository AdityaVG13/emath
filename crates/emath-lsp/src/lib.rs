#![forbid(unsafe_code)]

//! Minimal language-server-protocol skeleton for emath (P11).
//!
//! Implements a std-only, deterministic LSP server slice:
//!
//! - base-protocol framing (`Content-Length` headers, JSON-RPC messages);
//! - `initialize` capabilities with incremental text synchronization;
//! - `textDocument/didOpen` / `didChange` with incremental edits and
//!   publishDiagnostics computed by the real compiler session
//!   (`emath_sema::CompilerSession::check_owned`) — LSP and CLI agree on
//!   diagnostics because they share the same admission path;
//! - skeleton `completion` (Phase 1 grammar keywords), `hover` (keyword
//!   documentation) and `signatureHelp` (null response);
//! - typed refusal for unknown methods (`-32601`), deterministic writes.
//!
//! No network, filesystem watch, or third-party dependencies.

pub mod json;
pub mod protocol;
pub mod server;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::JsonValue;

    fn frame(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    fn initialize_then_exit(input: &mut Vec<u8>) -> Vec<u8> {
        let mut output = Vec::new();
        let mut cursor = std::io::Cursor::new(input);
        let _ = run(&mut cursor, &mut output);
        output
    }

    #[test]
    fn clean_shutdown_exit_returns_zero() {
        let mut input = Vec::new();
        input.extend(frame(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ));
        input.extend(frame(r#"{"jsonrpc":"2.0","method":"initialized"}"#));
        input.extend(frame(
            r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
        ));
        input.extend(frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        let mut output = Vec::new();
        assert_eq!(run(&mut std::io::Cursor::new(&mut input), &mut output), 0);
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("\"capabilities\""));
        assert!(text.contains("\"result\":null"));
    }

    #[test]
    fn exit_without_shutdown_is_abnormal() {
        let mut input = Vec::new();
        input.extend(frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        let mut output = Vec::new();
        assert_eq!(run(&mut std::io::Cursor::new(&mut input), &mut output), 1);
    }

    #[test]
    fn malformed_json_is_parse_error() {
        let mut input = Vec::new();
        input.extend(frame(r#"{"jsonrpc":"2.0","id":1,"method":12}"#));
        let mut output = Vec::new();
        assert_eq!(run(&mut std::io::Cursor::new(&mut input), &mut output), 1);
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("-32700"));
    }

    #[test]
    fn responses_are_byte_identical_across_runs() {
        let mut first = Vec::new();
        first.extend(frame(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ));
        first.extend(frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        let mut second = first.clone();
        let out_first = initialize_then_exit(&mut first);
        let out_second = initialize_then_exit(&mut second);
        assert_eq!(out_first, out_second);
        assert!(String::from_utf8_lossy(&out_first).contains("\"capabilities\""));
    }

    #[test]
    fn client_key_order_does_not_affect_response_bytes() {
        let ordered = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"a":1,"b":2}}"#;
        let shuffled =
            r#"{"b":2,"a":1,"jsonrpc":"2.0","id":1,"method":"initialize","params":{"b":2,"a":1}}"#;
        let mut first = Vec::new();
        first.extend(frame(ordered));
        first.extend(frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        let mut second = Vec::new();
        second.extend(frame(shuffled));
        second.extend(frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        assert_eq!(
            initialize_then_exit(&mut first),
            initialize_then_exit(&mut second)
        );
    }

    #[test]
    fn json_value_round_trips() {
        let text = r#"{"z":1,"a":[true,null,"x"],"f":1.5}"#;
        let value = JsonValue::parse(text).unwrap();
        assert_eq!(value, JsonValue::parse(&value.render()).unwrap());
    }
}
