use std::collections::BTreeMap;

use emath_core::{fnv1a64_bytes, sha256_digest};

const IMAGE_BYTES: &[u8] = include_bytes!("../../../language/generated/language.image");
const LOCK_BYTES: &[u8] = include_bytes!("../../../language/language.lock");
const SOURCE_MAP_BYTES: &[u8] = include_bytes!("../../../language/generated/source-map.lock");

#[derive(Clone, Debug, PartialEq, Eq)]
struct Partition {
    kind: String,
    content_id: String,
    body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndependentImage {
    schema: String,
    semantic_hash: String,
    distribution_hash: String,
    partitions: BTreeMap<String, Partition>,
}

fn line(bytes: &[u8], cursor: &mut usize) -> Result<String, String> {
    let rest = bytes.get(*cursor..).ok_or("cursor beyond image")?;
    let end = rest
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("unterminated line")?;
    let value = std::str::from_utf8(&rest[..end])
        .map_err(|_| "non-UTF-8 header")?
        .to_string();
    *cursor += end + 1;
    Ok(value)
}

fn header_value(text: &str, key: &str) -> Result<String, String> {
    text.strip_prefix(key)
        .map(str::to_string)
        .ok_or_else(|| format!("missing {key}"))
}

fn decode_image(bytes: &[u8]) -> Result<IndependentImage, String> {
    let mut cursor = 0;
    let schema = header_value(&line(bytes, &mut cursor)?, "schema=")?;
    let semantic_hash = header_value(&line(bytes, &mut cursor)?, "semantic_hash=")?;
    let distribution_hash = header_value(&line(bytes, &mut cursor)?, "distribution_hash=")?;
    if schema != "emath.language-image"
        || !valid_hash(&semantic_hash, "sha256:")
        || !valid_hash(&distribution_hash, "distribution-sha256:")
    {
        return Err("invalid image identity header".to_string());
    }

    let mut partitions = BTreeMap::new();
    let mut previous_partition = String::new();
    while cursor < bytes.len() {
        let header = line(bytes, &mut cursor)?;
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 5 || fields[0] != "partition" {
            return Err("invalid partition header".to_string());
        }
        if fields[1] <= previous_partition.as_str() {
            return Err("partition order drift".to_string());
        }
        previous_partition = fields[1].to_string();
        let length = fields[4]
            .parse::<usize>()
            .map_err(|_| "invalid partition length")?;
        let body = bytes
            .get(cursor..cursor + length)
            .ok_or("truncated partition")?
            .to_vec();
        cursor += length;
        if !body.ends_with(b"\n") {
            if bytes.get(cursor) != Some(&b'\n') {
                return Err("missing partition separator".to_string());
            }
            cursor += 1;
        }
        let expected = format!("fnv1a64:{:016x}", fnv1a64_bytes(&body));
        if fields[3] != expected {
            return Err(format!("corrupt partition {}", fields[1]));
        }
        if partitions
            .insert(
                fields[1].to_string(),
                Partition {
                    kind: fields[2].to_string(),
                    content_id: fields[3].to_string(),
                    body,
                },
            )
            .is_some()
        {
            return Err("duplicate partition".to_string());
        }
    }
    Ok(IndependentImage {
        schema,
        semantic_hash,
        distribution_hash,
        partitions,
    })
}

fn decode_map(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "non-UTF-8 map")?;
    let mut map = BTreeMap::new();
    let mut previous = "";
    for row in text.lines() {
        let (key, value) = row.split_once('=').ok_or("invalid map row")?;
        if key <= previous
            || !valid_feature_id(key)
            || value.is_empty()
            || map.insert(key.to_string(), value.to_string()).is_some()
        {
            return Err("noncanonical map".to_string());
        }
        previous = key;
    }
    Ok(map)
}

fn decode_lock(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "non-UTF-8 lock")?;
    let mut lock = BTreeMap::new();
    for row in text.lines() {
        let (key, value) = row.split_once('=').ok_or("invalid lock row")?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            || lock.insert(key.to_string(), value.to_string()).is_some()
        {
            return Err("noncanonical lock".to_string());
        }
    }
    Ok(lock)
}

fn valid_feature_id(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() >= 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
        })
}

fn valid_hash(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reproduce the one-field distribution hash envelope without using
/// LanguageImage, DistributionHash, CanonicalField, or the production decoder.
fn distribution_identity(image: &IndependentImage) -> String {
    let mut material = format!("{}\n{}\n", image.schema, image.semantic_hash);
    for (name, partition) in &image.partitions {
        if name != "language.lock" {
            material.push_str(&format!("{name}={}\n", partition.content_id));
        }
    }
    let name = b"image";
    let value = material.as_bytes();
    let mut envelope = b"emath.feature.distribution\0".to_vec();
    envelope.extend_from_slice(&1_u64.to_be_bytes());
    envelope.extend_from_slice(&(name.len() as u64).to_be_bytes());
    envelope.extend_from_slice(name);
    envelope.extend_from_slice(&(value.len() as u64).to_be_bytes());
    envelope.extend_from_slice(value);
    format!(
        "distribution-sha256:{}",
        digest_hex(sha256_digest(&envelope))
    )
}

fn exact_add(left: i64, right: i64) -> Result<i64, &'static str> {
    left.checked_add(right).ok_or("E-ARITH-OVERFLOW")
}

#[test]
fn independent_reader_reproduces_checked_in_identity_authority_and_exact_result() {
    let image = decode_image(IMAGE_BYTES).expect("canonical checked-in image");
    let lock = decode_lock(LOCK_BYTES).expect("canonical checked-in lock");
    assert_eq!(lock["schema"], "emath.language-lock");
    assert_eq!(lock["semantic_hash"], image.semantic_hash);
    assert_eq!(lock["distribution_hash"], image.distribution_hash);
    assert_eq!(distribution_identity(&image), image.distribution_hash);

    let embedded_lock = decode_lock(&image.partitions["language.lock"].body).unwrap();
    assert_eq!(embedded_lock, lock);
    let source_map = decode_map(SOURCE_MAP_BYTES).expect("canonical checked-in source map");
    assert_eq!(image.partitions["language.sources"].body, SOURCE_MAP_BYTES);

    let authority = decode_map(&image.partitions["language.authority"].body).unwrap();
    assert_eq!(authority["std.capability.math.add"], "capsule-active");
    assert_eq!(
        source_map["std.capability.math.add"],
        "language/spec/capabilities/core/add.emath"
    );

    let capsules = std::str::from_utf8(&image.partitions["language.capsules"].body).unwrap();
    let add = capsules
        .lines()
        .find(|row| row.starts_with("std.capability.math.add "))
        .expect("add capsule row");
    let add_hash = add.split_whitespace().nth(3).expect("capsule hash");
    let tables = std::str::from_utf8(&image.partitions["language.tables"].body).unwrap();
    let add_table = tables
        .lines()
        .find(|row| row.starts_with("std.capability.math.add "))
        .expect("add runtime row");
    assert!(add_table.contains(&format!("hash={add_hash}")));
    assert!(add_table.contains("handle=kernel=checked-add;arity=2"));
    assert_eq!(exact_add(2, 1), Ok(3));
    assert_eq!(exact_add(i64::MAX, 1), Err("E-ARITH-OVERFLOW"));
}

#[test]
fn independent_reader_refuses_mutated_checked_in_bytes() {
    let mut image = IMAGE_BYTES.to_vec();
    let needle = b"std.capability.math.add=capsule-active";
    let start = image
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    image[start] = b'x';
    assert!(
        decode_image(&image).is_err(),
        "partition stamp detects authority tampering"
    );

    let mut source_map = SOURCE_MAP_BYTES.to_vec();
    source_map[0] = b'x';
    assert_ne!(
        decode_image(IMAGE_BYTES).unwrap().partitions["language.sources"].body,
        source_map
    );

    let mut lock = LOCK_BYTES.to_vec();
    let digest = lock
        .windows(64)
        .position(|window| window.iter().all(|byte| byte.is_ascii_hexdigit()))
        .expect("lock digest");
    lock[digest] = if lock[digest] == b'a' { b'b' } else { b'a' };
    let changed = decode_lock(&lock).unwrap();
    let image = decode_image(IMAGE_BYTES).unwrap();
    assert_ne!(changed["semantic_hash"], image.semantic_hash);
}
