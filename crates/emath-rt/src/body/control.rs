// ── Finite-category surface ─────────────────────────
//
// The kernel: a finite
// `(dom, cod, comp)` — per-morphism object indices plus a DENSE k×k
// composition table. `comp[i][j] = m_i ∘ m_j` (j FIRST, then i) and is
// defined exactly when `cod[j] == dom[i]`; `-1.0` marks undefined. The
// composite's dom/cod are `dom[j]`/`cod[i]`. Objects are implicit
// `0..n`, `n = max(dom ∪ cod) + 1`. Equal morphism INDEX means equal
// morphism. Diagrams are face path-pairs: each face record is
// `[start, end, len_l, len_r, left…, right…]` (both paths ≥ 1
// morphism) and is commutative iff both path composites are the SAME
// morphism index.
//
// Category laws (composition totality/alignment, identity existence,
// associativity) are CERTIFIED by the gate before any commutativity
// answer — never assumed. Raw kernels are TOTAL with documented
// degenerate returns; the typed refusals E-CAT-001..007 live in the
// `category` wrapper module. Determinism class: fixed-order law
// passes, first-failure refusal, index-fold path evaluation;
// identical inputs are bit-identical.

/// Upper bound on morphisms for which associativity is certified by
/// the exhaustive triple check (64³ table probes). Larger carriers
/// refuse `E-CAT-007` — commutativity is never answered over an
/// unverified table.
pub const CATEGORY_ASSOCIATIVITY_BOUND: usize = 64;

/// Category-law gate status for a dense composition-table carrier.
/// `Valid` = the carrier is a category; the other variants name the
/// FIRST violated law in the documented pass order.
pub enum CategoryStatus {
    Valid,
    /// `E-CAT-001` — a non-finite entry anywhere in the carrier.
    NonFinite,
    /// `E-CAT-002` — shape: dimension mismatch, malformed face record,
    /// or a path that does not run its face's declared start→end.
    BadShape,
    /// `E-CAT-003` — an out-of-range or non-integral index.
    BadIndex,
    /// `E-CAT-004` — composition law: an aligned pair without an
    /// entry, a defined entry on a misaligned pair, or a dangling
    /// path segment.
    EntryLaw,
    /// `E-CAT-005` — identity law: an appearing object with no
    /// identity morphism.
    IdentityLaw,
    /// `E-CAT-006` — associativity law (or definedness disagreement).
    AssociativityLaw,
    /// `E-CAT-007` — more morphisms than the certifiable bound.
    TooLarge,
}

/// Parse one f64 field as an index in `0..bound`. Call only AFTER the
/// finiteness pass (NaN/non-finite refuse `E-CAT-001` before this).
fn category_index(value: f64, bound: usize) -> Option<usize> {
    if value < 0.0 || value.fract() != 0.0 {
        return None;
    }
    let index = value as usize;
    (index < bound).then_some(index)
}

/// The certified carrier: parsed object indices plus the composition
/// table as `i64` (-1 = undefined).
struct CertifiedCategory {
    dom: Vec<usize>,
    cod: Vec<usize>,
    table: Vec<Vec<i64>>,
    objects: usize,
}

/// The category-law gate (documented pass order: shape → finiteness →
/// indices → size bound → composition law → identity law →
/// associativity). Returns the parsed carrier on success.
fn category_certify(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
) -> Result<CertifiedCategory, CategoryStatus> {
    let k = dom.len();
    if k != cod.len() || comp.len() != k || comp.iter().any(|row| row.len() != k) {
        return Err(CategoryStatus::BadShape);
    }
    if dom
        .iter()
        .chain(cod.iter())
        .chain(comp.iter().flatten())
        .any(|value| !value.is_finite())
    {
        return Err(CategoryStatus::NonFinite);
    }
    let mut objects = 0usize;
    for value in dom.iter().chain(cod.iter()) {
        let index = category_index(*value, usize::MAX).ok_or(CategoryStatus::BadIndex)?;
        objects = objects.max(index + 1);
    }
    let mut table = vec![vec![-1i64; k]; k];
    for (i, row) in comp.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            if *value == -1.0 {
                continue;
            }
            let entry = category_index(*value, k).ok_or(CategoryStatus::BadIndex)?;
            table[i][j] = entry as i64;
        }
    }
    // Size gate before the quadratic/cubic law passes: a carrier too
    // large to certify is refused outright, never half-checked.
    if k > CATEGORY_ASSOCIATIVITY_BOUND {
        return Err(CategoryStatus::TooLarge);
    }
    let dom_i: Vec<usize> = dom.iter().map(|v| *v as usize).collect();
    let cod_i: Vec<usize> = cod.iter().map(|v| *v as usize).collect();
    // Composition law: defined exactly on aligned pairs, and the
    // composite carries the pair's dom/cod.
    for i in 0..k {
        for j in 0..k {
            let aligned = cod_i[j] == dom_i[i];
            let entry = table[i][j];
            if aligned {
                if entry < 0 {
                    return Err(CategoryStatus::EntryLaw);
                }
                let composite = entry as usize;
                if dom_i[composite] != dom_i[j] || cod_i[composite] != cod_i[i] {
                    return Err(CategoryStatus::EntryLaw);
                }
            } else if entry >= 0 {
                return Err(CategoryStatus::EntryLaw);
            }
        }
    }
    // Identity law: every APPEARING object has a morphism that acts as
    // its identity on both sides.
    let mut appears = vec![false; objects];
    for object in dom_i.iter().chain(cod_i.iter()) {
        appears[*object] = true;
    }
    for object in 0..objects {
        if !appears[object] {
            continue;
        }
        let mut found = false;
        'candidate: for m in 0..k {
            if dom_i[m] != object || cod_i[m] != object || table[m][m] != m as i64 {
                continue;
            }
            for x in 0..k {
                if cod_i[x] == object && table[m][x] != x as i64 {
                    continue 'candidate;
                }
                if dom_i[x] == object && table[x][m] != x as i64 {
                    continue 'candidate;
                }
            }
            found = true;
            break;
        }
        if !found {
            return Err(CategoryStatus::IdentityLaw);
        }
    }
    // Associativity: exhaustive triple check (definedness already
    // agrees with alignment under the composition law, so a
    // one-side-defined disagreement is also a violation).
    for a in 0..k {
        for b in 0..k {
            for c in 0..k {
                let ab = table[a][b];
                let bc = table[b][c];
                let left = if ab >= 0 { table[ab as usize][c] } else { -1 };
                let right = if bc >= 0 { table[a][bc as usize] } else { -1 };
                if left != right {
                    return Err(CategoryStatus::AssociativityLaw);
                }
            }
        }
    }
    Ok(CertifiedCategory {
        dom: dom_i,
        cod: cod_i,
        table,
        objects,
    })
}

/// The law-gate status view (the wrapper's typed surface).
pub fn category_check_status(dom: &[f64], cod: &[f64], comp: &[Vec<f64>]) -> CategoryStatus {
    match category_certify(dom, cod, comp) {
        Ok(_) => CategoryStatus::Valid,
        Err(status) => status,
    }
}

/// Diagram commutativity over face path-pairs (status view): the
/// carrier must certify first, then each face's two paths fold through
/// the table; a face is commutative iff both composites are the SAME
/// morphism index.
pub fn category_diagram_commutative_status(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
    faces: &[f64],
) -> Result<Vec<bool>, CategoryStatus> {
    let category = category_certify(dom, cod, comp)?;
    let k = dom.len();
    if faces.iter().any(|value| !value.is_finite()) {
        return Err(CategoryStatus::NonFinite);
    }
    let mut mask = Vec::new();
    let mut cursor = 0usize;
    while cursor < faces.len() {
        if cursor + 4 > faces.len() {
            return Err(CategoryStatus::BadShape);
        }
        let start =
            category_index(faces[cursor], category.objects).ok_or(CategoryStatus::BadIndex)?;
        let end =
            category_index(faces[cursor + 1], category.objects).ok_or(CategoryStatus::BadIndex)?;
        let len_l_raw = faces[cursor + 2];
        let len_r_raw = faces[cursor + 3];
        if len_l_raw.fract() != 0.0 || len_r_raw.fract() != 0.0 {
            return Err(CategoryStatus::BadIndex);
        }
        // Both paths carry at least one morphism: identities are
        // explicit carrier morphisms, never an implicit empty path.
        if len_l_raw < 1.0 || len_r_raw < 1.0 {
            return Err(CategoryStatus::BadShape);
        }
        let len_l = len_l_raw as usize;
        let len_r = len_r_raw as usize;
        if cursor + 4 + len_l + len_r > faces.len() {
            return Err(CategoryStatus::BadShape);
        }
        let left = &faces[cursor + 4..cursor + 4 + len_l];
        let right = &faces[cursor + 4 + len_l..cursor + 4 + len_l + len_r];
        let composite = |path: &[f64]| -> Result<usize, CategoryStatus> {
            let mut current = category_index(path[0], k).ok_or(CategoryStatus::BadIndex)?;
            for value in &path[1..] {
                let next = category_index(*value, k).ok_or(CategoryStatus::BadIndex)?;
                let entry = category.table[current][next];
                if entry < 0 {
                    return Err(CategoryStatus::EntryLaw);
                }
                current = entry as usize;
            }
            Ok(current)
        };
        let left_composite = composite(left)?;
        let right_composite = composite(right)?;
        // Path geometry: both paths must run the face's start→end.
        if category.dom[left_composite] != start
            || category.cod[left_composite] != end
            || category.dom[right_composite] != start
            || category.cod[right_composite] != end
        {
            return Err(CategoryStatus::BadShape);
        }
        mask.push(left_composite == right_composite);
        cursor += 4 + len_l + len_r;
    }
    Ok(mask)
}

/// Total commutativity view: the per-face mask (1.0/0.0 in face
/// order), or EMPTY on any refusal (the lp/graph empty-vector
/// convention; the reference interpreter surfaces the typed codes).
pub fn category_diagram_commutative(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
    faces: &[f64],
) -> Vec<f64> {
    match category_diagram_commutative_status(dom, cod, comp, faces) {
        Ok(mask) => mask
            .iter()
            .map(|face| if *face { 1.0 } else { 0.0 })
            .collect(),
        Err(_) => Vec::new(),
    }
}
