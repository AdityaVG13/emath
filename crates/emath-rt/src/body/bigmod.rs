// ── Big-integer modular arithmetic (emath-t63iz stage 2) ─────────────────
//
// `UBig`: an arbitrary-precision NON-NEGATIVE integer, little-endian
// base-2^32 limbs, canonical (no high zero limbs). This is the stage-2
// representation for the six number-theory builtins (`int_rem`,
// `mod_inv`, `pow_mod`, `sqrt_mod`, `poly_eval_mod`, `rs_encode`):
// |F| < 2^256, exactly the production regime the stage-1 i64/i128 lane
// cannot reach. The algorithms are the stage-1 algorithms
// (square-and-multiply, Tonelli-Shanks, extended Euclid, Horner) over
// the swapped representation — a representation change, not an
// algorithm rewrite. The stage boundary stays explicit: admission
// refuses values ≥ 2^256 (see `LIMIT_BITS`) — widening the bound later
// is a constant change, not a redesign.
//
// This file is embedded verbatim into every generated crate (`SOURCE`
// in emath-rt's lib.rs), so generated Rust runs the SAME kernels as the
// interpreter — parity is structural, not hoped for. Determinism:
// every routine is integer-exact, allocation-pattern free of timing
// variation claims (no-claim: not constant-time; these are research
// probes, not crypto primitives).

/// Stage-2 value bound: |F| < 2^256.
pub const LIMIT_BITS: u32 = 256;

/// Canonical non-negative big integer: little-endian base-2^32 limbs
/// with no high zero limbs (zero is the empty limb vector).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UBig {
    limbs: Vec<u32>,
}

/// Kernel-level error for big modular arithmetic (stage-2 style mirrors
/// the `&'static str` refusals of the stage-1 kernels).
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

    /// From an i64 by absolute value (`i64::MIN` → 2^63).
    pub fn from_i64_abs(value: i64) -> Self {
        UBig::from_u64(value.unsigned_abs())
    }

    /// From canonical little-endian u32 limbs (high zeros trimmed).
    pub fn from_limbs(mut limbs: Vec<u32>) -> Self {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        UBig { limbs }
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

    /// Exact decimal parse (no sign, no separators — the emitter strips
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

    /// Bit `i` of the little-endian bit string.
    fn bit(&self, i: u32) -> bool {
        let limb = i / 32;
        match self.limbs.get(limb as usize) {
            Some(&value) => (value >> (i % 32)) & 1 == 1,
            None => false,
        }
    }

    fn set_bit(&mut self, i: u32) {
        let limb = (i / 32) as usize;
        while self.limbs.len() <= limb {
            self.limbs.push(0);
        }
        self.limbs[limb] |= 1 << (i % 32);
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

    /// `(a + b) mod m` for `a, b < m` — subtraction instead of a
    /// double-width add.
    fn add_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        // a, b < m < 2^256 ⇒ a + b < 2^257: one extra limb suffices.
        let sum = a.add(b);
        if sum.cmp(m) != core::cmp::Ordering::Less {
            sum.sub(m)
        } else {
            sum
        }
    }

    /// `(a - b) mod m` for `a, b < m` — add m when the raw difference
    /// would be negative (never materialized).
    fn sub_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        if a.cmp(b) != core::cmp::Ordering::Less {
            a.sub(b)
        } else {
            m.sub(b).add(a)
        }
    }

    /// `a * b mod m` via the full product then one binary reduction.
    pub fn mul_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        UBig::rem(&a.mul(b), m)
    }

    /// Binary long division: `(a / b, a mod b)`. `b == 0` is the
    /// caller's typed refusal (mirrors int_rem's positive-modulus
    /// contract); bit-shift subtract keeps u32 limbs exact for
    /// 512-bit stage-2 products.
    fn rem(a: &UBig, b: &UBig) -> UBig {
        debug_assert!(!b.is_zero());
        if a.cmp(b) == core::cmp::Ordering::Less {
            return a.clone();
        }
        let mut remainder = UBig::zero();
        for i in (0..a.bits()).rev() {
            remainder = remainder.shl1();
            if a.bit(i) {
                remainder.set_bit(0);
            }
            if remainder.cmp(b) != core::cmp::Ordering::Less {
                remainder = remainder.sub(b);
            }
        }
        remainder
    }

    /// `base^exp mod m` (square-and-multiply over the big
    /// representation; same algorithm as the stage-1 i128 kernel).
    fn mod_pow(base: &UBig, exp: &UBig, m: &UBig) -> UBig {
        let mut result = UBig::rem(&UBig::one(), m);
        let mut b = UBig::rem(base, m);
        for i in (0..exp.bits()).rev() {
            result = UBig::mul_mod(&result, &result, m);
            if exp.bit(i) {
                result = UBig::mul_mod(&result, &b, m);
            }
        }
        result
    }

    /// `(v: i64) promoted into [0, m)`: sign-correct Euclidean
    /// placement without any i128 modulus cast (m may be ≥ 2^127).
    pub fn from_i64_rem(value: i64, m: &UBig) -> UBig {
        let magnitude = UBig::from_i64_abs(value);
        let rem = UBig::rem(&magnitude, m);
        if value >= 0 {
            rem
        } else if rem.is_zero() {
            rem
        } else {
            m.sub(&rem)
        }
    }
}

/// `a rem_euclid m` over `UBig` (stage-2 int_rem kernel). `a` is a
/// canonical non-negative big value; the i64-negative case promotes
/// through `from_i64_rem`.
pub fn big_int_rem_checked(a: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("int_rem: modulus must be non-zero");
    }
    Ok(UBig::rem(a, m))
}

/// `a rem_euclid m` with a signed i64 `a` and a big modulus.
pub fn big_int_rem_i64_checked(a: i64, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("int_rem: modulus must be non-zero");
    }
    Ok(UBig::from_i64_rem(a, m))
}

/// Modular inverse via the iterative extended Euclidean algorithm with
/// Bezout coefficients kept in `[0, m)` (same algorithm as the stage-1
/// `mod_inv_checked`; the representation is all that changed).
pub fn big_mod_inv_checked(a: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("mod_inv: modulus must be positive");
    }
    let a = UBig::rem(a, m);
    if a.is_zero() {
        return Err("mod_inv: no inverse exists (gcd != 1)");
    }
    let mut r0 = m.clone();
    let mut r1 = a;
    let mut t0 = UBig::zero();
    let mut t1 = UBig::one();
    while !r1.is_zero() {
        let (quotient, remainder) = big_div_rem(&r0, &r1);
        r0 = r1;
        r1 = remainder;
        // t ← (t0 - q·t1) mod m, staying in [0, m).
        let q_t = UBig::mul_mod(&quotient, &t1, m);
        let next_t = UBig::sub_mod(&t0, &q_t, m);
        t0 = core::mem::replace(&mut t1, next_t);
    }
    if r0.is_one() {
        Ok(t0)
    } else {
        Err("mod_inv: no inverse exists (gcd != 1)")
    }
}

/// Full binary long division: `(a / b, a mod b)`.
pub fn big_div_rem(a: &UBig, b: &UBig) -> (UBig, UBig) {
    let mut quotient = UBig::zero();
    let mut remainder = UBig::zero();
    for i in (0..a.bits()).rev() {
        remainder = remainder.shl1();
        if a.bit(i) {
            remainder.set_bit(0);
        }
        if remainder.cmp(b) != core::cmp::Ordering::Less {
            remainder = remainder.sub(b);
            quotient.set_bit(i);
        }
    }
    (quotient, remainder)
}

/// `base^exp mod m` (stage-2 pow_mod kernel).
pub fn big_pow_mod_checked(base: &UBig, exp: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("pow_mod: modulus must be positive");
    }
    Ok(UBig::mod_pow(base, exp, m))
}

/// Tonelli-Shanks square root in F_p over the stage-2 representation.
/// Same law set as stage-1 `sqrt_mod_checked`: odd prime `p` (2 handled
/// inline), deterministic smallest non-residue, `min(x, p - x)`
/// tie-break, and the exactness gate that doubles as the typed
/// non-residue refusal.
pub fn big_sqrt_mod_checked(a: &UBig, p: &UBig) -> Result<UBig, BigError> {
    if p.is_zero() {
        return Err("sqrt_mod: modulus must be positive");
    }
    let two = UBig::from_u64(2);
    if p.cmp(&two) == core::cmp::Ordering::Equal {
        return Ok(UBig::rem(a, p));
    }
    if p.bit(0) == false {
        return Err("sqrt_mod: modulus must be an odd prime (2 handled above)");
    }
    let modulus = UBig::rem(a, p);
    if modulus.is_zero() {
        return Ok(UBig::zero());
    }
    // Fast path: p ≡ 3 (mod 4) → x = a^((p+1)/4).
    let one = UBig::one();
    let four = UBig::from_u64(4);
    let p_mod_4 = UBig::rem(p, &four);
    let mut x = if p_mod_4 == UBig::from_u64(3) {
        let exp = p.add(&one).div_u64(4);
        UBig::mod_pow(&modulus, &exp, p)
    } else {
        // Legendre pre-check (emath-t63iz, found by the wide-mod tests):
        // the Tonelli-Shanks loop below assumes `a` is a residue — for a
        // non-residue the least-i search reaches i = m and the shift
        // m - i - 1 underflows. Refuse here; the exactness gate below
        // still backstops non-prime p.
        let p_minus_1 = p.sub(&one);
        if UBig::mod_pow(&modulus, &p_minus_1.div_u64(2), p) != one {
            return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
        }
        // General Tonelli-Shanks: p - 1 = q·2^s with q odd.
        let p_minus_1 = p.sub(&one);
        let mut q = p_minus_1.clone();
        let mut s: u64 = 0;
        while q.bit(0) == false {
            q = q.div_u64(2);
            s += 1;
        }
        // Deterministic non-residue search (smallest z with
        // Legendre symbol -1; always exists for prime p).
        let half = p_minus_1.div_u64(2);
        let mut z = UBig::from_u64(2);
        let pm1 = p.sub(&one);
        loop {
            if UBig::mod_pow(&z, &half, p) == pm1 {
                break;
            }
            z = z.add(&one);
        }
        let mut m = s;
        let mut c = UBig::mod_pow(&z, &q, p);
        let mut t = UBig::mod_pow(&modulus, &q, p);
        let mut r = UBig::mod_pow(&modulus, &q.add(&one).div_u64(2), p);
        while !(t.is_one()) {
            // Least i with t^(2^i) = 1.
            let mut i: u64 = 0;
            let mut tt = t.clone();
            while !(tt.is_one()) {
                tt = UBig::mul_mod(&tt, &tt, p);
                i += 1;
            }
            // b = c^(2^(m-i-1)).
            let shift = m - i - 1;
            let mut b = c.clone();
            for _ in 0..shift {
                b = UBig::mul_mod(&b, &b, p);
            }
            m = i;
            c = UBig::mul_mod(&b, &b, p);
            t = UBig::mul_mod(&t, &c, p);
            r = UBig::mul_mod(&r, &b, p);
        }
        r
    };
    // Defensive exactness gate: a fabricated root must never escape
    // (this is also the typed refusal path for quadratic non-residues).
    if UBig::mul_mod(&x, &x, p) != modulus {
        return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
    }
    let mirror = p.sub(&x);
    if x.cmp(&mirror) == core::cmp::Ordering::Greater {
        x = mirror;
    }
    Ok(x)
}

impl UBig {
    /// Divide by a small u64 (helper for the Tonelli-Shanks shifts).
    pub fn div_u64(&self, divisor: u64) -> UBig {
        UBig::div_small(&self.limbs, divisor).0
    }

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

/// Polynomial evaluation over GF(p) by Horner's method with big `x`/`p`
/// (coefficients stay on the f64 surface, exact ≤ 2^53; the Horner
/// products are the stage-2 wide step).
pub fn big_poly_eval_mod_checked(
    coeffs: &[f64],
    x: &UBig,
    p: &UBig,
) -> Result<UBig, BigError> {
    if p.is_zero() {
        return Err("poly_eval_mod: modulus must be positive");
    }
    let mut result = UBig::zero();
    for &c in coeffs.iter().rev() {
        let coefficient = exact_i64_coeff(c)?;
        result = UBig::mul_mod(&result, x, p);
        result = add_i64_mod(&result, coefficient, p);
    }
    Ok(result)
}

/// Reed-Solomon codeword over the big modulus: evaluate at x = 0..n
/// through the shared big Horner kernel.
pub fn big_rs_encode_checked(coeffs: &[f64], n: i64, p: &UBig) -> Result<Vec<UBig>, BigError> {
    if p.is_zero() {
        return Err("rs_encode: modulus must be positive");
    }
    if n <= 0 || UBig::from_u64(n as u64).cmp(p) != core::cmp::Ordering::Less {
        return Err("rs_encode: codeword length n must be in (0, p)");
    }
    let mut codeword = Vec::with_capacity(n as usize);
    for x in 0..n {
        codeword.push(big_poly_eval_mod_checked(coeffs, &UBig::from_u64(x as u64), p)?);
    }
    Ok(codeword)
}

/// `(r + c) mod m` for a signed i64 coefficient.
fn add_i64_mod(r: &UBig, c: i64, m: &UBig) -> UBig {
    if c >= 0 {
        UBig::add_mod(r, &UBig::from_u64(c as u64), m)
    } else {
        UBig::sub_mod(r, &UBig::from_i64_abs(c), m)
    }
}

/// `as i64` on the f64 coefficient surface: NaN→refused, Inf→refused,
/// fractional→refused. Integer kernels refuse silent finite lies.
fn exact_i64_coeff(value: f64) -> Result<i64, BigError> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err("coefficient must be a finite whole number");
    }
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err("coefficient exceeds i64 range");
    }
    Ok(value as i64)
}

// ── Codegen-facing wrappers (emath-t63iz stage 2) ────────────────────────
//
// Generated Rust runs only ADMITTED programs, so a refusal here is an
// internal invariant violation: the panic posture matches the i64
// wrappers in `numeric.rs` (interpreter refusals stay typed; the
// generated lane never observes a refused input).

/// Panicking `int_rem` for generated Rust (admission guarantees `m > 0`).
pub fn big_int_rem(a: &UBig, m: &UBig) -> UBig {
    big_int_rem_checked(a, m).expect("int_rem refusal leaked past admission")
}

/// Panicking `mod_inv` for generated Rust (admission guarantees
/// invertibility or an interpreter-visible fault).
pub fn big_mod_inv(a: &UBig, m: &UBig) -> UBig {
    big_mod_inv_checked(a, m).expect("mod_inv refusal leaked past admission")
}

/// Panicking `pow_mod` for generated Rust.
pub fn big_pow_mod(base: &UBig, exp: &UBig, m: &UBig) -> UBig {
    big_pow_mod_checked(base, exp, m).expect("pow_mod refusal leaked past admission")
}

/// Panicking `sqrt_mod` for generated Rust (admission guarantees a
/// residue base and an odd prime modulus).
pub fn big_sqrt_mod(a: &UBig, p: &UBig) -> UBig {
    big_sqrt_mod_checked(a, p).expect("sqrt_mod refusal leaked past admission")
}

/// Panicking `poly_eval_mod` for generated Rust.
pub fn big_poly_eval_mod(coeffs: &[f64], x: &UBig, p: &UBig) -> UBig {
    big_poly_eval_mod_checked(coeffs, x, p).expect("poly_eval_mod refusal leaked past admission")
}

/// Panicking `rs_encode` for generated Rust.
pub fn big_rs_encode(coeffs: &[f64], n: i64, p: &UBig) -> Vec<UBig> {
    big_rs_encode_checked(coeffs, n, p).expect("rs_encode refusal leaked past admission")
}
