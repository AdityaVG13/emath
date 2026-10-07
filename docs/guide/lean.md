# Certifying results in Lean

`emath export lean` turns your emath functions, their examples, and the
laws you state about them into a Lean 4 project. With `--check`, it
builds that project, so the Lean kernel proves each example and law
independently of emath.

Use it when you want more than "emath computed this". The kernel's
answer does not depend on emath's interpreter being right.

- [Setup](#setup)
- [A first export](#a-first-export)
- [What gets exported](#what-gets-exported)
- [Laws: statements about every n](#laws-statements-about-every-n)
- [Flags](#flags)
- [Reading the result](#reading-the-result)
- [Messages and what to do](#messages-and-what-to-do)
- [The Lean project it writes](#the-lean-project-it-writes)
- [Limits](#limits)
- [How the two checks fit together](#how-the-two-checks-fit-together)

## Setup

You need three things.

1. **emath** on your PATH. Outside the emath repository, set
   `EMATH_ROOT` to the folder that contains `language/`:

   ```console
   $ export EMATH_ROOT=/path/to/emath
   ```

2. **Lean**, installed with [elan](https://github.com/leanprover/elan).
   `--check` runs `lake`, which elan provides. Without `--check`,
   Lean is not needed at all.

3. **Mathlib**, only when you export laws. `--check` reuses compatible
   packages in `~/.cache/emath/lean-mathlib`. If absent, it downloads the
   pinned packages and the prebuilt modules the law library needs there.
   Existing downloaded cache files are reused, including when only extraction
   into the shared packages needs repairing. Every program and iteration uses
   the same packages, not a new Mathlib download.
   To use a copy you already have, set
   `EMATH_LEAN_MATHLIB` to its packages folder (the one containing
   `mathlib/` and its dependency checkouts). This export targets Lean/Mathlib
   **v4.34.1** and pinned dependency revisions. An incompatible installation is
   reported, never silently updated or replaced. emath checks prebuilt
   readiness without rebuilding the Mathlib library. If cache preparation
   fails, it stops and prints the shared setup directory for recovery.

   First-time setup is serialized by a shared lock. Mathlib's cache tool runs
   in `~/.cache/emath/lean-mathlib/.emath-setup-v4.34.1`, whose
   `.lake/packages` points to the shared packages. This keeps dependency
   artifacts out of individual exported projects. The small cache executable
   may be built during first-time preparation; the full library is not
   compiled from source.

## A first export

The example project is
[`docs/guide/examples/sequences/`](examples/sequences/). It has two files:

- [`docs/guide/examples/sequences/sequences.emath`](examples/sequences/sequences.emath):
  four ordinary functions (`fib`, `cubes`, `triangle`, `geometric`),
  each with one example.
- [`docs/guide/examples/sequences/laws.emath`](examples/sequences/laws.emath):
  three laws about them.

A function with an example:

```emath
emath function fib:
    # The n-th Fibonacci number, by a loop on the pair (F(k), F(k + 1))
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        pair = fold(function p in sequence(Int): function k in Int: [p[1], p[0] + p[1]], [0, 1], range(0, n))
        result = pair[0]
    tests:
        example <ten>:
            given n = 10
            expect result == 55
```

First make sure emath itself agrees with the examples:

```console
$ emath test docs/guide/examples/sequences/sequences.emath
ok ten --
ok five --
ok five --
ok halves --
4 authored tests passed
```

Export for review. This writes the Lean files without running Lean:

```console
$ emath export lean docs/guide/examples/sequences
exported 2 source files to /.../docs/guide/examples/sequences/lean: 4 functions, 4 examples, 15 library examples, 3 laws
```

Then check with Lean:

```console
$ emath export lean docs/guide/examples/sequences --check
exported 2 source files to /.../docs/guide/examples/sequences/lean: 4 functions, 4 examples, 15 library examples, 3 laws
lean: accepted (4 examples, 15 library examples and 3 laws proved by the kernel)
```

Exit code 0 means every example and every law was proved. Once Mathlib is
ready, `--check` skips cache fetching. Unchanged export files keep their
modification times, so Lake reuses the project's compiled modules too.
A new output directory still compiles its own translated files. Proof size
and changes determine check time; no constant-time check is promised.

"Library examples" are the examples of the emath library functions your
code uses (here from `fold.reduce` and `analysis.powers`). They are
checked as well, because your theorems rely on their Lean translations.

## What gets exported

**Functions.** Each `emath function` becomes a Lean `def` with the same
inputs and outputs. A function with several outputs becomes a Lean
structure with one field per output.

| emath | Lean |
| --- | --- |
| `Int`, `Nat` | `Int` |
| `Rat` | `Rat` (exact rationals) |
| `Bool` | `Bool` |
| `sequence(T)` | `List T` |
| an `emath object` with fields | a `structure` |
| `function x in T: ...` | `fun (x : T) => ...` |
| `fold`, `range`, `tabulate`, `sum`, `product`, `length` | the same names in `Emath.Prelude` |
| `int_quot`, `int_rem`, `int_pow`, `rat_pow` | the same names in `Emath.Prelude` |

Functions from library modules you `use` are translated too, into
`Export/lib/`. Recursive functions translate. Generic functions
translate when Lean can type them.

**Examples.** Each `example` becomes a theorem. Here is `fib`'s:

```lean
theorem fib.ten : (
    let n : Int := (10 : Int)
    let result := fib n
    decide (result = 55)) = true := by
  decide +kernel
```

`decide +kernel` makes the Lean kernel recompute `fib 10` from the
definition and compare it with 55. No emath code is trusted.

emath runs each example before exporting it. An example that faults in
emath (dividing by zero, reading past the end of a list) is not
exported. Lean's arithmetic would accept it: in Lean, `x / 0 = 0` and
`17 % 0 = 17`. Exporting it would let Lean prove something emath
refuses. The same holds for `constructor_refuse` and for an object
that breaks one of its `invariants:`. Lean gets the object as a plain
structure, with a comment naming the invariants it does not check.

**What does not translate.** Quoted code (`quote`, `Code`), floats and
decimal literals, text, sets, records, tuples, quantities with units,
`emath query`, buffers, and `recur` literals. Each one is reported
with its reason. Everything else in the file still exports. A function
that calls an untranslated function is not translated either, and the
message names the function it calls.

## Laws: statements about every n

Examples check single values. Laws state that two sequences agree for
every `n`. You state laws with the `search.certify` module:

```emath
use sequences
use search.certify
use analysis.powers.{rat_pow}

emath function laws:
    inputs:
        limit: Int
    outputs:
        result: sequence(Certificate)
    definitions:
        result = [certify_equal(quote(function n in Int: cubes(n)), quote(function n in Int: triangle(n) * triangle(n)), limit), certify_equal(quote(function n in Int: fib(n + 1) * fib(n + 1) - fib(n) * fib(n + 2)), quote(function n in Int: rat_pow(0 - 1, n)), limit)]
```

The two laws say:

- $0^3 + 1^3 + \dots + (n-1)^3 = T(n)^2$, where $T(n) = 0 + 1 + \dots + (n-1)$.
- $F(n+1)^2 - F(n)\,F(n+2) = (-1)^n$ (Cassini's identity).

### The four calls

| Call | States |
| --- | --- |
| `certify_equal(f, g, limit)` | `f(n) = g(n)` for every `n >= 0` |
| `certify_equal_from(f, g, start, limit)` | `f(n) = g(n)` for every `n >= start` (`start` must be a whole-number literal) |
| `certify_all(f, g, limit)` | the same for every value of the parameters before `n` |
| `certify_all_from(f, g, start, limit)` | both of the above |

Each side is a quoted function of one `Int` named `n`:
`quote(function n in Int: ...)`. For `certify_all`, the sides first take
`Rat` parameters, then `n`:

```emath
emath function geometric_law:
    inputs:
        limit: Int
    outputs:
        result: ParameterCertificate
    definitions:
        result = certify_all(quote(function r in Rat: function n in Int: geometric(r, n)), quote(function r in Rat: function n in Int: (rat_pow(r, n) - 1) / (r - 1)), limit)
```

This states $1 + r + \dots + r^{n-1} = \frac{r^n - 1}{r - 1}$. emath
notices the division by $r - 1$. In Lean, the law becomes a theorem
that assumes `r - 1 ≠ 0`.

### What emath says about a law

Run the law function to see emath's own certificate:

```console
$ emath run docs/guide/examples/sequences/laws.emath --function laws --set limit=100000
[Certificate(exhausted=false, points=5, proven=true, start=0), Certificate(exhausted=false, points=9, proven=true, start=0)]
$ emath run docs/guide/examples/sequences/laws.emath --function geometric_law --set limit=100000
ParameterCertificate(conditions=[], evaluations=7, points=3, proven=true, sides=[], start=0)
```

- `proven=true`: emath proved the law for every `n`. It reads both
  sides, bounds the order of a linear recurrence each one satisfies,
  and checks that they agree on enough starting values (`points`) to
  force agreement everywhere.
- `proven=false` with `points=0`: emath could not bound a side. The law
  may still be true, but emath has no proof of it. See [Limits](#limits).
- `proven=false` with `exhausted=true`: a side did not finish within
  `limit` at some checked point. Raise `limit` and run it again.
- `proven=false` with `points > 0` and `exhausted=false`: the sides
  differ at a checked point. The law is false.

`limit` is the work budget for evaluating each side at each point. It
is not a range of `n`. If it is too small, a true law comes back
`proven=false` with `exhausted=true`. A small `limit` can only lose a
proof; it never makes a false law look proven. `emath export lean`
certifies with its own large budget, so the value you pass only
matters for `emath run`.

[`docs/guide/examples/limit.emath`](examples/limit.emath) shows this
with Cassini's identity:

```emath
emath function cassini:
    # F(n + 1)^2 - F(n) F(n + 2) = (-1)^n, with the given limit
    inputs:
        limit: Int
    outputs:
        result: Certificate
    definitions:
        result = certify_equal(quote(function n in Int: fib(n + 1) * fib(n + 1) - fib(n) * fib(n + 2)), quote(function n in Int: rat_pow(0 - 1, n)), limit)
    tests:
        example <too_small>:
            given limit = 1000
            expect result.proven == false
            expect result.exhausted == true
        example <enough>:
            given limit = 100000
            expect result.proven == true
            expect result.exhausted == false
```

`certify_all` returns a `ParameterCertificate`, which has no
`exhausted` field yet. If one of its laws is unexpectedly not proven,
try a larger `limit`.

### What Lean does with a law

Each law becomes a theorem. Cassini's identity:

```lean
theorem laws.law_2 : ∀ n₀ : ℕ,
    (fun (n : Int) => ((Export.sequences.fib (n + 1) * Export.sequences.fib (n + 1) - Export.sequences.fib n * Export.sequences.fib (n + 2) : Int) : Rat)) (n₀ : Int) =
      (fun (n : Int) => Emath.rat_pow (0 - 1) n) (n₀ : Int) :=
  Emath.Law.eq_of_order
    (by emath_order [Export.sequences.fib])
    (by emath_order [])
    (by emath_check 0)
```

The proof follows the same method as emath, but every step is checked
by Lean. `emath_order` proves that each side satisfies a recurrence of
bounded order, and `emath_check` evaluates both sides at the starting
points. The supporting lemmas live in `Emath/Law.lean`, which is built
on Mathlib.

A function that certifies laws becomes its laws. Its own value (a list
of certificates) is not exported, so give it no `tests:`. An example on
it is reported as `not translated`.

## Flags

```
emath export lean <file.emath|dir> [--out <dir>] [--check] [--work N]
```

| Flag | Effect |
| --- | --- |
| `<file.emath>` | Export that file and the local files it `use`s |
| `<dir>` | Export every `.emath` file under the folder (hidden folders and the output folder are skipped) |
| `--out <dir>`, `-o <dir>` | Write the Lean project there. Default: `lean/` beside the source |
| `--check` | Build the project with `lake build` and report what Lean proved. Without it, only the files are written |
| `--work N` | The emath work budget each example runs under before export, as in `emath test --work`. Raise it when an example is reported as running out of budget |

| Environment variable | Effect |
| --- | --- |
| `EMATH_ROOT` | The folder containing `language/`, when you run outside the emath repository |
| `EMATH_LEAN_MATHLIB` | The prebuilt Mathlib packages to use. Default: `~/.cache/emath/lean-mathlib` |

Each export replaces changed generated files and leaves identical files
untouched. Do not edit them by hand.
If a shared dependency changes while multiple source files are being checked,
the export refuses before writing. Retry after the source edits are complete.

## Reading the result

The first line always reports what was written:

```
exported 2 source files to <dir>: 4 functions, 4 examples, 15 library examples, 3 laws
```

The next lines list anything left out, one per line, starting with
`not translated:`. With `--check`, a verdict follows:

| Verdict | Meaning | Exit |
| --- | --- | --- |
| `lean: accepted (...)` | Lean proved every exported example and law | 0, or 3 if something was not translated |
| `lean: not proven` | Nothing was refuted, but some claims were not proved. Each is listed after `unproven:` | 3 |
| `lean: rejected` | Lean showed at least one claim is false. Each is listed after `rejected:` | 1 |
| `lean: failed outside the translated examples and laws` | The build failed somewhere else. The Lean log follows | 1 |
| `lean: not checked: ...` | Lean could not run (no `lake`, or Mathlib unavailable) | 3 |

Without `--check`, the exit code is 0 when everything translated and 3
when something did not. Exit 2 means a file did not pass `emath check`,
and nothing was exported. Exit 4 means a file could not be written.

A rejected law names where the sides differ. Here the guess is
$(n-1)2^n + 2$ instead of $(n-2)2^n + 2$:

```
lean: rejected
rejected: guess.emath law 1 of `laws`: the two sides differ at n = 0
```

## Messages and what to do

| Message | Meaning | What to do |
| --- | --- | --- |
| `does not admit: E-...; run emath check on it` | The file has an emath error. Nothing is exported | Run `emath check` on the file and fix it |
| `not translated: ... : X has no Lean translation` | A type or construct Lean cannot take (see [What gets exported](#what-gets-exported)) | Rewrite with `Int`, `Rat`, `Bool`, sequences, objects, or move it to a file you don't export |
| `not translated: ... : calls F, which is not translated` | It depends on a function that was left out | Fix `F` first |
| `not translated: ... : it faults in emath (code)` | The example faults in emath, so it has no value to prove | Fix the example or the function |
| `not translated: ... : it runs out of emath's work budget` | The example needs more work than the default budget | Add `--work N` (for example `--work 100000000`) |
| `not translated: ... example of F: a function that certifies laws becomes the laws` | An example on a law function | Remove the example. The laws themselves are checked |
| `unproven: ... : Lean found no order bound for a side` | The law's shape is outside what the law library proves | See [Limits](#limits). This does not mean the law is false |
| `unproven: ... : Lean ran out of time or recursion depth before deciding it` | The kernel gave up on a big computation | Use a smaller example. Very large values (around a million loop steps) are beyond the kernel |
| `unproven: ... : Lean proves it, but emath does not certify it` | emath found no bound for a side, or a side faults in emath (such as a division by 0) where Lean's total arithmetic does not | Treat it as unproven. Run the law in emath: `points=0` means no bound, so rewrite the side; otherwise look for a division by something that can be 0 |
| `unproven: ... : emath does not certify it, and Lean did not check it` | Another file it imports failed to build | Fix the failure reported for that file |
| `rejected: ... example <x> of F` | The kernel computed a different value than the example expects | The example, or the Lean translation, is wrong. Compare with `emath test` |
| `rejected: ... law k of F: the two sides differ at n = m` | The law is false at `n = m` | Fix the law |
| `rejected: ... law k of F: the two sides differ at a parameter point the hypotheses allow` | A `certify_all` law is false for some parameter values | Fix the law |
| `lean: not checked: cannot run lake` | Lean is not installed | Install Lean with elan |
| `lean: not checked: Mathlib in ... is not prebuilt` | The Mathlib cache could not be fetched | Run `lake exe cache get` in the printed folder, or set `EMATH_LEAN_MATHLIB` |
| `lean: not checked: Mathlib ... needs ...` or `... is not at the required revision ...` | The selected installation is incompatible | Use matching v4.34.1 packages; the existing checkout is left unchanged |

## The Lean project it writes

```
lean/
  lakefile.lean, lean-toolchain, lake-manifest.json
  Emath/Prelude.lean    Lean versions of emath's built-in functions
  Emath/Checks.lean     the library functions' own examples, as theorems
  Emath/Law.lean        the law library (only when there are laws)
  Export/<file>.lean    one file per source file, same relative path
  Export/lib/...        translated library modules you use
```

It is an ordinary Lake project. You can open it in VS Code with the
Lean extension, or run `lake build` in it yourself. Projects without
laws do not need Mathlib. Projects with laws point at the shared
Mathlib through `lake-manifest.json`.

## Limits

**Which laws can be proved.** Both emath and Lean prove laws by
bounding recurrence orders. A side can contain:

- polynomials in `n`;
- `rat_pow(c, n)` with `c` not depending on `n`, and `rat_pow(c, n + j)`;
- `int_pow(c, n)` and `int_pow(c, n + j)` with `j >= 0` (a negative
  exponent faults in emath, so `int_pow(c, n - 1)` has no bound; write
  it with `certify_equal_from` and a start, or use `rat_pow`);
- `sum(tabulate(function i in Int: e, n))`;
- `fold` over `range(lo, n)` (or `range(lo, n + j)`), where the step is
  linear in the loop state. This covers running sums, affine updates
  like `x -> 2x + 1`, Fibonacci-style pairs, and matrix powers;
- reading entries of such a loop;
- branches comparing `n` with a fixed number;
- `int_quot(p, d)` and `int_rem(p, d)` with `p` a polynomial and `d` a
  positive literal (floors and remainders);
- calls to your own functions built from the above, and arguments like
  `n + 1`.

Outside this shape, the law comes back unproven, not false. Examples:
dividing by an expression in `n`, `int_pow(c, n - j)`,
a branch whose condition depends on `n` in any way other than comparing
`n` with a fixed number, and a loop
step that is nonlinear in the state (such as `x -> x * x`).

**Big computations.** The kernel recomputes every example from the
definitions. Around $10^5$ loop steps works. Around $10^6$ runs out of
time, which is reported as unproven after a few minutes.

**One broken function hides its callers.** If a library function you
use does not translate, every function calling it is left out, with the
reason.

**Not exported.** Floats, `emath query`, quoted code, and anything that
needs mutable buffers.

## How the two checks fit together

Lean never claims more than emath does:

- Every exported file must first pass `emath check`.
- An example is exported only if emath can run it.
- A law counts as proved only if both Lean proves it and emath
  certifies it.

So "accepted" means emath and the Lean kernel agree on every exported
claim. emath reached its answer by running the code. Lean reached the
same answer from its own kernel, without trusting emath.
