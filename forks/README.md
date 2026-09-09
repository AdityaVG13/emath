# Fork Constellation

This directory governs source acquisition and modification. It does not vendor upstream source inside the strategy ZIP.

## Bootstrap

```bash
cd forks
../implementation/scripts/bootstrap_constellation.sh --core /path/to/workspace
```

Use `--full` for all locked repositories. The script checks exact commits and copies metadata templates without changing upstream code.

## Rules

- Exact commits in `UPSTREAM_LOCK.json` are authoritative.
- Never build a release from an unrecorded branch head.
- Keep emath semantics in neutral IR and adapters.
- Preserve all licenses/notices and mark modified files where required.
- Run unmodified upstream tests before the first emath patch.
- Categorize every patch in `PATCH_LEDGER.md`.
- Rehearse upstream sync before depending on a permanent fork change.
