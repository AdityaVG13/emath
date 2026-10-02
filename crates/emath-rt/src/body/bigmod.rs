// ── Big field-element carrier (emath-t63iz stage 2) ─────────────────────
//
// `UBig`: an arbitrary-precision NON-NEGATIVE integer, little-endian
// base-2^32 limbs, canonical (no high zero limbs). It is the storage
// carrier for `BigInt` values (decimal parse/print, add, sub, mul,
// compare, narrowing to i64). Modular algorithms over it are authored
// `.emath`, not runtime kernels. Admission refuses values >= 2^256 (see
// `LIMIT_BITS`).
//
// This file is embedded verbatim into every generated crate (`SOURCE`
// in emath-rt's lib.rs), so generated Rust and the interpreter share one
// carrier. No-claim: not constant-time.

/// Stage-2 value bound: |F| < 2^256.
pub const LIMIT_BITS: u32 = 256;

/// Canonical non-negative big integer: little-endian base-2^32 limbs
/// with no high zero limbs (zero is the empty limb vector).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UBig {
    limbs: Vec<u32>,
}

/// Carrier-level refusal (decimal parse errors).
pub type BigError = &'static str;

impl UBig {
    /// Zero (canonical: empty limbs).
    pub fn zero() -> Self {
        UBig { limbs: Vec::new() }
    }

    /// One.
    pub fn one() -> Self {
        UBig { limbs: vec![1] }
    }

    /// From a u64.
    pub fn from_u64(value: u64) -> Self {
        let mut big = UBig {
            limbs: vec![value as u32, (value >> 32) as u32],
        };
        big.canonicalize();
        big
    }

    /// Canonical little-endian limbs (no high zeros; empty = zero).
    pub fn limbs(&self) -> &[u32] {
        &self.limbs
    }

    fn canonicalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    /// Exact decimal parse (no sign, no separators; the emitter strips
    /// `_` before calling). Refuses non-digits. Does NOT bound-check;
    /// the emitter enforces `LIMIT_BITS` at admission.
    pub fn parse_decimal(text: &str) -> Result<Self, BigError> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err("bigint literal must be a non-negative decimal integer");
        }
        let mut big = UBig::zero();
        let mut chunk_start = 0;
        while chunk_start < text.len() {
            let chunk_end = (chunk_start + 9).min(text.len());
            let chunk = &text[chunk_start..chunk_end];
            if chunk.is_empty() {
                break;
            }
            let value: u64 = chunk.parse().map_err(|_| "bigint literal chunk overflow")?;
            let scale = 10u64.pow((chunk_end - chunk_start) as u32);
            big.mul_small_add(value, scale);
            chunk_start = chunk_end;
        }
        Ok(big)
    }

    /// Exact decimal rendering (canonical, no leading zeros).
    pub fn to_decimal(&self) -> String {
        if self.limbs.is_empty() {
            return "0".to_string();
        }
        // Repeated division by 10^9; remainders are the digit groups.
        let mut chunks: Vec<u32> = Vec::new();
        let mut cur = self.limbs.clone();
        while !cur.is_empty() {
            let (quotient, remainder) = UBig::div_small(&cur, 1_000_000_000);
            chunks.push(remainder as u32);
            cur = quotient.limbs;
        }
        let mut text = String::new();
        for (index, chunk) in chunks.iter().enumerate().rev() {
            if index == chunks.len() - 1 {
                text.push_str(&chunk.to_string());
            } else {
                text.push_str(&format!("{chunk:09}"));
            }
        }
        text
    }

    /// Number of significant bits (0 for zero).
    pub fn bits(&self) -> u32 {
        match self.limbs.last() {
            None => 0,
            Some(&top) => (self.limbs.len() as u32 - 1) * 32 + (32 - top.leading_zeros()),
        }
    }

    /// True when the value is zero.
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// True when the value is one.
    pub fn is_one(&self) -> bool {
        self.limbs == [1]
    }

    pub fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for i in (0..self.limbs.len()).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => continue,
                other_order => return other_order,
            }
        }
        Ordering::Equal
    }

    pub fn add(&self, other: &Self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len().max(other.limbs.len()) + 1);
        let mut carry = 0u64;
        for i in 0..self.limbs.len().max(other.limbs.len()) {
            let a = u64::from(*self.limbs.get(i).unwrap_or(&0));
            let b = u64::from(*other.limbs.get(i).unwrap_or(&0));
            let sum = a + b + carry;
            limbs.push(sum as u32);
            carry = sum >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
        UBig { limbs }
    }

    /// `self - other` (caller guarantees `self ≥ other`).
    pub fn sub(&self, other: &Self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = i64::from(self.limbs[i]);
            let b = i64::from(*other.limbs.get(i).unwrap_or(&0)) + borrow;
            let (digit, new_borrow) = if a >= b { (a - b, 0) } else { (a + (1 << 32) - b, 1) };
            limbs.push(digit as u32);
            borrow = new_borrow;
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }

    pub fn mul(&self, other: &Self) -> UBig {
        if self.limbs.is_empty() || other.limbs.is_empty() {
            return UBig::zero();
        }
        let mut limbs = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let cur = u64::from(limbs[i + j]) + u64::from(a) * u64::from(b) + carry;
                limbs[i + j] = cur as u32;
                carry = cur >> 32;
            }
            limbs[i + other.limbs.len()] = carry as u32;
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }

    /// Multiply by a small u64 and add a small u64 (parse helper; both
    /// operands < 2^32-scale so u64 intermediates never overflow).
    fn mul_small_add(&mut self, small: u64, scale: u64) {
        let mut carry = small;
        for limb in &mut self.limbs {
            let cur = u64::from(*limb) * scale + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        while carry != 0 {
            self.limbs.push(carry as u32);
            carry >>= 32;
        }
    }

    /// Divide by a small u64: returns (quotient, remainder).
    pub fn div_small(limbs: &[u32], divisor: u64) -> (UBig, u64) {
        let mut quotient = vec![0u32; limbs.len()];
        let mut rem = 0u64;
        for i in (0..limbs.len()).rev() {
            let cur = (rem << 32) | u64::from(limbs[i]);
            quotient[i] = (cur / divisor) as u32;
            rem = cur % divisor;
        }
        let mut big = UBig { limbs: quotient };
        big.canonicalize();
        (big, rem)
    }

    /// Shift left by one bit (`self * 2`).
    pub fn shl1(&self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u32;
        for &limb in &self.limbs {
            limbs.push((limb << 1) | carry);
            carry = limb >> 31;
        }
        if carry != 0 {
            limbs.push(carry);
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }
}

impl UBig {
    /// Integer value when the big value fits `i64` (result-side
    /// narrowing for callers that stay in the stage-1 lane).
    pub fn to_i64(&self) -> Option<i64> {
        if self.bits() > 63 {
            return None;
        }
        let mut value: u64 = 0;
        for (index, &limb) in self.limbs.iter().enumerate() {
            value |= u64::from(limb) << (32 * index as u64);
        }
        i64::try_from(value).ok()
    }
}

