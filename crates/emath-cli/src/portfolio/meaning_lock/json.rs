//! Minimal JSON parsing for lock files.

use super::*;

pub(super) fn hex(value: u64) -> String {
    format!("{value:016x}")
}

pub(super) fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn parse_hex(text: &str) -> Result<u64, LockError> {
    if text.len() != 16 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LockError::Malformed {
            detail: format!("expected 16 hex digits, got `{text}`"),
        });
    }
    u64::from_str_radix(text, 16).map_err(|_| LockError::Malformed {
        detail: format!("invalid hex `{text}`"),
    })
}

pub(super) fn parse_decimal(text: &str) -> Result<u64, LockError> {
    text.parse::<u64>().map_err(|_| LockError::Malformed {
        detail: format!("expected decimal u64, got `{text}`"),
    })
}

pub(super) fn required_str<'a>(
    object: &'a BTreeMap<String, Json>,
    name: &str,
) -> Result<&'a str, LockError> {
    match object.get(name) {
        Some(Json::Str(value)) => Ok(value),
        Some(_) => Err(LockError::Malformed {
            detail: format!("field {name} must be a string"),
        }),
        None => Err(LockError::Malformed {
            detail: format!("missing field {name}"),
        }),
    }
}

pub(super) fn required_u32(object: &BTreeMap<String, Json>, name: &str) -> Result<u32, LockError> {
    match object.get(name) {
        Some(Json::Num(text)) => text.parse::<u32>().map_err(|_| LockError::Malformed {
            detail: format!("field {name} must be a u32"),
        }),
        Some(_) => Err(LockError::Malformed {
            detail: format!("field {name} must be a number"),
        }),
        None => Err(LockError::Malformed {
            detail: format!("missing field {name}"),
        }),
    }
}

pub(super) fn refuse_unknown_keys(
    object: &BTreeMap<String, Json>,
    allowed: &[&str],
) -> Result<(), LockError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(LockError::Malformed {
                detail: format!("unknown field `{key}`"),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Json {
    Str(String),
    Num(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub(super) fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Self::Obj(object) => Some(object),
            _ => None,
        }
    }

    pub(super) fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(items) => Some(items),
            _ => None,
        }
    }
}

pub(super) fn parse_json(text: &str) -> Result<Json, String> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let value = parse_value(bytes, &mut index).ok_or_else(|| "cannot parse JSON".to_string())?;
    skip_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err("trailing content after JSON document".to_string());
    }
    Ok(value)
}

pub(super) fn skip_ws(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *index += 1;
    }
}

pub(super) fn parse_value(bytes: &[u8], index: &mut usize) -> Option<Json> {
    skip_ws(bytes, index);
    match bytes.get(*index)? {
        b'{' => parse_object(bytes, index),
        b'[' => parse_array(bytes, index),
        b'"' => parse_string(bytes, index).map(Json::Str),
        b'-' | b'0'..=b'9' => parse_number(bytes, index).map(Json::Num),
        _ => None,
    }
}

pub(super) fn parse_object(bytes: &[u8], index: &mut usize) -> Option<Json> {
    *index += 1;
    let mut object = BTreeMap::new();
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b'}' {
            *index += 1;
            return Some(Json::Obj(object));
        }
        if !object.is_empty() {
            if bytes.get(*index)? != &b',' {
                return None;
            }
            *index += 1;
            skip_ws(bytes, index);
        }
        let key = parse_string(bytes, index)?;
        skip_ws(bytes, index);
        if bytes.get(*index)? != &b':' {
            return None;
        }
        *index += 1;
        let value = parse_value(bytes, index)?;
        if object.insert(key, value).is_some() {
            return None;
        }
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b'}' {
            *index += 1;
            return Some(Json::Obj(object));
        }
    }
}

pub(super) fn parse_array(bytes: &[u8], index: &mut usize) -> Option<Json> {
    *index += 1;
    let mut items = Vec::new();
    loop {
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b']' {
            *index += 1;
            return Some(Json::Arr(items));
        }
        if !items.is_empty() {
            if bytes.get(*index)? != &b',' {
                return None;
            }
            *index += 1;
            skip_ws(bytes, index);
        }
        items.push(parse_value(bytes, index)?);
        skip_ws(bytes, index);
        if bytes.get(*index)? == &b']' {
            *index += 1;
            return Some(Json::Arr(items));
        }
    }
}

pub(super) fn parse_string(bytes: &[u8], index: &mut usize) -> Option<String> {
    if bytes.get(*index)? != &b'"' {
        return None;
    }
    *index += 1;
    let mut out = String::new();
    loop {
        match bytes.get(*index)? {
            b'"' => {
                *index += 1;
                return Some(out);
            }
            b'\\' => {
                match bytes.get(*index + 1)? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let digits = bytes.get(*index + 2..*index + 6)?;
                        let text = std::str::from_utf8(digits).ok()?;
                        let code = u32::from_str_radix(text, 16).ok()?;
                        out.push(char::from_u32(code)?);
                        *index += 4;
                    }
                    _ => return None,
                }
                *index += 2;
            }
            byte => {
                out.push(char::from(*byte));
                *index += 1;
            }
        }
    }
}

pub(super) fn parse_number(bytes: &[u8], index: &mut usize) -> Option<String> {
    let start = *index;
    if bytes.get(*index) == Some(&b'-') {
        *index += 1;
    }
    let digits_start = *index;
    while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
        *index += 1;
    }
    if *index == digits_start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index])
        .ok()
        .map(ToOwned::to_owned)
}
