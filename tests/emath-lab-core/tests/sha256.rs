//! SHA-256 NIST vectors and determinism tests.

use emath_lab_core::{digest, hex};
use emath_test_harness::{Case, Probe, check_all, expect_ok};

#[test]
fn sha256() {
    let mut p = Probe::new("digest matches FIPS 180-4 and is a pure function");
    p.case("nist-vectors", |p| {
        expect_ok(check_all(&[Case::new("empty", b"".as_slice(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()), Case::new("abc", b"abc".as_slice(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string()), Case::new("two-block", b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".as_slice(), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1".to_string())], |input: &&[u8]| hex(&digest(input))));
        p.eq("long", hex(&digest(&vec![b'a'; 1_000_000])), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0".to_string());
    });
    p.case("deterministic", |p| {
        p.eq("stable", digest(b"state.scale"), digest(b"state.scale"));
        p.ne("sensitive", digest(b"state.scale"), digest(b"state.scalx"));
    });
    p.finish();
}
