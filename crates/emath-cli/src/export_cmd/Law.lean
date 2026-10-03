/-
Written by `emath export lean` when a project certifies laws; the next
such export overwrites it, and an export without laws leaves it unbuilt.

The Lean side of emath's `search.certify`: two sequences that satisfy
linear recurrences of orders `a` and `b` and agree at `a + b` points
agree everywhere (`eq_of_order`). `emath_order` bounds a sequence's
order by a subset of the shape rules certify uses, each rule a theorem
here: constants, `n`, powers, sums, scaling, products of order-one
sequences, running sums (`fold` with `acc + h(t)`) and linear readouts
of a linear loop over lists (Cayley–Hamilton). `emath_check` checks
the points and, when they disagree, names one. Imports are the few
Mathlib modules these need, so a check loads little of Mathlib.
-/
import Mathlib.LinearAlgebra.Charpoly.Basic
import Mathlib.Algebra.Polynomial.AlgebraMap
import Mathlib.Tactic.FinCases
import Mathlib.Tactic.Ring
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Linarith
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

theorem HasOrder.step {u : ℕ → ℚ} (h : HasOrder 1 u) : ∃ c : ℚ, ∀ n, u (n + 1) = c * u n := by
  obtain ⟨p, hp, hpd, hpa⟩ := h
  rcases Nat.le_one_iff_eq_zero_or_eq_one.mp hpd with h0 | h1
  · have : p = 1 := eq_one_of_monic_natDegree_zero hp h0
    subst this
    have hu : u = 0 := by simpa [Ann] using hpa
    exact ⟨0, fun n => by simp [hu]⟩
  · refine ⟨-p.coeff 0, fun n => ?_⟩
    have key := congrFun hpa n
    rw [aeval_apply, h1] at key
    have hlead : p.coeff 1 = 1 := by
      have := hp.coeff_natDegree; rwa [h1] at this
    simp [Finset.sum_range_succ, hlead] at key
    linarith

theorem HasOrder.const (c : ℚ) : HasOrder 1 (fun _ => c) :=
  HasOrder.of_step 1 (fun n => by simp)

theorem HasOrder.geom (c : ℚ) : HasOrder 1 (fun n => c ^ n) :=
  HasOrder.of_step c (fun n => by ring)

theorem HasOrder.rat_pow (c : ℚ) : HasOrder 1 (fun n : ℕ => Emath.rat_pow c (n : ℤ)) := by
  have : (fun n : ℕ => Emath.rat_pow c (n : ℤ)) = (fun n => c ^ n) := by
    funext n; simp [Emath.rat_pow]
  rw [this]; exact HasOrder.geom c

/-- The termwise product of two order-one sequences. -/
theorem HasOrder.mul_one {u v : ℕ → ℚ} (hu : HasOrder 1 u) (hv : HasOrder 1 v) :
    HasOrder 1 (fun n => u n * v n) := by
  obtain ⟨c, hc⟩ := hu.step
  obtain ⟨d, hd⟩ := hv.step
  exact HasOrder.of_step (c * d) (fun n => by rw [hc, hd]; ring)

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

/-- A sequence of order `k` keeps that order shifted by `s`. -/
theorem HasOrder.shift_by {k : ℕ} {u : ℕ → ℚ} (hu : HasOrder k u) (s : ℕ) :
    HasOrder k (fun n => u (s + n)) := by
  obtain ⟨p, hp, hpd, hpa⟩ := hu
  refine ⟨p, hp, hpd, ?_⟩
  have hcomm : aeval shift p * shift ^ s = shift ^ s * aeval shift p := by
    calc aeval shift p * shift ^ s = aeval shift (p * X ^ s) := by rw [map_mul, map_pow, aeval_X]
      _ = aeval shift (X ^ s * p) := by rw [mul_comm]
      _ = shift ^ s * aeval shift p := by rw [map_mul, map_pow, aeval_X]
  have : (fun n => u (s + n)) = (shift ^ s) u := by
    funext n; rw [shift_pow_apply, Nat.add_comm]
  unfold Ann at *
  rw [this, ← Module.End.mul_apply, hcomm, Module.End.mul_apply, hpa, map_zero]

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
        | (with_reducible apply HasOrder.fold_acc); (intro acc t; beta_reduce; ring)
        | emath_readout; all_goals (first | decide | emath_linear [] | emath_linear [$defs,*])
        | (with_reducible apply HasOrder.if_true); decide +kernel
        | (with_reducible apply HasOrder.if_false); decide +kernel
        | with_reducible apply HasOrder.add
        | with_reducible apply HasOrder.sub
        | with_reducible apply HasOrder.const_mul
        | with_reducible apply HasOrder.mul_const
        | with_reducible apply HasOrder.div_const
        | with_reducible apply HasOrder.neg
        | with_reducible apply HasOrder.mul_one
        | emath_unfold [$defs,*]))

end Emath.Law
