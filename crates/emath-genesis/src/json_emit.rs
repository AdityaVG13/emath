//! Shared compact JSON emit for genesis receipts.
//!
//! Family modules construct the variant subset they need. Key escape on
//! `'static` identifier keys matches the previous unescaped write.

use std::collections::BTreeMap;
use std::fmt::Write as _;

pub(crate) use emath_core::json_escape;

pub(crate) enum Json {
    Str(String),
    Number(String),
    Bool(bool),
    Null,
    Array(Vec<Json>),
    Object(BTreeMap<&'static str, Json>),
    Raw(String),
}

pub(crate) fn emit_object(fields: &BTreeMap<&'static str, Json>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", json_escape(key));
        emit_json(value, &mut out);
    }
    out.push('}');
    out
}

pub(crate) fn emit_json(value: &Json, out: &mut String) {
    match value {
        Json::Str(text) => {
            let _ = write!(out, "\"{}\"", json_escape(text));
        }
        Json::Number(text) => out.push_str(text),
        Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Json::Null => out.push_str("null"),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                emit_json(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push_str(&emit_object(fields));
        }
        Json::Raw(text) => out.push_str(text),
    }
}
