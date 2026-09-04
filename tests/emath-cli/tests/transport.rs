//! `emath-cli` lsp async stdio transport tests (migrated from
//! `crates/emath-cli/src/lsp/transport.rs`).
//!
//! The moved tests previously reached crate internals (`super::*`, the
//! private `MAX_FRAME_BODY` const, and the private `Transport.writer`
//! field). They now exercise only the public surface:
//!
//! - `read_frame` / `write_frame` / `Transport` / `Transport::with_control`
//!   / `TransportError` / `Control` from `emath_cli::lsp::transport`;
//! - `run_with_cx` from `emath_cli::lsp::lab`, `read_message` from
//!   `emath_cli::lsp::protocol`;
//! - a local `RecordingWriter` substitutes for the removed private
//!   `writer` field; the frame-body cap is spelled out locally because
//!   `MAX_FRAME_BODY` is a private const.

use asupersync::Cx;
use asupersync::channel::mpsc;
use asupersync::io::ReadBuf;
use asupersync::io::{AsyncRead, AsyncWrite};
use asupersync::runtime::JoinError;
use emath_cli::lsp::json::JsonValue;
use emath_cli::lsp::lab::run_with_cx;
use emath_cli::lsp::protocol::read_message;
use emath_cli::lsp::transport::{Control, Transport, TransportError, read_frame, write_frame};
use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

/// Maximum framed body length in bytes: the async lane's per-frame memory
/// bound, spelled out locally because the crate's `MAX_FRAME_BODY` const is
/// private (the value is pinned by the transport's documented contract).
const MAX_FRAME_BODY: usize = 16 * 1024 * 1024;

/// Runs async test code on a lab runtime with a live `Cx` (same entry
/// as the `lab` module): deterministic, virtual-time-capable, and off
/// the production runtime.
fn run<F, Fut>(f: F)
where
    F: FnOnce(Cx) -> Fut,
    Fut: Future<Output = ()>,
{
    run_with_cx(f);
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

/// Test writer that records every byte written, standing in for the
/// removed private `Transport.writer` field.
#[derive(Debug)]
struct RecordingWriter(Rc<RefCell<Vec<u8>>>);

impl AsyncWrite for RecordingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.borrow_mut().extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
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

/// Runs `serve` on the given wire bytes and returns `(exit_code, output)`.
async fn serve_bytes(cx: &Cx, wire: &[u8]) -> (u8, Vec<u8>) {
    let recorded = Rc::new(RefCell::new(Vec::new()));
    let mut transport = Transport::new(wire, RecordingWriter(recorded.clone()));
    let code = transport
        .serve(cx)
        .await
        .expect("framing refusal must not error, only exit 1");
    (code, recorded.borrow().clone())
}

/// Asserts `wire` is exactly one `-32700` parse-error frame (id null).
async fn assert_parse_error_frame(wire: &[u8]) {
    let mut cursor = wire;
    let body = read_frame(&mut cursor)
        .await
        .expect("error frame must be readable")
        .expect("one frame expected");
    let payload = response_json(body);
    assert_eq!(payload.get_int("id"), None, "parse error id must be null");
    let error = payload.get("error").expect("error object");
    assert_eq!(error.get_int("code"), Some(-32700));
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
        Poll::Ready(Err(io::Error::other("flush down")))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
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
        let message =
            read_message(&mut cursor).expect("blocking reader must accept the async frame");
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
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut transport = Transport::new(&input[..], RecordingWriter(recorded.clone()));
        let code = transport.serve(&cx).await.expect("serve must not error");
        assert_eq!(code, 0, "shutdown before exit must yield exit code 0");
        let output = recorded.borrow().clone();
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
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut transport = Transport::new(&input[..], RecordingWriter(recorded.clone()));
        let code = transport.serve(&cx).await.expect("serve must not error");
        assert_eq!(code, 1, "EOF before shutdown is the abnormal exit code");
        let output = recorded.borrow().clone();
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
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut transport = Transport::new(&input[..], RecordingWriter(recorded.clone()));
        let code = transport.serve(&cx).await.expect("serve must not error");
        assert_eq!(code, 0, "shutdown before EOF must yield exit code 0");
        let output = recorded.borrow().clone();
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
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut transport = Transport::new(&input[..], RecordingWriter(recorded.clone()));
        let code = transport
            .serve(&cx)
            .await
            .expect("serve must not error on refusal");
        assert_eq!(code, 1, "oversized frame is an abnormal exit");
        let output = recorded.borrow().clone();
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
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut transport =
            Transport::with_control(&input[..], RecordingWriter(recorded.clone()), Some(rx));
        let code = transport.serve(&cx).await.expect("serve must not error");
        assert_eq!(code, 0, "host control stop is a clean exit");
        let output = recorded.borrow().clone();
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

#[test]
fn serve_refuses_invalid_content_length_with_parse_error() {
    // Negative control: a non-numeric `Content-Length` value (the garbage
    // case) must be refused by the async lane with a -32700 response and
    // exit code 1, mirroring the blocking `protocol::read_message`'s
    // "invalid Content-Length" path. Fails if the lane accepts it or exits
    // with the wrong code.
    run(|cx| async move {
        let (code, output) = serve_bytes(&cx, b"Content-Length: xyz\r\n\r\n").await;
        assert_eq!(code, 1, "invalid Content-Length is an abnormal exit");
        assert_parse_error_frame(&output).await;
    });
}

#[test]
fn serve_refuses_eof_mid_header_with_parse_error() {
    // Negative control: EOF inside a header line (before the terminating
    // blank line) must be Frame("unexpected EOF inside header") and thus a
    // -32700 + exit 1, matching the blocking lane's identical path. Fails
    // if EOF mid-header is mistaken for a clean EOF (which would exit the
    // loop without an error).
    run(|cx| async move {
        let (code, output) = serve_bytes(&cx, b"Content-Length: ").await;
        assert_eq!(code, 1, "EOF mid-header is an abnormal exit");
        assert_parse_error_frame(&output).await;
    });
}

#[test]
fn serve_refuses_short_body_with_parse_error() {
    // Negative control: a valid header declaring more body bytes than are
    // present maps to Frame("short body") -> -32700 + exit 1, exactly the
    // blocking lane's `read_exact` failure mode. Fails if the lane
    // fabricates a frame or exits 0.
    run(|cx| async move {
        let (code, output) = serve_bytes(&cx, b"Content-Length: 20\r\n\r\nhello").await;
        assert_eq!(code, 1, "short body is an abnormal exit");
        assert_parse_error_frame(&output).await;
    });
}

#[test]
fn read_frame_accepts_header_case_and_whitespace_variants() {
    // Parity: the header grammar the blocking lane accepts (case-insensitive
    // Content-Length, trimmed value, CR-stripped lines) must be accepted
    // byte-for-byte by the async lane too. Each case returns the exact body
    // bytes. Fails if the async lane is stricter than the blocking lane (an
    // asymmetry would be a wire mismatch).
    run(|_cx| async move {
        let cases: &[(&str, &[u8])] = &[
            ("Content-Length: 5\r\n\r\nhello", b"hello"),
            ("content-length: 5\r\n\r\nhello", b"hello"),
            ("CONTENT-LENGTH: 5\r\n\r\nhello", b"hello"),
            // Whitespace around the trimmed value, as the blocking lane
            // accepts via `value.trim()`.
            ("Content-Length:   5   \r\n\r\nhello", b"hello"),
            // A foreign header before Content-Length must be ignored in
            // both lanes (only Content-Length is honored).
            ("X-Custom: abc\r\nContent-Length: 5\r\n\r\nhello", b"hello"),
        ];
        for (wire, expected) in cases {
            let mut cursor: &[u8] = wire.as_bytes();
            let body = read_frame(&mut cursor)
                .await
                .expect("variant must be accepted")
                .expect("one frame expected");
            assert_eq!(&body, expected, "acceptance parity for {wire:?}");
        }
    });
}

#[test]
fn read_frame_refuses_oversized_header_line() {
    // Negative control: a header line longer than the 4096-byte cap (both
    // lanes share MAX_HEADER_LINE) must be refused as a framing error, not
    // buffered unboundedly. Fails if the line cap is removed or raised
    // (unbounded header buffering).
    let long = format!("X-Pad: {}\r\n\r\n", "a".repeat(5000));
    run(|_cx| async move {
        let wire: &[u8] = long.as_bytes();
        let mut cursor = wire;
        match read_frame(&mut cursor).await {
            Err(TransportError::Frame(message)) => {
                assert!(
                    message.contains("header line too long"),
                    "cap message expected: {message}"
                );
            }
            other => panic!("expected Frame(header line too long), got {other:?}"),
        }
    });
}

#[test]
fn serve_partial_second_frame_yields_clean_error_no_partial_output() {
    // Cancellation/interleaving hygiene AND negative control: after a valid
    // first frame the stream breaks mid-second-frame (a shorter body than
    // its header declares, then EOF). The server must answer the first
    // frame, then emit exactly one complete -32700 error frame — never a
    // partial/clipped second response — and exit 1. Fails if a partial
    // frame leaks into `writer` or the wire is unparseable.
    run(|cx| async move {
        let mut input = Vec::new();
        write_frame(&mut input, INITIALIZE.as_bytes())
            .await
            .expect("frame initialize");
        // Second frame header declares 100 body bytes but only 3 arrive.
        input.extend_from_slice(b"Content-Length: 100\r\n\r\nabc");
        let (code, output) = serve_bytes(&cx, &input[..]).await;
        assert_eq!(code, 1, "broken second frame is an abnormal exit");
        let mut cursor = &output[..];
        let first = read_frame(&mut cursor)
            .await
            .expect("first response must be a complete frame")
            .expect("a first frame is expected");
        assert_eq!(response_json(first).get_int("id"), Some(1));
        let second = read_frame(&mut cursor)
            .await
            .expect("error frame must be a complete frame")
            .expect("the parse-error frame is expected");
        assert_eq!(response_json(second).get_int("id"), None);
        assert!(
            read_frame(&mut cursor).await.expect("clean EOF").is_none(),
            "no partial/stray bytes may follow the error frame"
        );
    });
}

#[test]
fn region_close_drains_mid_body_pending_read() {
    // Cancellation forensics: the reader pends partway through a declared
    // body (header consumed, `read_exact` blocked awaiting the remaining
    // bytes that never arrive). Region close must cancel and drain that
    // in-flight body read without hanging — the documented non-drop-cancel-
    // safe `read_exact` seam. Fails (hangs) if the pending body read is not
    // cancelled on region close.
    run(|cx| async move {
        let mut transport = Transport::new(
            PendAfterReader {
                // Header declares 10 body bytes; only 5 arrive, then pend.
                data: b"Content-Length: 10\r\n\r\nhello".to_vec(),
                pos: 0,
            },
            Vec::new(),
        );
        let task = cx.spawn(|task_cx| async move { transport.serve(&task_cx).await });
        assert!(task.is_ok(), "spawn must be admitted in a live region");
        drop(task.expect("checked above"));
    });
}

#[test]
fn serve_is_deterministic_identical_input_identical_output() {
    // Determinism contract: running the exact same byte stream through two
    // fresh, independent `Transport` instances must yield byte-identical
    // output frames AND the identical exit code. Fails on any per-run
    // nondeterminism (e.g. HashMap ordering in response render, a cached
    // clock, or ambient RNG leaking into output).
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
        let (code1, wire1) = serve_bytes(&cx, &input[..]).await;
        let (code2, wire2) = serve_bytes(&cx, &input[..]).await;
        assert_eq!(code1, code2, "exit code must be deterministic");
        assert_eq!(wire1, wire2, "output frames must be byte-identical");
        assert!(!wire1.is_empty(), "responses must actually be produced");
    });
}

#[test]
fn serve_dispatches_sequentially_with_strict_single_flight_order() {
    // Concurrency hygiene: two response-producing requests are dispatched
    // strictly in input order, each written as a complete single-flight
    // frame (write_all + flush before the next read). Rebuilding the
    // expected wire as `write_frame(body1) ++ write_frame(body2)` must be
    // byte-identical to the actual output — no interleaving, no padding, no
    // clipped frame. Fails if two frames ever interleave their bytes or a
    // frame is emitted non-canonically.
    run(|cx| async move {
        let mut input = Vec::new();
        write_frame(&mut input, INITIALIZE.as_bytes())
            .await
            .expect("frame initialize");
        write_frame(&mut input, SHUTDOWN.as_bytes())
            .await
            .expect("frame shutdown");
        let (code, output) = serve_bytes(&cx, &input[..]).await;
        assert_eq!(code, 0, "shutdown before EOF yields exit code 0");
        let mut cursor = &output[..];
        let body1 = read_frame(&mut cursor)
            .await
            .expect("first frame")
            .expect("a first frame is expected");
        assert_eq!(response_json(body1.clone()).get_int("id"), Some(1));
        let body2 = read_frame(&mut cursor)
            .await
            .expect("second frame")
            .expect("a second frame is expected");
        assert_eq!(response_json(body2.clone()).get_int("id"), Some(2));
        assert!(
            read_frame(&mut cursor).await.expect("clean EOF").is_none(),
            "no third frame"
        );
        // Strict single-flight: the frames must be exactly adjacent with
        // canonical headers and nothing in between.
        let mut expected = Vec::new();
        write_frame(&mut expected, &body1)
            .await
            .expect("re-header body1");
        write_frame(&mut expected, &body2)
            .await
            .expect("re-header body2");
        assert_eq!(output, expected, "frames must be non-interleaved, in order");
    });
}
