//! B44 nucleus — Clifford algebras `Cl(p, q)` over f64 (bead
//! `emath-r3-quaternions-cgvg`).
//!
//! C10 (value generics) is CLOSED, but the admitted `.emath` generic
//! surface is the follow-up; the nucleus exposes the algebra as a
//! runtime `(p, q)` value — `CliffordBasis::new(p, q)` — which is the
//! same carrier a const-generic wrapper would embed, and the same one
//! the sema admission table will bind for `Clifford<p, q>`.
//!
//! Multiplication table DERIVED from (p, q), never hand-listed:
//! for basis vectors e_i, the geometric product obeys
//!   e_i·e_i = +1 (i ≤ p, Euclidean) or −1 (i > p, anti-Euclidean),
//!   e_i·e_j = −e_j·e_i (i ≠ j).
//! A blade is a bitmask over the n = p+q basis vectors; products of
//! blades are computed by the sign of the permutation that sorts the
//! concatenated index lists (the canonical anticommutation rule).
//!
//! Honesty: coefficients are f64, labeled; `blade_count` is 2^(p+q)
//! (exponential in dimension — callers with large (p, q) carry the
//! cost they asked for; the nucleus does not silently truncate).

/// One graded basis blade of `Cl(p, q)`: a bitmask over the n basis
/// vectors (bit i set = e_i present in the blade product).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Blade {
    mask: u32,
}

impl Blade {
    /// The scalar blade (empty product).
    #[must_use]
    pub fn scalar() -> Blade {
        Blade { mask: 0 }
    }

    /// The blade's bitmask (bit i = e_i present).
    #[must_use]
    pub fn mask(self) -> u32 {
        self.mask
    }

    /// Grade = popcount of the mask (0 = scalar, 1 = vector, 2 =
    /// bivector, …).
    #[must_use]
    pub fn grade(self) -> u32 {
        self.mask.count_ones()
    }
}

/// The multiplication structure of `Cl(p, q)` for fixed `(p, q)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliffordBasis {
    p: u32,
    q: u32,
}

impl CliffordBasis {
    /// The algebra `Cl(p, q)`: p Euclidean directions (+1 squares),
    /// q anti-Euclidean (−1 squares).
    #[must_use]
    pub fn new(p: u32, q: u32) -> CliffordBasis {
        CliffordBasis { p, q }
    }

    /// n = p + q basis vectors.
    #[must_use]
    pub fn dimension(&self) -> u32 {
        self.p + self.q
    }

    /// 2^n basis blades.
    #[must_use]
    pub fn blade_count(&self) -> u32 {
        1 << (self.p + self.q)
    }

    /// The unit blade for a single basis vector e_index (1-based in
    /// the tests' spelling: `blade(1)` = e1). Mask bit (index−1).
    #[must_use]
    pub fn blade(&self, index: u32) -> Blade {
        debug_assert!(index >= 1 && index <= self.dimension());
        Blade {
            mask: 1 << (index - 1),
        }
    }

    /// The square sign of basis vector e_i (+1 Euclidean, −1
    /// anti-Euclidean).
    fn vector_square_sign(&self, bit: u32) -> f64 {
        if bit < self.p { 1.0 } else { -1.0 }
    }

    /// Multiply two BLADES; returns the resulting blade and sign.
    /// Rule: reduce the concatenated index list (bits(a) ++ bits(b))
    /// with two adjacent-pair rules — equal vectors annihilate into
    /// their square sign (both removed); swapped neighbors contribute
    /// −1 each (canonical anticommutation). Terminates because each
    /// reduction strictly shrinks the list or strictly increases its
    /// sortedness.
    fn multiply_blades(&self, a: Blade, b: Blade) -> (Blade, f64) {
        let mut list: Vec<u32> = Vec::new();
        for bit in 0..(self.p + self.q) {
            if a.mask & (1 << bit) != 0 {
                list.push(bit);
            }
        }
        for bit in 0..(self.p + self.q) {
            if b.mask & (1 << bit) != 0 {
                list.push(bit);
            }
        }
        let mut sign = 1.0_f64;
        loop {
            let mut reduced = false;
            for i in 0..list.len().saturating_sub(1) {
                if list[i] == list[i + 1] {
                    sign *= self.vector_square_sign(list[i]);
                    list.remove(i);
                    list.remove(i);
                    reduced = true;
                    break;
                }
                if list[i] > list[i + 1] {
                    list.swap(i, i + 1);
                    sign *= -1.0;
                    reduced = true;
                    break;
                }
            }
            if !reduced {
                break;
            }
        }
        (
            Blade {
                mask: list.iter().fold(0u32, |acc, bit| acc | (1 << bit)),
            },
            sign,
        )
    }

    /// Multiply two blades, returning the coefficient-carrying
    /// multivector (single term).
    #[must_use]
    pub fn multiply(&self, a: Blade, b: Blade) -> MultiVector {
        let (blade, sign) = self.multiply_blades(a, b);
        MultiVector {
            basis: self.clone(),
            terms: vec![(blade, sign)],
        }
    }

    /// Full geometric product of two multivectors (distributes over
    /// blade pairs and collects like terms).
    #[must_use]
    pub fn multiply_multivector(&self, a: &MultiVector, b: &MultiVector) -> MultiVector {
        let mut terms: Vec<(Blade, f64)> = Vec::new();
        for (blade_a, coefficient_a) in &a.terms {
            for (blade_b, coefficient_b) in &b.terms {
                let (blade, sign) = self.multiply_blades(*blade_a, *blade_b);
                terms.push((blade, sign * coefficient_a * coefficient_b));
            }
        }
        MultiVector::collect(self.clone(), terms)
    }
}

/// A multivector: a sparse sum of (blade, coefficient) terms over one
/// fixed basis.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiVector {
    basis: CliffordBasis,
    terms: Vec<(Blade, f64)>,
}

impl MultiVector {
    /// Build from (mask, coefficient) pairs.
    #[must_use]
    pub fn from_blades(basis: &CliffordBasis, terms: &[(u32, f64)]) -> MultiVector {
        MultiVector::collect(
            basis.clone(),
            terms
                .iter()
                .map(|(mask, coefficient)| (Blade { mask: *mask }, *coefficient))
                .collect(),
        )
    }

    /// Coefficient of a blade mask (0 when absent — sparse view).
    #[must_use]
    pub fn coefficient_of(&self, mask: u32) -> f64 {
        self.terms
            .iter()
            .find(|(blade, _)| blade.mask() == mask)
            .map_or(0.0, |(_, coefficient)| *coefficient)
    }

    /// All nonzero terms, ascending mask order (canonical view).
    #[must_use]
    pub fn terms(&self) -> &[(Blade, f64)] {
        &self.terms
    }

    fn collect(basis: CliffordBasis, mut terms: Vec<(Blade, f64)>) -> MultiVector {
        terms.sort_by_key(|(blade, _)| blade.mask());
        let mut collected: Vec<(Blade, f64)> = Vec::new();
        for (blade, coefficient) in terms {
            if coefficient == 0.0 {
                continue;
            }
            match collected.last_mut() {
                Some((last_blade, last_coefficient)) if last_blade.mask() == blade.mask() => {
                    *last_coefficient += coefficient;
                }
                _ => collected.push((blade, coefficient)),
            }
        }
        collected.retain(|(_, coefficient)| *coefficient != 0.0);
        MultiVector {
            basis,
            terms: collected,
        }
    }
}
