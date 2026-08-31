//! `emath-cli` lsp server state-machine tests (migrated from
//! `crates/emath-cli/src/lsp/server.rs`).

use emath_cli::lsp::json::JsonValue;
use emath_cli::lsp::protocol::RpcMessage;
use emath_cli::lsp::server::ServerState;
use std::collections::BTreeMap;

fn string_field(fields: BTreeMap<String, JsonValue>) -> JsonValue {
    JsonValue::Object(fields)
}

/// Byte-offset semantics on a glyph-bearing line: `character` counts
/// UTF-8 bytes when `positionEncoding: utf-8` is advertised, so
/// `⋈` occupies three character units, not one (SURF-0009). Exercised
/// through the public didOpen/didChange surface because the private
/// `range_offsets`/`offset_range` helpers are crate internals.
#[test]
fn range_offsets_use_utf8_byte_characters_on_glyph_lines() {
    let text = "ab ⋈ cd\n";
    // a(0) b(1) ' '(2) ⋈(3..5) ' '(6) c(7) d(8): a didChange range
    // {line 0, characters 7..8} must address exactly byte 7 (`c`).
    let uri = "file:///g.emath";
    let mut state = ServerState::new();
    let mut output = Vec::new();
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didOpen".into(),
                params: string_field(BTreeMap::from([(
                    "textDocument".into(),
                    JsonValue::Object(BTreeMap::from([
                        ("uri".into(), JsonValue::String(uri.into())),
                        ("text".into(), JsonValue::String(text.into())),
                    ])),
                )])),
            },
            &mut output,
        )
        .expect("didOpen handled");
    let change_params = JsonValue::Object(BTreeMap::from([
        (
            "textDocument".into(),
            JsonValue::Object(BTreeMap::from([("uri".into(), JsonValue::String(uri.into()))])),
        ),
        (
            "contentChanges".into(),
            JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
                (
                    "range".into(),
                    JsonValue::Object(BTreeMap::from([
                        (
                            "start".into(),
                            JsonValue::Object(BTreeMap::from([
                                ("line".into(), JsonValue::Number(0)),
                                ("character".into(), JsonValue::Number(7)),
                            ])),
                        ),
                        (
                            "end".into(),
                            JsonValue::Object(BTreeMap::from([
                                ("line".into(), JsonValue::Number(0)),
                                ("character".into(), JsonValue::Number(8)),
                            ])),
                        ),
                    ])),
                ),
                ("text".into(), JsonValue::String("X".into())),
            ]))]),
        ),
    ]));
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didChange".into(),
                params: change_params,
            },
            &mut output,
        )
        .expect("didChange handled");
    let document = state.documents.get(uri).expect("document present");
    assert_eq!(
        document.text, "ab ⋈ Xd\n",
        "the byte-offset edit must replace exactly byte 7 (`c`); a UTF-16 \
         encoding would count character 5"
    );
}

/// The initialize response advertises the encoding the server
/// actually implements (UTF-8 bytes), never a silent UTF-16 default.
#[test]
fn initialize_advertises_utf8_position_encoding() {
    let mut state = ServerState::new();
    let mut output = Vec::new();
    state
        .handle(
            &RpcMessage {
                id: Some(1),
                method: "initialize".into(),
                params: string_field(BTreeMap::new()),
            },
            &mut output,
        )
        .expect("initialize handled");
    let body = String::from_utf8(output).expect("utf-8 output");
    assert!(body.contains("\"positionEncoding\":\"utf-8\""), "{body}");
}

/// A glyph-bearing incremental edit round-trips through byte
/// offsets: the range {line 2, chars 21..27} selects `⊛ ζ` (bytes)
/// and the replacement lands exactly on it.
#[test]
fn glyph_bearing_did_change_round_trips_byte_offsets() {
    let original = "emath custom G:\n    bbs:\n        ⧖(a ⋈ b) ⊛ ζ\n";
    let mut state = ServerState::new();
    let mut output = Vec::new();
    let uri = "file:///g.emath";
    let open_params = JsonValue::Object(BTreeMap::from([(
        "textDocument".into(),
        JsonValue::Object(BTreeMap::from([
            ("uri".into(), JsonValue::String(uri.into())),
            ("text".into(), JsonValue::String(original.into())),
        ])),
    )]));
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didOpen".into(),
                params: open_params,
            },
            &mut output,
        )
        .expect("didOpen handled");
    let change_params = JsonValue::Object(BTreeMap::from([
        (
            "textDocument".into(),
            JsonValue::Object(BTreeMap::from([(
                "uri".into(),
                JsonValue::String(uri.into()),
            )])),
        ),
        (
            "contentChanges".into(),
            JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
                (
                    "range".into(),
                    JsonValue::Object(BTreeMap::from([
                        (
                            "start".into(),
                            JsonValue::Object(BTreeMap::from([
                                ("line".into(), JsonValue::Number(2)),
                                ("character".into(), JsonValue::Number(21)),
                            ])),
                        ),
                        (
                            "end".into(),
                            JsonValue::Object(BTreeMap::from([
                                ("line".into(), JsonValue::Number(2)),
                                ("character".into(), JsonValue::Number(27)),
                            ])),
                        ),
                    ])),
                ),
                ("text".into(), JsonValue::String("⋈ ζ".into())),
            ]))]),
        ),
    ]));
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didChange".into(),
                params: change_params,
            },
            &mut output,
        )
        .expect("didChange handled");
    let document = state.documents.get(uri).expect("document present");
    assert_eq!(
        document.text, "emath custom G:\n    bbs:\n        ⧖(a ⋈ b) ⋈ ζ\n",
        "the byte-offset edit must replace exactly `⊛ ζ`"
    );
}

/// Hover at byte offset 0 must resolve a leading keyword. The previous
/// `word_at` early-return on `offset == 0` made `emath` at the file start
/// unreachable from hover.
#[test]
fn hover_at_byte_zero_resolves_leading_keyword() {
    let uri = "file:///hover0.emath";
    let text = "emath custom X:\n";
    let mut state = ServerState::new();
    let mut output = Vec::new();
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didOpen".into(),
                params: string_field(BTreeMap::from([(
                    "textDocument".into(),
                    JsonValue::Object(BTreeMap::from([
                        ("uri".into(), JsonValue::String(uri.into())),
                        ("text".into(), JsonValue::String(text.into())),
                    ])),
                )])),
            },
            &mut output,
        )
        .expect("didOpen handled");
    output.clear();
    state
        .handle(
            &RpcMessage {
                id: Some(7),
                method: "textDocument/hover".into(),
                params: JsonValue::Object(BTreeMap::from([
                    (
                        "textDocument".into(),
                        JsonValue::Object(BTreeMap::from([(
                            "uri".into(),
                            JsonValue::String(uri.into()),
                        )])),
                    ),
                    (
                        "position".into(),
                        JsonValue::Object(BTreeMap::from([
                            ("line".into(), JsonValue::Number(0)),
                            ("character".into(), JsonValue::Number(0)),
                        ])),
                    ),
                ])),
            },
            &mut output,
        )
        .expect("hover handled");
    let body = String::from_utf8(output).expect("utf-8 output");
    assert!(
        body.contains("**emath**"),
        "hover at character 0 must resolve the leading keyword; got {body}"
    );
}

/// `publishDiagnostics` must use `primary.end`, not a zero-width caret at
/// `primary.start`. A token-length syntax refusal must advertise end > start.
#[test]
fn publish_diagnostics_uses_primary_end() {
    let uri = "file:///diag-end.emath";
    // Incomplete declaration: compiler attaches a non-empty primary span.
    let text = "emath custom\n";
    let mut state = ServerState::new();
    let mut output = Vec::new();
    state
        .handle(
            &RpcMessage {
                id: None,
                method: "textDocument/didOpen".into(),
                params: string_field(BTreeMap::from([(
                    "textDocument".into(),
                    JsonValue::Object(BTreeMap::from([
                        ("uri".into(), JsonValue::String(uri.into())),
                        ("text".into(), JsonValue::String(text.into())),
                    ])),
                )])),
            },
            &mut output,
        )
        .expect("didOpen handled");
    let body = String::from_utf8(output).expect("utf-8 output");
    assert!(
        body.contains("publishDiagnostics"),
        "didOpen must publish diagnostics: {body}"
    );
    assert!(
        range_with_nonzero_width(&body),
        "at least one diagnostic range must cover primary.end (end != start); got {body}"
    );
}

/// True when some LSP range in `body` has distinct start and end positions.
///
/// `JsonValue` renders object keys sorted, so a range is
/// `"range":{"end":{"character":E,"line":EL},"start":{"character":S,"line":SL}}`.
fn range_with_nonzero_width(body: &str) -> bool {
    const MARKER: &str = "\"range\":{\"end\":{\"character\":";
    let mut rest = body;
    while let Some(idx) = rest.find(MARKER) {
        let slice = &rest[idx + MARKER.len()..];
        let Some(end_char) = parse_leading_i64(slice) else {
            break;
        };
        let Some(after_end_char) = slice.split_once(",\"line\":").map(|(_, t)| t) else {
            rest = &rest[idx + 1..];
            continue;
        };
        let Some(end_line) = parse_leading_i64(after_end_char) else {
            rest = &rest[idx + 1..];
            continue;
        };
        let Some(after_start) = after_end_char.split_once("\"start\":{\"character\":") else {
            rest = &rest[idx + 1..];
            continue;
        };
        let Some(start_char) = parse_leading_i64(after_start.1) else {
            rest = &rest[idx + 1..];
            continue;
        };
        let Some(after_start_char) = after_start.1.split_once(",\"line\":").map(|(_, t)| t) else {
            rest = &rest[idx + 1..];
            continue;
        };
        let Some(start_line) = parse_leading_i64(after_start_char) else {
            rest = &rest[idx + 1..];
            continue;
        };
        if (end_line, end_char) != (start_line, start_char) {
            return true;
        }
        rest = &rest[idx + 1..];
    }
    false
}

fn parse_leading_i64(text: &str) -> Option<i64> {
    let end = text
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '-'))
        .unwrap_or(text.len());
    text[..end].parse().ok()
}
