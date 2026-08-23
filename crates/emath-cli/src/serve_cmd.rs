//! `emath web`: localhost HTTP server for the checked-in web playground.
//!
//! Binds `127.0.0.1` only. Serves files from a `web/dist` directory (or
//! `--dist` / `EMATH_WEB_DIST`) without caching. The process runs until
//! interrupted (Ctrl-C).

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

use crate::EXIT_USAGE;

/// Default bind port when `--port` is omitted.
const DEFAULT_PORT: u16 = 7878;
/// Default dist directory, resolved against the process working directory.
const DEFAULT_DIST: &str = "web/dist";
const USAGE: &str = "web [--port N] [--no-open] [--dist PATH]";
const MISSING_ASSETS: &str = "web assets not built; run `cargo xtask build-web` first";

/// Serve the web playground on `127.0.0.1` until the process is interrupted
/// (Ctrl-C).
///
/// Returns [`EXIT_USAGE`] when the dist directory is missing, the port is
/// busy, or arguments are invalid. Does not return on the success path.
pub fn web_cmd(args: &[String]) -> u8 {
    let parsed = match parse_serve_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("usage: emath {USAGE}");
            eprintln!("try: emath help web");
            return EXIT_USAGE;
        }
    };
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_dist = env::var("EMATH_WEB_DIST").ok();
    let dist = resolve_dist(parsed.dist.as_deref(), env_dist.as_deref(), &cwd);
    if !dist_is_ready(&dist) {
        eprintln!("error: {MISSING_ASSETS}");
        return EXIT_USAGE;
    }
    let port = parsed.port;
    let addr = format!("127.0.0.1:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("error: cannot bind {addr}: {error}");
            eprintln!("port is in use; pass --port with a free port");
            return EXIT_USAGE;
        }
    };
    let url = format!("http://127.0.0.1:{port}/");
    println!("emath web: {url} (dist: {})", dist.display());
    if !parsed.no_open {
        open_browser(&url);
    }
    // Handle one connection at a time on the accept thread. This is a
    // localhost playground (127.0.0.1 only): unbounded `thread::spawn` per
    // accept was a resource/DoS footgun and left JoinHandles detached.
    loop {
        let Ok((stream, _)) = listener.accept() else {
            continue;
        };
        handle_connection(stream, &dist);
    }
}

/// Backwards-compatible alias for [`web_cmd`].
#[allow(dead_code)]
pub fn serve_cmd(args: &[String]) -> u8 {
    web_cmd(args)
}

struct ServeArgs {
    port: u16,
    no_open: bool,
    dist: Option<PathBuf>,
}

fn parse_serve_args(args: &[String]) -> Result<ServeArgs, String> {
    let mut port = DEFAULT_PORT;
    let mut no_open = false;
    let mut dist = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--port requires a number".into());
                };
                let parsed: u16 = value
                    .parse()
                    .map_err(|_| "--port requires a number in 1..=65535".to_string())?;
                if parsed == 0 {
                    return Err("--port requires a number in 1..=65535".into());
                }
                port = parsed;
            }
            "--no-open" => no_open = true,
            "--dist" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--dist requires a path".into());
                };
                dist = Some(PathBuf::from(value));
            }
            "--" => break,
            other if other.starts_with('-') => {}
            _ => {}
        }
        index += 1;
    }
    Ok(ServeArgs {
        port,
        no_open,
        dist,
    })
}

/// `--dist` wins, then `EMATH_WEB_DIST`, then `<cwd>/web/dist` (with upward search).
fn resolve_dist(flag: Option<&Path>, env_value: Option<&str>, cwd: &Path) -> PathBuf {
    if let Some(path) = flag {
        return path.to_path_buf();
    }
    if let Some(value) = env_value.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    let direct = cwd.join(DEFAULT_DIST);
    if dist_is_ready(&direct) {
        return direct;
    }
    // Search upwards from cwd for web/dist (e.g. if invoked from a crate or subfolder)
    let mut current = cwd;
    while let Some(parent) = current.parent() {
        let candidate = parent.join(DEFAULT_DIST);
        if dist_is_ready(&candidate) {
            return candidate;
        }
        current = parent;
    }
    direct
}

fn dist_is_ready(dist: &Path) -> bool {
    dist.is_dir() && dist.join("index.html").is_file()
}

/// Maps a request URL path onto a file inside `dist`.
///
/// Rejects any path containing `..`, absolute escapes, and anything that
/// does not canonicalize inside `dist`.
fn safe_file_path(dist: &Path, request_path: &str) -> Option<PathBuf> {
    let path = request_path.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_decode(path)?;
    if decoded.contains("..") || decoded.contains('\0') {
        return None;
    }
    let trimmed = decoded.trim_start_matches('/');
    let relative = if trimmed.is_empty() {
        Path::new("index.html")
    } else {
        Path::new(trimmed)
    };
    if relative.is_absolute() {
        return None;
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    confirm_inside(dist, &dist.join(relative))
}

fn confirm_inside(dist: &Path, candidate: &Path) -> Option<PathBuf> {
    let dist_canon = dist.canonicalize().ok()?;
    let file_canon = candidate.canonicalize().ok()?;
    if file_canon.starts_with(&dist_canon) && file_canon.is_file() {
        Some(file_canon)
    } else {
        None
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = hex_nibble(bytes[index + 1])?;
            let low = hex_nibble(bytes[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Content-Type for a served file extension.
fn content_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn handle_connection(mut stream: TcpStream, dist: &Path) {
    let request_line = {
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        line
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    if method != "GET" {
        write_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed\n",
        );
        return;
    }
    let mapped = if path == "/" { "/index.html" } else { path };
    match safe_file_path(dist, mapped) {
        Some(file) => match fs::read(&file) {
            Ok(body) => {
                write_response(&mut stream, 200, "OK", content_type(&file), &body);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                write_not_found(&mut stream);
            }
            Err(_) => write_response(
                &mut stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                b"Internal Server Error\n",
            ),
        },
        None => write_not_found(&mut stream),
    }
}

fn write_not_found(stream: &mut TcpStream) {
    write_response(
        stream,
        404,
        "Not Found",
        "text/plain; charset=utf-8",
        b"Not Found\n",
    );
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) {
    let len = body.len();
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n"
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn open_browser(url: &str) {
    let _ = url;
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
