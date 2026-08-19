//! SHA-256 tests (origin `crates/emath-lab-core/src/sha256.rs`).

use emath_lab_core::{digest, hex};

/// NIST FIPS 180-4 vectors (empty, "abc", two-block, long).
#[test]
fn nist_vectors_match() {
    assert_eq!(
        hex(&digest(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&digest(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    let long = vec![b'a'; 1_000_000];
    assert_eq!(
        hex(&digest(&long)),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

/// Digest is a pure function of the bytes: same input, same output.
#[test]
fn digest_is_deterministic() {
    assert_eq!(digest(b"state.scale"), digest(b"state.scale"));
    assert_ne!(digest(b"state.scale"), digest(b"state.scalx"));
}
