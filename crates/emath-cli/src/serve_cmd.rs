//! `emath web`: localhost HTTP server for the checked-in web playground.
//!
//! Binds `127.0.0.1` only. Serves files from a `web/dist` directory (or
//! `--dist` / `EMATH_WEB_DIST`) without caching. The process runs until
//! interrupted (Ctrl-C).

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

use crate::{CliExit, EXIT_USAGE};

/// Default bind port when `--port` is omitted.
pub const DEFAULT_PORT: u16 = 7878;
/// Default dist directory, resolved against the process working directory.
const DEFAULT_DIST: &str = "web/dist";
const MISSING_ASSETS: &str = "web assets not built; run `cargo xtask build-web` first";
/// First HTTP request line only. Local serve is 1:1; do not slurp headers/body.
pub const MAX_REQUEST_LINE: u64 = 8192;

/// Serve the web playground on `127.0.0.1` until interrupted (Ctrl-C).
/// Returns [`EXIT_USAGE`] on missing dist or bind failure; does not return on success.
pub fn web_cmd(parsed: ServeArgs) -> CliExit {
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
            if error.kind() == ErrorKind::AddrInUse {
                eprintln!("port is in use; pass --port with a free port");
            }
            return EXIT_USAGE;
        }
    };
    let url = format!("http://127.0.0.1:{port}/");
    println!("emath web: {url} (dist: {})", dist.display());
    if !parsed.no_open {
        open_browser(&url);
    }
    // One connection at a time: unbounded per-accept spawn was a
    // resource/DoS footgun.
    loop {
        let Ok((stream, _)) = listener.accept() else {
            continue;
        };
        handle_connection(stream, &dist);
    }
}

/// Backwards-compatible alias for [`web_cmd`].
#[allow(dead_code)]
pub fn serve_cmd(args: ServeArgs) -> CliExit {
    web_cmd(args)
}

pub struct ServeArgs {
    pub port: u16,
    pub no_open: bool,
    pub dist: Option<PathBuf>,
}

fn assign_once<T>(slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.is_some() {
        Err("duplicate flag".to_string())
    } else {
        *slot = Some(value);
        Ok(())
    }
}

pub fn parse_serve_args(args: &[String]) -> Result<ServeArgs, String> {
    let mut port = None;
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
                assign_once(&mut port, parsed)?;
            }
            "--no-open" => no_open = true,
            "--dist" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--dist requires a path".into());
                };
                assign_once(&mut dist, PathBuf::from(value))?;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown flag `{other}`"));
            }
            other => {
                return Err(format!(
                    "unexpected argument `{other}`; pass --port N (default {DEFAULT_PORT})"
                ));
            }
        }
        index += 1;
    }
    Ok(ServeArgs {
        port: port.unwrap_or(DEFAULT_PORT),
        no_open,
        dist,
    })
}

/// `--dist` wins, then `EMATH_WEB_DIST`, then `<cwd>/web/dist` (with upward search).
pub fn resolve_dist(flag: Option<&Path>, env_value: Option<&str>, cwd: &Path) -> PathBuf {
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

/// Map a request URL path onto a file inside `dist`; rejects `..`,
/// absolute escapes, and anything not canonicalizing inside `dist`.
pub fn safe_file_path(dist: &Path, request_path: &str) -> Option<PathBuf> {
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
pub fn content_type(path: &Path) -> &'static str {
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

pub fn read_request_line(reader: impl BufRead) -> Option<String> {
    let mut limited = reader.take(MAX_REQUEST_LINE + 1);
    let mut line = String::new();
    limited.read_line(&mut line).ok()?;
    if line.is_empty() || u64::try_from(line.len()).unwrap_or(u64::MAX) > MAX_REQUEST_LINE {
        None
    } else {
        Some(line)
    }
}

fn handle_connection(mut stream: TcpStream, dist: &Path) {
    let request_line = {
        let reader = BufReader::new(&stream);
        match read_request_line(reader) {
            Some(line) => line,
            None => {
                write_response(
                    &mut stream,
                    400,
                    "Bad Request",
                    "text/plain; charset=utf-8",
                    b"Bad Request\n",
                );
                return;
            }
        }
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
