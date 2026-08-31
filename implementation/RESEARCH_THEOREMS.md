# Research Theorems T1-T4

Parked formal results, numbered T1-T4 (free-world existence, universal
evaluation, parametric compilation, constraint/resolution
monotonicity). Overlap with a companion theorem set's T1-T9 is
intentional and not flattened: that program uses a different numbering
(its T2 is free-world existence; its T6 is constraint monotonicity).

These are informal statements with code witnesses. They are **not**
machine-checked proofs. Each claim is an engineering invariant checked
on finite fixtures. See [No-claim boundary](#no-claim-boundary) and the
per-theorem no-claim lines.

## Notation

The term algebra is `emath-term`:

- Σ is a first-order signature: a finite map `SymbolId → arity`
  (`emath_term::Signature`).
- X is a finite set of free variables (`VariableId`).
- TΣ(X) is the term algebra over Σ with variables X: the smallest set
  containing X and closed under application of symbols of Σ at their
  declared arities. The carrier type is `emath_term::Term`:
  `Variable(VariableId) | Constant(SymbolId) | Apply { operator, arguments }`.
- A Σ-world W is any `emath_genesis::FirstOrderWorld` whose `constant`
  / `apply` interpret the symbols of Σ. Its carrier is `W::Value`.
- A valuation ρ: X → |W| is `Environment<W::Value>`
  (`BTreeMap<VariableId, W::Value>`).
- `evaluate(t, W, ρ)` is the unique recursive homomorphism
  TΣ(X) → |W| extending ρ, when it is defined (typed `EvalError`
  otherwise: missing variable, unknown symbol, arity).

World IR (`emath_world_ir::WorldIr`) is the provider-neutral record of a
world: signature, carriers, symbols, operator meanings, constructors,
laws, effects, holes, capabilities.

## T1: Free-world existence

**Statement.** For every finite first-order signature Σ there exists a
free Σ-world F_Σ whose carrier is TΣ(X) and whose interpretation of
each symbol is the corresponding syntactic constructor: a nullary
symbol c is interpreted as `Term::Constant(c)`, and an n-ary symbol f
applied to values t₁,…,tₙ as `Term::Apply { operator: f, arguments }`.
The World IR of F_Σ declares a single carrier (`Term` / `FreeTerm`),
marks every operator as `OperatorSemantics::StructuralConstructor` with
`MeaningOrigin::Derived`, and states the law `structural-totality`.
No authored meaning is required for F_Σ to exist.

**Witness.**

| Item | Location |
| --- | --- |
| `FirstOrderWorld` | `crates/emath-genesis/src/lib.rs` (`pub trait FirstOrderWorld`) |
| `free_symbolic_world(name, signature) -> WorldIr` | same file |
| `FreeTermWorld` (`Value = Term`, `Error = EvalError`) | same file |
| `WorldIr` carrier/operator record | `crates/emath-world-ir/src/lib.rs` (`pub struct WorldIr`) |

`free_symbolic_world` constructs the World IR witness from any
`Signature`. `FreeTermWorld` is the runtime algebra: `constant` and
`apply` are the free constructors and never fail on well-formed
arguments.

**Verification.** Existence is constructive: the two items compile and
the T2 round-trip uses `FreeTermWorld` as F_Σ. Targeted command (same
run as T2):

```text
rch exec -- env CARGO_NET_GIT_FETCH_WITH_CLI=true \
  CARGO_TARGET_DIR="${RCH_TARGET_BASE:-${TMPDIR:-/tmp}}/rch_target_emath_test" \
  cargo test -p emath-genesis --lib free_world_evaluation
```

**No-claim.** The constructor accepts any finite `Signature` the
`emath-term` insert rules allow. There is no separate existence proof
or property test over arbitrary Σ. The World IR witness is a record, not
a certified initial-algebra construction.

## T2: Universal evaluation

**Statement.** For every Σ-world W and every valuation ρ: X → |W|,
there is at most one homomorphism ⟦−⟧_{W,ρ}: TΣ(X) → |W| extending ρ
and commuting with the interpretations of the symbols of Σ. In the
codebase that homomorphism is `evaluate`. Specializing to W = F_Σ
(`FreeTermWorld`) and ρ = id_X (each free variable maps to itself as a
`Term`) yields the universal round-trip: ⟦t⟧_{F_Σ, id} = t, compared
byte-exactly on `Term::canonical`. Distinct terms remain distinct in
F_Σ (argument order is observed).

**Witness.**

| Item | Location |
| --- | --- |
| `evaluate<W: FirstOrderWorld>` | `crates/emath-genesis/src/lib.rs` |
| `tests::free_world_evaluation_is_a_universal_round_trip` | same file, `#[cfg(test)]` |
| `tests::free_world_detects_argument_mutation` | same file (order-sensitivity control) |

The round-trip test evaluates `reference_alien_term()` in
`FreeTermWorld` under the identity environment and asserts
`value.canonical() == term.canonical()`. The mutation test swaps the
arguments of `⋈(a,b)` and asserts the free values differ.

**Verification.**

```text
rch exec -- env CARGO_NET_GIT_FETCH_WITH_CLI=true \
  CARGO_TARGET_DIR="${RCH_TARGET_BASE:-${TMPDIR:-/tmp}}/rch_target_emath_test" \
  cargo test -p emath-genesis --lib free_world_evaluation
```

Evidence: `test tests::free_world_evaluation_is_a_universal_round_trip ... ok` (`1 passed; 0 failed; 12 filtered out`).

**No-claim.** The test covers one admitted reference term (glyphs
`⧖`, `⋈`, `⊛`, `ζ`) and the identity valuation. It does not quantify
over arbitrary terms, worlds, or valuations. `evaluate` is a recursive
function, not a uniqueness proof in a proof assistant. Totality holds
only for terms whose symbols the world implements and whose free
variables are in ρ; otherwise the result is a typed `EvalError`.

## T3: Parametric compilation

**Statement.** Given a term t ∈ TΣ(X), a signature Σ, and a finite
list of world labels whose declared operator semantics match the
fixed per-label interpretation, there is a deterministic compilation
to a self-contained Rust crate that evaluates t under those worlds
(plus a swapped-operator negative control). Compilation is parametric
in (t, Σ, labels): the file map is a `BTreeMap` and regeneration is
byte-comparable. A declared meaning the generator cannot honor is a
typed refusal (`E-GEN-094`), not a silent drop.

**Witness.**

| Item | Location |
| --- | --- |
| `generate(term, signature, worlds) -> Result<GeneratedPackage, CodegenRefusal>` | `crates/emath-world-codegen-rust/src/lib.rs` |
| `compile_cmd` (`emath compile --parametric <file> --out <dir>`) | `crates/emath-cli/src/genesis_cmd.rs` |
| CLI dispatch (`compile` + `--parametric`) | `crates/emath-cli/src/lib.rs` |
| End-to-end demo | `xtask/src/main.rs` (`demo_semantic_genesis` / `run_demo_semantic_genesis`) |

`compile_cmd` analyzes the source, builds `WorldSpec` values from
admitted World IR, and calls `generate`. The xtask demo runs
`emath compile --parametric` on
`tests/valid/arbitrary-glyphs.emath`, diffs the emitted crate
against `examples/generated/semantic-genesis-worlds`, runs the
generated tests, and checks the derived oracles
(`free` canonical term, `boolean = false`, `modular-17 = 6`,
`swapped-modular-17 = 5`).

**Verification.**

```text
rch exec -- env CARGO_NET_GIT_FETCH_WITH_CLI=true \
  CARGO_TARGET_DIR="${RCH_TARGET_BASE:-${TMPDIR:-/tmp}}/rch_target_emath_test" \
  cargo xtask demo semantic-genesis
```

Evidence: `semantic-genesis demo: ok` with
`free: apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))`,
`boolean: false`, `modular-17: 6`, `swapped-modular-17: 5`.

**No-claim.** Only the label-based G3 subset is generatable
(`free_symbolic`, `boolean_algebra`, `modular_numeric`, plus the
swapped control). Arbitrary worlds are refusals. The crate name is
fixed (`semantic-genesis-worlds`). Byte-identity is against one
committed golden and one reference source, not a proof that every
future Σ compiles.

## T4: Constraint / resolution monotonicity

**Statement.** Resolution is monotone in provider set and planner
budget with respect to artifact class. If `plan(G, R, C)` selects a
plan of artifact class A, then `plan(G, R ∪ {p}, C)` and
`plan(G, R, C′)` still select class A whenever C′ enlarges the
node and candidate budgets of C. Adding a provider or growing
budgets must not destroy a previously reachable artifact class
(total artifact protocol).

**Witness.**

| Item | Location |
| --- | --- |
| `plan(goal, registry, config) -> PlanningOutcome` | `crates/emath-plan/src/planner.rs` |
| `PlannerConfig` (`max_nodes`, `max_candidates`) | same file |
| `planner::tests::adding_providers_or_budget_preserves_the_artifact_class` | same file, `#[cfg(test)]` |

The test registers provider `p1` for capability `evaluate.target`,
records the selected `artifact_class`, registers a second provider
`p2` of the same capability, and asserts the class is unchanged. It
then multiplies `max_nodes` and `max_candidates` by 4 and asserts
the class is still unchanged.

**Verification.**

```text
rch exec -- env CARGO_NET_GIT_FETCH_WITH_CLI=true \
  CARGO_TARGET_DIR="${RCH_TARGET_BASE:-${TMPDIR:-/tmp}}/rch_target_emath_test" \
  cargo test -p emath-plan --lib adding_providers
```

Evidence:
`test planner::tests::adding_providers_or_budget_preserves_the_artifact_class ... ok`
(`1 passed; 0 failed; 7 filtered out`).

**No-claim.** One goal, two static providers of the same capability,
default vs 4× budgets. This is not a lattice-theoretic proof over
arbitrary registries, goals, fallback policies, or budget
shrinkage. Removing providers, shrinking budgets, or changing
tie-break / capability sets is out of scope. `plan` is a
deterministic total function; the test checks one monotone pair, not
all pairs.

## No-claim boundary

None of T1-T4 is a machine-checked theorem (no Lean, Coq, Isabelle,
or in-tree kernel proof object). The witnesses are Rust constructors
and `#[test]` functions on finite cases. Passing today means the named
commands returned ok on this tree; it does not license
language such as "proved", "certified", or "for all signatures /
worlds / planner states".

The companion theorem set remains a separate formal program. Do not
treat this file as a statement of T1-T9.
