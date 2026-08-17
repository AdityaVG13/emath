//! Minimal std-only SHA-256 for keep-gate identity dumps.
//!
//! The keep-gate harness records `identity=` as the SHA-256 of emitted
//! artifacts beside every measured cell. FIPS 180-4 round constants and
//! big-endian block processing; deterministic and dependency-free.

/// 32-bit rotate right.
fn rotate_right(value: u32, shift: u32) -> u32 {
    value.rotate_right(shift)
}

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// SHA-256 digest of `bytes` (FIPS 180-4), 32 big-endian bytes.
#[must_use]
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut state = H0;
    // Zero-padded message: bytes + 0x80 + zeros + 64-bit bit length.
    let bit_len = (bytes.len() as u128) * 8;
    let padded_len = (bytes.len() + 9).div_ceil(64) * 64;
    for chunk_start in (0..padded_len).step_by(64) {
        let mut fill = [0_u8; 64];
        let available = bytes.len().saturating_sub(chunk_start).min(64);
        if available > 0 {
            fill[..available].copy_from_slice(&bytes[chunk_start..chunk_start + available]);
        }
        // The 0x80 pad byte sits directly after the input tail (in the
        // next chunk when the input length is a multiple of 64).
        if (chunk_start..chunk_start + 64).contains(&bytes.len()) {
            fill[bytes.len() - chunk_start] = 0x80;
        }
        // The 64-bit big-endian bit length fills the last 8 bytes of the
        // final chunk.
        if chunk_start == padded_len - 64 {
            fill[56..64].copy_from_slice(&bit_len.to_be_bytes()[8..]);
        }
        compress(&mut state, &fill);
    }
    let mut out = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Compresses one 64-byte block into the state.
/// The compression schedule follows FIPS 180-4, which names the working
/// variables `a..=h`; the single-character names are the standard notation.
#[allow(clippy::many_single_char_names)]
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0_u32; 64];
    for (index, word) in w.iter_mut().enumerate().take(16) {
        let start = index * 4;
        *word = u32::from_be_bytes([
            block[start],
            block[start + 1],
            block[start + 2],
            block[start + 3],
        ]);
    }
    for index in 16..64 {
        let s0 =
            rotate_right(w[index - 15], 7) ^ rotate_right(w[index - 15], 18) ^ (w[index - 15] >> 3);
        let s1 =
            rotate_right(w[index - 2], 17) ^ rotate_right(w[index - 2], 19) ^ (w[index - 2] >> 10);
        w[index] = w[index - 16]
            .wrapping_add(s0)
            .wrapping_add(w[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (index, &round_k) in K.iter().enumerate() {
        let s1 = rotate_right(e, 6) ^ rotate_right(e, 11) ^ rotate_right(e, 25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(round_k)
            .wrapping_add(w[index]);
        let s0 = rotate_right(a, 2) ^ rotate_right(a, 13) ^ rotate_right(a, 22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

/// Lowercase hex of the digest.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{digest, hex};

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
}
