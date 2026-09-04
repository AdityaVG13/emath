// ── Thin control surface (transfer / state-space / stability) ──────
//
// ASCENDING polynomial carriers (the B28 representation law); the
// state-space carrier is a square row-major A with implicit D = 0.
// Raw kernels are TOTAL with documented degenerate returns (NaN for
// scalar refusals; the typed refusals E-CONTROL-001..005 live in the
// `control` wrapper module and the reference interpreter surfaces
// them). Controller DESIGN (pole placement, LQR) and the Itô/
// Stratonovich SDE surface (B37, world-dependent) are NOT claimed
// here. Determinism class: fixed-order Horner, fixed-order
// Faddeev–LeVerrier recurrence, partial-pivot Gauss elimination with
// FIRST-INDEX tie-breaking; identical inputs are bit-identical.

/// Routh–Hurwitz table status for a real polynomial (ASCENDING
/// coefficients). `Stable` = every root strictly in the open left half
/// plane; `Unstable` = at least one first-column sign change (a
/// right-half-plane root exists); `Degenerate` = a zero first-column
/// entry (marginal poles or the ε-ambiguous case — the
/// auxiliary-polynomial refinement is a named deferral);
/// `ZeroPolynomial` = no pole set; `NonFinite` = a non-finite
/// coefficient.
pub enum RouthStatus {
    Stable,
    Unstable,
    Degenerate,
    ZeroPolynomial,
    NonFinite,
}

/// Routh–Hurwitz first-column sign test over an ASCENDING carrier.
/// Leading-coefficient sign is normalized away (stability is invariant
/// under an overall sign flip); a constant nonzero polynomial has an
/// EMPTY pole set and is vacuously stable.
pub fn control_routh_status(den: &[f64]) -> RouthStatus {
    if den.iter().any(|c| !c.is_finite()) {
        return RouthStatus::NonFinite;
    }
    // Descending order for the table; strip leading zeros (trailing
    // zeros of the ASCENDING carrier) to find the true degree.
    let mut desc: Vec<f64> = den.iter().rev().copied().collect();
    while desc.last().is_some_and(|c| *c == 0.0) {
        desc.pop();
    }
    if desc.is_empty() {
        return RouthStatus::ZeroPolynomial;
    }
    if desc.len() == 1 {
        return RouthStatus::Stable;
    }
    if desc[0] < 0.0 {
        for c in desc.iter_mut() {
            *c = -*c;
        }
    }
    let mut row0: Vec<f64> = desc.iter().step_by(2).copied().collect();
    let mut row1: Vec<f64> = desc.iter().skip(1).step_by(2).copied().collect();
    let mut prev_positive = row0[0] > 0.0;
    loop {
        if row1.is_empty() {
            return RouthStatus::Stable;
        }
        // A zero first-column entry is the degenerate (marginal or
        // ε-ambiguous) case — refused typed upstream, never guessed.
        if row1[0] == 0.0 {
            return RouthStatus::Degenerate;
        }
        if (row1[0] > 0.0) != prev_positive {
            return RouthStatus::Unstable;
        }
        prev_positive = row1[0] > 0.0;
        // The new row has row0.len() - 1 entries; missing operands read
        // as 0.0 (the classical padded-table convention).
        let at = |row: &[f64], k: usize| if k < row.len() { row[k] } else { 0.0 };
        let width = row0.len();
        let mut next = Vec::with_capacity(width.saturating_sub(1));
        for k in 0..width.saturating_sub(1) {
            next.push((row1[0] * at(&row0, k + 1) - row0[0] * at(&row1, k + 1)) / row1[0]);
        }
        row0 = row1;
        row1 = next;
    }
}

/// Total bool view of the Routh status (generated-code convention:
/// degenerate/zero/non-finite all read false; the typed wrapper
/// refuses them upstream).
pub fn control_poles_stable(den: &[f64]) -> bool {
    matches!(control_routh_status(den), RouthStatus::Stable)
}

/// Characteristic polynomial of `A` (monic, ASCENDING coefficients)
/// via the Faddeev–LeVerrier recurrence: `M₁ = I`,
/// `a_{n−k} = −tr(A·M_k)/k`, `M_{k+1} = A·M_k + a_{n−k}·I`. Pure
/// matrix arithmetic — no eigenvalue is claimed.
pub fn control_char_poly(a: &[Vec<f64>]) -> Vec<f64> {
    let n = a.len();
    let mut m_prev = vec![0.0; n * n];
    let mut descending: Vec<f64> = vec![1.0];
    for k in 1..=n {
        let previous = descending[descending.len() - 1];
        let mut m_k = vec![0.0; n * n];
        for r in 0..n {
            for c in 0..n {
                let mut acc = 0.0;
                for j in 0..n {
                    acc += a[r][j] * m_prev[j * n + c];
                }
                m_k[r * n + c] = acc + if r == c { previous } else { 0.0 };
            }
        }
        let mut trace = 0.0;
        for d in 0..n {
            for j in 0..n {
                trace += a[d][j] * m_k[j * n + d];
            }
        }
        descending.push(-trace / k as f64);
        m_prev = m_k;
    }
    descending.reverse();
    descending
}

/// Transfer-function evaluation `num(x)/den(x)` over ASCENDING
/// carriers (Horner both sides). NaN = refused (zero denominator,
/// pole hit, or non-finite carrier) — the typed wrapper names it.
pub fn control_transfer_eval(num: &[f64], den: &[f64], x: f64) -> f64 {
    let denominator = poly_eval(den, x);
    if !denominator.is_finite()
        || denominator == 0.0
        || !num
            .iter()
            .chain(den.iter())
            .chain(std::iter::once(&x))
            .all(|v| v.is_finite())
    {
        return f64::NAN;
    }
    poly_eval(num, x) / denominator
}

/// State-space DC gain `c·(−A)⁻¹·b` (implicit D = 0). Refuses (NaN)
/// unless the Faddeev–LeVerrier characteristic polynomial is strictly
/// stable (Routh–Hurwitz): a non-asymptotically-stable carrier has no
/// DC gain. The solve is pivoted Gauss elimination with first-index
/// tie-breaking.
pub fn control_state_space_dc_gain(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> f64 {
    let n = a.len();
    if n == 0
        || a.iter().any(|row| row.len() != n)
        || b.len() != n
        || c.len() != n
        || a.iter()
            .flatten()
            .chain(b.iter())
            .chain(c.iter())
            .any(|v| !v.is_finite())
    {
        return f64::NAN;
    }
    if !matches!(
        control_routh_status(&control_char_poly(a)),
        RouthStatus::Stable
    ) {
        return f64::NAN;
    }
    // Solve (−A)x = b by pivoted Gauss elimination (ties → first row:
    // the strictly-greater comparison never swaps an equal pivot).
    let mut aug: Vec<Vec<f64>> = (0..n)
        .map(|r| {
            let mut row: Vec<f64> = (0..n).map(|cc| -a[r][cc]).collect();
            row.push(b[r]);
            row
        })
        .collect();
    for col in 0..n {
        let mut pivot = col;
        for r in col + 1..n {
            if aug[r][col].abs() > aug[pivot][col].abs() {
                pivot = r;
            }
        }
        if aug[pivot][col] == 0.0 {
            // Unreachable for a strictly stable carrier (det ≠ 0);
            // refused rather than invented.
            return f64::NAN;
        }
        aug.swap(col, pivot);
        for r in col + 1..n {
            let factor = aug[r][col] / aug[col][col];
            if factor != 0.0 {
                for cc in col..=n {
                    aug[r][cc] -= factor * aug[col][cc];
                }
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut acc = aug[r][n];
        for cc in r + 1..n {
            acc -= aug[r][cc] * x[cc];
        }
        x[r] = acc / aug[r][r];
    }
    let mut gain = 0.0;
    for (ci, xi) in c.iter().zip(x.iter()) {
        gain += ci * xi;
    }
    gain
}

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

/// Total check view (generated-code convention): TRUE only when the
/// carrier certifies; every law failure reads false (the reference
/// interpreter surfaces the typed E-CAT codes).
pub fn category_check(dom: &[f64], cod: &[f64], comp: &[Vec<f64>]) -> bool {
    matches!(category_check_status(dom, cod, comp), CategoryStatus::Valid)
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
