# emath-lsp CONTRACT

## Purpose and layer
- Minimal language-server-protocol skeleton (CRATE_MAP tier: sema/cli).
- Std-only, deterministic LSP server slice: base-protocol framing (`Content-Length` headers, JSON-RPC), `initialize` capabilities with incremental text synchronization, `textDocument/didOpen` / `didChange`, and `publishDiagnostics` computed by the real compiler session (`emath_sema::CompilerSession::check_owned`), so LSP and CLI agree on diagnostics (shared admission path).
- Skeleton `completion` (Phase 1 grammar keywords), `hover`, `signatureHelp` (null response). Typed method refusal (`-32601`), deterministic writes.

## Public types and semantics
- `run(input: &mut impl Read, output: &mut impl Write) -> u8`: the server loop until EOF. Returns 0 if the client performed `shutdown` before `exit`, 1 otherwise (the LSP exit-code contract).
- Modules: `json` (JSON value plumbing), `protocol` (message framing, `read_message`, `write_error`), `server` (`ServerState`, diagnostics/publish dispatch).

## Invariants
- UTF-8 byte offsets are converted to LSP character semantics on glyph-bearing lines; the position encoding is advertised as UTF-8 during `initialize`.
- Diagnostics come from the real compiler session, not a parallel parser, so LSP and CLI admission agree.
- Incremental `didChange` edits round-trip byte offsets correctly.
- Unknown methods get a typed refusal (`-32601`); framing parse failures yield JSON-RPC parse error `-32700`.
- Deterministic output ordering in all JSON documents.

## Error model
- JSON-RPC error codes: `-32700` (parse error) on a framing failure, `-32601` (method not found) on an unknown method; protocol `write_error` emits these. No panics escape the loop on malformed input.

## Determinism class
- Deterministic: no network, filesystem watch (tools like `fs.watch` unavailable), or third-party dependency; output derives only from the message stream and the shared compiler diagnostics.

## Cancellation behavior
- Not applicable: stdout/stdin request-loop with no background work or cancellation surface.

## Unsafe boundary
- None: crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags
- None: no `[features]` in Cargo.toml.

## Conformance tests
- In-crate `#[cfg(test)] mod tests` in `server.rs` (no `tests/` directory on disk): `range_offsets_use_utf8_byte_characters_on_glyph_lines`, `initialize_advertises_utf8_position_encoding`, `glyph_bearing_did_change_round_trips_byte_offsets`.

## No-claim boundaries
- Skeleton coverage only: `completion` is Phase 1 keyword completion, `signatureHelp` returns null, and no formatting/rename/code-action support is claimed.
- No network transport, filesystem watching, or third-party protocol client.
