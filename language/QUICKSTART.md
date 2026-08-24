# emath Quick Start

> Zero to working program in 60 seconds.

## 1. Read this program (10 seconds)

```emath
emath function Square:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = x * x

    tests:
        example <three_squared>:
            given x = 3
            expect y == 9
```

That's it. Every emath program is an `emath function` (or `emath model`
for simulations) with sections: `inputs`, `outputs`, `definitions`,
`tests`. You write math. It computes.

## 2. Run it (10 seconds)

```bash
emath check examples/intro/hello-square.emath    # does it make sense?
emath test examples/intro/hello-square.emath      # run the tests
emath run examples/intro/hello-square.emath       # evaluate definitions
```

`check` validates types and shapes. `test` runs examples. `run`
evaluates and prints outputs.

## 3. The shape of the language (20 seconds)

**Types:** `Float64`, `Int`, `Bool`, `Complex`, `GF<p>`, `Vector[n]`,
`Matrix[r, c]`, `Tensor[...]`.

**Expressions:** arithmetic (`+ - * /`), binders (`sum i in 0..n: f(i)`),
derivatives (`derivative(y) wrt x`), root-finding (`solve(f) wrt x`),
optimization (`minimize(loss) wrt x`), comparisons, logic.

**Builtins:** `sin`, `cos`, `exp`, `abs`, `mod`, `min`, `max`, `dot`,
`norm`, `transpose`, `einsum`, `factorial`, `mod_inv`, `congruence`,
`rs_encode`, `poly_eval_mod`, and more. See [NAMING.md](NAMING.md) for
the full list grouped by namespace.

**Sections in a function:**

| Section | What it does |
|---------|-------------|
| `inputs` | declare parameters and their types |
| `outputs` | declare results |
| `state` | mutable variables (for models) |
| `definitions` | where you write equations |
| `equations` | implicit equations (DAEs) |
| `constraints` | inequality constraints |
| `tests` | examples with `given`/`expect` |
| `compile` | Rust codegen settings |

## 4. Write your own (20 seconds)

```emath
emath function Hypotenuse:
    inputs:
        a: Float64
        b: Float64

    outputs:
        c: Float64

    definitions:
        c = sqrt(a * a + b * b)

    tests:
        example <three_four_five>:
            given a = 3
            given b = 4
            expect c == 5
```

Save as `my_first.emath`, run `emath test my_first.emath`.

## 5. Where to go next

| Want to | Read |
|---------|------|
| See what works today | [CAPABILITY.md](CAPABILITY.md) |
| Learn all the names and namespaces | [NAMING.md](NAMING.md) |
| Read complete examples | [examples/intro/](examples/intro/) |
| Understand the full language | [reference/overview.md](reference/overview.md) |
| Simulate ODEs/DAEs | [examples/intro/algebraic-dae.emath](examples/intro/algebraic-dae.emath) |
| Do linear algebra | [examples/intro/tensor-face.emath](examples/intro/tensor-face.emath) |
| Work with finite fields | [examples/intro/modular-arithmetic.emath](examples/intro/modular-arithmetic.emath) |
| Add a new language feature | [MAINTAINING.md](MAINTAINING.md) |
