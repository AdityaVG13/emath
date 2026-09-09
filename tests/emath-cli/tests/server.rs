//! `emath-cli` lsp server state-machine tests.
use emath_cli::lsp::json::JsonValue;
use emath_cli::lsp::protocol::RpcMessage;
use emath_cli::lsp::server::ServerState;
use emath_test_harness::Probe;
use std::collections::BTreeMap;
fn obj(fields: BTreeMap<String, JsonValue>) -> JsonValue { JsonValue::Object(fields) }
fn str_field(k: &str, v: &str) -> (String, JsonValue) { (k.into(), JsonValue::String(v.into())) }
fn num_field(k: &str, v: i64) -> (String, JsonValue) { (k.into(), JsonValue::Number(v)) }
fn doc(uri: &str, text: &str) -> JsonValue {
    obj(BTreeMap::from([("textDocument".into(), obj(BTreeMap::from([str_field("uri", uri), str_field("text", text)])))]))
}
fn pos(line: i64, ch: i64) -> JsonValue { obj(BTreeMap::from([num_field("line", line), num_field("character", ch)])) }
fn edit(uri: &str, sl: i64, sc: i64, el: i64, ec: i64, text: &str) -> JsonValue {
    let range = obj(BTreeMap::from([("start".into(), pos(sl, sc)), ("end".into(), pos(el, ec))]));
    let change = obj(BTreeMap::from([("range".into(), range), str_field("text", text)]));
    obj(BTreeMap::from([
        ("textDocument".into(), obj(BTreeMap::from([str_field("uri", uri)]))),
        ("contentChanges".into(), JsonValue::Array(vec![change])),
    ]))
}
fn open(state: &mut ServerState, uri: &str, text: &str, out: &mut Vec<u8>) {
    state.handle(&RpcMessage { id: None, method: "textDocument/didOpen".into(), params: doc(uri, text) }, out).expect("didOpen");
}
fn nonzero(body: &str) -> bool {
    const M: &str = "\"range\":{\"end\":{\"character\":";
    let num = |t: &str| t.find(|c: char| !(c.is_ascii_digit() || c == '-')).map(|e| t[..e].parse::<i64>().ok()).unwrap_or(None);
    let mut rest = body;
    while let Some(i) = rest.find(M) {
        let s = &rest[i + M.len()..];
        let (Some(ec), Some(a)) = (num(s), s.split_once(",\"line\":")) else { rest = &rest[i + 1..]; continue; };
        let (Some(el), Some(b)) = (num(a.1), a.1.split_once("\"start\":{\"character\":")) else { rest = &rest[i + 1..]; continue; };
        let (Some(sc), Some(c)) = (num(b.1), b.1.split_once(",\"line\":")) else { rest = &rest[i + 1..]; continue; };
        if num(c.1).is_some_and(|sl| (el, ec) != (sl, sc)) { return true; }
        rest = &rest[i + 1..];
    }
    false
}
#[test]
fn probe() {
    let mut p = Probe::new("lsp server keeps utf-8 byte offsets, hover at zero, nonzero diagnostic ranges");
    p.case("utf8-edit", |p| { let (mut s, mut o) = (ServerState::new(), vec![]); open(&mut s, "file:///g.emath", "ab ⋈ cd\n", &mut o); s.handle(&RpcMessage { id: None, method: "textDocument/didChange".into(), params: edit("file:///g.emath", 0, 7, 0, 8, "X") }, &mut o).expect("didChange"); p.eq("byte7", s.documents.get("file:///g.emath").expect("doc").text.clone(), "ab ⋈ Xd\n".to_string()); });
    p.case("encoding", |p| { let (mut s, mut o) = (ServerState::new(), vec![]); s.handle(&RpcMessage { id: Some(1), method: "initialize".into(), params: obj(BTreeMap::new()) }, &mut o).expect("init"); p.contains("utf8", &String::from_utf8(o).expect("utf8"), "\"positionEncoding\":\"utf-8\""); });
    p.case("glyph-edit", |p| { let (mut s, mut o) = (ServerState::new(), vec![]); open(&mut s, "file:///g.emath", "emath custom G:\n    bbs:\n        ⧖(a ⋈ b) ⊛ ζ\n", &mut o); s.handle(&RpcMessage { id: None, method: "textDocument/didChange".into(), params: edit("file:///g.emath", 2, 21, 2, 27, "⋈ ζ") }, &mut o).expect("didChange"); p.eq("roundtrip", s.documents.get("file:///g.emath").expect("doc").text.clone(), "emath custom G:\n    bbs:\n        ⧖(a ⋈ b) ⋈ ζ\n".to_string()); });
    p.case("hover-zero", |p| { let (mut s, mut o) = (ServerState::new(), vec![]); open(&mut s, "file:///hover0.emath", "emath custom X:\n", &mut o); o.clear(); s.handle(&RpcMessage { id: Some(7), method: "textDocument/hover".into(), params: obj(BTreeMap::from([("textDocument".into(), obj(BTreeMap::from([str_field("uri", "file:///hover0.emath")]))), ("position".into(), pos(0, 0))])) }, &mut o).expect("hover"); p.contains("keyword", &String::from_utf8(o).expect("utf8"), "**emath**"); });
    p.case("diag-end", |p| { let (mut s, mut o) = (ServerState::new(), vec![]); open(&mut s, "file:///diag-end.emath", "emath custom\n", &mut o); let body = String::from_utf8(o).expect("utf8"); p.contains("publish", &body, "publishDiagnostics"); p.demand("nonzero", nonzero(&body), "a range must cover primary.end"); });
    p.finish();
}
