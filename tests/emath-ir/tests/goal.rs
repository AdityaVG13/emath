//! Plan-identity witnesses: provider-permutation insensitivity, provider
//! set change detection, and the deliberate split between the `plan:`
//! payload layer and the JSON `$schema` layer.

use emath_core::ContentId;
use emath_ir::goal::{PLAN_SCHEMA, plan_identity};

#[test]
fn plan_identity_is_insensitive_to_provider_permutation() {
    let one = plan_identity(
        "goal",
        "policy",
        &["b".to_string(), "a".to_string(), "c".to_string()],
        "rust-library",
    );
    let two = plan_identity(
        "goal",
        "policy",
        &["c".to_string(), "b".to_string(), "a".to_string()],
        "rust-library",
    );
    assert_eq!(one, two);
}

#[test]
fn plan_identity_detects_provider_set_change() {
    let base = plan_identity("goal", "policy", &["a".to_string()], "rust-library");
    let added = plan_identity(
        "goal",
        "policy",
        &["a".to_string(), "b".to_string()],
        "rust-library",
    );
    assert_ne!(base, added);
}

/// The identity layer (`plan:` payload) and the JSON `$schema`
/// layer (`emath.resolution-plan`) are deliberately split; the
/// payload format is pinned here so neither layer can silently
/// converge on the other's string.
#[test]
fn plan_identity_payload_and_json_schema_are_distinct_layers() {
    let providers = ["z".to_string(), "a".to_string()];
    let id = plan_identity("goal", "policy", &providers, "rust-library");
    let mut payload = String::from("plan:goal\npolicy\n");
    payload.push_str("a\nz\nrust-library");
    assert_eq!(
        id,
        ContentId(format!(
            "fnv1a64:{:016x}",
            emath_core::fnv1a64_bytes(payload.as_bytes())
        )),
        "plan identity must hash the `plan:` payload (sorted providers, trailing target)"
    );
    assert_eq!(
        payload, "plan:goal\npolicy\na\nz\nrust-library",
        "payload format pin"
    );
    assert_ne!(
        PLAN_SCHEMA, "plan",
        "JSON `$schema` id must stay emath.resolution-plan, distinct from the identity prefix"
    );
}
