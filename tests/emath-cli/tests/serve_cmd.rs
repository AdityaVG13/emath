//! serve_cmd tests migrated from the in-crate `#[cfg(test)]` module.

use emath_cli::serve_cmd::*;
use std::path::{Path, PathBuf};

#[test]
fn serve_path_sanitization_rejects_traversal() {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("cli crate dir");
    assert!(
        safe_file_path(&dist, "/Cargo.toml").is_some(),
        "in-dist file must resolve"
    );
    assert!(
        safe_file_path(&dist, "/../Cargo.toml").is_none(),
        "parent traversal must be rejected"
    );
    assert!(
        safe_file_path(&dist, "/foo/../../Cargo.toml").is_none(),
        "nested parent traversal must be rejected"
    );
    assert!(
        safe_file_path(&dist, "/%2e%2e/Cargo.toml").is_none(),
        "percent-encoded traversal must be rejected"
    );
    assert!(
        safe_file_path(&dist, "/etc/passwd").is_none(),
        "absolute escape must be rejected"
    );
    assert!(
        safe_file_path(&dist, dist.join("Cargo.toml").to_str().unwrap_or("")).is_none(),
        "absolute filesystem path must not escape via join"
    );
}

#[test]
fn serve_content_type_maps_known_extensions() {
    assert_eq!(
        content_type(Path::new("index.html")),
        "text/html; charset=utf-8"
    );
    assert_eq!(content_type(Path::new("app.js")), "text/javascript");
    assert_eq!(content_type(Path::new("style.css")), "text/css");
    assert_eq!(content_type(Path::new("emath.wasm")), "application/wasm");
    assert_eq!(content_type(Path::new("data.json")), "application/json");
    assert_eq!(content_type(Path::new("favicon.ico")), "image/x-icon");
    assert_eq!(
        content_type(Path::new("blob.bin")),
        "application/octet-stream"
    );
}

#[test]
fn serve_dist_resolution_order() {
    let cwd = Path::new("/repo");
    assert_eq!(
        resolve_dist(Some(Path::new("/custom")), Some("/from-env"), cwd),
        PathBuf::from("/custom")
    );
    assert_eq!(
        resolve_dist(None, Some("/from-env"), cwd),
        PathBuf::from("/from-env")
    );
    assert_eq!(resolve_dist(None, Some(""), cwd), cwd.join("web/dist"));
    assert_eq!(resolve_dist(None, None, cwd), cwd.join("web/dist"));
}

#[test]
fn parse_serve_args_defaults_port_and_refuses_bare_positional() {
    let parsed = parse_serve_args(&[]).expect("empty args are defaults");
    assert_eq!(parsed.port, DEFAULT_PORT);
    assert!(!parsed.no_open);
    assert!(parsed.dist.is_none());
    assert!(parse_serve_args(&["8080".into()]).is_err());
    let flagged = parse_serve_args(&["--port".into(), "9000".into()]).expect("flagged port");
    assert_eq!(flagged.port, 9000);
    for bad in ["abc", "0", "65536", "7878.0", ""] {
        assert!(
            parse_serve_args(&["--port".into(), bad.into()]).is_err(),
            "malformed --port {bad} must not default to {DEFAULT_PORT}"
        );
    }
    assert!(
        parse_serve_args(&["--".into()]).is_err(),
        "`--` is not an independently legal no-op on serve/web"
    );
    assert!(
        parse_serve_args(&["--".into(), "extra".into()]).is_err(),
        "extra tokens after `--` must not start the server"
    );
}

#[test]
fn request_line_refuses_unbounded_input() {
    let ok = read_request_line("GET /index.html HTTP/1.1\r\n".as_bytes());
    assert_eq!(ok.as_deref(), Some("GET /index.html HTTP/1.1\r\n"));
    let oversized = format!(
        "GET /{} HTTP/1.1\r\n",
        "a".repeat(MAX_REQUEST_LINE as usize)
    );
    assert!(
        read_request_line(oversized.as_bytes()).is_none(),
        "request line above {MAX_REQUEST_LINE} bytes must fail closed"
    );
    let no_newline = "G".repeat(MAX_REQUEST_LINE as usize + 1);
    assert!(
        read_request_line(no_newline.as_bytes()).is_none(),
        "headerless flood without newline must fail closed"
    );
}
