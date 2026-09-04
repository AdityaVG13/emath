//! The `JsonValue` DOM and its parser.

use super::*;

/// Minimal JSON value tree accepted by [`parse_json_document`].
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    /// String literal.
    Str(String),
    /// Numeric literal (kept verbatim).
    Num(String),
    /// Boolean literal.
    Bool(bool),
    /// `null`.
    Null,
    /// Object (insertion order preserved).
    Obj(Vec<(String, JsonValue)>),
    /// Array.
    Arr(Vec<JsonValue>),
}

impl JsonValue {
    /// Look up an object field by name (typed parse-back support).
    pub fn field(&self, name: &str) -> Result<&JsonValue, ArtifactError> {
        match self {
            Self::Obj(entries) => entries
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
                .ok_or_else(|| ArtifactError::ManifestMalformed(format!("missing `{name}`"))),
            _ => Err(ArtifactError::ManifestMalformed(
                "not an object".to_string(),
            )),
        }
    }

    /// Read a string field (typed parse-back support).
    pub fn string_field(&self, name: &str) -> Result<String, ArtifactError> {
        match self.field(name)? {
            Self::Str(value) => Ok(value.clone()),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not a string"
            ))),
        }
    }

    pub(super) fn optional_string_field(
        &self,
        name: &str,
    ) -> Result<Option<String>, ArtifactError> {
        if self.obj_has(name)? {
            Ok(Some(self.string_field(name)?))
        } else {
            Ok(None)
        }
    }

    pub(super) fn obj_has(&self, name: &str) -> Result<bool, ArtifactError> {
        match self {
            Self::Obj(entries) => Ok(entries.iter().any(|(key, _)| key == name)),
            _ => Err(ArtifactError::ManifestMalformed(
                "not an object".to_string(),
            )),
        }
    }

    /// Read an integer field (typed parse-back support).
    pub fn int_field(&self, name: &str) -> Result<u64, ArtifactError> {
        match self.field(name)? {
            Self::Num(value) => value.parse::<u64>().map_err(|_| {
                ArtifactError::ManifestMalformed(format!("`{name}` is not an integer"))
            }),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not a number"
            ))),
        }
    }

    pub(super) fn strings_field(&self, name: &str) -> Result<Vec<String>, ArtifactError> {
        match self.field(name)? {
            Self::Arr(items) => items
                .iter()
                .map(|item| match item {
                    Self::Str(value) => Ok(value.clone()),
                    _ => Err(ArtifactError::ManifestMalformed(format!(
                        "`{name}` array has a non-string entry"
                    ))),
                })
                .collect(),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not an array"
            ))),
        }
    }

    pub(super) fn content_id_field(&self, name: &str) -> Result<ContentId, ArtifactError> {
        Ok(ContentId(self.string_field(name)?))
    }
}

/// Parse one deterministic writer document into a value tree. Accepts the
/// writer's grammar (RFC 8259 subset: objects, arrays, strings, integers,
/// booleans and `null`); anything else is a typed refusal.
pub fn parse_json_document(text: &str) -> Result<JsonValue, ArtifactError> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let value = parse_json_value(bytes, &mut index)
        .ok_or_else(|| ArtifactError::ManifestMalformed("cannot parse JSON".to_string()))?;
    skip_json_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err(ArtifactError::ManifestMalformed(
            "trailing content after JSON document".to_string(),
        ));
    }
    Ok(value)
}

pub(super) fn parse_json_value(bytes: &[u8], index: &mut usize) -> Option<JsonValue> {
    skip_json_ws(bytes, index);
    match bytes.get(*index) {
        Some(&b'{') => parse_json_object(bytes, index).map(JsonValue::Obj),
        Some(&b'[') => parse_json_array(bytes, index).map(JsonValue::Arr),
        Some(&b'"') => parse_json_string(bytes, *index).map(|(value, next)| {
            *index = next;
            JsonValue::Str(value)
        }),
        Some(&b't')
            if bytes.get(*index + 1) == Some(&b'r')
                && bytes.get(*index + 2) == Some(&b'u')
                && bytes.get(*index + 3) == Some(&b'e') =>
        {
            *index += 4;
            Some(JsonValue::Bool(true))
        }
        Some(&b'f')
            if bytes.get(*index + 1) == Some(&b'a')
                && bytes.get(*index + 2) == Some(&b'l')
                && bytes.get(*index + 3) == Some(&b's')
                && bytes.get(*index + 4) == Some(&b'e') =>
        {
            *index += 5;
            Some(JsonValue::Bool(false))
        }
        Some(&b'n')
            if bytes.get(*index + 1) == Some(&b'u')
                && bytes.get(*index + 2) == Some(&b'l')
                && bytes.get(*index + 3) == Some(&b'l') =>
        {
            *index += 4;
            Some(JsonValue::Null)
        }
        Some(&(b'-' | b'0'..=b'9')) => {
            let start = *index;
            while bytes
                .get(*index)
                .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
            {
                *index += 1;
            }
            Some(JsonValue::Num(
                std::str::from_utf8(&bytes[start..*index]).ok()?.to_string(),
            ))
        }
        _ => None,
    }
}

pub(super) fn parse_json_object(
    bytes: &[u8],
    index: &mut usize,
) -> Option<Vec<(String, JsonValue)>> {
    *index += 1; // '{'
    let mut entries = Vec::new();
    loop {
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b'}' => {
                *index += 1;
                return Some(entries);
            }
            b'"' => {}
            _ => return None,
        }
        let (key, next) = parse_json_string(bytes, *index)?;
        *index = next;
        skip_json_ws(bytes, index);
        if bytes.get(*index) != Some(&b':') {
            return None;
        }
        *index += 1;
        let value = parse_json_value(bytes, index)?;
        entries.push((key, value));
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b',' => *index += 1,
            b'}' => {
                *index += 1;
                return Some(entries);
            }
            _ => return None,
        }
    }
}

pub(super) fn parse_json_array(bytes: &[u8], index: &mut usize) -> Option<Vec<JsonValue>> {
    *index += 1; // '['
    let mut items = Vec::new();
    loop {
        skip_json_ws(bytes, index);
        if bytes.get(*index) == Some(&b']') {
            *index += 1;
            return Some(items);
        }
        items.push(parse_json_value(bytes, index)?);
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b',' => *index += 1,
            b']' => {
                *index += 1;
                return Some(items);
            }
            _ => return None,
        }
    }
}
