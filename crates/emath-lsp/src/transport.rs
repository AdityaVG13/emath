//! Async stdio JSON-RPC transport on the asupersync `Cx` (feature-gated).
//!
//! Pass 3 of the tokio to asupersync cutover: the async transport lane over
//! `asupersync::io` traits. It mirrors the blocking framing in `crate::protocol`
//! byte-for-byte (identical `Content-Length` headers and the same exit-code
//! contract as `crate::run`), so the blocking and async lanes are indistinguishable
//! on the wire. The blocking run loop, protocol, JSON, and server-state modules
//! are untouched by this pass.
//!
//! # Region ownership
//!
//! The whole message loop runs as one unit owned by the caller's region; the
//! caller typically wraps it in a region-owned task (`asupersync::Cx::spawn` +
//! `asupersync::runtime::TaskHandle`). The loop checkpoints before every frame
//! read and before every dispatch, so an upstream cancellation (region close /
//! `abort`) is acknowledged at message boundaries and in-flight frame I/O is
//! dropped; EOF shuts the loop down cleanly. Per-message `Scope` isolation needs
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

use std::fmt;
use std::io;

use crate::json::parse_request;
use crate::protocol::{RpcMessage, write_error};
use crate::server::ServerState;

use asupersync::Cx;
use asupersync::io::ext::{AsyncReadExt, AsyncWriteExt};
use asupersync::io::{AsyncRead, AsyncWrite};

/// Maximum header line length, matching the blocking protocol.
const MAX_HEADER_LINE: usize = 4096;

/// Error surface of the async transport lane.
#[derive(Debug)]
pub enum TransportError {
    /// Underlying async I/O failure (the stdin/stdout side).
    Io(io::Error),
    /// Malformed LSP framing or JSON (the `-32700` parse-error class).
    Frame(String),
    /// Cooperative cancellation observed at a checkpoint.
    Cancelled,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "transport io error: {error}"),
            Self::Frame(message) => write!(f, "transport frame error: {message}"),
            Self::Cancelled => f.write_str("transport cancelled"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Frame(_) | Self::Cancelled => None,
        }
    }
}

/// Reads one `Content-Length` framed body from `reader`.
///
/// `Ok(None)` at a clean EOF before any header byte, `Ok(Some(body))` with
/// the exact body bytes otherwise. Header rules (CR stripping, 4096-byte
/// line cap, missing-length errors) mirror `crate::protocol::read_message`.
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

/// Async transport owner: an in-memory (or future real-stdio) reader/writer
/// pair over the asupersync `io` traits.
pub struct Transport<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> Transport<R, W> {
    /// Creates a transport over an async reader/writer pair.
    #[must_use]
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R, W> Transport<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Runs the async message loop until EOF, `exit`, or cancellation.
    ///
    /// Returns the same exit-code contract as `crate::run`: `0` when
    /// `shutdown` preceded the terminal event, `1` otherwise. A framing or
    /// JSON parse error writes a `-32700` error response (id `null`) and
    /// returns `1`, mirroring the blocking lane.
    pub async fn serve(&mut self, cx: &Cx) -> Result<u8, TransportError> {
        let mut state = ServerState::new();
        loop {
            cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
            let body = match read_frame(&mut self.reader).await {
                Ok(body) => body,
                Err(TransportError::Frame(message)) => {
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
                    write_parse_error(&mut self.writer, "body is not utf-8").await?;
                    return Ok(1);
                }
            };
            let (id, method, params) = match parse_request(&text) {
                Ok(parts) => parts,
                Err(message) => {
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
                write_verbatim(&mut self.writer, &framed).await?;
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
}
