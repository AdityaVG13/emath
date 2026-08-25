# Repository Gate Scripts

Deterministic checks that keep the shipped engineering surface (`implementation/`)
honest against the workspace. Run `validate.sh` for the full gate; the two
`check_*` scripts are also wired into it as lanes. All scripts are stdlib-only
(Python 3 or bash) and read-only over the repository.

| Script | Purpose |
| --- | --- |
| `validate.sh` | Full repository gate: doc-drift lanes, negative controls, capstones. Every lane appends a JSONL record to `validate.jsonl`; failures retain the workdir for inspection (never silently cleaned). |
| `check_doc_gates.py` | Pins `implementation/CRATE_MAP.md` against the workspace manifest + `crates/` layout and `implementation/PUBLIC_API_INVENTORY.md` against `emath-sema` signatures. Fails on drift from HEAD. |
| `check_doc_pins.py` | Loads the hashed-doc contract set named in `implementation/contract-pins.json`; a pinned doc that changed without its pin being bumped fails the gate (drift must be a named bump). |
| `dump_error_codes.py` | Regenerates the error-code completeness annex inside `implementation/ERROR_CODES.md`; `crates/emath-hir/tests/registry_complete.rs` independently re-derives the emitted set and asserts it is a subset of the named codes. |

## Usage

```bash
./scripts/validate.sh                 # full gate (writes validate.jsonl)
python3 scripts/check_doc_gates.py    # drift check, defaults to repo root
python3 scripts/check_doc_pins.py     # pin check, defaults to repo root
python3 scripts/dump_error_codes.py   # refresh the ERROR_CODES.md annex
```
