# emath Naming Conventions

> How things are named in emath, why, and what to type.

## Types

| Canonical name | Removed aliases | Notes |
|----------------|-----------------|-------|
| `Float64` | ~~`Real`~~, ~~`f64`~~, ~~`float64`~~ | The only floating-point type. |
| `Bool` | - | |
| `Nat` | - | Non-negative integers. |
| `Int` | - | Signed integers. Exact i64 output. |
| `Complex` | - | `re + im*i`. Literals: `2i`, `3.5i`. |
| `GF<p>` | ~~`Mod<p>`~~ | Galois field / integers mod p. Maps to `Int`. |
| `Vector[n]` | - | |
| `Matrix[r, c]` | - | |
| `Tensor[...]` | - | |

`Real` still appears in `representation Real` (compile section numeric
profile) - that is a different system, not a type annotation.

## Builtin functions

### Namespaces

Builtins can be called with or without a namespace prefix. Both forms
are equivalent:

```emath
y = sin(x)           (* bare form *)
y = math::sin(x)     (* namespaced form *)
```

| Namespace | Covers | Examples |
|-----------|--------|---------|
| `math::` | Scalar math, modular arithmetic | `math::exp`, `math::factorial`, `math::congruence` |
| `linalg::` | Linear algebra | `linalg::dot`, `linalg::norm`, `linalg::einsum` |
| `pde::` | PDE operators | `pde::laplacian`, `pde::gradient` |
| `coding::` | Coding theory | `coding::rs_encode`, `coding::poly_eval_mod` |

Bare names are always available. Use namespaced forms when you want to
be explicit about the domain or when disambiguation matters.

### Naming rules

1. **Standard math names stay short**: `sin`, `cos`, `exp`, `ln`,
   `abs`, `mod`, `pow`, `min`, `max`. These are universally recognized.

2. **Multi-word names use snake_case**: `mod_inv`, `poly_eval_mod`,
   `rs_encode`, `is_finite`, `log2`, `log10`. No camelCase, no
   abbreviations beyond standard math.

3. **No ad-hoc abbreviations**: ~~`cong`~~ → `congruence`,
   ~~`len`~~ → `length`. If a name isn't a standard math symbol,
   spell it out.

4. **Domain-specific builtins get a namespace**: coding theory functions
   live under `coding::`, PDE operators under `pde::`. If a future
   builtin doesn't fit an existing namespace, create a new one.

5. **Aliases are removed, not kept**: one canonical name per concept.
   No `len`/`length` duality, no `Mod`/`GF` ambiguity.

### Full builtin list by namespace

**math::** (scalar math + modular arithmetic)
`exp` `ln` `log` `sqrt` `sin` `cos` `tan` `tanh` `sinh` `cosh` `atan`
`atan2` `abs` `floor` `ceil` `round` `sign` `log2` `log10` `cbrt`
`recip` `fract` `pow` `min` `max` `hypot` `mod` `is_finite`
`factorial` `mod_inv` `congruence`

**linalg::** (linear algebra)
`dot` `norm` `transpose` `length` `einsum`

**pde::** (PDE operators)
`laplacian` `laplacian_neumann` `laplacian_2d` `laplacian_2d_neumann`
`laplacian_dirichlet` `gradient` `gradient_2d_x` `gradient_2d_y`

**coding::** (coding theory)
`poly_eval_mod` `rs_encode` `hamming_distance`

**Reduction binders** (not namespaced - they are language constructs)
`sum` `product` `mean`

## Error codes

All error codes use `E-CATEGORY-NNN` format:

| Category | Range | Examples |
|----------|-------|---------|
| `E-TYPE` | 001–099 | `E-TYPE-002` (unknown variable), `E-TYPE-003` (unknown function), `E-TYPE-010` (unsupported type) |
| `E-SEC` | 100–199 | `E-SEC-101` (unknown section) |
| `E-SHAPE` | 001–099 | `E-SHAPE-002` (dimension mismatch), `E-SHAPE-004` (ill-formed shape) |
| `E-NUM` | 001–099 | `E-NUM-001` (unknown model), `E-NUM-004` (bare representation) |
| `E-CTOR` | 001–099 | `E-CTOR-032` (require must be Boolean) |

Rust source code uses descriptive constants (`E_UNKNOWN_FUNCTION`,
`E_UNSUPPORTED_TYPE`) that hold the canonical `E-CATEGORY-NNN` strings.
The user always sees the string form, never the constant name.

## EMIR op names

EMIR op names use kebab-case (`f64-add`, `mod-inv`, `poly-eval-mod`).
They are internal - users never type them. They appear in diagnostics
and debug output only.
