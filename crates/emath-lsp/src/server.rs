//! LSP server state machine: documents, incremental diagnostics via the
//! real compiler session, and skeleton completion/hover/signature services.
//! Everything is deterministic and std-only.

use std::collections::BTreeMap;

use emath_core::Severity;
use emath_sema::session::CompilerSession;

use crate::json::JsonValue;
use crate::protocol::{write_error, write_notification, write_response, RpcMessage};
use std::io::Write;

/// One document held by the server.
#[derive(Debug, Clone)]
pub struct Document {
    /// URI as sent by the client.
    pub uri: String,
    /// Current text after incremental updates.
    pub text: String,
}

/// Server state; deterministic across identical message sequences.
pub struct ServerState {
    /// Workspace documents by URI.
    pub documents: BTreeMap<String, Document>,
    /// Whether `shutdown` was requested (exit code contract).
    pub shutdown: bool,
}

/// Keywords offered by completion, with hover documentation.
const KEYWORDS: [(&str, &str); 14] = [
    (
        "emath",
        "package declaration keyword: `emath custom <Name> as <kind>`",
    ),
    (
        "custom",
        "declaration kind; Phase 1 accepts `custom`, `function`, `policy`-style bodies",
    ),
    ("function", "function-style declaration"),
    ("inputs", "section: typed inputs"),
    ("outputs", "section: typed outputs"),
    ("definitions", "section: scalar definitions"),
    ("state", "section: constructor state"),
    ("constructor", "checked constructor section"),
    ("require", "constructor invariant / evidence requirement"),
    ("define", "derived method definition"),
    ("goal", "compile/differentiate request"),
    ("evidence", "evidence requirements section"),
    ("host", "Rust host interface section"),
    ("tests", "example-test section"),
];

impl ServerState {
    /// Creates a fresh server state with default limits.
    #[must_use]
    pub fn new() -> Self {
        Self {
            documents: BTreeMap::new(),
            shutdown: false,
        }
    }

    /// Handles one message and writes all outbound responses/notifications.
    pub fn handle(&mut self, message: &RpcMessage, output: &mut impl Write) -> std::io::Result<()> {
        match message.method.as_str() {
            "initialize" => {
                let capabilities = JsonValue::Object(
                    [
                        (
                            "textDocumentSync".into(),
                            JsonValue::Number(2), // incremental
                        ),
                        (
                            "completionProvider".into(),
                            JsonValue::Object(
                                [(
                                    "triggerCharacters".into(),
                                    JsonValue::Array(vec![
                                        JsonValue::String(".".into()),
                                        JsonValue::String(":".into()),
                                    ]),
                                )]
                                .into(),
                            ),
                        ),
                        ("hoverProvider".into(), JsonValue::Bool(true)),
                        (
                            "signatureHelpProvider".into(),
                            JsonValue::Object(
                                [(
                                    "triggerCharacters".into(),
                                    JsonValue::Array(vec![
                                        JsonValue::String("(".into()),
                                        JsonValue::String(",".into()),
                                    ]),
                                )]
                                .into(),
                            ),
                        ),
                    ]
                    .into(),
                );
                let info = JsonValue::Object(
                    [
                        ("name".into(), JsonValue::String("emath-lsp".into())),
                        ("version".into(), JsonValue::String("0.1.0".into())),
                    ]
                    .into(),
                );
                let result = JsonValue::Object(
                    [
                        ("capabilities".into(), capabilities),
                        ("serverInfo".into(), info),
                    ]
                    .into(),
                );
                if let Some(id) = message.id {
                    write_response(output, id, &result)?;
                }
            }
            "shutdown" => {
                self.shutdown = true;
                if let Some(id) = message.id {
                    write_response(output, id, &JsonValue::Null)?;
                }
            }
            "exit" => return Ok(()),
            "initialized" | "$/setTrace" | "textDocument/didClose" => {
                if message.method == "textDocument/didClose" {
                    if let Some(uri) = document_uri(&message.params) {
                        self.documents.remove(&uri);
                    }
                }
            }
            "textDocument/didOpen" => {
                self.handle_did_open(&message.params);
                self.publish_diagnostics(output)?;
            }
            "textDocument/didChange" => {
                self.handle_did_change(&message.params);
                self.publish_diagnostics(output)?;
            }
            "textDocument/completion" => {
                if let Some(id) = message.id {
                    write_response(output, id, &completion_result())?;
                }
            }
            "textDocument/hover" => {
                if let Some(id) = message.id {
                    let hover = self.hover_at(&message.params);
                    write_response(output, id, &hover)?;
                }
            }
            "textDocument/signatureHelp" => {
                if let Some(id) = message.id {
                    write_response(output, id, &JsonValue::Null)?;
                }
            }
            other => {
                if let Some(id) = message.id {
                    write_error(
                        output,
                        Some(id),
                        -32601,
                        &format!("method not found: {other}"),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn handle_did_open(&mut self, params: &JsonValue) {
        let Some(text_document) = params.get("textDocument") else {
            return;
        };
        let Some(uri) = text_document.get_str("uri") else {
            return;
        };
        let text = text_document
            .get_str("text")
            .map(str::to_string)
            .unwrap_or_default();
        self.documents.insert(
            uri.into(),
            Document {
                uri: uri.into(),
                text,
            },
        );
    }

    fn handle_did_change(&mut self, params: &JsonValue) {
        let Some(text_document) = params.get("textDocument") else {
            return;
        };
        let Some(uri) = text_document.get_str("uri") else {
            return;
        };
        let Some(document) = self.documents.get_mut(uri) else {
            return;
        };
        let Some(changes) = params.get("contentChanges") else {
            return;
        };
        let JsonValue::Array(changes) = changes else {
            return;
        };
        for change in changes {
            let Some(text) = change.get_str("text") else {
                continue;
            };
            if let Some(range) = change.get("range") {
                if let Some((start, end)) = range_offsets(range, &document.text) {
                    document.text.replace_range(start..end, text);
                }
            } else {
                document.text.clear();
                document.text.push_str(text);
            }
        }
    }

    fn publish_diagnostics(&self, output: &mut impl Write) -> std::io::Result<()> {
        let documents = &self.documents;
        for (uri, document) in documents {
            // Fresh session per check: the source store deduplicates by
            // name, so a shared session would re-check stale text.
            let mut session = CompilerSession::new(emath_core::limits::Limits::default());
            let result = session.check_owned(uri, &document.text);
            let mut diagnostics = Vec::new();
            for item in result.diagnostics.items() {
                let severity = if matches!(item.severity, Severity::Error) {
                    1
                } else {
                    2
                };
                let (start, end) = offset_range(&document.text, item.primary.start);
                let range = JsonValue::Object(
                    [
                        ("start".into(), position(&start)),
                        ("end".into(), position(&end)),
                    ]
                    .into(),
                );
                diagnostics.push(JsonValue::Object(
                    [
                        ("range".into(), range),
                        ("severity".into(), JsonValue::Number(severity)),
                        ("code".into(), JsonValue::String(item.code.into())),
                        ("message".into(), JsonValue::String(item.message.clone())),
                        ("source".into(), JsonValue::String("emath".into())),
                    ]
                    .into(),
                ));
            }
            let params = JsonValue::Object(
                [
                    ("uri".into(), JsonValue::String(uri.clone())),
                    ("diagnostics".into(), JsonValue::Array(diagnostics)),
                ]
                .into(),
            );
            write_notification(output, "textDocument/publishDiagnostics", &params)?;
        }
        Ok(())
    }

    fn hover_at(&self, params: &JsonValue) -> JsonValue {
        let Some(document) = params.get("textDocument") else {
            return JsonValue::Null;
        };
        let Some(uri) = document.get_str("uri") else {
            return JsonValue::Null;
        };
        let Some(text) = self.documents.get(uri).map(|doc| doc.text.as_str()) else {
            return JsonValue::Null;
        };
        let Some(position) = params.get("position") else {
            return JsonValue::Null;
        };
        let Some(offset) = position_offsets(position, &line_starts(text)) else {
            return JsonValue::Null;
        };
        let Some(word) = word_at(text, offset) else {
            return JsonValue::Null;
        };
        for (keyword, explanation) in KEYWORDS {
            if keyword == word {
                return JsonValue::Object(
                    [(
                        "contents".into(),
                        JsonValue::Object(
                            [
                                ("kind".into(), JsonValue::String("markdown".into())),
                                (
                                    "value".into(),
                                    JsonValue::String(format!("**{keyword}**: {explanation}")),
                                ),
                            ]
                            .into(),
                        ),
                    )]
                    .into(),
                );
            }
        }
        JsonValue::Null
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Completion items for the Phase 1 grammar keywords.
fn completion_result() -> JsonValue {
    let items = KEYWORDS
        .iter()
        .map(|(label, _)| {
            JsonValue::Object(
                [
                    ("label".into(), JsonValue::String((*label).into())),
                    ("kind".into(), JsonValue::Number(14)), // Keyword
                ]
                .into(),
            )
        })
        .collect();
    JsonValue::Object([("items".into(), JsonValue::Array(items))].into())
}

/// Extracts the affected document URI from params.
fn document_uri(params: &JsonValue) -> Option<String> {
    params
        .get("textDocument")
        .and_then(|document| document.get_str("uri"))
        .map(str::to_string)
}

/// Converts a `{line, character}` position into a byte offset.
fn position_offsets(position: &JsonValue, line_starts: &[usize]) -> Option<usize> {
    line_character_offset(position, line_starts)
}

/// Converts a `{start, end}` range into byte offsets.
fn range_offsets(range: &JsonValue, text: &str) -> Option<(usize, usize)> {
    let line_starts = line_starts(text);
    let start = range.get("start")?;
    let end = range.get("end")?;
    let start = line_character_offset(start, &line_starts)?;
    let end = line_character_offset(end, &line_starts)?;
    Some((start, end.min(text.len())))
}

/// Byte offset for a `{line, character}` position given precomputed line starts.
fn line_character_offset(position: &JsonValue, line_starts: &[usize]) -> Option<usize> {
    let line = usize::try_from(position.get_int("line")?).ok()?;
    let character = usize::try_from(position.get_int("character")?).ok()?;
    let base = *line_starts.get(line)?;
    Some(base + character)
}

/// Byte-offset to `{line, character}`.
fn offset_range(text: &str, offset: u32) -> (Position, Position) {
    let offset = usize::try_from(offset)
        .unwrap_or(text.len())
        .min(text.len());
    let starts = line_starts(text);
    let line = starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1);
    let character = offset - starts[line];
    (Position { line, character }, Position { line, character })
}

struct Position {
    line: usize,
    character: usize,
}

fn position(value: &Position) -> JsonValue {
    JsonValue::Object(
        [
            (
                "line".into(),
                JsonValue::Number(i64::try_from(value.line).unwrap_or(i64::MAX)),
            ),
            (
                "character".into(),
                JsonValue::Number(i64::try_from(value.character).unwrap_or(i64::MAX)),
            ),
        ]
        .into(),
    )
}

/// Byte offsets of line starts (line 0 = 0).
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// The word containing `offset`, if any.
fn word_at(text: &str, offset: usize) -> Option<&str> {
    let offset = offset.min(text.len());
    let bytes = text.as_bytes();
    if offset == 0 {
        return None;
    }
    let mut start = offset;
    while start > 0 && is_word_byte(bytes.get(start - 1).copied()) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_word_byte(bytes.get(end).copied()) {
        end += 1;
    }
    if start == end {
        None
    } else {
        Some(&text[start..end])
    }
}

fn is_word_byte(byte: Option<u8>) -> bool {
    match byte {
        Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => true,
        Some(byte) if byte >= 0x80 => true, // UTF-8 glyphs are word bytes
        _ => false,
    }
}
