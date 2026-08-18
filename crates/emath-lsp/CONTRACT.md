# emath-lsp CONTRACT.md

## Purpose and layer

- Tier: sema/cli (CRATE_MAP) — the language-server-protocol transport and
  server skeleton for emath.
- Minimal, std-only LSP server slice: base-protocol framing
  (`Content-Length` headers, JSON-RPC messages), `initialize` capabilities
  with incremental text synchronization, `textDocument/didOpen`/`didChange`
  with incremental edits, and `publishDiagnostics` computed by the real
  compiler session (`emath_sema::CompilerSession::check_owned`), so LSP and
  CLI agree on diagnostics (shared admission path).
- Skeleton `completion` (Phase 1 grammar keywords), `hover` (keyword
  documentation), `signatureHelp` (null response). Typed method refusal
  (`-32601`), deterministic writes.
- Two lanes with identical wire syntax:
  - **default blocking stdio lane** (`crate::run`, `crate::protocol`,
    `crate::server`): std-only, dependency-free.
  - **`async-runtime` lane** (`crate::transport`, `crate::lab`): the same
    framing on the asupersync `Cx`/io traits, plus a Cx/lab-runtime test
    entry. Feature-gated; the blocking lane is untouched by it.

## Public types and semantics

- `run(input: &mut impl Read, output: &mut impl Write) -> u8`: the blocking
  server loop until EOF. Returns `0` if the client performed `shutdown`
  before `exit`, `1` otherwise (the LSP exit-code contract).
- `Transport<R, W>::new` / `Transport::with_control` / `Transport::serve`:
  the async lane generic over `R: AsyncRead + Unpin`, `W: AsyncWrite +
  Unpin`. `serve(&Cx)` returns `Ok(0)` on `shutdown`-then-EOF / host
  `Control::Shutdown`, `Ok(1)` on abnormal exit, or `Err(TransportError)`.
- `TransportError` — the async lane error surface: `Io(io::Error)`,
  `Frame(String)`, `BodyTooLarge { length, max }`, `Cancelled`.
- `Control::Shutdown` — host stop signal on an optional bounded mpsc.
- Modules: `json` (deterministic JSON), `protocol` (blocking framing),
  `server` (`ServerState`, diagnostics/publish dispatch), `lab` and
  `transport` (feature-gated).

## Invariants

- **Framing parity:** the async lane is byte-for-byte identical to the
  blocking `protocol.rs` — identical `Content-Length` headers and the same
  exit-code contract, so the two lanes are indistinguishable on the wire
  (enforced by `async_written_frame_reads_back_through_blocking_protocol`).
- **Exit-code contract:** `0` ⇔ `shutdown` preceded the terminal event
  (EOF, `exit`, or host `Control::Shutdown`); `1` ⇔ any other terminal.
- **Frame cap:** async lane refuses a `Content-Length` body > 16 MiB
  (`TransportError::BodyTooLarge`) before any allocation, answering `-32700`
  and exit code `1`; header lines are capped at 4096 bytes in both lanes.
  The blocking `protocol::read_message` body stays uncapped.
- **Checkpoint discipline:** the async loop checkpoints before every frame
  read, before every dispatch, and before every write, so upstream
  cancellation (region close / abort) and any region budget (deadline / poll
  quota) are acknowledged at message boundaries; in-flight frame I/O is
  dropped.
- **Std-only default:** no network, filesystem watch, or third-party
  dependencies unless `async-runtime` is enabled.
- UTF-8 byte offsets are converted to LSP character semantics on
  glyph-bearing lines; `positionEncoding: utf-8` is advertised at
  `initialize`. Incremental `didChange` edits round-trip byte offsets
  correctly. Unknown methods get a typed refusal (`-32601`); parse/framing
  failures yield JSON-RPC `-32700`. Deterministic output ordering.

## Error model

- **JSON-RPC (`-32700`/`-32601`):** the entire agent/user-visible surface.
  Protocol parse-error responses are code `-32700` (id `null`), unknown-method
  refusals are `-32601`. Message text is deterministic. Both lanes emit the
  identical wire behavior. See `implementation/ERROR_CODES.md` (LSP entry);
  this is the whole surface — no `E-LSP-*` family is defined or necessary.
- **`TransportError` (async lane only, internal):** typed, not user-facing.
  `Io` carries the underlying async I/O error; `Frame` is a malformed-framing
  class that maps to the `-32700` response; `BodyTooLarge` is the 16 MiB cap
  refusal (also → `-32700` + exit 1); `Cancelled` is cooperative
  cancellation observed at a checkpoint (propagates upward, no response
  written). Writes are bounded and check-flushed: `write_all`/`flush`
  failures surface as `TransportError::Io`, never swallowed.
- No panics escape either loop on malformed input.

## Determinism class

- **Deterministic.** No wall clock, no network, no filesystem watch, no
  third-party dependency in the default build. Output derives only from the
  message stream and shared compiler diagnostics. The async lane checkpoints
  are deterministic; region budgets are the caller's to set. Tests run on a
  deterministic lab runtime (`crate::lab::run_with_cx`), never wall-clock.

## Cancellation behavior

- Blocking lane: not applicable (synchronous request loop, no background
  work).
- Async lane: the whole loop is one region-owned unit (typically
  `Cx::spawn` + `TaskHandle`). Cancellation is acknowledged at message
  boundaries via `cx.checkpoint()` (→ `TransportError::Cancelled`); region
  close drains/loses the in-flight frame; host `Control::Shutdown` stops
  cleanly after flushing the in-flight frame with exit code `0`. EOF beats a
  queued control signal. `read_exact`/`write_all` retain their crate-documented
  (non-drop-cancel-safe) semantics — a documented seam, not a guarantee.

## Unsafe boundary

- None. Crate declares `#![forbid(unsafe_code)]`; workspace lint forbids it.

## Feature flags

- None in the default build.
- `async-runtime = ["dep:asupersync"]`: the single gate under which
  emath-lsp pulls the pinned asupersync git revision and exposes the
  `lab` and `transport` modules. This feature is the only path to the
  asupersync dependency.

## Conformance tests

In-crate `#[cfg(test)]` modules (no `tests/` dir on disk). 17 tests:

`server.rs`:
- `range_offsets_use_utf8_byte_characters_on_glyph_lines`
- `initialize_advertises_utf8_position_encoding`
- `glyph_bearing_did_change_round_trips_byte_offsets`

`transport.rs`:
- `async_written_frame_reads_back_through_blocking_protocol`
- `serve_round_trips_initialize_shutdown_exit_with_zero_exit_code`
- `serve_eof_before_shutdown_returns_one`
- `serve_shutdown_then_eof_returns_zero`
- `aborted_serve_task_join_reports_cancelled`
- `dropped_serve_region_drains_cleanly`
- `read_frame_refuses_oversized_content_length`
- `serve_refuses_oversized_frame_with_parse_error_and_exit_one`
- `control_shutdown_exits_zero_after_flushing_in_flight_frame`
- `writer_error_propagates_as_typed_io_error`
- `writer_flush_error_surfaces`

`lab.rs`:
- `region_task_completes`
- `region_close_cancels_dropped_task`
- `abort_makes_join_report_cancellation`

## No-claim boundaries

- No real-stdio binding yet: the async `Transport` is generic over
  `AsyncRead`/`AsyncWrite` in-memory impls; OS stdin/stdout (native or an
  asupersync-tokio-compat io bridge) is a future drop-in. Tests use
  in-memory readers/writers only.
- No `tokio` in the crate; asupersync only behind the `async-runtime`
  feature. No network transport, no filesystem watch (`fs.watch` /
  distribution-tools unavailable), no third-party protocol client.
- Skeleton coverage only: `completion` is Phase 1 keyword completion,
  `signatureHelp` returns null, no formatting/rename/code-action support.
- Per-message wall-clock budgets are a documented seam, not wired; the async
  lane defers budgets to the caller's region-scoped `Cx` budgets.
- The blocking `protocol::read_message` body is uncapped (16 MiB is the
  async-lane-only bound).
- Mid-handler cancellation of the sync `ServerState::handle` is an indivisible
  seam; per-message `Scope` isolation is deferred pending an actor/`Arc<Mutex>`
  refactor.
