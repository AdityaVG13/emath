//! Async stdio JSON-RPC transport on the asupersync `Cx` (feature-gated).
//!
//! Pass 3 (framing) + pass 4 (transport hardening) of the tokio to
//! asupersync cutover: the async transport lane over `asupersync::io` traits.
//! It mirrors the blocking framing in `crate::protocol` byte-for-byte
//! (identical `Content-Length` headers and the same exit-code contract as
//! `crate::run`), so the blocking and async lanes are indistinguishable on the
//! wire. The blocking run loop, protocol, JSON, and server-state modules are
//! untouched by this pass.
//!
//! # Region ownership
//!
//! The whole message loop runs as one unit owned by the caller's region; the
//! caller typically wraps it in a region-owned task (`asupersync::Cx::spawn` +
//! `asupersync::runtime::TaskHandle`). The loop checkpoints before every frame
//! read, before every dispatch, and before every write, so an upstream
//! cancellation (region close / `abort`) is acknowledged at message boundaries
//! and in-flight frame I/O is dropped; EOF shuts the loop down cleanly.
//! Per-message `Scope` isolation needs
//! shared handler state (an actor / `Arc<Mutex>` refactor of
//! `crate::server::ServerState`) because `Cx::spawn` takes `Send + 'static'
//! closures, so it is deferred to the state-ownership step; the sync handler
//! itself is indivisible and mid-handler cancellation is a documented seam.
//!
//! # Real-stdio seam
//!
//! asupersync at the pinned rev exposes async I/O traits plus in-memory impls
//! (`&[u8]` and `Cursor<T>` readers, `Vec<u8>` writers) but no stdio/duplex
//! binding. `Transport` is generic over `R: AsyncRead + Unpin` and
//! `W: AsyncWrite + Unpin`; binding OS stdin/stdout later (a native stdio
//! surface, or an `asupersync-tokio-compat` `io::*` bridge per
//! COMPAT-BOUNDARY.md) is a drop-in `Transport::new(reader, writer)` change with
//! no logic changes here. Tests use in-memory readers/writers so they stay
//! deterministic.
//!
//! # Cancel-safety notes
//!
//! - `read_frame`: header reads are cancel-safe; the terminal `read_exact` is
//!   **not** (partial body bytes remain in the buffer), matching the crate's
//!   documented semantics for `read_exact`.
//! - verbatim writes: `write_all` is not fully drop-cancel-safe (a dropped
//!   future may leave partial output), matching the crate's documented
//!   semantics; the transport writes whole frames from the sync handler buffer.
//!
//! # Bounded-resource hardening (pass 4)
//!
//! - Frame bodies are capped at [`MAX_FRAME_BODY`] (16 MiB): `read_frame`
//!   refuses an oversized `Content-Length` with the typed
//!   [`TransportError::BodyTooLarge`] **before** any allocation, and `serve`
//!   answers with a `-32700` response and exit code `1`, mirroring how the
//!   blocking lane refuses an over-long header line. The blocking
//!   `protocol::read_message` stays uncapped; this bound is async-lane-only.
//! - Writes are bounded and check-flushed: one frame's output is written and
//!   flushed before the next frame is read, and `write_all`/`flush` failures
//!   surface as typed [`TransportError::Io`] rather than being swallowed.
//! - Optional host [`Control`] (bounded `mpsc`): `Transport::with_control`
//!   polls the receiver at message boundaries; [`Control::Shutdown`] exits
//!   `serve` with code `0` after the in-flight frame's responses are written
//!   and flushed (at most one frame of slack: it is honored between frames,
//!   never mid-`read_exact`). A host that must break a blocked read closes
//!   the reader or aborts the owning region (the existing cancel path); EOF
//!   beats a queued control signal.
//! - Per-message wall-clock budgets are a documented seam, not wired: the
//!   pinned asupersync rev has no generic future-timeout combinator, and `Cx`
//!   budgets are region-scoped, so whatever deadline/poll-quota the caller's
//!   region carries is already enforced by `cx.checkpoint()` at every boundary
//!   (`TransportError::Cancelled`).

use std::fmt;
use std::io;

use crate::json::parse_request;
use crate::protocol::{RpcMessage, write_error};
use crate::server::ServerState;

use asupersync::Cx;
use asupersync::channel::mpsc::{self, RecvError};
use asupersync::io::ext::{AsyncReadExt, AsyncWriteExt};
use asupersync::io::{AsyncRead, AsyncWrite};

/// Maximum header line length, matching the blocking protocol.
const MAX_HEADER_LINE: usize = 4096;

/// Maximum framed body length in bytes: the async lane's per-frame memory
/// bound. A `Content-Length` beyond this is refused with
/// [`TransportError::BodyTooLarge`] before any allocation, so a hostile
/// client cannot force unbounded buffering.
const MAX_FRAME_BODY: usize = 16 * 1024 * 1024;

/// Error surface of the async transport lane.
#[derive(Debug)]
pub enum TransportError {
    /// Underlying async I/O failure (the stdin/stdout side).
    Io(io::Error),
    /// Malformed LSP framing or JSON (the `-32700` parse-error class).
    Frame(String),
    /// `Content-Length` body exceeds [`MAX_FRAME_BODY`]; refused before
    /// allocation so a hostile client cannot force unbounded buffering.
    BodyTooLarge { length: usize, max: usize },
    /// Cooperative cancellation observed at a checkpoint.
    Cancelled,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "transport io error: {error}"),
            Self::Frame(message) => write!(f, "transport frame error: {message}"),
            Self::BodyTooLarge { length, max } => {
                write!(f, "frame body {length} bytes exceeds the {max} byte limit")
            }
            Self::Cancelled => f.write_str("transport cancelled"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Frame(_) | Self::BodyTooLarge { .. } | Self::Cancelled => None,
        }
    }
}

/// Reads one `Content-Length` framed body from `reader`.
///
/// `Ok(None)` at a clean EOF before any header byte, `Ok(Some(body))` with
/// the exact body bytes otherwise. Header rules (CR stripping, 4096-byte
/// line cap, missing-length errors) mirror `crate::protocol::read_message`;
/// a body longer than [`MAX_FRAME_BODY`] is refused with
/// [`TransportError::BodyTooLarge`] before any allocation (async-lane cap).
pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, TransportError>
where
    R: AsyncRead + Unpin,
{
    let mut length = None;
    loop {
        let Some(line) = read_header_line(reader).await? else {
            return Ok(None);
        };
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(TransportError::Frame("malformed header line".to_owned()));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| TransportError::Frame("invalid Content-Length".to_owned()))?,
            );
        }
    }
    let length =
        length.ok_or_else(|| TransportError::Frame("missing Content-Length header".to_owned()))?;
    if length > MAX_FRAME_BODY {
        return Err(TransportError::BodyTooLarge {
            length,
            max: MAX_FRAME_BODY,
        });
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|error| TransportError::Frame(format!("short body: {error}")))?;
    Ok(Some(body))
}

/// Reads one header line; `Ok(None)` at EOF before any content, `Err` at
/// EOF inside a line.
async fn read_header_line<R>(reader: &mut R) -> Result<Option<String>, TransportError>
where
    R: AsyncRead + Unpin,
{
    let mut buffer = String::new();
    let mut saw_byte = false;
    while buffer.len() < MAX_HEADER_LINE {
        let mut byte = [0u8; 1];
        let read = reader.read(&mut byte).await.map_err(TransportError::Io)?;
        if read == 0 {
            if saw_byte {
                return Err(TransportError::Frame(
                    "unexpected EOF inside header".to_owned(),
                ));
            }
            return Ok(None);
        }
        saw_byte = true;
        if byte[0] == 0x0A {
            return Ok(Some(buffer));
        }
        if byte[0] != 0x0D {
            buffer.push(char::from(byte[0]));
        }
    }
    Err(TransportError::Frame("header line too long".to_owned()))
}

/// Writes `body` as one `Content-Length` framed message, then flushes.
///
/// Produces byte-identical output to the blocking `crate::protocol::write_frame`.
pub async fn write_frame<W>(writer: &mut W, body: &[u8]) -> Result<(), TransportError>
where
    W: AsyncWrite + Unpin,
{
    // CR LF separator, spelled with byte literals so the source carries no
    // escaped control characters (framing parity with protocol.rs).
    let mut header = format!("Content-Length: {}", body.len()).into_bytes();
    header.extend_from_slice(&[0x0D, 0x0A, 0x0D, 0x0A]);
    write_verbatim(writer, &header).await?;
    write_verbatim(writer, body).await
}

/// Writes raw bytes with a final flush.
async fn write_verbatim<W>(writer: &mut W, bytes: &[u8]) -> Result<(), TransportError>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(bytes).await.map_err(TransportError::Io)?;
    writer.flush().await.map_err(TransportError::Io)
}

/// Host control signal for the transport loop.
///
/// Carried on an optional bounded [`mpsc`] receiver
/// (`Transport::with_control`). A [`Control::Shutdown`] stops [`serve`]
/// cleanly at a message boundary with exit code `0`, independent of the LSP
/// `shutdown`/`exit` handshake.
///
/// [`serve`]: Transport::serve
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Stop the loop after the in-flight frame completes.
    Shutdown,
}

/// Async transport owner: an in-memory (or future real-stdio) reader/writer
/// pair over the asupersync `io` traits, with optional host control.
pub struct Transport<R, W> {
    reader: R,
    writer: W,
    control: Option<mpsc::Receiver<Control>>,
}

impl<R, W> Transport<R, W> {
    /// Creates a transport over an async reader/writer pair.
    #[must_use]
    pub fn new(reader: R, writer: W) -> Self {
        Self::with_control(reader, writer, None)
    }

    /// Creates a transport that also polls an optional host control channel
    /// at message boundaries (see [`Control`]).
    #[must_use]
    pub fn with_control(reader: R, writer: W, control: Option<mpsc::Receiver<Control>>) -> Self {
        Self {
            reader,
            writer,
            control,
        }
    }
}

impl<R, W> Transport<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Runs the async message loop until EOF, `exit`, control, or
    /// cancellation.
    ///
    /// Returns the same exit-code contract as `crate::run`: `0` when
    /// `shutdown` preceded the terminal event, `1` otherwise. A framing,
    /// oversized-frame, or JSON parse error writes a `-32700` error response
    /// (id `null`) and returns `1`, mirroring the blocking lane. A host
    /// [`Control::Shutdown`] returns `0` after the in-flight frame's
    /// responses have been written and flushed.
    pub async fn serve(&mut self, cx: &Cx) -> Result<u8, TransportError> {
        let mut state = ServerState::new();
        loop {
            // Frame boundary: acknowledge cancellation and any region budget
            // (deadline / poll quota) before touching the reader.
            cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
            let body = match read_frame(&mut self.reader).await {
                Ok(body) => body,
                Err(TransportError::Frame(message)) => {
                    cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                    write_parse_error(&mut self.writer, &message).await?;
                    return Ok(1);
                }
                Err(TransportError::BodyTooLarge { length, max }) => {
                    cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                    let message =
                        format!("frame body length {length} exceeds the {max} byte limit");
                    write_parse_error(&mut self.writer, &message).await?;
                    return Ok(1);
                }
                Err(error) => return Err(error),
            };
            let Some(body) = body else {
                return Ok(u8::from(!state.shutdown));
            };
            let text = match String::from_utf8(body) {
                Ok(text) => text,
                Err(_) => {
                    cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                    write_parse_error(&mut self.writer, "body is not utf-8").await?;
                    return Ok(1);
                }
            };
            let (id, method, params) = match parse_request(&text) {
                Ok(parts) => parts,
                Err(message) => {
                    cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                    write_parse_error(&mut self.writer, &message).await?;
                    return Ok(1);
                }
            };
            if method == "exit" {
                return Ok(u8::from(!state.shutdown));
            }
            let message = RpcMessage { id, method, params };
            cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
            let mut framed = Vec::new();
            if state.handle(&message, &mut framed).is_err() {
                return Ok(1);
            }
            if !framed.is_empty() {
                cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                write_verbatim(&mut self.writer, &framed).await?;
            }
            // Message boundary after the in-flight frame: a host control
            // signal stops the loop once its responses are flushed.
            if self.control_shutdown().await? {
                return Ok(0);
            }
        }
    }

    /// Polls the optional host control channel at a message boundary.
    ///
    /// `Ok(true)` when [`Control::Shutdown`] was received: the writer is
    /// flushed so frames produced before the stop are delivered and the loop
    /// exits `0`. An empty channel, a dropped sender, or no receiver all mean
    /// `Ok(false)` — the control channel is optional by design.
    async fn control_shutdown(&mut self) -> Result<bool, TransportError> {
        let Some(control) = self.control.as_mut() else {
            return Ok(false);
        };
        match control.try_recv() {
            Ok(Control::Shutdown) => {
                self.writer.flush().await.map_err(TransportError::Io)?;
                Ok(true)
            }
            Err(RecvError::Empty) | Err(RecvError::Disconnected) | Err(RecvError::Cancelled) => {
                Ok(false)
            }
        }
    }
}

/// Writes a `-32700` parse/JSON-RPC error response (id `null`).
async fn write_parse_error<W>(writer: &mut W, message: &str) -> Result<(), TransportError>
where
    W: AsyncWrite + Unpin,
{
    let mut framed = Vec::new();
    write_error(&mut framed, None, -32700, message).map_err(TransportError::Io)?;
    write_verbatim(writer, &framed).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::JsonValue;
    use asupersync::io::ReadBuf;
    use asupersync::runtime::JoinError;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Runs async test code on a lab runtime with a live `Cx` (same entry
    /// as the `lab` module): deterministic, virtual-time-capable, and off
    /// the production runtime.
    fn run<F, Fut>(f: F)
    where
        F: FnOnce(Cx) -> Fut,
        Fut: Future<Output = ()>,
    {
        crate::lab::run_with_cx(f);
    }

    /// Reader that yields `data` once then pends forever (never EOF), so
    /// tests can hold the loop mid-stream and observe cancellation.
    #[derive(Debug)]
    struct PendAfterReader {
        data: Vec<u8>,
        pos: usize,
    }

    impl AsyncRead for PendAfterReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            if this.pos == this.data.len() {
                return Poll::Pending;
            }
            let available = &this.data[this.pos..];
            let take = available.len().min(buf.remaining());
            buf.put_slice(&available[..take]);
            this.pos += take;
            Poll::Ready(Ok(()))
        }
    }

    const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    const SHUTDOWN: &str = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#;
    const EXIT: &str = r#"{"jsonrpc":"2.0","method":"exit"}"#;

    /// Parses a response frame's body into JSON.
    fn response_json(body: Vec<u8>) -> JsonValue {
        let text = String::from_utf8(body).expect("response body must be utf-8");
        JsonValue::parse(&text).expect("response body must be valid JSON")
    }

    #[test]
    fn async_written_frame_reads_back_through_blocking_protocol() {
        // Interop: a frame written by the async transport must be byte- and
        // semantics-identical to what the blocking parser consumes. Fails if
        // the async framing drifts from protocol.rs.
        run(|_cx| async move {
            let mut wire = Vec::new();
            write_frame(&mut wire, INITIALIZE.as_bytes())
                .await
                .expect("async framing must succeed");
            let mut cursor = std::io::Cursor::new(&wire[..]);
            let message = crate::protocol::read_message(&mut cursor)
                .expect("blocking reader must accept the async frame");
            let message = message.expect("one frame expected");
            assert_eq!(message.id, Some(1));
            assert_eq!(message.method, "initialize");
        });
    }

    #[test]
    fn serve_round_trips_initialize_shutdown_exit_with_zero_exit_code() {
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            write_frame(&mut input, SHUTDOWN.as_bytes())
                .await
                .expect("frame shutdown");
            write_frame(&mut input, EXIT.as_bytes())
                .await
                .expect("frame exit");
            let mut transport = Transport::new(&input[..], Vec::new());
            let code = transport.serve(&cx).await.expect("serve must not error");
            assert_eq!(code, 0, "shutdown before exit must yield exit code 0");
            let output = transport.writer;
            let mut cursor = &output[..];
            let first = read_frame(&mut cursor)
                .await
                .expect("first response frame")
                .expect("a response is expected");
            let first = response_json(first);
            assert_eq!(first.get_int("id"), Some(1));
            let result = first.get("result").expect("initialize must respond");
            assert!(
                result.get("capabilities").is_some(),
                "capabilities advertised"
            );
            assert!(result.get("serverInfo").is_some(), "serverInfo advertised");
            let second = read_frame(&mut cursor)
                .await
                .expect("second response frame")
                .expect("a second response is expected");
            let second = response_json(second);
            assert_eq!(second.get_int("id"), Some(2));
            assert!(
                second.get("result").is_some_and(JsonValue::is_null),
                "shutdown result must be null"
            );
            assert!(
                read_frame(&mut cursor).await.expect("clean EOF").is_none(),
                "no frame may follow exit"
            );
        });
    }

    #[test]
    fn serve_eof_before_shutdown_returns_one() {
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            let mut transport = Transport::new(&input[..], Vec::new());
            let code = transport.serve(&cx).await.expect("serve must not error");
            assert_eq!(code, 1, "EOF before shutdown is the abnormal exit code");
            let output = transport.writer;
            let mut cursor = &output[..];
            let first = read_frame(&mut cursor)
                .await
                .expect("initialize response frame")
                .expect("a response is expected");
            assert_eq!(response_json(first).get_int("id"), Some(1));
            assert!(
                read_frame(&mut cursor).await.expect("clean EOF").is_none(),
                "EOF must end the loop"
            );
        });
    }

    #[test]
    fn serve_shutdown_then_eof_returns_zero() {
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            write_frame(&mut input, SHUTDOWN.as_bytes())
                .await
                .expect("frame shutdown");
            let mut transport = Transport::new(&input[..], Vec::new());
            let code = transport.serve(&cx).await.expect("serve must not error");
            assert_eq!(code, 0, "shutdown before EOF must yield exit code 0");
            let output = transport.writer;
            let mut cursor = &output[..];
            let mut frames = 0;
            while let Some(body) = read_frame(&mut cursor)
                .await
                .expect("frame read cannot fail")
            {
                frames += 1;
                assert!(response_json(body).get("result").is_some());
            }
            assert_eq!(frames, 2, "initialize + shutdown responses expected");
        });
    }

    #[test]
    fn aborted_serve_task_join_reports_cancelled() {
        // A region-owned serve task cancelled mid-stream (the reader pends
        // after the first frame) must surface `JoinError::Cancelled`, not a
        // fake success. Fails if the transport lane loses cancellation.
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            let mut transport = Transport::new(
                PendAfterReader {
                    data: input,
                    pos: 0,
                },
                Vec::new(),
            );
            let task = cx.spawn(|task_cx| async move { transport.serve(&task_cx).await });
            let mut task = task.expect("spawn must be admitted in a live region");
            task.abort();
            match task.join(&cx).await {
                Err(JoinError::Cancelled(_)) => {}
                other => panic!("expected Cancelled, got {other:?}"),
            }
        });
    }

    #[test]
    fn dropped_serve_region_drains_cleanly() {
        // Region close = quiescence: a serve task whose reader pends forever
        // must be cancelled and drained when the region returns. A leaked
        // task or a lost cancel would hang this test.
        run(|cx| async move {
            let mut transport = Transport::new(
                PendAfterReader {
                    data: Vec::new(),
                    pos: 0,
                },
                Vec::new(),
            );
            let task = cx.spawn(|task_cx| async move { transport.serve(&task_cx).await });
            assert!(task.is_ok(), "spawn must be admitted in a live region");
            drop(task.expect("checked above"));
        });
    }

    /// Writer whose `poll_write` always fails; proves typed writer-error
    /// propagation.
    struct FailingWriter;

    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "sink down")))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    /// Writer that accepts bytes but fails on `flush`; proves flush errors
    /// surface instead of being dropped.
    struct FlushFailingWriter;

    impl AsyncWrite for FlushFailingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, "flush down")))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn read_frame_refuses_oversized_content_length() {
        // A header alone claiming more than MAX_FRAME_BODY must be refused
        // with the typed error before any body is read (no allocation). Fails
        // if the cap is missing: read_frame would then try to read the body
        // and report `Frame("short body")` on the immediate EOF instead.
        run(|_cx| async move {
            let wire: &[u8] = b"Content-Length: 17000000\r\n\r\n";
            let mut cursor = wire;
            match read_frame(&mut cursor).await {
                Err(TransportError::BodyTooLarge { length, max }) => {
                    assert_eq!(length, 17_000_000);
                    assert_eq!(max, MAX_FRAME_BODY);
                }
                other => panic!("expected BodyTooLarge, got {other:?}"),
            }
        });
    }

    #[test]
    fn serve_refuses_oversized_frame_with_parse_error_and_exit_one() {
        // Wire-level contract: an oversized frame is a protocol failure
        // answered with a -32700 error (id null) and exit code 1, mirroring
        // how the blocking lane treats an over-long header line. Fails if
        // the cap is bypassed (no -32700 message citing the limit).
        run(|cx| async move {
            let mut input = Vec::new();
            input.extend_from_slice(
                format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BODY + 1).as_bytes(),
            );
            let mut transport = Transport::new(&input[..], Vec::new());
            let code = transport
                .serve(&cx)
                .await
                .expect("serve must not error on refusal");
            assert_eq!(code, 1, "oversized frame is an abnormal exit");
            let output = transport.writer;
            let mut cursor = &output[..];
            let body = read_frame(&mut cursor)
                .await
                .expect("error frame must be readable")
                .expect("one frame expected");
            let payload = response_json(body);
            assert_eq!(payload.get_int("id"), None, "parse error id must be null");
            let error = payload.get("error").expect("error object");
            assert_eq!(error.get_int("code"), Some(-32700));
            let message = error.get_str("message").expect("error message").to_owned();
            assert!(
                message.contains("exceeds"),
                "message must cite the cap: {message}"
            );
            assert!(
                read_frame(&mut cursor).await.expect("clean EOF").is_none(),
                "no frame may follow the refusal"
            );
        });
    }

    #[test]
    fn control_shutdown_exits_zero_after_flushing_in_flight_frame() {
        // Host stop via the optional mpsc control channel: with a signal
        // queued before serve, the loop processes exactly one frame
        // (initialize), flushes its response, then exits 0 without reading
        // further input. Fails if control is ignored (serve would run through
        // shutdown + EOF and emit two responses) or treated as an error.
        run(|cx| async move {
            let (tx, rx) = mpsc::channel::<Control>(1);
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            write_frame(&mut input, SHUTDOWN.as_bytes())
                .await
                .expect("frame shutdown");
            tx.try_send(Control::Shutdown)
                .expect("capacity-1 channel admits the control signal");
            let mut transport = Transport::with_control(&input[..], Vec::new(), Some(rx));
            let code = transport.serve(&cx).await.expect("serve must not error");
            assert_eq!(code, 0, "host control stop is a clean exit");
            let output = transport.writer;
            let mut cursor = &output[..];
            let first = read_frame(&mut cursor)
                .await
                .expect("response frame")
                .expect("a response is expected");
            assert_eq!(
                response_json(first).get_int("id"),
                Some(1),
                "the in-flight initialize response must be flushed"
            );
            assert!(
                read_frame(&mut cursor).await.expect("clean EOF").is_none(),
                "no frame may follow the control stop (shutdown frame unread)"
            );
        });
    }

    #[test]
    fn writer_error_propagates_as_typed_io_error() {
        // A failing sink must surface as `TransportError::Io`, not be
        // swallowed into a fake `Ok(1)` exit code. Fails if the write path
        // ignores `write_all` errors.
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            let mut transport = Transport::new(&input[..], FailingWriter);
            match transport.serve(&cx).await {
                Err(TransportError::Io(_)) => {}
                other => panic!("expected typed Io error, got {other:?}"),
            }
        });
    }

    #[test]
    fn writer_flush_error_surfaces() {
        // A writer that accepts bytes but fails `flush` must still surface
        // `TransportError::Io`. Fails if the flush result is dropped.
        run(|cx| async move {
            let mut input = Vec::new();
            write_frame(&mut input, INITIALIZE.as_bytes())
                .await
                .expect("frame initialize");
            let mut transport = Transport::new(&input[..], FlushFailingWriter);
            match transport.serve(&cx).await {
                Err(TransportError::Io(_)) => {}
                other => panic!("expected typed Io error, got {other:?}"),
            }
        });
    }
}
