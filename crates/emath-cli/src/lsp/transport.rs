//! Async stdio JSON-RPC transport on the asupersync `Cx` (feature-gated).
//!
//! Mirrors the blocking `crate::lsp::protocol` framing byte-for-byte (same
//! `Content-Length` headers and exit-code contract), so both lanes are
//! wire-identical. Frame bodies are capped at [`MAX_FRAME_BODY`] (16 MiB)
//! before any allocation; cancellation is acknowledged at message boundaries
//! via `cx.checkpoint()` (`TransportError::Cancelled`).

use std::fmt;
use std::io;

use crate::lsp::json::parse_request;
use crate::lsp::protocol::{RpcMessage, write_error};
use crate::lsp::server::ServerState;

use asupersync::Cx;
use asupersync::channel::mpsc::{self, RecvError};
use asupersync::io::ext::{AsyncReadExt, AsyncWriteExt};
use asupersync::io::{AsyncRead, AsyncWrite};

/// Maximum header line length, matching the blocking protocol.
const MAX_HEADER_LINE: usize = 4096;

/// Async-lane per-frame body cap; oversized `Content-Length` is refused with
/// [`TransportError::BodyTooLarge`] before any allocation.
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
            Self::BodyTooLarge { length, max } => write!(
                f,
                "frame body {length} bytes exceeds the {max}-byte ({} MiB) per-frame cap; \
                 the Content-Length header is refused before any allocation",
                max / (1024 * 1024)
            ),
            Self::Cancelled => f.write_str(
                "transport cancelled: the owning region was aborted or closed; \
                 no further frames are processed",
            ),
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

/// Reads one `Content-Length` framed body; `Ok(None)` at clean EOF.
///
/// Header rules mirror `crate::lsp::protocol::read_message`; a body over
/// [`MAX_FRAME_BODY`] is refused with [`TransportError::BodyTooLarge`].
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
/// Produces byte-identical output to the blocking `crate::lsp::protocol::write_frame`.
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
/// [`Control::Shutdown`] stops [`serve`] cleanly at a message boundary with
/// exit code `0`, independent of the LSP `shutdown`/`exit` handshake.
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
    /// Runs the async message loop until EOF/`exit`/cancellation. Same
    /// exit-code contract as `crate::lsp::run`; framing/parse errors write
    /// `-32700` (id `null`) and return `1`.
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
            let Some(text) = String::from_utf8(body).ok() else {
                cx.checkpoint().map_err(|_| TransportError::Cancelled)?;
                write_parse_error(&mut self.writer, "body is not utf-8").await?;
                return Ok(1);
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
    /// `Ok(true)` on [`Control::Shutdown`] (writer flushed, loop exits `0`);
    /// empty, dropped, or missing channel means `Ok(false)`.
    async fn control_shutdown(&mut self) -> Result<bool, TransportError> {
        let Some(control) = self.control.as_mut() else {
            return Ok(false);
        };
        match control.try_recv() {
            Ok(Control::Shutdown) => {
                self.writer.flush().await.map_err(TransportError::Io)?;
                Ok(true)
            }
            Err(RecvError::Empty | RecvError::Disconnected | RecvError::Cancelled) => Ok(false),
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
