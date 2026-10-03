/-
Written by `emath export lean` when a project certifies laws; the next
such export overwrites it, and an export without laws leaves it unbuilt.

The Lean side of emath's `search.certify`: two sequences that satisfy
linear recurrences of orders `a` and `b` and agree at `a + b` points
agree everywhere (`eq_of_order`, `eq_of_order_from`). `emath_order`
bounds a sequence's order by a subset of the shape rules certify uses,
each rule a theorem here: constants, `n`, powers, sums, scaling,
products, eventual branches, running sums (`fold` with `acc + h(t)`)
and linear readouts of a linear loop over lists (Cayley–Hamilton).
`emath_check` checks the points and, when they disagree, names one.

`certify_all` laws hold for every parameter value the hypotheses allow.
At each `n` both sides are rational functions of the parameters, so
`params_eq` reduces the law to one evaluation: `Bnd` bounds each side's
numerator and denominator (degrees and coefficient sizes) and
`kron_eq` picks a single rational point, large enough that a nonzero
polynomial cannot vanish there (`eq_zero_of_kpt`), where agreement at
the `a + b` start points implies agreement as rational functions. The
same point serves every `n` (`kshared`), so `emath_kcheck` asks the
kernel for one evaluation, and when the sides disagree it reports
whether they differ at an allowed parameter point.

Imports are the few Mathlib modules these need, so a check loads
little of Mathlib.
-/
import Mathlib.LinearAlgebra.Charpoly.Basic
import Mathlib.Algebra.Polynomial.AlgebraMap
import Mathlib.Tactic.FinCases
import Mathlib.Tactic.Ring
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Linarith
import Mathlib.Algebra.MvPolynomial.Equiv
import Mathlib.Algebra.MvPolynomial.CommRing
import Mathlib.Algebra.Polynomial.Inductions
import Mathlib.Tactic.FieldSimp
import Mathlib.Tactic.Positivity
import Mathlib.Tactic.LinearCombination
import Mathlib.Tactic.Tauto
import Emath.Prelude

namespace Emath.Law
open Polynomial

/-- The shift on sequences: `(shift u) n = u (n + 1)`. -/
def shift : Module.End ℚ (ℕ → ℚ) where
  toFun u n := u (n + 1)
  map_add' _ _ := rfl
  map_smul' _ _ := rfl

theorem shift_pow_apply (i : ℕ) (u : ℕ → ℚ) (n : ℕ) : (shift ^ i) u n = u (n + i) := by
  induction i generalizing u with
  | zero => rfl
  | succ i ih =>
    rw [pow_succ, Module.End.mul_apply, ih]
    rfl

theorem aeval_apply (p : ℚ[X]) (u : ℕ → ℚ) (n : ℕ) :
    aeval shift p u n = ∑ i ∈ Finset.range (p.natDegree + 1), p.coeff i * u (n + i) := by
  rw [aeval_eq_sum_range]
  simp [LinearMap.sum_apply, Finset.sum_apply, shift_pow_apply]

/-- `p`, read as a recurrence, holds for `u` at every `n`. -/
def Ann (p : ℚ[X]) (u : ℕ → ℚ) : Prop := aeval shift p u = 0

/-- `u` satisfies a linear recurrence with constant coefficients of
order at most `k`: `u (n + d) = c₁ u (n + d - 1) + ... + c_d u n` with
`d ≤ k`, for every `n`. -/
def HasOrder (k : ℕ) (u : ℕ → ℚ) : Prop := ∃ p : ℚ[X], p.Monic ∧ p.natDegree ≤ k ∧ Ann p u

theorem Ann.mul_left {p : ℚ[X]} {u : ℕ → ℚ} (h : Ann p u) (q : ℚ[X]) : Ann (q * p) u := by
  unfold Ann at *
  rw [map_mul, Module.End.mul_apply, h, map_zero]

theorem Ann.mul_right {p : ℚ[X]} {u : ℕ → ℚ} (h : Ann p u) (q : ℚ[X]) : Ann (p * q) u := by
  rw [mul_comm]; exact h.mul_left q

/-- A sequence of order `k` that vanishes at `0, ..., k - 1` vanishes. -/
theorem HasOrder.eq_zero {k : ℕ} {w : ℕ → ℚ} (hw : HasOrder k w) (h0 : ∀ i < k, w i = 0) :
    ∀ n, w n = 0 := by
  obtain ⟨p, hp, hpk, hpa⟩ := hw
  intro n
  induction n using Nat.strong_induction_on with
  | _ n ih =>
    by_cases hn : n < k
    · exact h0 n hn
    · set d := p.natDegree
      have key := congrFun hpa (n - d)
      rw [aeval_apply, Finset.sum_range_succ] at key
      have hlead : p.coeff d = 1 := hp
      have hrest : ∑ i ∈ Finset.range d, p.coeff i * w (n - d + i) = 0 := by
        apply Finset.sum_eq_zero
        intro i hi
        rw [Finset.mem_range] at hi
        rw [ih (n - d + i) (by omega), mul_zero]
      rw [hrest, hlead, zero_add, one_mul, Nat.sub_add_cancel (by omega)] at key
      simpa using key

theorem HasOrder.mono {a b : ℕ} {u : ℕ → ℚ} (h : HasOrder a u) (hab : a ≤ b) : HasOrder b u := by
  obtain ⟨p, hp, hpd, hpa⟩ := h
  exact ⟨p, hp, hpd.trans hab, hpa⟩

theorem HasOrder.add {a b : ℕ} {u v : ℕ → ℚ} (hu : HasOrder a u) (hv : HasOrder b v) :
    HasOrder (a + b) (fun n => u n + v n) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  obtain ⟨q, hq, hqd, hqa⟩ := hv
  refine ⟨p * q, hp.mul hq, by rw [hp.natDegree_mul hq]; omega, ?_⟩
  show aeval shift (p * q) (u + v) = 0
  rw [map_add, hpa.mul_right q, hqa.mul_left p, add_zero]

theorem HasOrder.sub {a b : ℕ} {u v : ℕ → ℚ} (hu : HasOrder a u) (hv : HasOrder b v) :
    HasOrder (a + b) (fun n => u n - v n) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  obtain ⟨q, hq, hqd, hqa⟩ := hv
  refine ⟨p * q, hp.mul hq, by rw [hp.natDegree_mul hq]; omega, ?_⟩
  show aeval shift (p * q) (u - v) = 0
  rw [map_sub, hpa.mul_right q, hqa.mul_left p, sub_zero]

theorem HasOrder.smul {k : ℕ} {u : ℕ → ℚ} (c : ℚ) (hu : HasOrder k u) :
    HasOrder k (c • u) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  refine ⟨p, hp, hpd, ?_⟩
  unfold Ann at *
  rw [map_smul, hpa, smul_zero]

theorem HasOrder.const_mul {k : ℕ} {u : ℕ → ℚ} (c : ℚ) (hu : HasOrder k u) :
    HasOrder k (fun n => c * u n) := hu.smul c

theorem HasOrder.mul_const {k : ℕ} {u : ℕ → ℚ} (c : ℚ) (hu : HasOrder k u) :
    HasOrder k (fun n => u n * c) := by
  convert hu.smul c using 1
  funext n; simp [mul_comm]

theorem HasOrder.div_const {k : ℕ} {u : ℕ → ℚ} (c : ℚ) (hu : HasOrder k u) :
    HasOrder k (fun n => u n / c) := by
  convert hu.smul c⁻¹ using 1
  funext n; simp [div_eq_mul_inv, mul_comm]

theorem HasOrder.neg {k : ℕ} {u : ℕ → ℚ} (hu : HasOrder k u) : HasOrder k (fun n => -u n) := by
  convert hu.smul (-1) using 1
  funext n; simp

/-- An order-one recurrence, stated as a step. -/
theorem HasOrder.of_step {u : ℕ → ℚ} (c : ℚ) (h : ∀ n, u (n + 1) = c * u n) : HasOrder 1 u := by
  refine ⟨X - C c, monic_X_sub_C c, by simp, ?_⟩
  funext n
  simp [shift, h]

theorem HasOrder.const (c : ℚ) : HasOrder 1 (fun _ => c) :=
  HasOrder.of_step 1 (fun n => by simp)

theorem HasOrder.geom (c : ℚ) : HasOrder 1 (fun n => c ^ n) :=
  HasOrder.of_step c (fun n => by ring)

theorem HasOrder.rat_pow (c : ℚ) : HasOrder 1 (fun n : ℕ => Emath.rat_pow c (n : ℤ)) := by
  have : (fun n : ℕ => Emath.rat_pow c (n : ℤ)) = (fun n => c ^ n) := by
    funext n; simp [Emath.rat_pow]
  rw [this]; exact HasOrder.geom c

theorem rat_pow_eq_zpow {c : ℚ} (hc : c ≠ 0) (e : ℤ) : Emath.rat_pow c e = c ^ e := by
  unfold Emath.rat_pow
  split_ifs with he
  · rw [← zpow_natCast, inv_zpow', Int.toNat_of_nonneg (by omega), neg_neg]
  · rw [← zpow_natCast, Int.toNat_of_nonneg (by omega)]

/-- `rat_pow(c, n + j)`: geometric when `c ≠ 0` or the exponent is never
negative (`rat_pow(0, -1)` is `0`, not `0⁻¹ · 0⁰`). -/
theorem HasOrder.rat_pow_add (c : ℚ) (j : ℤ) (h : c ≠ 0 ∨ 0 ≤ j) :
    HasOrder 1 (fun n : ℕ => Emath.rat_pow c ((n : ℤ) + j)) := by
  refine HasOrder.of_step c (fun n => ?_)
  rcases h with hc | hj
  · rw [rat_pow_eq_zpow hc, rat_pow_eq_zpow hc]
    push_cast
    rw [add_right_comm, zpow_add_one₀ hc, mul_comm]
  · have h1 : ¬ ((n : ℤ) + j < 0) := by omega
    have h2 : ¬ (((n + 1 : ℕ) : ℤ) + j < 0) := by omega
    simp only [Emath.rat_pow, h1, h2, ↓reduceIte]
    rw [show (((n + 1 : ℕ) : ℤ) + j).toNat = ((n : ℤ) + j).toNat + 1 by omega, pow_succ, mul_comm]

theorem HasOrder.rat_pow_sub (c : ℚ) (j : ℤ) (h : c ≠ 0 ∨ j ≤ 0) :
    HasOrder 1 (fun n : ℕ => Emath.rat_pow c ((n : ℤ) - j)) := by
  have := HasOrder.rat_pow_add c (-j) (h.imp id fun hj => by omega)
  simpa [sub_eq_add_neg] using this

theorem shift_mul (x y : ℕ → ℚ) : shift (x * y) = shift x * shift y := rfl

/-- The shifts `shift^i u`, `i < natDegree p`, span a shift-invariant
space when `p` annihilates `u`. -/
theorem shift_mem_orbit {p : ℚ[X]} {u : ℕ → ℚ} (hp : p.Monic) (hpa : Ann p u) :
    ∀ x ∈ Submodule.span ℚ (Set.range fun i : Fin p.natDegree => (shift ^ (i : ℕ)) u),
      shift x ∈ Submodule.span ℚ (Set.range fun i : Fin p.natDegree => (shift ^ (i : ℕ)) u) := by
  set d := p.natDegree
  set S := Submodule.span ℚ (Set.range fun i : Fin d => (shift ^ (i : ℕ)) u)
  have top : (shift ^ d) u ∈ S := by
    have key := hpa
    unfold Ann at key
    rw [aeval_eq_sum_range, Finset.sum_range_succ, hp.coeff_natDegree, one_smul] at key
    simp only [LinearMap.add_apply, LinearMap.sum_apply, LinearMap.smul_apply] at key
    rw [eq_neg_of_add_eq_zero_right key]
    refine S.neg_mem (S.sum_mem fun i hi => S.smul_mem _ (Submodule.subset_span ?_))
    exact ⟨⟨i, Finset.mem_range.mp hi⟩, rfl⟩
  intro x hx
  induction hx using Submodule.span_induction with
  | mem x hx =>
    obtain ⟨i, rfl⟩ := hx
    show (shift * shift ^ (i : ℕ)) u ∈ S
    rw [← pow_succ']
    by_cases h : (i : ℕ) + 1 < d
    · exact Submodule.subset_span ⟨⟨i + 1, h⟩, rfl⟩
    · rw [show (i : ℕ) + 1 = d by omega]; exact top
  | zero => simp
  | add x y _ _ hx hy => rw [map_add]; exact S.add_mem hx hy
  | smul a x _ hx => rw [map_smul]; exact S.smul_mem a hx

theorem Ann.eq_zero {r : ℚ[X]} {w : ℕ → ℚ} (hr : r.Monic) (h0 : r.natDegree = 0) (hw : Ann r w) :
    w = 0 := by
  have : r = 1 := eq_one_of_monic_natDegree_zero hr h0
  subst this; simpa [Ann] using hw

/-- The termwise product of sequences of orders `a` and `b` has order at
most `a b`: the products `shift^i u * shift^j v` span a shift-invariant
space of dimension at most `a b`, and the shift's characteristic
polynomial there (Cayley–Hamilton) annihilates `u v`. -/
theorem HasOrder.mul {a b : ℕ} {u v : ℕ → ℚ} (hu : HasOrder a u) (hv : HasOrder b v) :
    HasOrder (a * b) (fun n => u n * v n) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  obtain ⟨q, hq, hqd, hqa⟩ := hv
  have zero : HasOrder (a * b) (fun _ => (0 : ℚ)) := ⟨1, monic_one, by simp, by simp [Ann]; rfl⟩
  by_cases hd : p.natDegree = 0
  · have := hpa.eq_zero hp hd; subst this; simpa using zero
  by_cases he : q.natDegree = 0
  · have := hqa.eq_zero hq he; subst this; simpa using zero
  set d := p.natDegree
  set e := q.natDegree
  let f : Fin d × Fin e → (ℕ → ℚ) := fun ij => (shift ^ (ij.1 : ℕ)) u * (shift ^ (ij.2 : ℕ)) v
  let U := Submodule.span ℚ (Set.range fun i : Fin d => (shift ^ (i : ℕ)) u)
  let W := Submodule.span ℚ (Set.range fun j : Fin e => (shift ^ (j : ℕ)) v)
  let V := Submodule.span ℚ (Set.range f)
  have hUW : U * W ≤ V := by
    rw [Submodule.span_mul_span]
    apply Submodule.span_mono
    rintro _ ⟨_, ⟨i, rfl⟩, _, ⟨j, rfl⟩, rfl⟩
    exact ⟨(i, j), rfl⟩
  have hVUW : V ≤ U * W := Submodule.span_le.mpr (by
    rintro _ ⟨⟨i, j⟩, rfl⟩
    exact Submodule.mul_mem_mul (Submodule.subset_span ⟨i, rfl⟩) (Submodule.subset_span ⟨j, rfl⟩))
  have hinv : ∀ x ∈ V, shift x ∈ V := by
    have : U * W ≤ V.comap shift := Submodule.mul_le.mpr fun y hy z hz => by
      show shift (y * z) ∈ V
      rw [shift_mul]
      exact hUW (Submodule.mul_mem_mul (shift_mem_orbit hp hpa y hy) (shift_mem_orbit hq hqa z hz))
    exact fun x hx => this (hVUW hx)
  have huv : (fun n => u n * v n) ∈ V := by
    have := Submodule.subset_span (R := ℚ) (s := Set.range f) ⟨(⟨0, by omega⟩, ⟨0, by omega⟩), rfl⟩
    show u * v ∈ V
    simpa [f] using this
  have : FiniteDimensional ℚ V := FiniteDimensional.span_of_finite ℚ (Set.finite_range f)
  let T := shift.restrict hinv
  refine ⟨T.charpoly, T.charpoly_monic, ?_, ?_⟩
  · rw [LinearMap.charpoly_natDegree]
    calc Module.finrank ℚ V ≤ Fintype.card (Fin d × Fin e) := finrank_range_le_card f
      _ = d * e := by simp
      _ ≤ a * b := Nat.mul_le_mul hpd hqd
  · have key : ∀ (r : ℚ[X]) (x : V), aeval shift r (x : ℕ → ℚ) = ((aeval T r) x : ℕ → ℚ) := by
      intro r x
      rw [aeval_eq_sum_range, aeval_eq_sum_range]
      simp only [LinearMap.sum_apply, LinearMap.smul_apply, Submodule.coe_sum, Submodule.coe_smul]
      refine Finset.sum_congr rfl fun i _ => ?_
      rw [Module.End.pow_restrict, LinearMap.restrict_apply]
    show aeval shift T.charpoly (fun n => u n * v n) = 0
    rw [key T.charpoly ⟨_, huv⟩, LinearMap.aeval_self_charpoly]
    rfl

theorem HasOrder.cast_nat : HasOrder 2 (fun n : ℕ => (((n : ℤ)) : ℚ)) := by
  refine ⟨(X - C 1) * (X - C 1), (monic_X_sub_C 1).mul (monic_X_sub_C 1), ?_, ?_⟩
  · rw [(monic_X_sub_C 1).natDegree_mul (monic_X_sub_C 1), natDegree_X_sub_C]
  · funext n
    simp [shift]

/-- Partial sums, from any start, gain one order. -/
theorem HasOrder.partial_sum {k : ℕ} {h : ℕ → ℚ} (c : ℚ) (hh : HasOrder k h) :
    HasOrder (k + 1) (fun n => c + ∑ t ∈ Finset.range n, h t) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hh
  refine ⟨p * (X - C 1), hp.mul (monic_X_sub_C 1), ?_, ?_⟩
  · rw [hp.natDegree_mul (monic_X_sub_C 1), natDegree_X_sub_C]; omega
  · unfold Ann at *
    rw [map_mul, Module.End.mul_apply]
    have : aeval shift (X - C 1 : ℚ[X]) (fun n => c + ∑ t ∈ Finset.range n, h t) = h := by
      funext n; simp [shift, Finset.sum_range_succ]
    rw [this, hpa]

/-- `φ (Tᵗ v)` for a linear map `T` of a `k`-dimensional space has order
`k` (Cayley–Hamilton). -/
theorem HasOrder.linear_iterate {k : ℕ} (T : Module.End ℚ (Fin k → ℚ)) (v : Fin k → ℚ)
    (φ : (Fin k → ℚ) →ₗ[ℚ] ℚ) : HasOrder k (fun t => φ ((T ^ t) v)) := by
  refine ⟨LinearMap.charpoly T, LinearMap.charpoly_monic T, ?_, ?_⟩
  · rw [LinearMap.charpoly_natDegree]; simp
  · funext t
    rw [aeval_apply, Pi.zero_apply]
    have hct := congrArg (fun S : Module.End ℚ (Fin k → ℚ) => φ ((T ^ t) (S v)))
      (LinearMap.aeval_self_charpoly T)
    simp only [aeval_eq_sum_range, LinearMap.sum_apply, LinearMap.smul_apply, map_sum,
      map_smul, smul_eq_mul, LinearMap.zero_apply, map_zero] at hct
    rw [← hct]
    refine Finset.sum_congr rfl (fun i _ => ?_)
    rw [pow_add, Module.End.mul_apply]


/-! ### The prelude's loops as sequences -/

theorem foldl_const {α β : Type} (f : β → β) (s : β) (xs : List α) :
    xs.foldl (fun p _ => f p) s = f^[xs.length] s := by
  induction xs generalizing s with
  | nil => rfl
  | cons x xs ih => simp [List.foldl, ih, Function.iterate_succ_apply]

theorem fold_iterate {β : Type} (f : β → β) (s : β) (lo m : ℤ) :
    Emath.fold (fun p (_ : ℤ) => f p) s (Emath.range lo m) = f^[m.toNat] s := by
  simp [Emath.fold, Emath.range, foldl_const]

theorem fold_acc_eq (f : ℚ → ℤ → ℚ) (hf : ∀ acc t, f acc t = acc + f 0 t) (init : ℚ) (lo : ℤ)
    (n : ℕ) : Emath.fold f init (Emath.range lo n) = init + ∑ t ∈ Finset.range n, f 0 (lo + t) := by
  induction n with
  | zero => simp [Emath.fold, Emath.range]
  | succ n ih =>
    simp only [Emath.fold, Emath.range, Int.toNat_natCast] at ih ⊢
    rw [List.range_succ, List.map_append, List.foldl_append, ih]
    simp only [List.map_cons, List.map_nil, List.foldl_cons, List.foldl_nil]
    rw [hf, Finset.sum_range_succ]
    ring

/-- `fold(function acc: function t: acc + h(t), init, range(lo, n))`. -/
theorem HasOrder.fold_acc {k : ℕ} (f : ℚ → ℤ → ℚ) (init : ℚ) (lo : ℤ)
    (hf : ∀ acc t, f acc t = acc + f 0 t) (hh : HasOrder k (fun t : ℕ => f 0 (lo + t))) :
    HasOrder (k + 1) (fun n : ℕ => Emath.fold f init (Emath.range lo n)) := by
  have := hh.partial_sum init
  convert this using 1
  funext n
  exact fold_acc_eq f hf init lo n

theorem index_ofFn {k : ℕ} (w : Fin k → ℚ) (j : ℤ) :
    Emath.index (List.ofFn w) j = if h : j.toNat < k then w ⟨j.toNat, h⟩ else 0 := by
  unfold Emath.index
  split_ifs with h
  · simp [List.getD_eq_getElem?_getD, h]
  · simp [List.getD_eq_getElem?_getD, h]; rfl

theorem ofFn_index {k : ℕ} (xs : List ℚ) (h : xs.length = k) :
    List.ofFn (fun i : Fin k => Emath.index xs (i : ℕ)) = xs := by
  apply List.ext_getElem
  · simp [h]
  · intro n h1 h2
    simp [Emath.index, List.getD_eq_getElem?_getD, h2]

/-- A linear readout `L` of a loop whose step `f` is linear on lists of the
start's length: `L(fold(function p: function i: f(p), s, range(lo, c + t)))`
has order `s.length` in `t` (Cayley–Hamilton for the step). -/
theorem HasOrder.list_readout (f : List ℚ → List ℚ) (s : List ℚ) (lo c : ℤ) (L : List ℚ → ℚ)
    (hL : ∀ (a : ℚ) (x y : Fin s.length → ℚ),
      L (List.ofFn (a • x + y)) = a * L (List.ofFn x) + L (List.ofFn y))
    (hc : 0 ≤ c)
    (hlen : ∀ x : Fin s.length → ℚ, (f (List.ofFn x)).length = s.length)
    (hlin : ∀ (a : ℚ) (x y : Fin s.length → ℚ) (i : Fin s.length),
      Emath.index (f (List.ofFn (a • x + y))) (i : ℕ) =
        a * Emath.index (f (List.ofFn x)) (i : ℕ) + Emath.index (f (List.ofFn y)) (i : ℕ)) :
    HasOrder s.length (fun t : ℕ => L (Emath.fold (fun p (_ : ℤ) => f p) s
      (Emath.range lo (c + t)))) := by
  have hf0 : ∀ i : Fin s.length, Emath.index (f (List.ofFn (0 : Fin s.length → ℚ))) (i : ℕ) = 0 := by
    intro i; have := hlin 1 0 0 i; simp at this; linarith
  have hL0 : L (List.ofFn (0 : Fin s.length → ℚ)) = 0 := by have := hL 1 0 0; simp at this; linarith
  let T : Module.End ℚ (Fin s.length → ℚ) :=
    { toFun := fun x i => Emath.index (f (List.ofFn x)) (i : ℕ)
      map_add' := fun x y => funext fun i => by simpa using hlin 1 x y i
      map_smul' := fun a x => funext fun i => by simpa [hf0] using hlin a x 0 i }
  let φ : (Fin s.length → ℚ) →ₗ[ℚ] ℚ :=
    { toFun := fun x => L (List.ofFn x)
      map_add' := fun x y => by simpa using hL 1 x y
      map_smul' := fun a x => by simpa [hL0] using hL a x 0 }
  let v : Fin s.length → ℚ := fun i => Emath.index s (i : ℕ)
  have hit : ∀ m, f^[m] s = List.ofFn ((T ^ m) v) := by
    intro m
    induction m with
    | zero => exact (ofFn_index s rfl).symm
    | succ m ih =>
      rw [Function.iterate_succ_apply', ih, pow_succ', Module.End.mul_apply]
      exact (ofFn_index _ (hlen _)).symm
  convert HasOrder.linear_iterate T ((T ^ c.toNat) v) φ using 1
  funext t
  have hct : (c + (t : ℤ)).toNat = t + c.toNat := by omega
  rw [fold_iterate, hct, hit, pow_add, Module.End.mul_apply]
  rfl

/-- `list_readout` over `range(lo, t)`. -/
theorem HasOrder.list_readout_zero (f : List ℚ → List ℚ) (s : List ℚ) (lo : ℤ) (L : List ℚ → ℚ)
    (hL : ∀ (a : ℚ) (x y : Fin s.length → ℚ),
      L (List.ofFn (a • x + y)) = a * L (List.ofFn x) + L (List.ofFn y))
    (hlen : ∀ x : Fin s.length → ℚ, (f (List.ofFn x)).length = s.length)
    (hlin : ∀ (a : ℚ) (x y : Fin s.length → ℚ) (i : Fin s.length),
      Emath.index (f (List.ofFn (a • x + y))) (i : ℕ) =
        a * Emath.index (f (List.ofFn x)) (i : ℕ) + Emath.index (f (List.ofFn y)) (i : ℕ)) :
    HasOrder s.length (fun t : ℕ => L (Emath.fold (fun p (_ : ℤ) => f p) s
      (Emath.range lo t))) := by
  simpa using HasOrder.list_readout f s lo 0 L hL le_rfl hlen hlin

open Lean Meta Elab Tactic in
/-- On `HasOrder k (fun t => body)`, finds a loop `Emath.fold _ _ _` over
lists in `body` that reads `t`, and applies `list_readout` with the rest of
`body` as the readout. Fails when `body` reads `t` outside the loop. -/
elab "emath_readout" : tactic => withMainContext do
  let goal ← instantiateMVars (← getMainTarget)
  let some (_, u) := goal.app2? ``HasOrder | throwError "emath_readout: not a HasOrder goal"
  let .lam n ty body bi := u | throwError "emath_readout: not a function"
  let readout ← withLocalDecl n bi ty fun t => do
    let body := body.instantiate1 t
    let isLoop (e : Expr) : Bool :=
      e.isAppOfArity ``Emath.fold 5 && (e.getArg! 1).isAppOf ``List && e.containsFVar t.fvarId!
    let some loop := body.find? isLoop | throwError "emath_readout: no loop"
    let abst ← kabstract body loop
    if (abst.instantiate1 (mkConst ``Unit.unit)).containsFVar t.fvarId! then
      throwError "emath_readout: reads t outside the loop"
    return Expr.lam `xs (← inferType loop) abst .default
  let L ← Term.exprToSyntax readout
  let saved ← saveState
  try
    evalTactic (← `(tactic| with_reducible apply HasOrder.list_readout (L := $L)))
    return
  catch _ => saved.restore
  try
    evalTactic (← `(tactic| with_reducible apply HasOrder.list_readout_zero (L := $L)))
    return
  catch _ => saved.restore
  throwError "emath_readout: the loop is not over range(lo, c + t)"

/-- Two sequences of orders `a` and `b` that agree at `0, ..., a + b - 1`
agree everywhere. -/
theorem eq_of_order {a b : ℕ} {u v : ℕ → ℚ} (hu : HasOrder a u) (hv : HasOrder b v)
    (h : ∀ i < a + b, u i = v i) (n : ℕ) : u n = v n := by
  have := (hu.sub hv).eq_zero (fun i hi => by simp [h i hi]) n
  exact sub_eq_zero.mp this

theorem aeval_comm_shift_pow (p : ℚ[X]) (m : ℕ) :
    aeval shift p * shift ^ m = shift ^ m * aeval shift p := by
  calc aeval shift p * shift ^ m = aeval shift (p * X ^ m) := by rw [map_mul, map_pow, aeval_X]
    _ = aeval shift (X ^ m * p) := by rw [mul_comm]
    _ = shift ^ m * aeval shift p := by rw [map_mul, map_pow, aeval_X]

/-- A sequence of order `k` keeps that order shifted by `s`. -/
theorem HasOrder.shift_by {k : ℕ} {u : ℕ → ℚ} (hu : HasOrder k u) (s : ℕ) :
    HasOrder k (fun n => u (s + n)) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  refine ⟨p, hp, hpd, ?_⟩
  have : (fun n => u (s + n)) = (shift ^ s) u := by
    funext n; rw [shift_pow_apply, Nat.add_comm]
  unfold Ann at *
  rw [this, ← Module.End.mul_apply, aeval_comm_shift_pow, Module.End.mul_apply, hpa, map_zero]

/-- A sequence equal to one of order `k` from `m` on has order `k + m`. -/
theorem HasOrder.eventually {k : ℕ} {u : ℕ → ℚ} (m : ℕ) (w : ℕ → ℚ) (hw : HasOrder k w)
    (h : ∀ n, m ≤ n → u n = w n) : HasOrder (k + m) u := by
  obtain ⟨p, hp, hpd, hpa⟩ := hw
  refine ⟨p * X ^ m, hp.mul (monic_X_pow m), ?_, ?_⟩
  · rw [hp.natDegree_mul (monic_X_pow m), natDegree_X_pow]; omega
  · have hs : (shift ^ m) u = (shift ^ m) w := by
      funext n; rw [shift_pow_apply, shift_pow_apply, h _ (by omega)]
    unfold Ann at *
    calc aeval shift (p * X ^ m) u = aeval shift p ((shift ^ m) u) := by
          rw [map_mul, map_pow, aeval_X, Module.End.mul_apply]
      _ = (aeval shift p * shift ^ m) w := by rw [hs]; rfl
      _ = 0 := by rw [aeval_comm_shift_pow, Module.End.mul_apply, hpa, map_zero]

open Lean Meta Elab Tactic in
/-- On `HasOrder k (fun n => if decide (x ~ y) = true then a else b)`
where one of `x`, `y` does not read `n` (call it `K`): a comparison of
`n` with `K` takes one side for every `n ≥ (K + 1).toNat`, so the order
is that side's plus `(K + 1).toNat` (`eventually`). Which side, and that
the comparison is with `n` at all, `omega` proves. -/
elab "emath_branch" : tactic => withMainContext do
  let goal ← instantiateMVars (← getMainTarget)
  let some (_, u) := goal.app2? ``HasOrder | throwError "emath_branch: not a HasOrder goal"
  let .lam n ty body bi := u | throwError "emath_branch: not a function"
  let (bound, sides) ← withLocalDecl n bi ty fun t => do
    let body := body.instantiate1 t
    unless body.isAppOfArity ``ite 5 do throwError "emath_branch: not a branch"
    let some (_, test, _) := (body.getArg! 1).eq? | throwError "emath_branch: not a Bool condition"
    unless test.isAppOfArity ``Decidable.decide 2 do throwError "emath_branch: not a comparison"
    let args := (test.getArg! 0).getAppArgs
    unless args.size ≥ 2 do throwError "emath_branch: not a comparison"
    let (x, y) := (args[args.size - 2]!, args[args.size - 1]!)
    let reads (e : Expr) := e.containsFVar t.fvarId!
    let bound ← if !reads x && reads y then pure x
      else if reads x && !reads y then pure y
      else throwError "emath_branch: the condition does not compare n with a fixed value"
    let side (e : Expr) := Expr.lam n ty (e.abstract #[t]) bi
    return (bound, #[side (body.getArg! 4), side (body.getArg! 3)])
  let k ← Term.exprToSyntax bound
  let whole ← Term.exprToSyntax u
  for side in sides do
    let w ← Term.exprToSyntax side
    let saved ← saveState
    -- `apply`, not `refine`: the order stays a goal later rules assign.
    try
      evalTactic (← `(tactic| apply HasOrder.eventually (u := $whole) (Int.toNat ($k + 1)) $w ?_ (fun n hn => by
        beta_reduce
        split <;> first | rfl | (exfalso; simp only [decide_eq_true_eq] at *; omega))))
      return
    catch _ => saved.restore
  throwError "emath_branch: no side holds from (K + 1).toNat on"

/-- Agreement at `s, ..., s + a + b - 1` gives agreement at every `n ≥ s`. -/
theorem eq_of_order_from {a b : ℕ} {u v : ℕ → ℚ} (s : ℕ) (hu : HasOrder a u) (hv : HasOrder b v)
    (h : ∀ i < a + b, u (s + i) = v (s + i)) (n : ℕ) (hn : s ≤ n) : u n = v n := by
  have := eq_of_order (hu.shift_by s) (hv.shift_by s) h (n - s)
  simpa [Nat.add_sub_cancel' hn] using this

/-- A branch on a condition that does not read `n` takes one side at every `n`. -/
theorem HasOrder.if_true {k : ℕ} {c : Bool} {u v : ℕ → ℚ} (h : c = true) (hu : HasOrder k u) :
    HasOrder k (fun n => if c = true then u n else v n) := by
  simpa [h] using hu

theorem HasOrder.if_false {k : ℕ} {c : Bool} {u v : ℕ → ℚ} (h : c = false) (hv : HasOrder k v) :
    HasOrder k (fun n => if c = true then u n else v n) := by
  simpa [h] using hv

open Lean Meta Elab Tactic in
/-- Proves `∀ i < N, u (s + i) = v (s + i)` by kernel evaluation. When
that fails it looks for an `i` where the kernel proves the sides
differ, so a failure either names a counterexample `n = s + i` or says
the evaluation did not finish; the two must not be confused. -/
elab "emath_check" start:num : tactic => withMainContext do
  let saved ← saveState
  try
    evalTactic (← `(tactic| decide +kernel))
  catch e =>
    saved.restore
    let goal ← instantiateMVars (← getMainTarget)
    let .forallE _ _ (.forallE _ bound body _) _ := goal | throw e
    let some count ← (evalNat (← reduce bound.appArg!)).run | throw e
    for k in [0:count] do
      let point := body.instantiateRev #[mkNatLit k, mkConst ``True.intro]
      let differs ← try
          let mvar ← mkFreshExprMVar (mkNot point)
          let rest ← Tactic.run mvar.mvarId! (evalTactic (← `(tactic| decide +kernel)))
          pure rest.isEmpty
        catch _ => pure false
      if differs then
        throwError "emath_check: the two sides differ at n = {start.getNat + k}"
    throwError "emath_check: Lean could not evaluate the two sides at the {count} checked points"

/-- Unfolds the loop step on a list of variables; closes linearity goals. -/
syntax "emath_linear" "[" ident,* "]" : tactic
macro_rules
  | `(tactic| emath_linear [$defs,*]) => do
    let lemmas ← defs.getElems.mapM fun d => `(Lean.Parser.Tactic.simpLemma| $d:ident)
    `(tactic|
      (intros; (first | (rename_i i; fin_cases i) | skip);
       all_goals (simp [Emath.tabulate, Emath.fold, Emath.range, Emath.index, Emath.int_quot,
         Emath.int_rem, List.ofFn_succ, List.range_succ, List.map_append, List.foldl_append,
         $lemmas,*]; try ring); done))

/-- Unfolds the first of `defs` that occurs in the goal. -/
syntax "emath_unfold" "[" ident,* "]" : tactic
macro_rules
  | `(tactic| emath_unfold [$defs,*]) => do
    let mut tac ← `(Lean.Parser.Tactic.tacticSeq| fail "emath_unfold: nothing to unfold")
    for d in defs.getElems.reverse do
      tac ← `(Lean.Parser.Tactic.tacticSeq| first | simp only [$d:ident] | $tac)
    `(tactic| ($tac))

/-- Bounds the order of `fun n => body` by the rules of emath's
`search.certify` shape analysis. `defs` are the user functions `body`
reaches; one is unfolded only when no rule applies, so a loop step's
coefficients stay opaque while its linearity is checked. -/
syntax "emath_order" "[" ident,* "]" : tactic
macro_rules
  | `(tactic| emath_order [$defs,*]) => `(tactic|
      repeat' ((try beta_reduce); first
        | with_reducible exact HasOrder.const _
        | with_reducible exact HasOrder.cast_nat
        | with_reducible exact HasOrder.rat_pow _
        | (with_reducible apply HasOrder.rat_pow_add); decide +kernel
        | (with_reducible apply HasOrder.rat_pow_sub); decide +kernel
        | (with_reducible apply HasOrder.fold_acc); (intro acc t; beta_reduce; ring)
        | emath_readout; all_goals (first | decide | emath_linear [] | emath_linear [$defs,*])
        | (with_reducible apply HasOrder.if_true); (first | decide +kernel | exact decide_eq_true ‹_›)
        | (with_reducible apply HasOrder.if_false); (first | decide +kernel | exact decide_eq_false ‹_›)
        | emath_branch
        | with_reducible apply HasOrder.add
        | with_reducible apply HasOrder.sub
        | with_reducible apply HasOrder.const_mul
        | with_reducible apply HasOrder.mul_const
        | with_reducible apply HasOrder.div_const
        | with_reducible apply HasOrder.neg
        | with_reducible apply HasOrder.mul
        | simp only [Int.cast_add, Int.cast_sub, Int.cast_mul, Int.cast_neg, Int.cast_zero,
            Int.cast_one, Int.cast_ofNat]
        | emath_unfold [$defs,*]))

end Emath.Law

namespace Emath.Law
open MvPolynomial

/-! ### One polynomial evaluation decides an identity

A polynomial with integer coefficients smaller than `X` in absolute
value that vanishes at `X` is zero: its value is its coefficients
written in base `X`. In several variables, evaluate the last at a
number above every coefficient and each earlier one above everything
the later ones can reach. -/

/-- Base-`X` digits are unique. -/
theorem digits_eq_zero (X : ℤ) (hX : 0 < X) (n : ℕ) :
    ∀ U : Polynomial ℤ, U.natDegree ≤ n → (∀ j, |U.coeff j| < X) → U.eval X = 0 → U = 0 := by
  induction n with
  | zero =>
    intro U hd _ he
    rw [Polynomial.eq_C_of_natDegree_le_zero hd] at he ⊢
    simp only [Polynomial.eval_C] at he
    simp [he]
  | succ n ih =>
    intro U hd hc he
    have hsplit := Polynomial.divX_mul_X_add U
    have h1 : U.divX.eval X * X + U.coeff 0 = 0 := by
      rw [← he]; conv_rhs => rw [← hsplit]
      simp
    have hV : U.divX.eval X = 0 := by
      by_contra hne
      have h2 : 1 ≤ |U.divX.eval X| := Int.one_le_abs hne
      have h3 : |U.coeff 0| = |U.divX.eval X| * X := by
        rw [show U.coeff 0 = -(U.divX.eval X * X) by linarith, abs_neg, abs_mul, abs_of_pos hX]
      have := hc 0
      nlinarith
    have h0 : U.coeff 0 = 0 := by rw [hV] at h1; simpa using h1
    have hVz : U.divX = 0 :=
      ih _ (by rw [Polynomial.natDegree_divX_eq_natDegree_tsub_one]; omega)
        (fun j => by rw [Polynomial.coeff_divX]; exact hc _) hV
    rw [← hsplit, hVz, h0]; simp

/-- The sum of the absolute values of the coefficients. -/
noncomputable def l1 {k : ℕ} (P : MvPolynomial (Fin k) ℤ) : ℕ :=
  ∑ m ∈ P.support, (P.coeff m).natAbs

theorem l1_eq_of_subset {k : ℕ} (P : MvPolynomial (Fin k) ℤ) {s : Finset (Fin k →₀ ℕ)}
    (hs : P.support ⊆ s) : l1 P = ∑ m ∈ s, (P.coeff m).natAbs := by
  unfold l1
  exact Finset.sum_subset hs fun m _ hm => by simp [notMem_support_iff.mp hm]

theorem l1_add_le {k : ℕ} (P Q : MvPolynomial (Fin k) ℤ) : l1 (P + Q) ≤ l1 P + l1 Q := by
  classical
  rw [l1_eq_of_subset (P + Q) (support_add (p := P) (q := Q)),
    l1_eq_of_subset P Finset.subset_union_left, l1_eq_of_subset Q Finset.subset_union_right,
    ← Finset.sum_add_distrib]
  exact Finset.sum_le_sum fun m _ => by simpa using Int.natAbs_add_le (P.coeff m) (Q.coeff m)

theorem l1_neg {k : ℕ} (P : MvPolynomial (Fin k) ℤ) : l1 (-P) = l1 P := by
  unfold l1; rw [support_neg]; simp

theorem l1_sub_le {k : ℕ} (P Q : MvPolynomial (Fin k) ℤ) : l1 (P - Q) ≤ l1 P + l1 Q := by
  rw [sub_eq_add_neg]; exact (l1_add_le _ _).trans (by rw [l1_neg])

theorem l1_monomial {k : ℕ} (m : Fin k →₀ ℕ) (c : ℤ) : l1 (monomial m c) = c.natAbs := by
  classical
  unfold l1
  by_cases hc : c = 0
  · simp [hc]
  · simp [support_monomial, hc]

theorem l1_sum_le {k : ℕ} {ι : Type} (s : Finset ι) (f : ι → MvPolynomial (Fin k) ℤ) :
    l1 (∑ i ∈ s, f i) ≤ ∑ i ∈ s, l1 (f i) := by
  classical
  induction s using Finset.induction_on with
  | empty => simp [l1]
  | insert a s ha ih =>
    rw [Finset.sum_insert ha, Finset.sum_insert ha]
    exact (l1_add_le _ _).trans (by omega)

theorem l1_mul_le {k : ℕ} (P Q : MvPolynomial (Fin k) ℤ) : l1 (P * Q) ≤ l1 P * l1 Q := by
  classical
  conv_lhs => rw [P.as_sum, Q.as_sum, Finset.sum_mul_sum]
  refine (l1_sum_le _ _).trans ?_
  unfold l1
  rw [Finset.sum_mul_sum]
  refine Finset.sum_le_sum fun a _ => (l1_sum_le _ _).trans (Finset.sum_le_sum fun b _ => ?_)
  rw [monomial_mul_monomial, l1_monomial, Int.natAbs_mul]

theorem l1_C {k : ℕ} (c : ℤ) : l1 (C c : MvPolynomial (Fin k) ℤ) = c.natAbs := by
  rw [← monomial_zero', l1_monomial]

theorem l1_X {k : ℕ} (i : Fin k) : l1 (X i : MvPolynomial (Fin k) ℤ) = 1 := by
  rw [X, l1_monomial]; rfl

theorem l1_coeff_finSuccEquiv {k : ℕ} (P : MvPolynomial (Fin (k + 1)) ℤ) (j : ℕ) :
    l1 ((finSuccEquiv ℤ k P).coeff j) ≤ l1 P := by
  classical
  unfold l1
  simp_rw [finSuccEquiv_coeff_coeff]
  rw [← Finset.sum_image (g := fun m : Fin k →₀ ℕ => m.cons j)
    (f := fun m => (P.coeff m).natAbs) (fun a _ b _ h => Finsupp.cons_right_injective j h)]
  exact Finset.sum_le_sum_of_subset (fun m hm => by
    obtain ⟨a, ha, rfl⟩ := Finset.mem_image.mp hm
    rw [mem_support_iff, ← finSuccEquiv_coeff_coeff]; exact mem_support_iff.mp ha)

/-- `f 0 * f 1 * ... * f (k - 1)`, computed by recursion so the kernel evaluates it. -/
def fprod : (k : ℕ) → (Fin k → ℕ) → ℕ
  | 0, _ => 1
  | k + 1, f => f 0 * fprod k (Fin.tail f)

theorem fprod_eq : ∀ (k : ℕ) (f : Fin k → ℕ), fprod k f = ∏ i, f i
  | 0, _ => by simp [fprod]
  | k + 1, f => by rw [fprod, fprod_eq k, Fin.prod_univ_succ]; rfl

/-- The evaluation point for bounds `h` on coefficient sums and `d` on degrees. -/
def kpt (h : ℕ) : (k : ℕ) → (Fin k → ℕ) → Fin k → ℕ
  | 0, _ => Fin.elim0
  | k + 1, d =>
    let r := kpt h k (Fin.tail d)
    Fin.cons (h * fprod k (fun i => r i ^ d i.succ) + 1) r

theorem one_le_kpt (h : ℕ) : ∀ (k : ℕ) (d : Fin k → ℕ) (i : Fin k), 1 ≤ kpt h k d i
  | 0, _, i => i.elim0
  | k + 1, d, i => by
    refine Fin.cases ?_ (fun i => ?_) i
    · simp [kpt]
    · simp only [kpt, Fin.cons_succ]; exact one_le_kpt h k _ i

theorem abs_eval_le {k : ℕ} (P : MvPolynomial (Fin k) ℤ) (r d : Fin k → ℕ) (hr : ∀ i, 1 ≤ r i)
    (hd : ∀ i, degreeOf i P ≤ d i) :
    |eval (fun i => (r i : ℤ)) P| ≤ (l1 P : ℤ) * (fprod k (fun i => r i ^ d i) : ℕ) := by
  rw [eval_eq, l1, Nat.cast_sum, Finset.sum_mul]
  refine (Finset.abs_sum_le_sum_abs _ _).trans (Finset.sum_le_sum fun m hm => ?_)
  rw [abs_mul, Int.abs_eq_natAbs]
  refine mul_le_mul_of_nonneg_left ?_ (by positivity)
  have key : (∏ x ∈ m.support, r x ^ m x) ≤ ∏ i, r i ^ d i :=
    calc ∏ x ∈ m.support, r x ^ m x ≤ ∏ x ∈ m.support, r x ^ d x :=
          Finset.prod_le_prod fun i _ =>
            Nat.pow_le_pow_right (hr i) ((monomial_le_degreeOf i hm).trans (hd i))
      _ ≤ ∏ i, r i ^ d i :=
          Finset.prod_le_prod_of_subset_of_one_le (Finset.subset_univ _)
            fun i _ _ => Nat.one_le_pow _ _ (hr i)
  rw [fprod_eq, Finset.abs_prod]
  have : ∏ x ∈ m.support, |((r x : ℤ)) ^ m x| = ((∏ x ∈ m.support, r x ^ m x : ℕ) : ℤ) := by
    push_cast; simp [abs_pow]
  rw [this]; exact_mod_cast key

/-- A polynomial whose degrees are at most `d` and coefficient sum at most `h`
that vanishes at `kpt h k d` is zero. -/
theorem eq_zero_of_kpt (h : ℕ) : ∀ (k : ℕ) (d : Fin k → ℕ) (P : MvPolynomial (Fin k) ℤ),
    (∀ i, degreeOf i P ≤ d i) → l1 P ≤ h → eval (fun i => (kpt h k d i : ℤ)) P = 0 → P = 0
  | 0, d, P, _, _, he => by
    rw [P.eq_C_of_isEmpty] at he ⊢
    simp only [eval_C] at he
    simp [he]
  | k + 1, d, P, hd, hl, he => by
    set r := kpt h k (Fin.tail d) with hr
    set y : ℕ := h * fprod k (fun i => r i ^ d i.succ) + 1 with hy
    have hpt : (fun i => (kpt h (k + 1) d i : ℤ)) = Fin.cons (y : ℤ) (fun i => (r i : ℤ)) := by
      funext i; refine Fin.cases ?_ (fun i => ?_) i <;> simp [kpt, hr, hy]
    rw [hpt, eval_eq_eval_mv_eval'] at he
    set Q := finSuccEquiv ℤ k P
    have hcoeff : ∀ j, eval (fun i => (r i : ℤ)) (Q.coeff j) = 0 := by
      have hU := digits_eq_zero (y : ℤ) (by positivity) _ (Polynomial.map (eval fun i => (r i : ℤ)) Q)
        le_rfl (fun j => by
          rw [Polynomial.coeff_map]
          refine (abs_eval_le _ r (Fin.tail d) (one_le_kpt h k _)
            (fun i => (degreeOf_coeff_finSuccEquiv P i j).trans (hd _))).trans_lt ?_
          have := l1_coeff_finSuccEquiv P j
          have hy' : ((h * fprod k (fun i => r i ^ d i.succ) : ℕ) : ℤ) < y := by
            rw [hy]; push_cast; omega
          calc ((l1 (Q.coeff j) : ℕ) : ℤ) * (fprod k (fun i => r i ^ Fin.tail d i) : ℕ)
              ≤ (h : ℤ) * (fprod k (fun i => r i ^ d i.succ) : ℕ) := by
                apply mul_le_mul_of_nonneg_right _ (by positivity)
                exact_mod_cast this.trans hl
            _ < y := by exact_mod_cast hy') he
      intro j
      have := congrArg (fun U => Polynomial.coeff U j) hU
      simpa [Polynomial.coeff_map] using this
    have hQ : Q = 0 := by
      ext j : 1
      rw [Polynomial.coeff_zero]
      exact eq_zero_of_kpt h k (Fin.tail d) _
        (fun i => (degreeOf_coeff_finSuccEquiv P i j).trans (hd _))
        ((l1_coeff_finSuccEquiv P j).trans hl) (hcoeff j)
    exact (finSuccEquiv ℤ k).injective (by rw [map_zero]; exact hQ)

theorem aeval_intCast {k : ℕ} (z : Fin k → ℤ) (P : MvPolynomial (Fin k) ℤ) :
    aeval (fun i => ((z i : ℤ) : ℚ)) P = ((eval z P : ℤ) : ℚ) := by
  induction P using MvPolynomial.induction_on with
  | C a => simp
  | add p q hp hq => simp [hp, hq]
  | mul_X p i hp => simp [hp]

/-! ### Bounds on a rational function of the parameters -/

/-- Bounds for a function of the parameters that equals `N / (c D)` with
integer polynomials `N`, `D` and a positive integer `c`: per-parameter
degrees (`dn`, `dd`) and coefficient sums (`hn`, `hd`) of `N` and `D`.
`ok = false` marks a bound the analysis gave up on; it claims nothing. -/
structure Bd where
  ok : Bool
  dn : List ℕ
  hn : ℕ
  dd : List ℕ
  hd : ℕ
  c : ℕ

/-- Degrees at most `ds` (0 past its end) and coefficient sum at most `h`. -/
def Fits {k : ℕ} (ds : List ℕ) (h : ℕ) (P : MvPolynomial (Fin k) ℤ) : Prop :=
  (∀ i : Fin k, degreeOf i P ≤ ds.getD i 0) ∧ l1 P ≤ h

/-- On the parameter points `S`, `φ` is `N / (c D)` within the bounds `b`. -/
def Bnd {k : ℕ} (S : (Fin k → ℚ) → Prop) (b : Bd) (φ : (Fin k → ℚ) → ℚ) : Prop :=
  b.ok = true → 0 < b.c ∧ ∃ N D : MvPolynomial (Fin k) ℤ, Fits b.dn b.hn N ∧ Fits b.dd b.hd D ∧
    ∀ x, S x → aeval x D ≠ 0 ∧ φ x = aeval x N / (b.c * aeval x D)

/-- Entrywise `f`, a missing entry read as 0. -/
def vzip (f : ℕ → ℕ → ℕ) : List ℕ → List ℕ → List ℕ
  | [], ys => ys.map (f 0)
  | x :: xs, [] => (x :: xs).map (f · 0)
  | x :: xs, y :: ys => f x y :: vzip f xs ys

theorem getD_map0 (g : ℕ → ℕ) (hg : g 0 = 0) :
    ∀ (l : List ℕ) (i : ℕ), (l.map g).getD i 0 = g (l.getD i 0)
  | [], i => by simp [hg]
  | _ :: _, 0 => by simp
  | _ :: xs, i + 1 => by simpa using getD_map0 g hg xs i

theorem vzip_getD (f : ℕ → ℕ → ℕ) (hf : f 0 0 = 0) :
    ∀ (a b : List ℕ) (i : ℕ), (vzip f a b).getD i 0 = f (a.getD i 0) (b.getD i 0)
  | [], ys, i => by rw [vzip, getD_map0 _ hf]; simp
  | x :: xs, [], i => by rw [vzip, getD_map0 (f · 0) hf]; simp
  | _ :: _, _ :: _, 0 => by simp [vzip]
  | _ :: xs, _ :: ys, i + 1 => by simpa [vzip] using vzip_getD f hf xs ys i

def vadd : List ℕ → List ℕ → List ℕ := vzip (· + ·)
def vmax : List ℕ → List ℕ → List ℕ := vzip max

theorem vadd_getD (a b : List ℕ) (i : ℕ) : (vadd a b).getD i 0 = a.getD i 0 + b.getD i 0 :=
  vzip_getD _ rfl a b i
theorem vmax_getD (a b : List ℕ) (i : ℕ) : (vmax a b).getD i 0 = max (a.getD i 0) (b.getD i 0) :=
  vzip_getD _ rfl a b i

def bconst (q : ℚ) : Bd := ⟨true, [], q.num.natAbs, [], 1, q.den⟩
def bvar (i : ℕ) : Bd := ⟨true, List.replicate i 0 ++ [1], 1, [], 1, 1⟩
def bzero : Bd := bconst 0
def bfail : Bd := ⟨false, [], 0, [], 0, 1⟩
def badd (a b : Bd) : Bd :=
  ⟨a.ok && b.ok, vmax (vadd a.dn b.dd) (vadd b.dn a.dd),
    Nat.lcm a.c b.c / a.c * a.hn * b.hd + Nat.lcm a.c b.c / b.c * b.hn * a.hd,
    vadd a.dd b.dd, a.hd * b.hd, Nat.lcm a.c b.c⟩
def bmul (a b : Bd) : Bd :=
  ⟨a.ok && b.ok, vadd a.dn b.dn, a.hn * b.hn, vadd a.dd b.dd, a.hd * b.hd, a.c * b.c⟩
def bdiv (a b : Bd) : Bd :=
  ⟨a.ok && b.ok, vadd a.dn b.dd, b.c * a.hn * b.hd, vadd a.dd b.dn, a.hd * b.hn, a.c⟩
def bpow (a : Bd) : ℕ → Bd
  | 0 => bconst 1
  | n + 1 => bmul (bpow a n) a
def brat_pow (a : Bd) (e : ℤ) : Bd := if 0 ≤ e then bpow a e.toNat else bfail

section
variable {k : ℕ} {S : (Fin k → ℚ) → Prop}

theorem Fits.C_mul_mul {a b : List ℕ} {ha hb : ℕ} {N D : MvPolynomial (Fin k) ℤ} (r : ℕ)
    (hN : Fits a ha N) (hD : Fits b hb D) : Fits (vadd a b) (r * ha * hb) (C (r : ℤ) * N * D) := by
  refine ⟨fun i => ?_, ?_⟩
  · rw [vadd_getD]
    refine (degreeOf_mul_le _ _ _).trans (Nat.add_le_add ((degreeOf_mul_le _ _ _).trans ?_) (hD.1 i))
    rw [degreeOf_C, zero_add]; exact hN.1 i
  · refine (l1_mul_le _ _).trans (Nat.mul_le_mul ((l1_mul_le _ _).trans ?_) hD.2)
    rw [l1_C, Int.natAbs_natCast]; exact Nat.mul_le_mul_left _ hN.2

theorem Fits.mul {a b : List ℕ} {ha hb : ℕ} {N D : MvPolynomial (Fin k) ℤ}
    (hN : Fits a ha N) (hD : Fits b hb D) : Fits (vadd a b) (ha * hb) (N * D) := by
  have := Fits.C_mul_mul 1 hN hD
  simpa using this

theorem Bnd.congr {b : Bd} {φ ψ : (Fin k → ℚ) → ℚ} (h : Bnd S b φ) (he : ∀ x, S x → φ x = ψ x) :
    Bnd S b ψ := fun hok => by
  obtain ⟨hc, N, D, hN, hD, hr⟩ := h hok
  exact ⟨hc, N, D, hN, hD, fun x hx => ⟨(hr x hx).1, (he x hx) ▸ (hr x hx).2⟩⟩

theorem Bnd.const (q : ℚ) : Bnd S (bconst q) (fun _ => q) := fun _ => by
  refine ⟨q.den_pos, C q.num, C 1, ⟨fun i => by rw [degreeOf_C]; exact Nat.zero_le _, (l1_C _).le⟩,
    ⟨fun i => by rw [degreeOf_C]; exact Nat.zero_le _, (l1_C 1).le⟩, fun x _ => ⟨by simp, ?_⟩⟩
  simp [bconst, Rat.num_div_den]

theorem Bnd.var (i : Fin k) : Bnd S (bvar i) (fun x => x i) := fun _ => by
  refine ⟨Nat.one_pos, X i, C 1, ⟨fun j => ?_, (l1_X i).le⟩,
    ⟨fun j => by rw [degreeOf_C]; exact Nat.zero_le _, (l1_C 1).le⟩, fun x _ => ⟨by simp, by simp [bvar]⟩⟩
  rw [degreeOf_X]
  split_ifs with h
  · subst h; simp [bvar, List.getD_eq_getElem?_getD]
  · exact Nat.zero_le _

theorem Bnd.add {a b : Bd} {φ ψ : (Fin k → ℚ) → ℚ} (h1 : Bnd S a φ) (h2 : Bnd S b ψ) :
    Bnd S (badd a b) (fun x => φ x + ψ x) := fun hok => by
  dsimp only [badd] at hok ⊢
  simp only [Bool.and_eq_true] at hok
  obtain ⟨ha, N1, D1, hN1, hD1, hr1⟩ := h1 hok.1
  obtain ⟨hb, N2, D2, hN2, hD2, hr2⟩ := h2 hok.2
  set L := Nat.lcm a.c b.c
  have hu : L / a.c * a.c = L := Nat.div_mul_cancel (Nat.dvd_lcm_left _ _)
  have hv : L / b.c * b.c = L := Nat.div_mul_cancel (Nat.dvd_lcm_right _ _)
  have hL : 0 < L := Nat.lcm_pos ha hb
  refine ⟨hL, C ((L / a.c : ℕ) : ℤ) * N1 * D2 + C ((L / b.c : ℕ) : ℤ) * N2 * D1, D1 * D2,
    ⟨fun i => ?_, (l1_add_le _ _).trans (Nat.add_le_add (Fits.C_mul_mul _ hN1 hD2).2
      (Fits.C_mul_mul _ hN2 hD1).2)⟩, Fits.mul hD1 hD2, fun x hx => ?_⟩
  · rw [vmax_getD]
    exact (degreeOf_add_le _ _ _).trans (max_le_max ((Fits.C_mul_mul _ hN1 hD2).1 i)
      ((Fits.C_mul_mul _ hN2 hD1).1 i))
  obtain ⟨hz1, he1⟩ := hr1 x hx
  obtain ⟨hz2, he2⟩ := hr2 x hx
  refine ⟨by rw [map_mul]; exact mul_ne_zero hz1 hz2, ?_⟩
  have hu' : ((L / a.c : ℕ) : ℚ) * a.c = L := by exact_mod_cast hu
  have hv' : ((L / b.c : ℕ) : ℚ) * b.c = L := by exact_mod_cast hv
  have ha' : (a.c : ℚ) ≠ 0 := by positivity
  have hb' : (b.c : ℚ) ≠ 0 := by positivity
  rw [he1, he2]
  simp only [map_add, map_mul, map_natCast]
  rw [div_add_div _ _ (mul_ne_zero ha' hz1) (mul_ne_zero hb' hz2),
    div_eq_div_iff (mul_ne_zero (mul_ne_zero ha' hz1) (mul_ne_zero hb' hz2))
      (mul_ne_zero (by positivity) (mul_ne_zero hz1 hz2))]
  linear_combination (-(aeval x N1 * b.c * aeval x D1 * aeval x D2 ^ 2)) * hu' +
    (-(aeval x N2 * a.c * aeval x D1 ^ 2 * aeval x D2)) * hv'

theorem Bnd.neg {a : Bd} {φ : (Fin k → ℚ) → ℚ} (h : Bnd S a φ) : Bnd S a (fun x => -φ x) :=
  fun hok => by
  obtain ⟨ha, N, D, hN, hD, hr⟩ := h hok
  refine ⟨ha, -N, D, ⟨fun i => by rw [degreeOf_neg]; exact hN.1 i, by rw [l1_neg]; exact hN.2⟩, hD,
    fun x hx => ⟨(hr x hx).1, ?_⟩⟩
  show -φ x = _
  rw [(hr x hx).2, map_neg, neg_div]

theorem Bnd.sub {a b : Bd} {φ ψ : (Fin k → ℚ) → ℚ} (h1 : Bnd S a φ) (h2 : Bnd S b ψ) :
    Bnd S (badd a b) (fun x => φ x - ψ x) :=
  (Bnd.add h1 (Bnd.neg h2)).congr fun _ _ => (sub_eq_add_neg _ _).symm

theorem Bnd.mul {a b : Bd} {φ ψ : (Fin k → ℚ) → ℚ} (h1 : Bnd S a φ) (h2 : Bnd S b ψ) :
    Bnd S (bmul a b) (fun x => φ x * ψ x) := fun hok => by
  dsimp only [bmul] at hok ⊢
  simp only [Bool.and_eq_true] at hok
  obtain ⟨ha, N1, D1, hN1, hD1, hr1⟩ := h1 hok.1
  obtain ⟨hb, N2, D2, hN2, hD2, hr2⟩ := h2 hok.2
  refine ⟨Nat.mul_pos ha hb, N1 * N2, D1 * D2, Fits.mul hN1 hN2, Fits.mul hD1 hD2, fun x hx => ?_⟩
  obtain ⟨hz1, he1⟩ := hr1 x hx
  obtain ⟨hz2, he2⟩ := hr2 x hx
  refine ⟨by rw [map_mul]; exact mul_ne_zero hz1 hz2, ?_⟩
  rw [he1, he2, div_mul_div_comm, map_mul, map_mul]
  push_cast; ring

theorem Bnd.div {a b : Bd} {φ ψ : (Fin k → ℚ) → ℚ} (hz : ∀ x, S x → ψ x ≠ 0)
    (h1 : Bnd S a φ) (h2 : Bnd S b ψ) : Bnd S (bdiv a b) (fun x => φ x / ψ x) := fun hok => by
  dsimp only [bdiv] at hok ⊢
  simp only [Bool.and_eq_true] at hok
  obtain ⟨ha, N1, D1, hN1, hD1, hr1⟩ := h1 hok.1
  obtain ⟨hb, N2, D2, hN2, hD2, hr2⟩ := h2 hok.2
  refine ⟨ha, C (b.c : ℤ) * N1 * D2, D1 * N2, Fits.C_mul_mul _ hN1 hD2, Fits.mul hD1 hN2,
    fun x hx => ?_⟩
  obtain ⟨hz1, he1⟩ := hr1 x hx
  obtain ⟨hz2, he2⟩ := hr2 x hx
  have hb' : (b.c : ℚ) ≠ 0 := by positivity
  have ha' : (a.c : ℚ) ≠ 0 := by positivity
  have hN2x : aeval x N2 ≠ 0 := by
    intro h0; apply hz x hx; rw [he2, h0, zero_div]
  refine ⟨by rw [map_mul]; exact mul_ne_zero hz1 hN2x, ?_⟩
  show φ x / ψ x = _
  rw [he1, he2]
  simp only [map_mul, map_natCast]
  field_simp

theorem Bnd.pow {a : Bd} {φ : (Fin k → ℚ) → ℚ} (h : Bnd S a φ) :
    ∀ m : ℕ, Bnd S (bpow a m) (fun x => φ x ^ m)
  | 0 => (Bnd.const 1).congr fun _ _ => (pow_zero _).symm
  | m + 1 => (Bnd.mul (Bnd.pow h m) h).congr fun _ _ => (pow_succ _ _).symm

theorem Bnd.rat_pow {a : Bd} {φ : (Fin k → ℚ) → ℚ} (h : Bnd S a φ) (e : ℤ) :
    Bnd S (brat_pow a e) (fun x => Emath.rat_pow (φ x) e) := by
  unfold brat_pow
  split_ifs with he
  · exact (Bnd.pow h e.toNat).congr fun x _ => by
      simp [Emath.rat_pow, not_lt.mpr he]
  · intro hok; exact absurd hok (by simp [bfail])

theorem Bnd.ite (c : Prop) [Decidable c] {a b : Bd} {φ ψ : (Fin k → ℚ) → ℚ}
    (h1 : Bnd S a φ) (h2 : Bnd S b ψ) :
    Bnd S (if c then a else b) (fun x => if c then φ x else ψ x) := by
  by_cases hc : c
  · simp only [hc, ↓reduceIte]; exact h1
  · simp only [hc, ↓reduceIte]; exact h2

/-- A branch on the parameters takes the side its hypothesis names. -/
theorem Bnd.ite_pos (c : (Fin k → ℚ) → Prop) [DecidablePred c] {a : Bd} {φ ψ : (Fin k → ℚ) → ℚ}
    (hc : ∀ x, S x → c x) (h1 : Bnd S a φ) :
    Bnd S a (fun x => if c x then φ x else ψ x) :=
  h1.congr fun x hx => by simp only [hc x hx, ↓reduceIte]

theorem Bnd.ite_neg (c : (Fin k → ℚ) → Prop) [DecidablePred c] {b : Bd} {φ ψ : (Fin k → ℚ) → ℚ}
    (hc : ∀ x, S x → ¬ c x) (h2 : Bnd S b ψ) :
    Bnd S b (fun x => if c x then φ x else ψ x) :=
  h2.congr fun x hx => by simp only [hc x hx, ↓reduceIte]

theorem Bnd.fold {δ : Type} (F : Bd → δ → Bd) (step : (Fin k → ℚ) → ℚ → δ → ℚ)
    (xs : List δ) (b0 : Bd) (init : (Fin k → ℚ) → ℚ)
    (hs : ∀ b ψ, Bnd S b ψ → ∀ t, Bnd S (F b t) (fun x => step x (ψ x) t)) (h0 : Bnd S b0 init) :
    Bnd S (xs.foldl F b0) (fun x => Emath.fold (step x) (init x) xs) := by
  induction xs generalizing b0 init with
  | nil => exact h0
  | cons t xs ih => exact ih _ _ (hs _ _ h0 t)

/-- Bounds on every entry of a list (an entry past the end reads 0). -/
def BndL {k : ℕ} (S : (Fin k → ℚ) → Prop) (bs : List Bd) (φ : (Fin k → ℚ) → List ℚ) : Prop :=
  ∀ j : ℕ, Bnd S (bs.getD j bzero) (fun x => (φ x).getD j 0)

theorem BndL.nil : BndL S [] (fun _ => []) := fun j => by
  simpa [bzero] using (Bnd.const (S := S) 0)

theorem BndL.cons {b : Bd} {bs : List Bd} {φ : (Fin k → ℚ) → ℚ} {ψ : (Fin k → ℚ) → List ℚ}
    (h : Bnd S b φ) (hs : BndL S bs ψ) : BndL S (b :: bs) (fun x => φ x :: ψ x)
  | 0 => by simpa using h
  | j + 1 => by simpa using hs j

theorem BndL.const : ∀ l : List ℚ, BndL S (l.map bconst) (fun _ => l)
  | [] => BndL.nil
  | q :: l => BndL.cons (Bnd.const q) (BndL.const l)

theorem Bnd.index {bs : List Bd} {φ : (Fin k → ℚ) → List ℚ} (h : BndL S bs φ) (i : ℤ) :
    Bnd S (bs.getD i.toNat bzero) (fun x => Emath.index (φ x) i) := h i.toNat

theorem BndL.tabulate (B : ℤ → Bd) (f : (Fin k → ℚ) → ℤ → ℚ) (m : ℤ)
    (h : ∀ j, Bnd S (B j) (fun x => f x j)) :
    BndL S (Emath.tabulate B m) (fun x => Emath.tabulate (f x) m) := fun j => by
  unfold Emath.tabulate
  by_cases hj : j < m.toNat
  · simpa [List.getD_eq_getElem?_getD, hj] using h j
  · simpa [List.getD_eq_getElem?_getD, hj, bzero] using (Bnd.const (S := S) 0)

theorem BndL.fold {δ : Type} (F : List Bd → δ → List Bd)
    (step : (Fin k → ℚ) → List ℚ → δ → List ℚ) (xs : List δ) (b0 : List Bd)
    (init : (Fin k → ℚ) → List ℚ)
    (hs : ∀ b ψ, BndL S b ψ → ∀ t, BndL S (F b t) (fun x => step x (ψ x) t)) (h0 : BndL S b0 init) :
    BndL S (xs.foldl F b0) (fun x => Emath.fold (step x) (init x) xs) := by
  induction xs generalizing b0 init with
  | nil => exact h0
  | cons t xs ih => exact ih _ _ (hs _ _ h0 t)

/-! ### The check -/

/-- Degree bounds of `cg Nf Dg - cf Ng Df`, the numerator of `f - g`. -/
def crossDeg (f g : Bd) : List ℕ := vmax (vadd f.dn g.dd) (vadd g.dn f.dd)
/-- A bound on its coefficient sum. -/
def crossH (f g : Bd) : ℕ := g.c * f.hn * g.hd + f.c * g.hn * f.hd

theorem kron_eq (S : (Fin k → ℚ) → Prop) {bf bg : Bd} {φ ψ : (Fin k → ℚ) → ℚ}
    (hf : Bnd S bf φ) (hg : Bnd S bg ψ) (hfo : bf.ok = true) (hgo : bg.ok = true)
    (h : ℕ) (d : Fin k → ℕ) (hd : ∀ j : Fin k, (crossDeg bf bg).getD j 0 ≤ d j)
    (hh : crossH bf bg ≤ h) (hS0 : S (fun i => (kpt h k d i : ℚ)))
    (he0 : φ (fun i => (kpt h k d i : ℚ)) = ψ (fun i => (kpt h k d i : ℚ))) :
    ∀ x, S x → φ x = ψ x := by
  obtain ⟨hcf, Nf, Df, hNf, hDf, hrf⟩ := hf hfo
  obtain ⟨hcg, Ng, Dg, hNg, hDg, hrg⟩ := hg hgo
  set P := C (bg.c : ℤ) * Nf * Dg - C (bf.c : ℤ) * Ng * Df with hP
  have hcf' : (bf.c : ℚ) ≠ 0 := by positivity
  have hcg' : (bg.c : ℚ) ≠ 0 := by positivity
  have key : ∀ x, S x → φ x - ψ x = aeval x P / (bf.c * bg.c * aeval x Df * aeval x Dg) := by
    intro x hx
    obtain ⟨hzf, hef⟩ := hrf x hx
    obtain ⟨hzg, heg⟩ := hrg x hx
    rw [hef, heg, hP]
    simp only [map_sub, map_mul, map_natCast]
    field_simp
  have hfit1 := Fits.C_mul_mul bg.c hNf hDg
  have hfit2 := Fits.C_mul_mul bf.c hNg hDf
  have hz : P = 0 := by
    refine eq_zero_of_kpt h k d P (fun i => ?_)
      (((l1_sub_le _ _).trans (Nat.add_le_add hfit1.2 hfit2.2)).trans hh) ?_
    · refine le_trans ?_ (hd i)
      rw [crossDeg, vmax_getD]
      exact (degreeOf_sub_le _ _ _).trans (max_le_max (hfit1.1 i) (hfit2.1 i))
    · have h0 := key _ hS0
      rw [he0, sub_self, eq_comm, div_eq_zero_iff] at h0
      have hzf := (hrf _ hS0).1
      have hzg := (hrg _ hS0).1
      rcases h0 with h0 | h0
      · have := aeval_intCast (fun i => (kpt h k d i : ℤ)) P
        simp only [Int.cast_natCast] at this
        exact_mod_cast this.symm.trans h0
      · exfalso; exact mul_ne_zero (mul_ne_zero (mul_ne_zero hcf' hcg') hzf) hzg h0
  intro x hx
  have := key x hx
  rw [hz, map_zero, zero_div, sub_eq_zero] at this
  exact this

/-- Entrywise maximum of `F i` over `is`, above `a`. -/
theorem foldl_vmax_ge {ι : Type} (F : ι → List ℕ) (j : ℕ) :
    ∀ (is : List ι) (a : List ℕ), a.getD j 0 ≤ (is.foldl (fun acc i => vmax acc (F i)) a).getD j 0 ∧
      ∀ i ∈ is, (F i).getD j 0 ≤ (is.foldl (fun acc i => vmax acc (F i)) a).getD j 0
  | [], a => ⟨le_rfl, by simp⟩
  | i :: is, a => by
    obtain ⟨h1, h2⟩ := foldl_vmax_ge F j is (vmax a (F i))
    rw [vmax_getD] at h1
    refine ⟨(le_max_left _ _).trans h1, fun i' hi' => ?_⟩
    rcases List.mem_cons.mp hi' with rfl | hi'
    · exact (le_max_right _ _).trans h1
    · exact h2 i' hi'

theorem foldl_max_ge {ι : Type} (F : ι → ℕ) :
    ∀ (is : List ι) (a : ℕ), a ≤ is.foldl (fun acc i => max acc (F i)) a ∧
      ∀ i ∈ is, F i ≤ is.foldl (fun acc i => max acc (F i)) a
  | [], a => ⟨le_rfl, by simp⟩
  | i :: is, a => by
    obtain ⟨h1, h2⟩ := foldl_max_ge F is (max a (F i))
    refine ⟨(le_max_left _ _).trans h1, fun i' hi' => ?_⟩
    rcases List.mem_cons.mp hi' with rfl | hi'
    · exact (le_max_right _ _).trans h1
    · exact h2 i' hi'

/-- One point for the checks at `n = s, ..., s + K - 1`: above every bound
any of them needs, so that the evaluations share their work. -/
def kshared (k : ℕ) (Bf Bg : ℕ → Bd) (s K : ℕ) : Fin k → ℕ :=
  kpt ((List.range K).foldl (fun acc i => max acc (crossH (Bf (s + i)) (Bg (s + i)))) 0) k
    (fun j => ((List.range K).foldl (fun acc i => vmax acc (crossDeg (Bf (s + i)) (Bg (s + i))))
      []).getD j 0)

/-- Every bound holds, the shared point is in `S`, and `f`, `g` agree there
at each `n`. -/
def kcheck {k : ℕ} (S : (Fin k → ℚ) → Prop) [DecidablePred S] (Bf Bg : ℕ → Bd)
    (f g : (Fin k → ℚ) → ℕ → ℚ) (s K : ℕ) : Bool :=
  let x : Fin k → ℚ := fun i => (kshared k Bf Bg s K i : ℚ)
  decide (S x) && (List.range K).all fun i =>
    (Bf (s + i)).ok && (Bg (s + i)).ok && decide (f x (s + i) = g x (s + i))

/-- The shared point is in `S` and `f`, `g` differ there at some `n`: the
law fails at that parameter point. -/
def kdiffer {k : ℕ} (S : (Fin k → ℚ) → Prop) [DecidablePred S] (Bf Bg : ℕ → Bd)
    (f g : (Fin k → ℚ) → ℕ → ℚ) (s K : ℕ) : Bool :=
  let x : Fin k → ℚ := fun i => (kshared k Bf Bg s K i : ℚ)
  decide (S x) && (List.range K).any fun i => decide (f x (s + i) ≠ g x (s + i))

/-- At the parameter points `S`, `f x` and `g x` agree at `n = s, ..., s + K - 1`
when the check passes. -/
theorem params_eq (S : (Fin k → ℚ) → Prop) [DecidablePred S] (f g : (Fin k → ℚ) → ℕ → ℚ)
    (Bf Bg : ℕ → Bd) (hf : ∀ n, Bnd S (Bf n) (fun x => f x n))
    (hg : ∀ n, Bnd S (Bg n) (fun x => g x n)) (s K : ℕ) (hc : kcheck S Bf Bg f g s K = true)
    (x : Fin k → ℚ) (hx : S x) : ∀ i < K, f x (s + i) = g x (s + i) := by
  intro i hi
  simp only [kcheck, Bool.and_eq_true, decide_eq_true_eq, List.all_eq_true, List.mem_range] at hc
  obtain ⟨hS, hall⟩ := hc
  obtain ⟨⟨hfo, hgo⟩, he⟩ := hall i hi
  have hmem : i ∈ List.range K := List.mem_range.mpr hi
  exact kron_eq S (hf _) (hg _) hfo hgo _ _
    (fun j => (foldl_vmax_ge (fun i => crossDeg (Bf (s + i)) (Bg (s + i))) j _ []).2 i hmem)
    ((foldl_max_ge (fun i => crossH (Bf (s + i)) (Bg (s + i))) _ 0).2 i hmem) hS he x hx

end

/-- Closes `∀ x, S x → d x ≠ 0` for a divisor `d`: a nonzero constant, one
of the hypotheses `S` lists, or one of them once the branches in `d` take
the sides `S` gives their conditions. -/
macro "emath_nonzero" : tactic =>
  `(tactic| (intro x hx; (try simp only at hx ⊢); first | (norm_num; done) | tauto |
    (split_ifs <;> (try simp only [decide_eq_true_eq] at *) <;> tauto)))

/-- Closes `∀ x, S x → c x` (or `¬ c x`) for a condition `S` decides. -/
macro "emath_side" : tactic =>
  `(tactic| (intro x hx; (try simp only [decide_eq_true_eq] at hx ⊢); first | tauto | (simp_all; done)))

/-- Bounds `fun x => body` as a rational function of the parameters `x`
(`Bnd`), by the rules above; `defs` are the user functions it reaches. -/
syntax "emath_bound" "[" ident,* "]" : tactic
macro_rules
  | `(tactic| emath_bound [$defs,*]) => `(tactic|
      repeat' ((try beta_reduce); first
        | assumption
        | with_reducible exact Bnd.const _
        | with_reducible exact BndL.const _
        | with_reducible exact Bnd.var _
        | ((with_reducible apply Bnd.fold); intro _ _ _ _)
        | ((with_reducible apply BndL.fold); intro _ _ _ _)
        | with_reducible apply Bnd.index
        | ((with_reducible apply BndL.tabulate); intro _)
        | with_reducible apply BndL.cons
        | with_reducible exact BndL.nil
        | with_reducible apply Bnd.add
        | with_reducible apply Bnd.sub
        | with_reducible apply Bnd.neg
        | with_reducible apply Bnd.mul
        | ((with_reducible apply Bnd.div); emath_nonzero)
        | with_reducible apply Bnd.rat_pow
        | with_reducible apply Bnd.ite
        | ((with_reducible apply Bnd.ite_pos); emath_side)
        | ((with_reducible apply Bnd.ite_neg); emath_side)
        | dsimp only
        | emath_unfold [$defs,*]))

open Lean Meta Elab Tactic in
/-- Proves `kcheck ... = true` by one kernel evaluation. When that fails
it asks the kernel whether the sides differ at the shared point (which
the hypotheses allow), so a failure either refutes the law or says the
evaluation did not finish; the two must not be confused. -/
elab "emath_kcheck" : tactic => withMainContext do
  let saved ← saveState
  try
    evalTactic (← `(tactic| decide +kernel))
  catch e =>
    saved.restore
    let goal ← instantiateMVars (← getMainTarget)
    let some (_, lhs, _) := goal.eq? | throw e
    unless lhs.isAppOfArity ``kcheck 9 do throw e
    let differ ← mkEq (mkAppN (mkConst ``kdiffer) lhs.getAppArgs) (mkConst ``Bool.true)
    let refuted ← try
        let mvar ← mkFreshExprMVar differ
        let rest ← Tactic.run mvar.mvarId! (evalTactic (← `(tactic| decide +kernel)))
        pure rest.isEmpty
      catch _ => pure false
    if refuted then
      throwError "emath_kcheck: the two sides differ at a parameter point the hypotheses allow"
    throwError "emath_kcheck: Lean could not evaluate the two sides at the evaluation point"

end Emath.Law
