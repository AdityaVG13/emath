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

    pub fn to_usize(&self) -> Option<usize> {
        self.to_i128().and_then(|n| usize::try_from(n).ok())
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            Self::Small(n) => *n as f64,
            Self::Big { neg, limbs } => {
                let bits = limb_bits(limbs);
                if bits == 0 {
                    return 0.0;
                }
                if bits > 1023 {
                    return if *neg {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    };
                }
                let mut acc = 0.0f64;
                for (i, limb) in limbs.iter().enumerate() {
                    acc += f64::from(*limb) * 2.0f64.powi((i as i32) * 32);
                }
                if *neg { -acc } else { acc }
            }
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

    pub fn floor_root(&self, q: &Self) -> Result<Self, ExactError> {
        if self.is_zero() || self.is_negative() {
            return Ok(Self::zero());
        }
        if q <= &Self::zero() {
            return Ok(Self::zero());
        }
        if q == &Self::one() {
            return Ok(self.clone());
        }
        let q_u = match q.to_i128() {
            Some(q) if (2..=i128::from(u32::MAX)).contains(&q) => q as u32,
            _ => return Ok(Self::one()),
        };
        let mut lo = Self::zero();
        // Exclusive upper bound. `n+1` can exceed the limb cap when `n`
        // is already at it; for q >= 2 the root is then < n.
        let mut hi = match self.add(&Self::one()) {
            Ok(hi) => hi,
            Err(ExactError::Overflow) => self.clone(),
            Err(err) => return Err(err),
        };
        while hi.sub(&lo)?.cmp(&Self::one()) == Ordering::Greater {
            let mid = lo.add(&hi.sub(&lo)?.quot(&Self::from(2i128))?)?;
            match mid.pow_u32(q_u) {
                Ok(powered) if powered.cmp(self) != Ordering::Greater => lo = mid,
                Ok(_) | Err(ExactError::Overflow) => hi = mid,
                Err(err) => return Err(err),
            }
        }
        Ok(lo)
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

    /// `(g, s, t)` with `g = gcd(a, b) >= 0` and `s * a + t * b = g`.
    pub fn egcd(a: &Self, b: &Self) -> Result<(Self, Self, Self), ExactError> {
        let mut old_r = a.clone();
        let mut r = b.clone();
        let mut old_s = Self::one();
        let mut s = Self::zero();
        let mut old_t = Self::zero();
        let mut t = Self::one();
        while !r.is_zero() {
            let q = old_r.quot(&r)?;
            let next_r = old_r.sub(&q.mul(&r)?)?;
            old_r = r;
            r = next_r;
            let next_s = old_s.sub(&q.mul(&s)?)?;
            old_s = s;
            s = next_s;
            let next_t = old_t.sub(&q.mul(&t)?)?;
            old_t = t;
            t = next_t;
        }
        if old_r.is_negative() {
            old_r = old_r.checked_neg()?;
            old_s = old_s.checked_neg()?;
            old_t = old_t.checked_neg()?;
        }
        Ok((old_r, old_s, old_t))
    }

    /// Multiplicative `C(n, k)`. `k < 0`, `n < 0`, or `k > n` is 0. `C(n, 0) = 1`.
    pub fn binomial(&self, k: &Self) -> Result<Self, ExactError> {
        if self.is_negative() || k.is_negative() {
            return Ok(Self::zero());
        }
        if k.cmp(self) == Ordering::Greater {
            return Ok(Self::zero());
        }
        let nmk = self.sub(k)?;
        let kk = if k.cmp(&nmk) == Ordering::Greater {
            nmk
        } else {
            k.clone()
        };
        if kk.is_zero() {
            return Ok(Self::one());
        }
        let mut acc = Self::one();
        let mut i = Self::one();
        while i.cmp(&kk) != Ordering::Greater {
            acc = acc.mul(&self.sub(&i)?.add(&Self::one())?)?;
            acc = acc.quot(&i)?;
            i = i.add(&Self::one())?;
        }
        Ok(acc)
    }

    /// Census `| { k : 1 <= k <= n, gcd(k, n) = 1 } |`. `n <= 0` is 0.
    /// `n` past 1_000_000 is `overflow` (same bound as constructor ranges).
    pub fn totient(&self) -> Result<Self, ExactError> {
        if self.is_negative() || self.is_zero() {
            return Ok(Self::zero());
        }
        if self.is_one() {
            return Ok(Self::one());
        }
        let Some(n) = self.to_i128() else {
            return Err(ExactError::Overflow);
        };
        if n > 1_000_000 {
            return Err(ExactError::Overflow);
        }
        let mut acc = Self::zero();
        let mut k = Self::one();
        while k.cmp(self) != Ordering::Greater {
            if Self::gcd(&k, self)? == Self::one() {
                acc = acc.add(&Self::one())?;
            }
            k = k.add(&Self::one())?;
        }
        Ok(acc)
    }

    pub fn factorial(&self) -> Result<Self, ExactError> {
        if self.is_negative() {
            return Ok(Self::zero());
        }
        if self.is_zero() {
            return Ok(Self::one());
        }
        let mut acc = Self::one();
        let mut k = Self::one();
        while k.cmp(self) != Ordering::Greater {
            acc = acc.mul(&k)?;
            k = k.add(&Self::one())?;
        }
        Ok(acc)
    }

    pub fn rising(&self, k: &Self) -> Result<Self, ExactError> {
        if k.is_negative() {
            return Ok(Self::zero());
        }
        if k.is_zero() {
            return Ok(Self::one());
        }
        let mut acc = Self::one();
        let mut x = self.clone();
        let mut i = Self::zero();
        while i.cmp(k) == Ordering::Less {
            acc = acc.mul(&x)?;
            x = x.add(&Self::one())?;
            i = i.add(&Self::one())?;
        }
        Ok(acc)
    }

    pub fn falling(&self, k: &Self) -> Result<Self, ExactError> {
        if k.is_negative() {
            return Ok(Self::zero());
        }
        if k.is_zero() {
            return Ok(Self::one());
        }
        let mut acc = Self::one();
        let mut x = self.clone();
        let mut i = Self::zero();
        while i.cmp(k) == Ordering::Less {
            acc = acc.mul(&x)?;
            x = x.sub(&Self::one())?;
            i = i.add(&Self::one())?;
        }
        Ok(acc)
    }

    pub fn double_factorial(&self) -> Result<Self, ExactError> {
        if self.is_negative() {
            return Ok(Self::zero());
        }
        if self.is_zero() {
            return Ok(Self::one());
        }
        let mut acc = Self::one();
        let mut n = self.clone();
        let two = Self::from(2i128);
        while n.cmp(&Self::zero()) == Ordering::Greater {
            acc = acc.mul(&n)?;
            n = n.sub(&two)?;
        }
        Ok(acc)
    }

    pub fn mod_inv(&self, modulus: &Self) -> Result<Self, ExactError> {
        if modulus <= &Self::one() {
            return Ok(Self::zero());
        }
        let mut t = Self::zero();
        let mut newt = Self::one();
        let mut r = modulus.abs();
        let mut newr = self.rem_euclid(modulus)?;
        while !newr.is_zero() {
            let q = r.quot(&newr)?;
            let next_t = t.sub(&q.mul(&newt)?)?;
            t = newt;
            newt = next_t;
            let next_r = r.sub(&q.mul(&newr)?)?;
            r = newr;
            newr = next_r;
        }
        if r > Self::one() {
            return Ok(Self::zero());
        }
        if t.is_negative() {
            t = t.add(&modulus.abs())?;
        }
        Ok(t)
    }

    pub fn pow_mod(&self, exp: &Self, modulus: &Self) -> Result<Self, ExactError> {
        if modulus.is_zero() || modulus.is_negative() {
            return Ok(Self::zero());
        }
        if exp.is_zero() {
            return Self::one().rem_euclid(modulus);
        }
        if exp.is_negative() {
            let inv = self.mod_inv(modulus)?;
            if inv.is_zero() {
                return Ok(Self::zero());
            }
            return inv.pow_mod(&exp.abs(), modulus);
        }
        let mut base = self.rem_euclid(modulus)?;
        let mut acc = Self::one();
        let mut e = exp.clone();
        let two = Self::from(2i128);
        while !e.is_zero() {
            if e.rem_euclid(&two)?.is_one() {
                acc = acc.mul(&base)?.rem_euclid(modulus)?;
            }
            e = e.quot(&two)?;
            if !e.is_zero() {
                base = base.mul(&base)?.rem_euclid(modulus)?;
            }
        }
        Ok(acc)
    }

    pub fn sqrt_mod(&self, prime: &Self) -> Result<Self, ExactError> {
        if prime <= &Self::one() {
            return Ok(Self::zero());
        }
        let value = self.rem_euclid(prime)?;
        let mut trial = Self::zero();
        while trial.cmp(prime) == Ordering::Less {
            if trial.mul(&trial)?.rem_euclid(prime)? == value {
                return Ok(trial);
            }
            trial = trial.add(&Self::one())?;
        }
        Ok(Self::zero())
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

pub fn exact_int_sum_from(items: &[ExactInt], start: &ExactInt) -> Result<ExactInt, ExactError> {
    exact_int_sum(suffix(items, start))
}

pub fn exact_int_prod_from(items: &[ExactInt], start: &ExactInt) -> Result<ExactInt, ExactError> {
    exact_int_prod(suffix(items, start))
}

fn suffix<'a>(items: &'a [ExactInt], start: &ExactInt) -> &'a [ExactInt] {
    let from = match start.to_usize() {
        Some(n) => n.min(items.len()),
        None if start.is_negative() => 0,
        None => items.len(),
    };
    &items[from..]
}

pub fn exact_int_hamming(left: &[ExactInt], right: &[ExactInt]) -> Result<ExactInt, ExactError> {
    let mut acc = ExactInt::from(left.len().abs_diff(right.len()));
    for (a, b) in left.iter().zip(right) {
        if a != b {
            acc = acc.add(&ExactInt::one())?;
        }
    }
    Ok(acc)
}

pub fn exact_int_weighted_prod(
    xs: &[ExactInt],
    ws: &[ExactInt],
    start: &ExactInt,
    acc: &ExactInt,
) -> Result<ExactInt, ExactError> {
    let from = match start.to_usize() {
        Some(n) => n,
        None if start.is_negative() => 0,
        None => return Ok(acc.clone()),
    };
    let mut acc = acc.clone();
    for i in from..xs.len() {
        let Some(weight) = ws.get(i) else {
            break;
        };
        acc = acc.mul(&xs[i].pow(weight)?)?;
    }
    Ok(acc)
}

pub fn exact_int_poly_eval(
    coeffs: &[ExactInt],
    point: &ExactInt,
    modulus: &ExactInt,
) -> Result<ExactInt, ExactError> {
    if modulus.is_zero() || modulus.is_negative() {
        return Ok(ExactInt::zero());
    }
    let mut acc = ExactInt::zero();
    for coeff in coeffs.iter().rev() {
        acc = coeff.add(&point.mul(&acc)?)?.rem_euclid(modulus)?;
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

fn bit(limbs: &[u32], index: usize) -> bool {
    let limb = index / 32;
    let offset = index % 32;
    limbs
        .get(limb)
        .is_some_and(|value| (value >> offset) & 1 == 1)
}

fn set_bit(limbs: &mut Vec<u32>, index: usize) {
    let limb = index / 32;
    let offset = index % 32;
    if limbs.len() <= limb {
        limbs.resize(limb + 1, 0);
    }
    limbs[limb] |= 1 << offset;
}

fn limb_bits(limbs: &[u32]) -> usize {
    let Some(last) = limbs.last() else {
        return 0;
    };
    (limbs.len() - 1) * 32 + (32 - last.leading_zeros() as usize)
}

fn shl1(limbs: &mut Vec<u32>) {
    let mut carry = 0u32;
    for limb in limbs.iter_mut() {
        let next = *limb >> 31;
        *limb = (*limb << 1) | carry;
        carry = next;
    }
    if carry != 0 {
        limbs.push(carry);
    }
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
    let mut rem = Vec::new();
    let mut quot = vec![0u32; num.len()];
    for i in (0..limb_bits(num)).rev() {
        shl1(&mut rem);
        if bit(num, i) {
            set_bit(&mut rem, 0);
        }
        if cmp_limbs(&rem, den) != Ordering::Less {
            rem = sub_limbs(&rem, den);
            set_bit(&mut quot, i);
        }
    }
    while quot.last() == Some(&0) {
        quot.pop();
    }
    while rem.last() == Some(&0) {
        rem.pop();
    }
    (quot, rem)
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
