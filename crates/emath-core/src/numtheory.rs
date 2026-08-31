//! B16 (05 §3.3 #3) — number theory stdlib nucleus. std-only, no
//! external crates.
//!
//! Carriers and honesty boundaries:
//! - Primality and factorization live on the `u64` carrier (the full
//!   unsigned machine range); the admitted `.emath` integer surface is
//!   the same range today. Wider carriers refuse by construction (the
//!   function takes `u64`).
//! - Exactness: every returned value is exact; every overflow or
//!   undefined operation (`lcm` past `u64`, modulus 0) is a typed
//!   refusal, never a wrapped value.
//! - `congruence` is the reference contract for the admitted EMIR
//!   `Congruence` op (C9's Wilson spelling:
//!   `congruence(factorial(p - 1), -1, p)`).
//! - `is_prime` / `factorize` / `gcd` / `lcm` are contract-first: the
//!   sema call table does not admit those names yet, so `.emath`
//!   models calling them refuse with the standard unknown-function
//!   diagnostic until the admission-table follow-up lands (the
//!   special-functions seam pattern).

/// Primary decomposition: distinct primes in ascending order with
/// their exponents, `(prime, exponent)`; the product reconstructs `n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factorization {
    pub factors: Vec<(u64, u32)>,
}

/// Deterministic Miller–Rabin primality on the `u64` carrier. The
/// witness set is the first twelve primes
/// `{2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37}` — a proven
/// deterministic certificate for every `n < 2^64` (strong pseudoprimes
/// to the first nine of these, e.g. 3 825 123 056 546 413 051, are
/// caught by 29/31/37). No probabilistic mode: the answer is exact.
pub fn is_prime(n: u64) -> bool {
    const SMALL: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    if n < 2 {
        return false;
    }
    for &p in &SMALL {
        if n == p {
            return true;
        }
        if n % p == 0 {
            return false;
        }
    }
    // Every composite ≤ 37² has a small factor, so what remains is an
    // odd n > 1369 with n − 1 = d·2^s, s ≥ 1.
    let mut d = n - 1;
    let mut s = 0u32;
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'witnesses: for &a in &SMALL {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 0..s - 1 {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'witnesses;
            }
        }
        return false;
    }
    true
}

/// `(a * b) % m` without overflow via the `u128` intermediate.
fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

/// `base^exp % m` by square-and-multiply.
fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut acc = 1u64;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mul_mod(acc, base, m);
        }
        base = mul_mod(base, base, m);
        exp >>= 1;
    }
    acc
}

/// Trial-division primary decomposition — the named reference
/// implementation. `n = 0` refuses (0 has no primary decomposition);
/// `n = 1` is the empty product. Trial division to `√n` is the
/// reference choice: deterministic and simple; Pollard rho is a
/// performance follow-up, not a semantic one.
pub fn factorize(n: u64) -> Result<Factorization, String> {
    if n == 0 {
        return Err("factorize(0) refuses: 0 has no primary decomposition".into());
    }
    let mut rest = n;
    let mut factors = Vec::new();
    if rest % 2 == 0 {
        let mut e = 0u32;
        while rest % 2 == 0 {
            rest /= 2;
            e += 1;
        }
        factors.push((2, e));
    }
    let mut d = 3u64;
    loop {
        // d² may pass 2^64 before n is exhausted; a checked square
        // ends the scan and the remaining rest (if any) is prime.
        let Some(square) = d.checked_mul(d) else {
            break;
        };
        if square > rest {
            break;
        }
        if rest % d == 0 {
            let mut e = 0u32;
            while rest % d == 0 {
                rest /= d;
                e += 1;
            }
            factors.push((d, e));
        }
        d += 2;
    }
    if rest > 1 {
        factors.push((rest, 1));
    }
    Ok(Factorization { factors })
}

/// Euclid's algorithm; `gcd(0, 0) = 0` by the divisibility-lattice
/// convention (0 divides only 0, and gcd is the lattice meet).
pub fn gcd(a: u64, b: u64) -> u64 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Least common multiple on the `u64` carrier: `lcm(0, x) = 0`; a
/// result past `u64::MAX` refuses instead of wrapping.
pub fn lcm(a: u64, b: u64) -> Result<u64, String> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    (a / gcd(a, b))
        .checked_mul(b)
        .ok_or_else(|| format!("lcm({a}, {b}) overflows u64 — no exact carrier"))
}

/// Congruence predicate: `value ≡ residue (mod modulus)`. Both operands
/// normalize into `0..modulus` (Euclidean remainder), so negative
/// values and negative residues compare honestly — `congruence(24, -1, 5)`
/// is true, which is the C9 Wilson spelling. `modulus = 0` refuses:
/// congruence mod 0 is not a defined relation.
pub fn congruence(value: i128, residue: i128, modulus: u64) -> Result<bool, String> {
    if modulus == 0 {
        return Err("congruence refuses modulus 0 — congruence mod 0 is undefined".into());
    }
    let m = modulus as i128;
    Ok(value.rem_euclid(m) == residue.rem_euclid(m))
}
