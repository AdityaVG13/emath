// Signed exact integers for the constructor VM.
//
// Values that fit in `i128` stay there. Larger results promote to a
// little-endian limb magnitude. A limb cap is the named `overflow`
// bound (memory), not a wrap. Floor q-th roots are a machine loop;
// they are not safe as constructor-VM recursion.

use std::cmp::Ordering;
use std::fmt;

const MAX_LIMBS: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExactError {
    Overflow,
    DivisionByZero,
}

impl fmt::Display for ExactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => write!(f, "exact integer exceeded the constructor memory bound"),
            Self::DivisionByZero => write!(f, "exact division by zero"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExactInt {
    Small(i128),
    Big { neg: bool, limbs: Vec<u32> },
}

impl PartialEq for ExactInt {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ExactInt {}

impl PartialOrd for ExactInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExactInt {
    fn cmp(&self, other: &Self) -> Ordering {
        if let (Self::Small(a), Self::Small(b)) = (self, other) {
            return a.cmp(b);
        }
        let sign = self.signum();
        let other_sign = other.signum();
        if sign != other_sign {
            return sign.cmp(&other_sign);
        }
        if sign == 0 {
            return Ordering::Equal;
        }
        let mut left_small = [0_u32; 4];
        let mut right_small = [0_u32; 4];
        let order = cmp_limbs(
            self.magnitude_slice(&mut left_small),
            other.magnitude_slice(&mut right_small),
        );
        if sign < 0 { order.reverse() } else { order }
    }
}

impl fmt::Display for ExactInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Small(n) => write!(f, "{n}"),
            Self::Big { neg, limbs } => {
                if *neg {
                    write!(f, "-")?;
                }
                write!(f, "{}", limbs_to_decimal(limbs))
            }
        }
    }
}

impl From<i128> for ExactInt {
    fn from(n: i128) -> Self {
        Self::Small(n)
    }
}

impl From<i64> for ExactInt {
    fn from(n: i64) -> Self {
        Self::Small(i128::from(n))
    }
}

impl From<u64> for ExactInt {
    fn from(n: u64) -> Self {
        Self::Small(i128::from(n))
    }
}

impl From<usize> for ExactInt {
    fn from(n: usize) -> Self {
        Self::from_u128(n as u128)
    }
}

impl From<i32> for ExactInt {
    fn from(n: i32) -> Self {
        Self::Small(i128::from(n))
    }
}

impl ExactInt {
    pub fn zero() -> Self {
        Self::Small(0)
    }

    pub fn one() -> Self {
        Self::Small(1)
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Self::Small(n) => *n == 0,
            Self::Big { limbs, .. } => limbs.is_empty(),
        }
    }

    pub fn is_one(&self) -> bool {
        matches!(self, Self::Small(1))
    }

    pub fn is_negative(&self) -> bool {
        self.signum() < 0
    }

    pub fn signum(&self) -> i8 {
        match self {
            Self::Small(n) => n.signum() as i8,
            Self::Big { neg, limbs } => {
                if limbs.is_empty() {
                    0
                } else if *neg {
                    -1
                } else {
                    1
                }
            }
        }
    }

    pub fn abs(&self) -> Self {
        match self {
            Self::Small(n) => {
                if let Some(abs) = n.checked_abs() {
                    Self::Small(abs)
                } else {
                    Self::from_parts(false, vec![0, 0, 0, 0x8000_0000]).expect("2^127 fits")
                }
            }
            Self::Big { limbs, .. } => Self::from_parts(false, limbs.clone()).expect("abs shrinks"),
        }
    }

    pub fn checked_neg(&self) -> Result<Self, ExactError> {
        match self {
            Self::Small(n) => {
                if let Some(neg) = n.checked_neg() {
                    Ok(Self::Small(neg))
                } else {
                    Self::from_parts(false, vec![0, 0, 0, 0x8000_0000])
                }
            }
            Self::Big { neg, limbs } => Self::from_parts(!neg, limbs.clone()),
        }
    }

    pub fn to_i128(&self) -> Option<i128> {
        match self {
            Self::Small(n) => Some(*n),
            Self::Big { .. } => None,
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        self.to_i128().and_then(|n| i64::try_from(n).ok())
    }

    /// Significant bits of the magnitude (0 for zero).
    pub fn bit_len(&self) -> usize {
        match self {
            Self::Small(n) => 128 - n.unsigned_abs().leading_zeros() as usize,
            Self::Big { limbs, .. } => limb_bits(limbs),
        }
    }

    pub fn to_usize(&self) -> Option<usize> {
        self.to_i128().and_then(|n| usize::try_from(n).ok())
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            // Integer-to-float `as` rounds to nearest, ties to even.
            Self::Small(n) => *n as f64,
            Self::Big { .. } => exact_ratio_f64(self, &Self::one()),
        }
    }

    pub fn parse(text: &str) -> Result<Self, ExactError> {
        let owned = text.replace('_', "");
        let cleaned = owned.trim();
        if cleaned.is_empty() {
            return Err(ExactError::Overflow);
        }
        if let Ok(n) = cleaned.parse::<i128>() {
            return Ok(Self::Small(n));
        }
        let (neg, digits) = if let Some(rest) = cleaned.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = cleaned.strip_prefix('+') {
            (false, rest)
        } else {
            (false, cleaned)
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ExactError::Overflow);
        }
        let mut limbs = Vec::new();
        let mut start = 0;
        while start < digits.len() {
            let end = (start + 9).min(digits.len());
            let chunk: u64 = digits[start..end]
                .parse()
                .map_err(|_| ExactError::Overflow)?;
            let scale = 10u64.pow((end - start) as u32);
            mul_small_add(&mut limbs, scale, chunk);
            start = end;
        }
        Self::from_parts(neg, limbs)
    }

    pub fn add(&self, other: &Self) -> Result<Self, ExactError> {
        if let (Self::Small(a), Self::Small(b)) = (self, other) {
            if let Some(sum) = a.checked_add(*b) {
                return Ok(Self::Small(sum));
            }
        }
        signed_add(self.neg_limbs(), other.neg_limbs())
    }

    pub fn sub(&self, other: &Self) -> Result<Self, ExactError> {
        self.add(&other.checked_neg()?)
    }

    pub fn mul(&self, other: &Self) -> Result<Self, ExactError> {
        if let (Self::Small(a), Self::Small(b)) = (self, other) {
            if let Some(prod) = a.checked_mul(*b) {
                return Ok(Self::Small(prod));
            }
        }
        if self.is_zero() || other.is_zero() {
            return Ok(Self::zero());
        }
        let (an, al) = self.neg_limbs();
        let (bn, bl) = other.neg_limbs();
        Self::from_parts(an != bn, mul_limbs(&al, &bl))
    }

    /// Euclidean quotient: `n = q * d + r` with `0 <= r < |d|`.
    pub fn quot(&self, other: &Self) -> Result<Self, ExactError> {
        let (q, r) = self.div_rem(other)?;
        if r.is_negative() {
            if other.is_negative() {
                q.add(&Self::one())
            } else {
                q.sub(&Self::one())
            }
        } else {
            Ok(q)
        }
    }

    pub fn rem_euclid(&self, other: &Self) -> Result<Self, ExactError> {
        if other.is_zero() {
            return Err(ExactError::DivisionByZero);
        }
        let (_, rem) = self.div_rem(other)?;
        if rem.is_negative() {
            rem.add(&other.abs())
        } else {
            Ok(rem)
        }
    }

    pub fn pow(&self, exp: &Self) -> Result<Self, ExactError> {
        if exp.is_negative() {
            return Ok(Self::one());
        }
        let Some(e) = exp.to_i128() else {
            return Err(ExactError::Overflow);
        };
        if e > i128::from(u32::MAX) {
            return Err(ExactError::Overflow);
        }
        self.pow_u32(e as u32)
    }

    fn pow_u32(&self, exp: u32) -> Result<Self, ExactError> {
        if exp == 0 {
            return Ok(Self::one());
        }
        let mut base = self.clone();
        let mut acc = Self::one();
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&base)?;
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base)?;
            }
        }
        Ok(acc)
    }

    pub fn gcd(a: &Self, b: &Self) -> Result<Self, ExactError> {
        let mut x = a.abs();
        let mut y = b.abs();
        while !y.is_zero() {
            let r = x.rem_euclid(&y)?;
            x = y;
            y = r;
        }
        Ok(x)
    }

    pub fn div_rem(&self, other: &Self) -> Result<(Self, Self), ExactError> {
        if other.is_zero() {
            return Err(ExactError::DivisionByZero);
        }
        match (self, other) {
            (Self::Small(a), Self::Small(b)) if *b != 0 => {
                if *a == i128::MIN && *b == -1 {
                    return Ok((
                        Self::from_parts(false, vec![0, 0, 0, 0x8000_0000])?,
                        Self::zero(),
                    ));
                }
                return Ok((Self::Small(a / b), Self::Small(a % b)));
            }
            _ => {}
        }
        let (an, al) = self.neg_limbs();
        let (_, bl) = other.neg_limbs();
        if cmp_limbs(&al, &bl) == Ordering::Less {
            return Ok((Self::zero(), self.clone()));
        }
        let (ql, rl) = div_rem_limbs(&al, &bl);
        let quot = Self::from_parts(an != other.is_negative() && !ql.is_empty(), ql)?;
        let rem = Self::from_parts(an && !rl.is_empty(), rl)?;
        Ok((quot, rem))
    }

    fn from_u128(n: u128) -> Self {
        if n <= i128::MAX as u128 {
            Self::Small(n as i128)
        } else {
            Self::from_parts(false, u128_limbs(n)).expect("u128 fits")
        }
    }

    fn from_parts(neg: bool, mut limbs: Vec<u32>) -> Result<Self, ExactError> {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        if limbs.len() > MAX_LIMBS {
            return Err(ExactError::Overflow);
        }
        if limbs.is_empty() {
            return Ok(Self::Small(0));
        }
        if let Some(small) = limbs_to_i128(neg, &limbs) {
            return Ok(Self::Small(small));
        }
        Ok(Self::Big { neg, limbs })
    }

    fn magnitude_slice<'a>(&'a self, small: &'a mut [u32; 4]) -> &'a [u32] {
        match self {
            Self::Big { limbs, .. } => limbs,
            Self::Small(value) => {
                let mut magnitude = value.unsigned_abs();
                let mut len = 0;
                while magnitude != 0 {
                    small[len] = magnitude as u32;
                    magnitude >>= 32;
                    len += 1;
                }
                &small[..len]
            }
        }
    }

    fn neg_limbs(&self) -> (bool, Vec<u32>) {
        match self {
            Self::Small(n) => {
                if *n == 0 {
                    (false, Vec::new())
                } else if *n == i128::MIN {
                    (true, vec![0, 0, 0, 0x8000_0000])
                } else {
                    (n.is_negative(), u128_limbs(n.unsigned_abs()))
                }
            }
            Self::Big { neg, limbs } => (*neg, limbs.clone()),
        }
    }
}

pub fn exact_ratio(num: ExactInt, den: ExactInt) -> Result<(ExactInt, ExactInt), ExactError> {
    if den.is_zero() {
        return Err(ExactError::DivisionByZero);
    }
    let g = ExactInt::gcd(&num, &den)?;
    let mut n = num.quot(&g)?;
    let mut d = den.quot(&g)?;
    if d.is_negative() {
        n = n.checked_neg()?;
        d = d.checked_neg()?;
    }
    Ok((n, d))
}

/// `num / den` rounded once to the nearest binary64, ties to even
/// (overflow to infinity, gradual underflow, `den == 0` gives NaN).
/// Dividing two separately converted halves overflows for operands past
/// 2^1024 and double-rounds for operands past 2^53.
pub fn exact_ratio_f64(num: &ExactInt, den: &ExactInt) -> f64 {
    if den.is_zero() {
        return f64::NAN;
    }
    if num.is_zero() {
        return 0.0;
    }
    if let (ExactInt::Small(n), ExactInt::Small(d)) = (num, den) {
        const EXACT: u128 = 1 << 53;
        if n.unsigned_abs() <= EXACT && d.unsigned_abs() <= EXACT {
            // Both operands are exact binary64 values; IEEE division
            // rounds the quotient once.
            return *n as f64 / *d as f64;
        }
    }
    let negative = num.is_negative() != den.is_negative();
    let a = num.abs();
    let b = den.abs();
    let shift = |x: &ExactInt, k: i64| {
        x.mul(&ExactInt::Small(2).pow(&ExactInt::from(k)).expect("shift fits"))
            .expect("shift fits")
    };
    // Scale so the integer quotient carries 55 or 56 significant bits:
    // q = floor(a * 2^s / b), the value is (q + rest) * 2^-s.
    let s = 55 - (a.bit_len() as i64 - b.bit_len() as i64);
    let (scaled_a, scaled_b) = if s >= 0 { (shift(&a, s), b) } else { (a, shift(&b, -s)) };
    let (q, r) = scaled_a.div_rem(&scaled_b).expect("nonzero divisor");
    let q = q.to_i128().expect("quotient has at most 56 bits") as u128;
    let sticky = !r.is_zero();
    let q_bits = 128 - i64::from(q.leading_zeros());
    // The value lies in [2^k, 2^(k+1)); binary64 keeps 53 bits from
    // 2^-1022 up and a fixed 2^-1074 quantum below it.
    let k = q_bits - 1 - s;
    if k > 1023 {
        return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let precision = if k >= -1022 { 53 } else { 53 - (-1022 - k) };
    let drop = q_bits - precision;
    let m = if drop <= 0 {
        q << (-drop)
    } else if drop >= 128 {
        // Below half the smallest subnormal.
        0
    } else {
        let kept = q >> drop;
        let half = 1u128 << (drop - 1);
        let rest = q & ((1u128 << drop) - 1);
        let up = rest > half || (rest == half && (sticky || kept & 1 == 1));
        kept + u128::from(up)
    };
    let mut x = m as f64;
    // m * 2^e is representable (or overflows to infinity), so scaling by
    // exact powers of two in bounded steps adds no rounding.
    let mut e = drop - s;
    while e > 0 {
        let step = e.min(1000);
        x *= f64::from_bits(((step + 1023) as u64) << 52);
        e -= step;
    }
    while e < 0 {
        let step = (-e).min(1000);
        x *= f64::from_bits(((1023 - step) as u64) << 52);
        e += step;
    }
    if negative { -x } else { x }
}

pub fn exact_int_sum(items: &[ExactInt]) -> Result<ExactInt, ExactError> {
    let mut acc = ExactInt::zero();
    for item in items {
        acc = acc.add(item)?;
    }
    Ok(acc)
}

pub fn exact_int_prod(items: &[ExactInt]) -> Result<ExactInt, ExactError> {
    let mut acc = ExactInt::one();
    for item in items {
        acc = acc.mul(item)?;
    }
    Ok(acc)
}

fn u128_limbs(mut n: u128) -> Vec<u32> {
    let mut limbs = Vec::new();
    while n > 0 {
        limbs.push(n as u32);
        n >>= 32;
    }
    limbs
}

fn limbs_to_i128(neg: bool, limbs: &[u32]) -> Option<i128> {
    if limbs.len() > 4 {
        return None;
    }
    let mut acc = 0u128;
    for (i, limb) in limbs.iter().enumerate() {
        acc |= u128::from(*limb) << (32 * i);
    }
    if !neg {
        i128::try_from(acc).ok()
    } else if acc == 1u128 << 127 {
        Some(i128::MIN)
    } else {
        i128::try_from(acc).ok().and_then(|n| n.checked_neg())
    }
}

fn cmp_limbs(a: &[u32], b: &[u32]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_limbs(a: &[u32], b: &[u32]) -> Vec<u32> {
    let n = a.len().max(b.len());
    let mut limbs = Vec::with_capacity(n + 1);
    let mut carry = 0u64;
    for i in 0..n {
        let sum = u64::from(*a.get(i).unwrap_or(&0)) + u64::from(*b.get(i).unwrap_or(&0)) + carry;
        limbs.push(sum as u32);
        carry = sum >> 32;
    }
    if carry != 0 {
        limbs.push(carry as u32);
    }
    limbs
}

fn sub_limbs(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut limbs = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let left = i64::from(a[i]);
        let right = i64::from(*b.get(i).unwrap_or(&0)) + borrow;
        if left >= right {
            limbs.push((left - right) as u32);
            borrow = 0;
        } else {
            limbs.push((left + (1 << 32) - right) as u32);
            borrow = 1;
        }
    }
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    limbs
}

fn mul_limbs(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut limbs = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let cur = u64::from(limbs[i + j]) + u64::from(x) * u64::from(y) + carry;
            limbs[i + j] = cur as u32;
            carry = cur >> 32;
        }
        limbs[i + b.len()] = carry as u32;
    }
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    limbs
}

fn mul_small_add(limbs: &mut Vec<u32>, scale: u64, add: u64) {
    let mut carry = add;
    for limb in limbs.iter_mut() {
        let cur = u64::from(*limb) * scale + carry;
        *limb = cur as u32;
        carry = cur >> 32;
    }
    while carry != 0 {
        limbs.push(carry as u32);
        carry >>= 32;
    }
}

fn limb_bits(limbs: &[u32]) -> usize {
    let Some(last) = limbs.last() else {
        return 0;
    };
    (limbs.len() - 1) * 32 + (32 - last.leading_zeros() as usize)
}

fn div_rem_limbs(num: &[u32], den: &[u32]) -> (Vec<u32>, Vec<u32>) {
    if cmp_limbs(num, den) == Ordering::Less {
        return (Vec::new(), num.to_vec());
    }
    if den.len() == 1 && den[0] != 0 {
        let divisor = u64::from(den[0]);
        let mut quot = num.to_vec();
        let mut rem = 0_u64;
        for limb in quot.iter_mut().rev() {
            // rem < divisor <= u32::MAX, so the joined word fits in u64.
            let current = (rem << 32) | u64::from(*limb);
            *limb = (current / divisor) as u32;
            rem = current % divisor;
        }
        while quot.last() == Some(&0) {
            quot.pop();
        }
        let rem = if rem == 0 {
            Vec::new()
        } else {
            vec![rem as u32]
        };
        return (quot, rem);
    }
    // Knuth, TAOCP vol. 2, 4.3.1, Algorithm D (as in Hacker's Delight
    // divmnu): normalize so the divisor's top bit is set; then each
    // estimated quotient limb is at most two too large, and one
    // multiply-subtract per limb (plus a rare add-back) settles it.
    let shift = den[den.len() - 1].leading_zeros();
    let v = shl_bits(den, shift, den.len());
    let mut u = shl_bits(num, shift, num.len() + 1);
    let n = v.len();
    let m = num.len() - n;
    let base = 1u64 << 32;
    let top = u64::from(v[n - 1]);
    let next = u64::from(v[n - 2]);
    let mut quot = vec![0u32; m + 1];
    for j in (0..=m).rev() {
        let joined = (u64::from(u[j + n]) << 32) | u64::from(u[j + n - 1]);
        let mut qhat = joined / top;
        let mut rhat = joined % top;
        while qhat >= base || qhat * next > ((rhat << 32) | u64::from(u[j + n - 2])) {
            qhat -= 1;
            rhat += top;
            if rhat >= base {
                break;
            }
        }
        let mut borrow = 0i64;
        for i in 0..n {
            let product = qhat * u64::from(v[i]);
            let t = i64::from(u[i + j]) - borrow - (product & 0xFFFF_FFFF) as i64;
            u[i + j] = t as u32;
            borrow = (product >> 32) as i64 - (t >> 32);
        }
        let t = i64::from(u[j + n]) - borrow;
        u[j + n] = t as u32;
        if t < 0 {
            qhat -= 1;
            let mut carry = 0u64;
            for i in 0..n {
                let sum = u64::from(u[i + j]) + u64::from(v[i]) + carry;
                u[i + j] = sum as u32;
                carry = sum >> 32;
            }
            u[j + n] = u[j + n].wrapping_add(carry as u32);
        }
        quot[j] = qhat as u32;
    }
    let mut rem = vec![0u32; n];
    for i in 0..n {
        rem[i] = if shift == 0 {
            u[i]
        } else {
            (u[i] >> shift) | (u[i + 1] << (32 - shift))
        };
    }
    while quot.last() == Some(&0) {
        quot.pop();
    }
    while rem.last() == Some(&0) {
        rem.pop();
    }
    (quot, rem)
}

fn shl_bits(limbs: &[u32], shift: u32, len: usize) -> Vec<u32> {
    let mut out = vec![0u32; len];
    let mut carry = 0u32;
    for (i, &limb) in limbs.iter().enumerate() {
        out[i] = if shift == 0 {
            limb
        } else {
            (limb << shift) | carry
        };
        carry = if shift == 0 { 0 } else { limb >> (32 - shift) };
    }
    if len > limbs.len() {
        out[limbs.len()] = carry;
    }
    out
}

fn signed_add(a: (bool, Vec<u32>), b: (bool, Vec<u32>)) -> Result<ExactInt, ExactError> {
    if a.0 == b.0 {
        return ExactInt::from_parts(a.0, add_limbs(&a.1, &b.1));
    }
    match cmp_limbs(&a.1, &b.1) {
        Ordering::Equal => Ok(ExactInt::zero()),
        Ordering::Greater => ExactInt::from_parts(a.0, sub_limbs(&a.1, &b.1)),
        Ordering::Less => ExactInt::from_parts(b.0, sub_limbs(&b.1, &a.1)),
    }
}

fn limbs_to_decimal(limbs: &[u32]) -> String {
    if limbs.is_empty() {
        return "0".to_string();
    }
    let mut cur = limbs.to_vec();
    let mut chunks = Vec::new();
    while !cur.is_empty() {
        let mut rem = 0u64;
        for limb in cur.iter_mut().rev() {
            let cur_val = (rem << 32) | u64::from(*limb);
            *limb = (cur_val / 1_000_000_000) as u32;
            rem = cur_val % 1_000_000_000;
        }
        chunks.push(rem as u32);
        while cur.last() == Some(&0) {
            cur.pop();
        }
    }
    let mut text = String::new();
    for (i, chunk) in chunks.iter().enumerate().rev() {
        if i + 1 == chunks.len() {
            text.push_str(&chunk.to_string());
        } else {
            text.push_str(&format!("{chunk:09}"));
        }
    }
    text
}
