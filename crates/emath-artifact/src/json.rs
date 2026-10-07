//! Streaming JSON writer re-export and low-level scanning helpers.

pub use emath_core::{JsonObject, JsonWriter};

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
