# Pass 1 — first-try evidence

HEAD before this pass: 9322455.

Confirmed first-try failures (release-perf binary):

| Invocation | Before | After |
|---|---|---|
| `emath --version` / `-V` / `version` | exit 2, unknown command + full help | exit 0, `emath 0.1.0` |
| `emath help check` | full catalog | one-command usage |
| `emath check --help` | exit 2, missing file | exit 0, one-command usage |
| `emath chek` / `emath buld` | unknown + dumped catalog on stdout | stderr `did you mean`, no catalog dump |
| `emath check` (no file) | usage only | usage + `try: emath help check` |
| `emath agent build <file>` (no --out) | exit 2, required --out | defaults to `target/emath` (same as `emath build`) |

Unit tests: `cargo test --offline -p emath-cli --lib` (9 tests).
