# Chapter 14: Rust Interop and Generation

## Type mapping

The Rust backend maps semantic types according to target profile. Examples:

```text
Bool → bool
Float64 → f64
Nat exact → BigNat provider or bounded representation with proof/check
Result<T,E> → Result<T,E>
record → struct
variant → enum
Tensor<T,S> → generated static type or provider tensor
```

`Real` requires a selected representation; it never maps to `f64` without profile evidence.

## Constructors

Validating constructors become associated functions returning `Result`. Fields remain private when required for invariants. Unsafe unchecked constructors are not generated publicly by default.

## Ownership

The backend chooses owned, borrowed or shared types based on host/API contract. Provider buffers use explicit lifetime/layout contracts. No provider pointer leaks into durable artifacts.

## Errors

Semantic failure variants remain typed. Provider/backend detail may be wrapped without losing stable reason codes.

## Generics

Static dimensions and types map to Rust generics/const generics where practical. Constraints without a Rust type-system representation become private checks or sealed evidence tokens.

## Build scripts and macros

The package workflow is authoritative. Build-script and macro conveniences invoke the same compiler and lock resolution. They cannot create a separate mini-language with different semantics.

## Generated documentation

Public APIs include mathematical definition, assumptions, numeric profile, source reference, evidence level and fallback behavior.

## Naming contract

One name travels through the whole pipeline; every mapping below is
deterministic and pinned by `scripts/validate.sh` (byte-identical
regeneration of the committed generated crate).

| Stage | Rule | Example (`affine_scorer.emath`) |
|---|---|---|
| Source file | snake_case of the program name | `affine_scorer.emath` |
| Declaration | `emath <kind> <Name<Params>>:` | `emath policy AffineScorer:` |
| Package identity | first declaration's leaf name (set by `seal`) | `AffineScorer` |
| Generated crate | identity sanitized for Cargo: ASCII alphanumerics, `-`, `_`, lowercased | `affinescorer` |
| Module | A single declaration emits at the crate root | crate root |
| Rust type | declaration name, verbatim | `pub struct AffineScorer` |
| Error type | fixed name | `ConfigError` |
| Constructor | `public fn new(...)` becomes an associated fn returning `Result<Self, ConfigError>` with every `require` checked | `pub fn new(scale: f64, bias: f64) -> Result<Self, ConfigError>` |
| Exported definition | `exports:` name, verbatim (identifier-escaped when needed) | `pub fn score(&self, x: f64) -> f64` |
| Host method | `host rust: implement <Trait>` `method <name>` maps to the trait method | `fn score(...)` in the `impl` |
| Test fn | `example <name>` → `snake_case(name)` | `#[test] fn score_is_seven()` |
| Test locals | `given`/`expect` names keep their source spelling | `let x = 3.0;` |
| Instance binding | `snake_case(declaration name)` | `let affine_scorer = AffineScorer::new(...)` |

Notes:

- Curated committed examples keep a human-readable kebab-case crate name
  (`examples/generated/affine-scorer`, `name = "affine-scorer"`). They are
  byte-identical to fresh pipeline output in `src/lib.rs`; the artifact's
  `Cargo.toml` is always pipeline-generated from the identity above.
- Semantic-genesis worlds emit the fixed crate `semantic-genesis-worlds`.
- File naming is part of the contract: valid fixtures live under
  `tests/valid/<program>.emath` (`tests/valid/affine_scorer.emath`,
  `tests/valid/square.emath`); invalid fixtures are named by the diagnostic
  they pin.
