# Writing programs

This page shows how to write an emath program: functions, numbers,
branches, recursion, lists, objects, imports, and tests. Every program
here lives in [`docs/guide/examples/programs/`](examples/programs/),
passes `emath test`, and exports to Lean with nothing left out.

To run and test what you write, see
[running-and-testing.md](running-and-testing.md).

## The shape of a file

A file holds declarations. Most are `emath function`. Each one has
named sections:

```emath
emath function gcd:
    # Euclid's algorithm. A function may call itself.
    inputs:
        a: Int
        b: Int
    outputs:
        result: Int
    definitions:
        result = if b == 0: a else: gcd(b, int_rem(a, b))
    tests:
        example <twelve_eighteen>:
            given a = 12
            given b = 18
            expect result == 6
```

| Section | What it holds |
| --- | --- |
| `inputs:` | Named, typed inputs. May be empty |
| `outputs:` | Named, typed results. At least one |
| `definitions:` | `name = expression` rows. Every output needs one. Other names are local helpers |
| `tests:` | `example <label>:` blocks with `given` and `expect` rows |

Indentation is four spaces. A `#` starts a comment. The comment right
under a function's head is its summary in `emath api`.

At the top of a file:

- `use module.path` imports a module (see [Imports](#imports)).
- `exports: a, b, c` lists the names other files may `use`. A file
  without `exports:` keeps everything private to itself.

## Numbers

| Type | Values | Notes |
| --- | --- | --- |
| `Int` | Exact integers of any size | `int_quot(a, b)` and `int_rem(a, b)` divide with floor rounding: `int_quot(-7, 2)` is `-4`, `int_rem(-7, 2)` is `1` |
| `Rat` | Exact fractions | `/` on two `Int` values gives a `Rat`, never a rounded number. `r.numer` and `r.denom` give its parts |
| `Float64` | Machine floats | Use only where rounding is fine. Floats do not export to Lean |
| `Bool` | `true`, `false` | `and`, `or`, `not` |

Comparisons are `== != < <= > >=`. There is no `Real` type: choose
`Rat` for exact work or `Float64` for fast approximate work.

The `^` operator parses but does not compute. For powers, use
`rat_pow(base, n)` from `analysis.powers` or `int_pow(base, n)`.

From [`basics.emath`](examples/programs/basics.emath):

```emath
emath function mean:
    # `/` on two Int values gives an exact Rat, never a rounded number.
    inputs:
        xs: sequence(Int)
    outputs:
        result: Rat
    definitions:
        n = length(xs)
        result = if n == 0: constructor_refuse(quote(empty_list)) else: int_sum(xs) / n
    tests:
        example <three>:
            given xs = [1, 2, 4]
            expect result == 7 / 3
            expect result.numer == 7
            expect result.denom == 3
```

## Definitions and branches

Definitions run top to bottom. Define a name before the rows that use
it, or `emath check` refuses with `E-TYPE-002` ("unbound").

`if c: a else: b` is an expression. It fits on one line and can be
nested.

```emath
    definitions:
        capped = if x > high: high else: x
        result = if x < low: low else: capped
```

There are no loops and no reassignment. Repeat work with recursion or
with `fold` (see [Lists](#lists)).

## Recursion

A function may call itself or another function in the same file. Make
sure every call gets closer to a base case. A runaway recursion stops
with `budget_exhausted`, not a crash.

```emath
emath function harmonic:
    # 1 + 1/2 + ... + 1/n, exactly.
    inputs:
        n: Int
    outputs:
        result: Rat
    definitions:
        result = if n <= 0: 0 else: 1 / n + harmonic(n - 1)
```

Calls that still have work to do after they return, like
`1 / n + harmonic(n - 1)`, may nest at most 256 deep. Past that, emath
stops with `recursion_depth_exceeded`. To go deeper, carry the running
result in an input so that the call is the whole result, or use `fold`:

```emath
emath function sum_to:
    # 1 + 2 + ... + n, carrying the running total in `acc`.
    inputs:
        n: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 0: acc else: sum_to(n - 1, acc + n)
```

`sum_to(1000, 0)` is `500500`.

## Several outputs

A function may have several outputs. The caller gets a record and
reads each output as a field.

```emath
emath function divide:
    inputs:
        a: Int
        b: Int
    outputs:
        q: Int
        r: Int
    definitions:
        q = int_quot(a, b)
        r = a - b * q

emath function digit_sum:
    # Calls a function with several outputs and reads its fields.
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        d = divide(n, 10)
        result = if n < 10: n else: d.r + digit_sum(d.q)
```

`emath run` prints such a result as `divide(q=3, r=2)`.

## Refusing

When an input has no answer, refuse by name instead of returning a
made-up value:

```emath
result = if n == 0: constructor_refuse(quote(empty_list)) else: int_sum(xs) / n
```

Only the branch that runs matters. Here the refusal fires only for an
empty list. `emath run` then stops with `error: empty_list: ...` and
exit code 4. emath refuses on its own too: `division_by_zero`,
`invalid_index` (reading past the end of a list), and
`budget_exhausted`.

A test can demand a refusal. See
[`refusal.emath`](examples/refusal.emath):

```emath
        example <empty>:
            given xs = []
            expect diagnostic.code == empty_list
```

Such a test has no value for Lean to prove, so `emath export lean`
lists it as `not translated`. Keep refusal tests in files you do not
export, as the guide does.

## Lists

`sequence(T)` is a list of `T`. Build one with `[1, 2, 3]`; `[]` is
empty. `xs[i]` reads an item (from 0), `length(xs)` counts them, and
`[x, ..xs]` puts `x` in front. `concat(a, b)` joins two lists.

The module `fold.reduce` holds the list tools. Import it with
`use fold.reduce`.

| Tool | Result |
| --- | --- |
| `range(start, count)` | `start, start + 1, ..., start + count - 1` (a start and a count, not an end) |
| `tabulate(f, n)` | `f(0), f(1), ..., f(n - 1)` |
| `map(f, xs)` | `f` applied to each item |
| `filter(p, xs)` | The items where `p` is true |
| `fold(step, start, xs)` | Carries a value through the list: `step(...step(step(start, x0), x1)..., xn)` |
| `sum(xs)`, `product(xs)` | Of an `Int` list. `int_sum(xs)` needs no import |
| `any(p, xs)`, `all(p, xs)`, `reverse(xs)`, `take(xs, n)` | As named |

`function x in T: body` is an anonymous function. It may use names
from around it. `fold` takes a function of two arguments, written as
one function returning another:

```emath
use fold.reduce
use basics.{gcd}

emath function gcd_all:
    inputs:
        xs: sequence(Int)
    outputs:
        result: Int
    definitions:
        result = fold(function g in Int: function x in Int: gcd(g, x), 0, xs)
```

More in [`lists.emath`](examples/programs/lists.emath).

## Objects

An object is a record with typed fields. Build one with
`Name: {field: value}` and read a field with `value.field`. Objects can
hold other objects. `invariants:` rows are checked every time the
object is built; a value that breaks one is refused
(`object_invariant_failed`).

```emath
emath object Point:
    representation:
        x: Rat
        y: Rat

emath object Box:
    representation:
        low: Point
        high: Point
    invariants:
        ordered = low.x <= high.x and low.y <= high.y

emath function far_points:
    inputs:
        points: sequence(Point)
        r: Rat
    outputs:
        result: sequence(Rat)
    definitions:
        r2 = r * r
        far = filter(function p in Point: p.x * p.x + p.y * p.y > r2, points)
        result = map(function p in Point: p.x, far)
    tests:
        example <outside_unit>:
            given points = [Point: {x: 1 / 2, y: 0}, Point: {x: 2, y: 1}, Point: {x: 0, y: 1}, Point: {x: -3, y: 0}]
            given r = 1
            expect result == [2, -3]
```

Two objects are equal when all their fields are equal:
`expect result.high == Point: {x: 4, y: 5}` works. More in
[`objects.emath`](examples/programs/objects.emath).

`emath run --set` cannot build an object. Give object inputs in a
test, or write a small function that builds them from numbers.

## Imports

| Form | Meaning |
| --- | --- |
| `use fold.reduce` | Every exported name of the library module `fold/reduce.emath` |
| `use analysis.powers.{rat_pow}` | Only `rat_pow` |
| `use analysis.powers.{rat_pow as power}` | `rat_pow` under the name `power` |
| `use basics` | A file `basics.emath` in the same folder as this one |

A module name is looked up in the `language/` library first, then
next to the importing file. A name a module does not export is refused
with `E-NAME-025`, and the message lists what it does export.

To find a library function, search by what it does:

```console
$ emath api --search "rational power"
...
use analysis.powers: rat_pow(p: Rat, n: Int) -> result: Rat
  Rational base to an integer exponent. n >= 0 is the direct product; n < 0 inverts the base first ...
```

Each hit shows the `use` line to write, the signature, and the summary.

## Tests

Every `example` sets the inputs with `given` and checks results with
`expect`. An `expect` row is any `Bool` expression over the outputs:

```emath
    tests:
        example <four>:
            given n = 4
            expect result == [1, 4, 9, 16]
            expect length(result) == 4
            expect result[3] == 16
```

- Write at least one example per function. They are the cheapest check
  you have, and `emath export lean` turns each one into a Lean theorem.
- Give each example a label that says what it checks.
- `expect diagnostic.code == name` demands a refusal (see
  [Refusing](#refusing)).

## Common mistakes

| Symptom | Cause | Fix |
| --- | --- | --- |
| `E-TYPE-002 ... unbound `x`` | `x` is used before its definition, or not defined | Move its row up, or define it |
| `E-TYPE-012 ... declared Int, got Rat` | `/` made a `Rat` where an `Int` is declared | Use `int_quot(a, b)`, or declare `Rat` |
| `E-SYN-153 expected the right-hand side ...` | A line ends with an operator | Finish the expression on one line, or wrap it in `( )` |
| `implementation_unavailable: operator Pow` | `^` does not compute | Use `rat_pow` or `int_pow` |
| A list is one item too long or too short | `range(start, count)` takes a count | `range(1, n)` is `1, ..., n` |
| `E-NAME-025` | The module does not export that name | Read the names the message lists |
| `unbound `p.x`` when running with `--set` | `--set` cannot build an object | Pass objects through a test or a builder function |
| `recursion_depth_exceeded` | More than 256 nested calls that still have work to do | Carry the result in an input, or use `fold` |

`emath explain CODE` describes any error code.
