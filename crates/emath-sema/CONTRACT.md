# emath-sema CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Semantic admission (Phase 1): syntax tree to typed neutral SIR.
- Orchestrates field checks, constructor/invariant admission, definition typing, goal elaboration, and plan construction through the public `CompilerSession` surface.
- Everything outside the Phase 1 subset receives a typed capability refusal; nothing is silently dropped.
- May depend on core/ir/syntax/goal; declares only `emath-core` and `emath-ir` (plus `emath-syntax` as a dev-dependency for the parser backend).

## Public types and semantics

- `CompilerSession` — the session facade: `new`, `load_package`, `load_text`, `parse_text`, `check`, `check_owned`, `plan`. Carries a `SourceStore` and `Limits`.
- `SourcePackage` — a loaded source: file id, display name, text.
- `CheckResult` — admitted `SemanticPackage` plus `Diagnostics` and a `SemanticTrace`.
- `PlanResult` — admitted package, elaborated `RequestSpec`s, `ResolutionPlan`s, and `Diagnostics`.
- `CompilerPolicy` — build policy knobs (e.g. `verify_generated_crate`).
- `GeneratedCrate` — build-step result produced by `emath-build`: crate/package/version, file map, `EmittedAnchor` source-map anchors.
- `EmittedAnchor`, `SemanticTrace`, `TraceEntry` — anchor and trace bookkeeping (not exhaustive).

## Invariants

- Goals attach to declarations by construction (the ids built for that declaration), never by span geometry.
- Missing or unloaded source is a typed refusal (`E-PKG-080`), never an empty-source plan that passes silently.
- Declarations distinguishable only by lookalike glyphs are refused (`E-NAME-024`); duplicate names (`E-NAME-022`) and `_` names (`E-NAME-023`) are refused.
- Session limits reach the lexer through the installed parser backend (`E-SYN-108`).
- Request targets must be outputs or definitions of the declaration (`E-GOAL-041`); produce targets outside `rust.library` are refused (`E-GOAL-042`); request kinds other than `evaluate` are refused (`E-GOAL-043`).

## Error model

- Emits stable `E-*` diagnostics through `emath_core::Diagnostics`: `E-PKG-080`, `E-SYN-101`, `E-SYN-108`, `E-SYN-120`, `E-GOAL-041/042/043`, `E-NAME-022/023/024`.
- `E-SYN-120` is a typed refusal when the parser backend is not installed; hosts call `emath_syntax::install_source_parser` once per process.

## Determinism class

- `plan` builds deterministic native resolution plans; candidate ordering and tie-breaks follow seeded, ordered rules.
- `parse` and `check` are deterministic given the same source and limits.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-sema/tests/` directory on disk; conformance is unit-level in `session.rs` and `recognition.rs` `#[cfg(test)]` modules: `tiny_session_token_budget_refuses_parse`, `duplicate_declaration_name_is_refused_with_e_name_022`, `underscore_declaration_name_is_refused_with_e_name_023`, `confusable_lookalike_declaration_is_refused_with_e_name_024`, `names_that_are_not_lookalikes_are_not_refused`, `goals_attach_to_their_own_declaration_by_id_not_span`.

## No-claim boundaries

- Phase 1 admits only the strict-f64 subset and the `evaluate`/`rust.library` goal shape; exact arithmetic, arbitrary produce targets, and non-evaluate request kinds are refused.
- The build step (backend plus artifact emission) lives in `emath-build`, not here.
