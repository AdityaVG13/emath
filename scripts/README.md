# Repository Gate Scripts

Deterministic checks that keep the shipped engineering surface (`implementation/`)
and the language pipeline (`elps/`, `language/`) honest against the workspace.
Run `validate.sh` for the full gate; every script below is wired into it as a
lane. All scripts are stdlib-only (Python 3 or bash) and read-only over the
repository.

| Script | Purpose |
| --- | --- |
| `validate.sh` | Full repository gate: doc-drift lanes, ELP/G4 lanes, negative controls, capstones. Every lane appends a JSONL record to `validate.jsonl`; failures retain the workdir for inspection (never silently cleaned). |
| `check_doc_gates.py` | Pins `implementation/CRATE_MAP.md` against the workspace manifest + `crates/` layout and `implementation/PUBLIC_API_INVENTORY.md` against `emath-sema` signatures. Fails on drift from HEAD. |
| `check_doc_pins.py` | Loads the hashed-doc contract set named in `implementation/contract-pins.json`; a pinned doc that changed without its pin being bumped fails the gate (drift must be a named bump). |
| `check_spec_pin.py` | Language spec pin register: `language/reference/*.md` + `language/grammar/*.ebnf` are SHA-256-pinned under an edition id in `implementation/SPEC_PIN.json`; drift without a named edition bump fails the gate. Regenerate a bump with `--regenerate --note "..."`. |
| `check_upstream_lock.py` | Upstream-lock honesty: validates `forks/UPSTREAM_LOCK.json` against `implementation/schemas/upstream-lock.schema.json` (jsonschema if installed, structural fallback otherwise) and requires adapter seams to name their locked commit (dew `seam.rs` const; rumoca `CONTRACT.md` no-claim fence). |
| `dump_error_codes.py` | Regenerates the error-code completeness annex inside `implementation/ERROR_CODES.md`; `crates/emath-hir/tests/registry_complete.rs` independently re-derives the emitted set and asserts it is a subset of the named codes. |
| `check_elp.py` | ELP document-shape gate: `elps/ELP-NNNN-<slug>.md` files must carry the seven canonical sections, a matching title, no placeholders, and a four-artifact plan (see `elps/README.md`). |
| `check_four_artifact.py` | Four-artifact rule over a git range or a supplied file list: a grammar change must also touch reference + examples + tests; reference-only changes need the `Reference-Only: true` trailer. |
| `g4_ambiguity.py` | G4 ambiguity scan over the shipped EBNF (first-set overlap, identical/prefix alternatives, nullable siblings). Runs against a pinned baseline so only NEW conflict signatures fail; `--delta` audits a unified diff. |
| `g4_confusable.py` | G4 confusable-glyph scan (NFC + the sema `confusable_fold` table) over every grammar literal; `--delta` flags newly added glyphs colliding with the existing surface. |
| `g4_precedence.py` | G4 precedence battery: flags operator glyphs without an explicit `notation_decl`, and regenerates/checks the pinned boundary corpus (`tests/language-gates/fixtures/g4-precedence-boundary.corpus`). |

## Usage

```bash
./scripts/validate.sh                 # full gate (writes validate.jsonl)
python3 scripts/check_doc_gates.py    # drift check, defaults to repo root
python3 scripts/check_doc_pins.py     # pin check, defaults to repo root
python3 scripts/dump_error_codes.py   # refresh the ERROR_CODES.md annex
python3 scripts/check_elp.py          # ELP shape gate (defaults to elps/)
python3 scripts/check_four_artifact.py --files-from - < files.txt
python3 scripts/g4_ambiguity.py --baseline tests/language-gates/fixtures/g4-ambiguity-baseline.json language/grammar/*.ebnf
python3 scripts/g4_confusable.py language/grammar/*.ebnf
python3 scripts/g4_precedence.py --check --out tests/language-gates/fixtures/g4-precedence-boundary.corpus language/grammar/*.ebnf
```
