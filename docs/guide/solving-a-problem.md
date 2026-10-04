# Solving a math problem with emath

This page goes through one problem from start to finish: model it,
compute small cases, guess a formula, prove the guess in emath, and
then have Lean prove it too. Every command and output below is real.
The files are in [`docs/guide/examples/problem/`](examples/problem/).

The same steps work for any question of the form "find a formula for
this sequence" or "show these two computations always agree".

## The problem

Find a closed form for

$$S(n) = 0 \cdot 2^0 + 1 \cdot 2^1 + 2 \cdot 2^2 + \dots + (n-1) \cdot 2^{n-1}.$$

## Step 1: write the definition as a program

Write $S$ exactly as defined, not as you hope it simplifies.
[`docs/guide/examples/problem/weighted.emath`](examples/problem/weighted.emath):

```emath
# Problem: find a closed form for S(n) = 0*2^0 + 1*2^1 + ... + (n-1)*2^(n-1).
use fold.reduce
use analysis.powers.{rat_pow}

exports: weighted

emath function weighted:
    # S(n), summed by a loop over k = 0, 1, ..., n - 1
    inputs:
        n: Int
    outputs:
        result: Rat
    definitions:
        result = fold(function acc in Rat: function k in Int: acc + k * rat_pow(2, k), 0 / 1, range(0, n))
    tests:
        example <none>:
            given n = 0
            expect result == 0
        example <four>:
            given n = 4
            expect result == 34

emath function table:
    # S(0), S(1), ..., S(count - 1)
    inputs:
        count: Int
    outputs:
        result: sequence(Rat)
    definitions:
        result = tabulate(function n in Int: weighted(n), count)
```

Notes:

- `fold(step, start, range(0, n))` is a loop: it starts from `start`
  and applies `step(acc, k)` for `k = 0, 1, ..., n - 1`.
- `use fold.reduce` brings in `fold`, `range`, `tabulate` and `sum`.
  `use analysis.powers.{rat_pow}` brings in powers. To find a function
  you need, run `emath api --search "words"`.
- `exports: weighted` lets other files `use` this one and call
  `weighted`. Without an `exports:` line, a file exports nothing.
- The examples are values worked out by hand:
  $S(4) = 0 + 2 + 8 + 24 = 34$. They catch a wrong definition before
  you spend time on it.
- Powers use `rat_pow`, so the sum is a `Rat`. With `Int` throughout,
  `int_pow(2, k)` works the same way.

Check it:

```console
$ emath check docs/guide/examples/problem/weighted.emath
$ emath test docs/guide/examples/problem/weighted.emath
ok none --
ok four --
2 authored tests passed
```

`emath check` prints nothing when the file is fine.

## Step 2: compute small cases

```console
$ emath run docs/guide/examples/problem/weighted.emath --function table --set count=10
[0/1, 0/1, 2/1, 10/1, 34/1, 98/1, 258/1, 642/1, 1538/1, 3586/1]
```

Values print as exact fractions, so `34/1` is 34. One value:

```console
$ emath run docs/guide/examples/problem/weighted.emath --function weighted --set n=5
98/1
```

## Step 3: guess a formula

The values are 0, 0, 2, 10, 34, 98, 258, 642, 1538, 3586. Subtract 2:
−2, −2, 0, 8, 32, 96, 256, 640, 1536, 3584. Divide by $2^n$:
−2, −1, 0, 1, 2, 3, 4, 5, 6, 7. That is $n - 2$. So the guess is

$$S(n) = (n-2)\,2^n + 2.$$

A table is evidence, not a proof. The next step turns it into one.

## Step 4: state the guess as a law and prove it in emath

[`docs/guide/examples/problem/closed_form.emath`](examples/problem/closed_form.emath):

```emath
# The guess, stated as a law: S(n) = (n - 2) 2^n + 2 for every n >= 0.
use weighted
use search.certify
use analysis.powers.{rat_pow}

emath function laws:
    inputs:
        limit: Int
    outputs:
        result: sequence(Certificate)
    definitions:
        result = [certify_equal(quote(function n in Int: weighted(n)), quote(function n in Int: (n - 2) * rat_pow(2, n) + 2), limit)]
```

```console
$ emath run docs/guide/examples/problem/closed_form.emath --function laws --set limit=100000
[Certificate(exhausted=false, points=5, proven=true, start=0)]
```

`proven=true` is a proof for every $n \ge 0$, not a test of a few
values. emath reads both programs and finds that each satisfies a
linear recurrence of bounded order: the loop adds $k \cdot 2^k$, and
the formula is a polynomial times $2^n$. The difference of the two
sides then satisfies a recurrence of order at most 5. If such a
sequence is 0 at five consecutive points, it is 0 everywhere. emath
checks those 5 points (`points=5`).

`limit` is the work budget for evaluating each side at each point. If
a law comes back `proven=false` with `exhausted=true`, a side ran out
of that budget: raise `limit` and run it again.

A wrong guess, $(n-1)\,2^n + 2$, comes back not proven, with
`exhausted=false`: the sides were computed and they differ.

```
[Certificate(exhausted=false, points=5, proven=false, start=0)]
```

`proven=false` with `points=0` means something else. emath could not
bound one of the sides, so it has no proof either way. Rewrite the
sides into a shape it can read (see
[Limits in the Lean page](lean.md#limits)), or split the law into
smaller laws.

## Step 5: have Lean prove it too

```console
$ emath export lean docs/guide/examples/problem --check
exported 2 source files to /.../docs/guide/examples/problem/lean: 2 functions, 2 examples, 13 library examples, 1 laws
lean: accepted (2 examples, 13 library examples and 1 laws proved by the kernel)
```

The Lean kernel has now proved, from the definitions:

- the two examples, $S(0) = 0$ and $S(4) = 34$;
- the law $S(n) = (n-2)\,2^n + 2$ for every natural number $n$.

Here is the law as Lean states it:

```lean
theorem laws.law_1 : ∀ n₀ : ℕ,
    (fun (n : Int) => Export.weighted.weighted n) (n₀ : Int) =
      (fun (n : Int) => (n - 2) * Emath.rat_pow 2 n + 2) (n₀ : Int) :=
  Emath.Law.eq_of_order
    (by emath_order [Export.weighted.weighted])
    (by emath_order [])
    (by emath_check 0)
```

With the wrong guess, Lean rejects it and says where the sides differ
(exit 1):

```
lean: rejected
rejected: guess.emath law 1 of `laws`: the two sides differ at n = 0
```

Setup, every flag, and every message are on the
[Lean page](lean.md).

## The recipe

1. **Model.** Write the definition literally as an `emath function`.
   Add examples you can check by hand. Run `emath check` and
   `emath test`.
2. **Compute.** Use a `tabulate` helper and `emath run --function` to
   print small cases.
3. **Guess.** Look for a pattern: subtract, divide, take differences.
   (To have emath search for formulas, see `search.synthesis` in
   `language/DISCOVERY.md` in your emath install.)
4. **Prove in emath.** State the guess with `certify_equal` (or
   `certify_all` for laws with parameters) and run it. `proven=true`
   is a proof.
5. **Prove in Lean.** Run `emath export lean <folder> --check`.
   `lean: accepted` means the Lean kernel proved the same claims
   independently.
6. **When it fails:**
   - `proven=false` with `exhausted=true`: `limit` is too small.
   - `proven=false` with `points > 0` and `exhausted=false`: the guess
     is wrong.
   - `proven=false` with `points=0`: the shape is not supported.
     Rewrite the sides.
   - Lean's `rejected` gives a counterexample. `unproven` gives a
     reason.

Keep the problem's definition and the law in separate files, as here.
The definition file has examples. The law file has none, because a
function that states laws is exported as the laws.
