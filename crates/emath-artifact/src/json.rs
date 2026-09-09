//! Streaming JSON writer re-export and low-level scanning helpers.

use super::*;

pub use emath_core::{JsonObject, JsonWriter};

/// Serialize an id field; an unresolved (empty) id still must produce a
/// valid JSON string, otherwise `"field": ` would be emitted and no
/// reader could ever parse the document (documents are read back).
pub(super) fn content_id_or_empty(id: &ContentId) -> String {
    if id.0.is_empty() {
        quote("")
    } else {
        quote(&id.0)
    }
}

/// Parse the `files` inventory of a serialized artifact manifest into
/// `path -> declared content id`. Accepts exactly the writer's shape;
/// anything else is refused, so a corrupted manifest cannot disable
/// content-identity verification.
pub fn manifest_files_declared(
    manifest_json: &str,
) -> Result<BTreeMap<String, String>, ArtifactError> {
    let bytes = manifest_json.as_bytes();
    let malformed = |detail: &str| ArtifactError::ManifestMalformed(detail.to_string());
    let key = b"\"files\"";
    let Some(relative) = find_subslice(bytes, key) else {
        return Err(malformed("missing `files` field"));
    };
    let mut index = relative + key.len();
    skip_json_ws(bytes, &mut index);
    if bytes.get(index) != Some(&b':') {
        return Err(malformed("`files` field has no colon"));
    }
    index += 1;
    skip_json_ws(bytes, &mut index);
    if bytes.get(index) != Some(&b'{') {
        return Err(malformed("`files` field is not an object"));
    }
    index += 1;
    let mut files = BTreeMap::new();
    loop {
        skip_json_ws(bytes, &mut index);
        match bytes.get(index) {
            Some(b'}') => break,
            Some(b',') => {
                index += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return Err(malformed("unexpected token in `files` object")),
        }
        let Some((path, next)) = parse_json_string(bytes, index) else {
            return Err(malformed("malformed path string in `files` object"));
        };
        index = next;
        skip_json_ws(bytes, &mut index);
        if bytes.get(index) != Some(&b':') {
            return Err(malformed("path entry has no colon"));
        }
        index += 1;
        skip_json_ws(bytes, &mut index);
        let Some((id, next)) = parse_json_string(bytes, index) else {
            return Err(malformed("malformed content-id string in `files` object"));
        };
        index = next;
        files.insert(path, id);
    }
    Ok(files)
}

pub(super) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn skip_json_ws(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *index += 1;
    }
}

/// Decode one JSON string literal (the writer's own escaping rules:
/// `\"`, `\\`, `\n`, `\r`, `\t`, `\uXXXX`).
pub(super) fn parse_json_string(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut index = start;
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;
    loop {
        match bytes.get(index)? {
            b'"' => return Some((out, index + 1)),
            b'\\' => {
                match bytes.get(index + 1)? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if bytes.len() < index + 6 {
                            return None;
                        }
                        let digits = &bytes[index + 2..index + 6];
                        if digits.len() < 4 {
                            return None;
                        }
                        let text = std::str::from_utf8(digits).ok()?;
                        let code = u32::from_str_radix(text, 16).ok()?;
                        out.push(char::from_u32(code)?);
                        index += 4;
                    }
                    _ => return None,
                }
                index += 2;
            }
            _ => {
                let run_start = index;
                while let Some(byte) = bytes.get(index) {
                    if matches!(byte, b'"' | b'\\') {
                        break;
                    }
                    index += 1;
                }
                out.push_str(std::str::from_utf8(&bytes[run_start..index]).ok()?);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reader side: the writer's documents are parsed back with the
// same std-only discipline, so the CLI and checker never rely on
// write-only artifacts.
// ---------------------------------------------------------------------------
