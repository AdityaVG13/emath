//! LSP base-protocol framing: `Content-Length` headers + JSON-RPC messages.
//! Deterministic writes; malformed input yields typed parse errors.

use std::io::{Read, Write};

use crate::lsp::json::{JsonValue, parse_request};

/// A decoded JSON-RPC message.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcMessage {
    /// Request id; `None` for notifications.
    pub id: Option<i64>,
    /// Method name.
    pub method: String,
    /// Parameters (may be null).
    pub params: JsonValue,
}

/// Reads one framed message; `Ok(None)` at clean EOF before any header.
pub fn read_message(reader: &mut impl Read) -> Result<Option<RpcMessage>, String> {
    let mut length = None;
    loop {
        let Some(line) = read_header_line(reader)? else {
            return Ok(None);
        };
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("malformed header line".into());
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid Content-Length".to_string())?,
            );
        }
    }
    let length = length.ok_or("missing Content-Length header")?;
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("short body: {error}"))?;
    let text = String::from_utf8(body).map_err(|_| "body is not utf-8".to_string())?;
    let (id, method, params) = parse_request(&text)?;
    Ok(Some(RpcMessage { id, method, params }))
}

/// Reads one header line (max 4096 bytes); `Ok(None)` at EOF before any
/// content, `Err` at EOF inside a line.
fn read_header_line(reader: &mut impl Read) -> Result<Option<String>, String> {
    let mut buffer = String::new();
    let mut saw_byte = false;
    while buffer.len() < 4096 {
        let mut byte = [0u8; 1];
        let read = reader
            .read(&mut byte)
            .map_err(|error| format!("read error: {error}"))?;
        if read == 0 {
            if saw_byte {
                return Err("unexpected EOF inside header".into());
            }
            return Ok(None);
        }
        saw_byte = true;
        if byte[0] == b'\n' {
            return Ok(Some(buffer));
        }
        if byte[0] != b'\r' {
            buffer.push(char::from(byte[0]));
        }
    }
    Err("header line too long".into())
}

/// Writes a JSON-RPC response with a result payload.
pub fn write_response(output: &mut impl Write, id: i64, result: &JsonValue) -> std::io::Result<()> {
    let payload = JsonValue::Object(
        [
            ("jsonrpc".into(), JsonValue::String("2.0".into())),
            ("id".into(), JsonValue::Number(id)),
            ("result".into(), result.clone()),
        ]
        .into(),
    );
    write_frame(output, &payload.render())
}

/// Writes a JSON-RPC error response.
pub fn write_error(
    output: &mut impl Write,
    id: Option<i64>,
    code: i64,
    message: &str,
) -> std::io::Result<()> {
    let id_value = match id {
        Some(id) => JsonValue::Number(id),
        None => JsonValue::Null,
    };
    let payload = JsonValue::Object(
        [
            ("jsonrpc".into(), JsonValue::String("2.0".into())),
            ("id".into(), id_value),
            (
                "error".into(),
                JsonValue::Object(
                    [
                        ("code".into(), JsonValue::Number(code)),
                        ("message".into(), JsonValue::String(message.into())),
                    ]
                    .into(),
                ),
            ),
        ]
        .into(),
    );
    write_frame(output, &payload.render())
}

/// Writes a JSON-RPC notification (no id).
pub fn write_notification(
    output: &mut impl Write,
    method: &str,
    params: &JsonValue,
) -> std::io::Result<()> {
    let payload = JsonValue::Object(
        [
            ("jsonrpc".into(), JsonValue::String("2.0".into())),
            ("method".into(), JsonValue::String(method.into())),
            ("params".into(), params.clone()),
        ]
        .into(),
    );
    write_frame(output, &payload.render())
}

/// Writes the `Content-Length` framed body.
fn write_frame(output: &mut impl Write, body: &str) -> std::io::Result<()> {
    write!(output, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    output.flush()
}
