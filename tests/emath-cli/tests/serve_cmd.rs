//! serve_cmd tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli_lab::serve_cmd::*;
use emath_test_harness::{Case, Probe, check_all, expect_ok};
use std::path::{Path, PathBuf};
#[test]
fn probe() {
    let mut p = Probe::new("serve resolves dist safely, sniffs content types, bounds request lines");
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().expect("dir");
    expect_ok(check_all(&[Case::new("in-dist", "/Cargo.toml", true), Case::new("parent", "/../Cargo.toml", false), Case::new("nested", "/foo/../../Cargo.toml", false), Case::new("encoded", "/%2e%2e/Cargo.toml", false), Case::new("escape", "/etc/passwd", false)], |req| safe_file_path(&dist, req).is_some()));
    p.demand("abs-join", safe_file_path(&dist, dist.join("Cargo.toml").to_str().unwrap_or("")).is_none(), "absolute join must not escape");
    expect_ok(check_all(&[Case::new("html", "index.html", "text/html; charset=utf-8"), Case::new("js", "app.js", "text/javascript"), Case::new("css", "style.css", "text/css"), Case::new("wasm", "emath.wasm", "application/wasm"), Case::new("json", "data.json", "application/json"), Case::new("ico", "favicon.ico", "image/x-icon"), Case::new("bin", "blob.bin", "application/octet-stream")], |f| content_type(Path::new(f))));
    p.case("dist", |p| { let cwd = Path::new("/repo"); p.eq("cli", resolve_dist(Some(Path::new("/custom")), Some("/from-env"), cwd), PathBuf::from("/custom")); p.eq("env", resolve_dist(None, Some("/from-env"), cwd), PathBuf::from("/from-env")); p.eq("empty-env", resolve_dist(None, Some(""), cwd), cwd.join("web/dist")); p.eq("default", resolve_dist(None, None, cwd), cwd.join("web/dist")); });
    p.case("args", |p| { let d = parse_serve_args(&[]).expect("defaults"); p.eq("port", d.port, DEFAULT_PORT); p.demand("open", !d.no_open, "open by default"); p.demand("dist", d.dist.is_none(), "no dist by default"); p.demand("bare", parse_serve_args(&["8080".into()]).is_err(), "bare positional refuses"); p.eq("flagged", parse_serve_args(&["--port".into(), "9000".into()]).expect("flag").port, 9000); for bad in ["abc", "0", "65536", "7878.0", ""] { p.demand(bad, parse_serve_args(&["--port".into(), bad.into()]).is_err(), "malformed port refuses"); } p.demand("dashdash", parse_serve_args(&["--".into()]).is_err(), "-- refuses"); p.demand("extra", parse_serve_args(&["--".into(), "extra".into()]).is_err(), "extra refuses"); });
    p.case("request-line", |p| { p.eq("ok", read_request_line("GET /index.html HTTP/1.1\r\n".as_bytes()).as_deref(), Some("GET /index.html HTTP/1.1\r\n")); p.demand("oversized", read_request_line(format!("GET /{} HTTP/1.1\r\n", "a".repeat(MAX_REQUEST_LINE as usize)).as_bytes()).is_none(), "oversized line fails closed"); p.demand("flood", read_request_line("G".repeat(MAX_REQUEST_LINE as usize + 1).as_bytes()).is_none(), "headerless flood fails closed"); });
    p.finish();
}
