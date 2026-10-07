#![forbid(unsafe_code)]

//! Repository tasks: `cargo xtask generate-language`, `check-wasm`,
//! `build-web` and `serve-web`. Std-only.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_core::fnv1a64_bytes as fnv1a64;

mod generate_language;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = if args.first().map(String::as_str) == Some("generate-language") {
        generate_language::run(args.get(1).map(String::as_str) == Some("--cargo-target"))
    } else if args.first().map(String::as_str) == Some("build-web") {
        build_web()
    } else if args.first().map(String::as_str) == Some("check-wasm") {
        check_wasm()
    } else if args.first().map(String::as_str) == Some("serve-web")
        || args.first().map(String::as_str) == Some("serve")
    {
        let port = args
            .get(1)
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        serve_web(port)
    } else {
        eprintln!("usage: cargo xtask generate-language");
        eprintln!("       cargo xtask build-web");
        eprintln!("       cargo xtask check-wasm");
        eprintln!("       cargo xtask serve-web [port]");
        2
    };
    std::process::exit(i32::from(code));
}

/// rustc-check `emath-wasm` for `wasm32-unknown-unknown` so the target
/// does not rot. Not a test suite.
fn check_wasm() -> u8 {
    println!("== check-wasm ==");
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|list| {
            list.lines()
                .any(|line| line.trim() == "wasm32-unknown-unknown")
        });
    if !installed {
        eprintln!("check-wasm: rustup target wasm32-unknown-unknown is not installed");
        eprintln!("            rustup target add wasm32-unknown-unknown");
        return 1;
    }
    let status = Command::new("cargo")
        .args([
            "check",
            "-p",
            "emath-wasm",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .status();
    match status {
        Ok(status) if status.success() => {
            println!("check-wasm: emath-wasm wasm32-unknown-unknown ok");
            0
        }
        Ok(_) => {
            eprintln!(
                "check-wasm: cargo check -p emath-wasm --target wasm32-unknown-unknown failed"
            );
            1
        }
        Err(error) => {
            eprintln!("check-wasm: failed to spawn cargo: {error}");
            1
        }
    }
}

/// Build `emath-wasm` for `wasm32-unknown-unknown` and stage `web/dist/`.
fn build_web() -> u8 {
    println!("== build-web ==");
    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            "emath-wasm",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status();
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            eprintln!("build-web: failed to spawn cargo: {error}");
            return 1;
        }
    };
    if !status.success() {
        eprintln!("build-web: cargo build -p emath-wasm failed");
        return 1;
    }
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let wasm_src = PathBuf::from(&target_dir)
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("emath_wasm.wasm");
    if !wasm_src.is_file() {
        eprintln!("build-web: missing {}", wasm_src.display());
        return 1;
    }
    let dist = PathBuf::from("web/dist");
    if let Err(error) = std::fs::create_dir_all(&dist) {
        eprintln!("build-web: cannot create {}: {error}", dist.display());
        return 1;
    }
    let wasm_dest = dist.join("emath.wasm");
    if let Err(error) = std::fs::copy(&wasm_src, &wasm_dest) {
        eprintln!(
            "build-web: cannot copy {} -> {}: {error}",
            wasm_src.display(),
            wasm_dest.display()
        );
        return 1;
    }
    let wasm_opt_available = Command::new("wasm-opt")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if wasm_opt_available {
        println!("build-web: running wasm-opt -O3 --strip-debug --strip-producers on emath.wasm");
        let opt_status = Command::new("wasm-opt")
            .args(["-O3", "--strip-debug", "--strip-producers", "-o"])
            .arg(&wasm_dest)
            .arg(&wasm_dest)
            .status();
        match opt_status {
            Ok(status) if status.success() => {
                println!("build-web: wasm-opt optimization complete");
            }
            Ok(status) => {
                eprintln!(
                    "build-web: warning: wasm-opt exited with status {:?}",
                    status.code()
                );
            }
            Err(error) => {
                eprintln!("build-web: warning: failed to run wasm-opt: {error}");
            }
        }
    } else {
        println!(
            "build-web: note: wasm-opt not found in PATH; skipping wasm-opt post-processing pass"
        );
    }
    for name in ["index.html", "app.js", "style.css"] {
        let src = PathBuf::from("web").join(name);
        if !src.is_file() {
            eprintln!("build-web: note: {} not found; skipping", src.display());
        }
    }

    // 1. WASM hash
    let wasm_bytes = match std::fs::read(&wasm_dest) {
        Ok(b) => b,
        Err(error) => {
            eprintln!("build-web: cannot read {}: {error}", wasm_dest.display());
            return 1;
        }
    };
    let wasm_hash = format!("{:012x}", fnv1a64(&wasm_bytes));

    // 2. CSS staging and content hash
    let css_src = PathBuf::from("web/style.css");
    let css_hash = if css_src.is_file() {
        let css_bytes = match std::fs::read(&css_src) {
            Ok(b) => b,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", css_src.display());
                return 1;
            }
        };
        if let Err(error) = std::fs::write(dist.join("style.css"), &css_bytes) {
            eprintln!("build-web: cannot write dist/style.css: {error}");
            return 1;
        }
        format!("{:012x}", fnv1a64(&css_bytes))
    } else {
        "0".to_string()
    };

    // 3. JS staging with stamped WASM URL and content hash
    let js_src = PathBuf::from("web/app.js");
    let js_hash = if js_src.is_file() {
        let js_str = match std::fs::read_to_string(&js_src) {
            Ok(s) => s,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", js_src.display());
                return 1;
            }
        };
        let stamped_wasm_target = format!("\"/emath.wasm?v={wasm_hash}\"");
        let stamped_js = js_str.replace("\"/emath.wasm\"", &stamped_wasm_target);
        if let Err(error) = std::fs::write(dist.join("app.js"), stamped_js.as_bytes()) {
            eprintln!("build-web: cannot write dist/app.js: {error}");
            return 1;
        }
        format!("{:012x}", fnv1a64(stamped_js.as_bytes()))
    } else {
        "0".to_string()
    };

    // 4. HTML staging with stamped CSS and JS references
    let html_src = PathBuf::from("web/index.html");
    if html_src.is_file() {
        let html_str = match std::fs::read_to_string(&html_src) {
            Ok(s) => s,
            Err(error) => {
                eprintln!("build-web: cannot read {}: {error}", html_src.display());
                return 1;
            }
        };
        let stamped_css_ref = format!("href=\"/style.css?v={css_hash}\"");
        let stamped_js_ref = format!("src=\"/app.js?v={js_hash}\"");
        let stamped_html = html_str
            .replace("href=\"/style.css\"", &stamped_css_ref)
            .replace("href=\"style.css\"", &stamped_css_ref)
            .replace("src=\"/app.js\"", &stamped_js_ref)
            .replace("src=\"app.js\"", &stamped_js_ref);
        if let Err(error) = std::fs::write(dist.join("index.html"), stamped_html.as_bytes()) {
            eprintln!("build-web: cannot write dist/index.html: {error}");
            return 1;
        }
    }

    println!("build-web: asset cache stamping:");
    println!("  emath.wasm  v={wasm_hash}");
    println!("  style.css   v={css_hash}");
    println!("  app.js      v={js_hash}");
    println!("build-web: wrote {}", dist.display());
    match std::fs::read_dir(&dist) {
        Ok(entries) => {
            let mut names = Vec::new();
            for entry in entries.flatten() {
                names.push(entry.path());
            }
            names.sort();
            let mut budget_failed = false;
            for path in names {
                let raw_size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                match compute_gzip_size(&path) {
                    Ok(gzip_size) => {
                        println!(
                            "  {}  {raw_size} bytes (raw) / {gzip_size} bytes (gzip)",
                            path.display()
                        );
                        if path.file_name().and_then(|n| n.to_str()) == Some("emath.wasm") {
                            const MAX_RAW_BYTES: u64 = 2 * 1024 * 1024; // 2.0 MB
                            const MAX_GZIP_BYTES: u64 = 500 * 1024; // 500 KB
                            if raw_size > MAX_RAW_BYTES {
                                eprintln!(
                                    "build-web: error: emath.wasm raw size {raw_size} exceeds budget of {MAX_RAW_BYTES} bytes"
                                );
                                budget_failed = true;
                            }
                            if gzip_size > MAX_GZIP_BYTES {
                                eprintln!(
                                    "build-web: error: emath.wasm gzip size {gzip_size} exceeds budget of {MAX_GZIP_BYTES} bytes"
                                );
                                budget_failed = true;
                            }
                        }
                    }
                    Err(error) => {
                        println!(
                            "  {}  {raw_size} bytes (raw) / gzip check skipped ({error})",
                            path.display()
                        );
                    }
                }
            }
            if budget_failed {
                return 1;
            }
        }
        Err(error) => eprintln!("build-web: cannot list {}: {error}", dist.display()),
    }
    0
}

fn compute_gzip_size(path: &Path) -> Result<u64, String> {
    let output = Command::new("gzip")
        .args(["-9", "-c"])
        .arg(path)
        .output()
        .map_err(|error| format!("failed to spawn gzip: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "gzip failed with exit status {:?}",
            output.status.code()
        ));
    }
    u64::try_from(output.stdout.len()).map_err(|_| "gzip output size exceeds u64".to_string())
}

/// Serve `web/dist` on `127.0.0.1:{port}` with standard COOP/COEP and cache headers.
fn serve_web(port: u16) -> u8 {
    let dist = PathBuf::from("web/dist");
    let index = dist.join("index.html");
    let wasm = dist.join("emath.wasm");
    if !index.is_file() || !wasm.is_file() {
        println!("serve-web: web/dist incomplete; building web assets...");
        let code = build_web();
        if code != 0 {
            return code;
        }
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("serve-web: failed to bind {addr}: {error}");
            return 1;
        }
    };

    println!("== serve-web ==");
    println!("Serving http://{addr}/");
    println!("Document root: {}", dist.display());
    println!("Headers: COOP=same-origin, COEP=require-corp");
    println!("Press Ctrl+C to stop");

    // One connection at a time on the accept thread. Unbounded
    // `thread::spawn` per accept detached JoinHandles and was a localhost
    // resource/DoS footgun (same fix as `emath web` / serve_cmd).
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = handle_http_connection(stream, &dist);
            }
            Err(error) => {
                eprintln!("serve-web: connection failed: {error}");
            }
        }
    }
    0
}

fn handle_http_connection(mut stream: std::net::TcpStream, dist_dir: &Path) -> std::io::Result<()> {
    use std::io::Read;

    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return Ok(()),
    };

    let request_str = String::from_utf8_lossy(&buf[..n]);
    let mut lines = request_str.lines();
    let req_line = match lines.next() {
        Some(l) => l,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            );
        }
    };

    let mut parts = req_line.split_whitespace();
    let method = match parts.next() {
        Some(m) => m,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            );
        }
    };
    let uri = match parts.next() {
        Some(u) => u,
        None => {
            return send_http_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Bad Request",
                false,
                None,
            );
        }
    };

    if method != "GET" && method != "HEAD" {
        return send_http_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            false,
            None,
        );
    }
    let is_head = method == "HEAD";

    let (path_part, query_part) = match uri.find('?') {
        Some(idx) => (&uri[..idx], Some(&uri[idx + 1..])),
        None => (uri, None),
    };

    let decoded_path = percent_decode(path_part);
    let clean_path = decoded_path.trim_start_matches('/');
    let target_rel = if clean_path.is_empty() {
        "index.html"
    } else {
        clean_path
    };

    if target_rel.contains("..") || target_rel.starts_with('/') || target_rel.starts_with('\\') {
        return send_http_response(
            &mut stream,
            403,
            "Forbidden",
            "text/plain; charset=utf-8",
            b"Forbidden",
            is_head,
            None,
        );
    }

    let file_path = dist_dir.join(target_rel);
    if !file_path.is_file() {
        return send_http_response(
            &mut stream,
            404,
            "Not Found",
            "text/html; charset=utf-8",
            b"<!DOCTYPE html><html><body><h1>404 Not Found</h1></body></html>",
            is_head,
            None,
        );
    }

    let file_bytes = match std::fs::read(&file_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let msg = format!("500 Internal Server Error: {error}");
            return send_http_response(
                &mut stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                msg.as_bytes(),
                is_head,
                None,
            );
        }
    };

    let mime = mime_for_path(&file_path);
    let cache_control = if target_rel == "index.html" || target_rel == "index.htm" {
        "no-cache, no-store, must-revalidate"
    } else if query_part.is_some_and(|q| q.starts_with("v="))
        || file_path.extension().and_then(|s| s.to_str()) == Some("wasm")
    {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    send_http_response(
        &mut stream,
        200,
        "OK",
        mime,
        &file_bytes,
        is_head,
        Some(cache_control),
    )
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(v1), Some(v2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((v1 << 4) | v2);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn send_http_response(
    stream: &mut std::net::TcpStream,
    status_code: u16,
    status_msg: &str,
    content_type: &str,
    body: &[u8],
    is_head: bool,
    cache_control: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write;
    let cc = cache_control.unwrap_or("no-cache");
    let header = format!(
        "HTTP/1.1 {status_code} {status_msg}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\n\
         Cache-Control: {cc}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !is_head {
        stream.write_all(body)?;
    }
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_for_path() {
        assert_eq!(
            mime_for_path(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            mime_for_path(Path::new("app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_for_path(Path::new("style.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(mime_for_path(Path::new("emath.wasm")), "application/wasm");
        assert_eq!(
            mime_for_path(Path::new("data.json")),
            "application/json; charset=utf-8"
        );
        assert_eq!(mime_for_path(Path::new("icon.png")), "image/png");
        assert_eq!(mime_for_path(Path::new("vector.svg")), "image/svg+xml");
        assert_eq!(
            mime_for_path(Path::new("unknown.bin")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("index.html"), "index.html");
        assert_eq!(percent_decode("%2Fpath%2Fto%2Ffile"), "/path/to/file");
        assert_eq!(percent_decode("invalid%2"), "invalid%2");
    }
}
