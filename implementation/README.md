# Implementation Contracts

The shipped engineering surface: what emath is, how it is layered, and what it
guarantees. Every file here is pinned against the workspace by `scripts/`

(`validate.sh` lanes and `check_doc_pins.py`), so the docs cannot drift from
HEAD without a named, intentional change.

## Reading order

1. `VISION.md` — positioning and the umbrella argument.
2. `CONSTITUTION.md` — thirteen laws plus additions C1–C10, each tracked to an enforceability claim.
3. `CRATE_MAP.md` — canonical crate layers and ownership.
4. `PUBLIC_API_INVENTORY.md` — the public API surface, pinned against `emath-sema` signatures.
5. `ERROR_CODES.md` — the stable error/refusal-code registry (`E-*`), with the generated completeness annex.
6. `CLI_REFERENCE.md` — the implemented CLI surface.
7. `SG00_STRICT_BASELINE.md` — the deterministic semantic-genesis pipeline baseline.
8. `GENESIS_KERNEL.md` — the trusted deterministic core of semantic genesis.
9. `RESEARCH_THEOREMS.md` — parked formal results (T1–T4).
10. `contract-pins.json` — the hashed-doc pin set enforcing the above.

## Maintenance

- `scripts/validate.sh` runs the doc-gates; `scripts/check_doc_gates.py` and
  `scripts/check_doc_pins.py` are the individual lanes.
- `ERROR_CODES.md`'s annex is regenerated with `scripts/dump_error_codes.py`;
  `crates/emath-hir/tests/registry_complete.rs` enforces the same extraction
  rule so the two cannot drift.
- Other material in this directory is local-only (gitignored); the shipped
  set is exactly the files listed here.
