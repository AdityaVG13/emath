//! `emath-cli` lsp async stdio transport tests.
use asupersync::Cx;
use asupersync::channel::mpsc;
use asupersync::io::ReadBuf;
use asupersync::io::{AsyncRead, AsyncWrite};
use asupersync::runtime::JoinError;
use emath_cli::lsp::json::JsonValue;
use emath_cli::lsp::lab::run_with_cx;
use emath_cli::lsp::protocol::read_message;
use emath_cli::lsp::transport::{Control, Transport, TransportError, read_frame, write_frame};
use emath_test_harness::Probe;
use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
const MAX_FRAME_BODY: usize = 16 * 1024 * 1024;
fn run<F, Fut>(f: F) where F: FnOnce(Cx) -> Fut, Fut: Future<Output = ()> { run_with_cx(f); }
#[derive(Debug)]
struct PendAfterReader { data: Vec<u8>, pos: usize }
impl AsyncRead for PendAfterReader { fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> { let t = self.get_mut(); if t.pos == t.data.len() { return Poll::Pending; } let take = (t.data.len() - t.pos).min(buf.remaining()); buf.put_slice(&t.data[t.pos..t.pos + take]); t.pos += take; Poll::Ready(Ok(())) } }
#[derive(Debug)]
struct RecordingWriter(Rc<RefCell<Vec<u8>>>);
impl AsyncWrite for RecordingWriter { fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> { self.0.borrow_mut().extend_from_slice(buf); Poll::Ready(Ok(buf.len())) } fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) } fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) } }
struct FailingWriter;
impl AsyncWrite for FailingWriter { fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &[u8]) -> Poll<io::Result<usize>> { Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "sink down"))) } fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) } fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) } }
struct FlushFailingWriter;
impl AsyncWrite for FlushFailingWriter { fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> { Poll::Ready(Ok(buf.len())) } fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Err(io::Error::other("flush down"))) } fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) } }
const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
const SHUTDOWN: &str = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#;
const EXIT: &str = r#"{"jsonrpc":"2.0","method":"exit"}"#;
fn json(body: Vec<u8>) -> JsonValue { JsonValue::parse(&String::from_utf8(body).expect("utf8")).expect("json") }
fn framed(parts: &[&str]) -> Vec<u8> { let slot = Rc::new(RefCell::new(Vec::new())); let s = slot.clone(); let owned: Vec<String> = parts.iter().map(|x| x.to_string()).collect(); run(|_cx| async move { let mut v = Vec::new(); for part in &owned { write_frame(&mut v, part.as_bytes()).await.expect("frame"); } *s.borrow_mut() = v; }); slot.borrow().clone() }
fn serve_sync(input: Vec<u8>) -> (u8, Vec<u8>) { let slot = Rc::new(RefCell::new((0u8, Vec::new()))); let s = slot.clone(); run(|cx| async move { let rec = Rc::new(RefCell::new(Vec::new())); let mut t = Transport::new(&input[..], RecordingWriter(rec.clone())); let code = t.serve(&cx).await.expect("serve"); *s.borrow_mut() = (code, rec.borrow().clone()); }); slot.borrow().clone() }
fn parse_all(out: Vec<u8>) -> String { let slot = Rc::new(RefCell::new(String::new())); let (s, oc) = (slot.clone(), out.clone()); run(|_cx| async move { let mut cur = &oc[..]; let mut ids = vec![]; while let Some(b) = read_frame(&mut cur).await.expect("read") { ids.push(json(b).get_int("id").unwrap_or(-999)); } *s.borrow_mut() = format!("{ids:?}|{}", cur.is_empty()); }); slot.borrow().clone() }
fn parse_two(out: Vec<u8>) -> (JsonValue, JsonValue, bool) { let slot = Rc::new(RefCell::new(None)); let (s, oc) = (slot.clone(), out.clone()); run(|_cx| async move { let mut cur = &oc[..]; let a = json(read_frame(&mut cur).await.expect("r").expect("first")); let b = json(read_frame(&mut cur).await.expect("r").expect("second")); let clean = read_frame(&mut cur).await.expect("eof").is_none(); *s.borrow_mut() = Some((a, b, clean)); }); slot.borrow_mut().take().expect("parsed") }
#[test]
fn probe() {
    let mut p = Probe::new("async transport frames, serves, refuses, and cancels like the blocking lane");
    p.case("interop", |p| { let wire = framed(&[INITIALIZE]); let mut cur = std::io::Cursor::new(&wire[..]); let m = read_message(&mut cur).expect("blocking reads async frame").expect("frame"); p.eq("id", m.id, Some(1)); p.eq("method", m.method, "initialize".to_string()); });
    p.case("exits", |p| { p.eq("full", serve_sync(framed(&[INITIALIZE, SHUTDOWN, EXIT])).0, 0); p.eq("eof", serve_sync(framed(&[INITIALIZE])).0, 1); p.eq("shutdown-eof", serve_sync(framed(&[INITIALIZE, SHUTDOWN])).0, 0); });
    p.case("responses", |p| {
        let (_, o) = serve_sync(framed(&[INITIALIZE, SHUTDOWN, EXIT])); let (a, b, clean) = parse_two(o);
        p.eq("first-id", a.get_int("id"), Some(1)); p.demand("caps", a.get("result").is_some_and(|r| r.get("capabilities").is_some()), "capabilities advertised"); p.demand("info", a.get("result").is_some_and(|r| r.get("serverInfo").is_some()), "serverInfo advertised");
        p.eq("second-id", b.get_int("id"), Some(2)); p.demand("null", b.get("result").is_some_and(JsonValue::is_null), "shutdown result null"); p.demand("clean", clean, "no frame follows exit");
    });
    p.case("eof-frames", |p| {
        let (_, o) = serve_sync(framed(&[INITIALIZE])); let slot = Rc::new(RefCell::new(String::new())); let (s, oc) = (slot.clone(), o.clone()); let t = s.clone();
        run(|_cx| async move { let mut cur = &oc[..]; let a = json(read_frame(&mut cur).await.expect("r").expect("resp")); *t.borrow_mut() = format!("{}|{}", a.get_int("id").unwrap_or(-1), read_frame(&mut cur).await.expect("eof").is_none()); });
        p.eq("init-then-eof", s.borrow().clone(), "1|true".to_string());
        let (_, o) = serve_sync(framed(&[INITIALIZE, SHUTDOWN])); p.eq("two-frames", parse_all(o), "[1, 2]|true".to_string());
    });
    p.case("refusals", |p| { for (name, wire) in [("garbage-cl", b"Content-Length: xyz\r\n\r\n".as_slice()), ("eof-header", b"Content-Length: ".as_slice()), ("short-body", b"Content-Length: 20\r\n\r\nhello".as_slice())] { let (c, o) = serve_sync(wire.to_vec()); p.eq(name, c, 1); let slot = Rc::new(RefCell::new(None)); let (s, oc) = (slot.clone(), o.clone()); let t = s.clone(); run(|_cx| async move { let mut cur = &oc[..]; let a = json(read_frame(&mut cur).await.expect("r").expect("error")); let clean = read_frame(&mut cur).await.expect("eof").is_none(); *t.borrow_mut() = Some((a, clean)); }); let (a, clean) = slot.borrow_mut().take().expect("parsed"); p.eq(name, (a.get_int("id"), a.get("error").expect("e").get_int("code"), clean), (None, Some(-32700), true)); } });
    p.case("caps", |p| {
        let slot = Rc::new(RefCell::new(String::new())); let s = slot.clone(); let t = s.clone(); run(|_cx| async move { let wire: &[u8] = b"Content-Length: 17000000\r\n\r\n"; let mut cur = wire; match read_frame(&mut cur).await { Err(TransportError::BodyTooLarge { length: 17_000_000, max }) => *t.borrow_mut() = max.to_string(), other => *t.borrow_mut() = format!("wrong {other:?}") } }); p.eq("body-cap", s.borrow().clone(), MAX_FRAME_BODY.to_string());
        let (c, o) = serve_sync(format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BODY + 1).into_bytes()); p.eq("serve-cap-exit", c, 1);
        let slot = Rc::new(RefCell::new(String::new())); let (s, oc) = (slot.clone(), o.clone()); let t = s.clone(); run(|_cx| async move { let mut cur = &oc[..]; let q = json(read_frame(&mut cur).await.expect("r").expect("e")); *t.borrow_mut() = format!("{:?}|{:?}|{}|{}", q.get_int("id"), q.get("error").expect("e").get_int("code"), q.get("error").expect("e").get_str("message").expect("m").contains("exceeds"), read_frame(&mut cur).await.expect("eof").is_none()); }); p.eq("serve-cap-frame", s.borrow().clone(), "None|Some(-32700)|true|true".to_string());
        let slot = Rc::new(RefCell::new(String::new())); let s = slot.clone(); let t = s.clone(); run(|_cx| async move { for (w, e) in [("Content-Length: 5\r\n\r\nhello", "hello"), ("content-length: 5\r\n\r\nhello", "hello"), ("CONTENT-LENGTH: 5\r\n\r\nhello", "hello"), ("Content-Length:   5   \r\n\r\nhello", "hello"), ("X-Custom: abc\r\nContent-Length: 5\r\n\r\nhello", "hello")] { let mut cur: &[u8] = w.as_bytes(); t.borrow_mut().push_str(&format!("{}|", String::from_utf8(read_frame(&mut cur).await.expect("accept").expect("body")).unwrap() == e)); } }); p.eq("parity", s.borrow().clone(), "true|true|true|true|true|".to_string());
        let slot = Rc::new(RefCell::new(String::new())); let s = slot.clone(); let t = s.clone(); run(|_cx| async move { let wire = format!("X-Pad: {}\r\n\r\n", "a".repeat(5000)); let mut cur: &[u8] = wire.as_bytes(); match read_frame(&mut cur).await { Err(TransportError::Frame(m)) => *t.borrow_mut() = m.contains("header line too long").to_string(), other => *t.borrow_mut() = format!("wrong {other:?}") } }); p.eq("header-cap", s.borrow().clone(), "true".to_string());
    });
    p.case("control", |p| { let input = framed(&[INITIALIZE, SHUTDOWN]); let slot = Rc::new(RefCell::new((0u8, String::new()))); let s = slot.clone(); let t = s.clone(); run(|cx| async move { let (tx, rx) = mpsc::channel::<Control>(1); tx.try_send(Control::Shutdown).expect("signal"); let rec = Rc::new(RefCell::new(Vec::new())); let mut tr = Transport::with_control(&input[..], RecordingWriter(rec.clone()), Some(rx)); let code = tr.serve(&cx).await.expect("serve"); let out = rec.borrow().clone(); let mut cur = &out[..]; let first = json(read_frame(&mut cur).await.expect("r").expect("resp")); *t.borrow_mut() = (code, format!("{}|{}", first.get_int("id").unwrap_or(-1), read_frame(&mut cur).await.expect("eof").is_none())); }); let (c, f) = s.borrow().clone(); p.eq("exit", c, 0); p.eq("flushed-once", f, "1|true".to_string()); });
    p.case("writers", |p| { for flush in [false, true] { let input = framed(&[INITIALIZE]); let slot = Rc::new(RefCell::new(String::new())); let s = slot.clone(); let t = s.clone(); run(|cx| async move { let r = if flush { let mut tr = Transport::new(&input[..], FlushFailingWriter); match tr.serve(&cx).await { Err(TransportError::Io(_)) => "io", _ => "wrong" } } else { let mut tr = Transport::new(&input[..], FailingWriter); match tr.serve(&cx).await { Err(TransportError::Io(_)) => "io", _ => "wrong" } }; *t.borrow_mut() = r.to_string(); }); p.eq(if flush { "flush" } else { "write" }, s.borrow().clone(), "io".to_string()); } });
    p.case("cancel", |p| {
        let input = framed(&[INITIALIZE]); let slot = Rc::new(RefCell::new(String::new())); let s = slot.clone(); let t = s.clone(); run(|cx| async move { let mut tr = Transport::new(PendAfterReader { data: input, pos: 0 }, Vec::new()); let task = cx.spawn(|tc| async move { tr.serve(&tc).await }); let mut task = task.expect("spawn"); task.abort(); match task.join(&cx).await { Err(JoinError::Cancelled(_)) => *t.borrow_mut() = "cancelled".into(), other => *t.borrow_mut() = format!("wrong {other:?}") } }); p.eq("abort", s.borrow().clone(), "cancelled".to_string());
        {
            let slot = Rc::new(RefCell::new(false));
            let t = slot.clone();
            run(|cx| async move {
                let mut tr = Transport::new(PendAfterReader { data: Vec::new(), pos: 0 }, Vec::new());
                let task = cx.spawn(|tc| async move { tr.serve(&tc).await });
                *t.borrow_mut() = task.is_ok();
                if let Ok(task) = task { drop(task); }
            });
            p.demand("drain", *slot.borrow(), "region close drains a pending serve without hanging");
        }
        {
            let slot = Rc::new(RefCell::new(false));
            let t = slot.clone();
            run(|cx| async move {
                let mut tr = Transport::new(PendAfterReader { data: b"Content-Length: 10\r\n\r\nhello".to_vec(), pos: 0 }, Vec::new());
                let task = cx.spawn(|tc| async move { tr.serve(&tc).await });
                *t.borrow_mut() = task.is_ok();
                if let Ok(task) = task { drop(task); }
            });
            p.demand("mid-body", *slot.borrow(), "pending body read cancels on region close");
        }
    });
    p.case("second-frame", |p| { let mut input = framed(&[INITIALIZE]); input.extend_from_slice(b"Content-Length: 100\r\n\r\nabc"); let (code, out) = serve_sync(input); let slot = Rc::new(RefCell::new((0u8, String::new()))); let (s, oc) = (slot.clone(), out.clone()); let t = s.clone(); run(|_cx| async move { let mut cur = &oc[..]; let a = json(read_frame(&mut cur).await.expect("r").expect("first")); let b = json(read_frame(&mut cur).await.expect("r").expect("error")); *t.borrow_mut() = (0, format!("{}|{:?}|{}|{}", a.get_int("id").unwrap_or(-1), b.get_int("id"), b.get("error").is_some(), read_frame(&mut cur).await.expect("eof").is_none())); }); let (_, f) = s.borrow().clone(); p.eq("exit", code, 1); p.eq("frames", f, "1|None|true|true".to_string()); });
    p.case("determinism", |p| { let (a, b) = (serve_sync(framed(&[INITIALIZE, SHUTDOWN, EXIT])), serve_sync(framed(&[INITIALIZE, SHUTDOWN, EXIT]))); p.eq("code", a.0, b.0); p.eq("bytes", a.1.clone(), b.1.clone()); p.demand("nonempty", !a.1.is_empty(), "responses produced"); });
    p.case("single-flight", |p| { let (code, out) = serve_sync(framed(&[INITIALIZE, SHUTDOWN])); let slot = Rc::new(RefCell::new(String::new())); let (s, oc) = (slot.clone(), out.clone()); let t = s.clone(); run(|_cx| async move { let mut cur = &oc[..]; let b1 = read_frame(&mut cur).await.expect("r").expect("first"); let b2 = read_frame(&mut cur).await.expect("r").expect("second"); let clean = read_frame(&mut cur).await.expect("eof").is_none(); let mut exp = Vec::new(); write_frame(&mut exp, &b1).await.expect("h1"); write_frame(&mut exp, &b2).await.expect("h2"); *t.borrow_mut() = format!("{}|{}|{clean}|{}", json(b1).get_int("id").unwrap_or(-1), json(b2).get_int("id").unwrap_or(-1), oc == exp); }); p.eq("order", (code, s.borrow().clone()), (0, "1|2|true|true".to_string())); });
    p.finish();
}
