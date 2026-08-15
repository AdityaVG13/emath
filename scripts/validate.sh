#!/usr/bin/env bash
# eMath Phase 1 validation gate (AGENTS.md): fmt, test, clippy, artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== fmt =="
cargo fmt --all -- --check

echo "== test =="
cargo test --workspace

echo "== clippy =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== artifact determinism =="
ARTIFACT_DIR="${TMPDIR:-/tmp}/emath-validate-$$"
mkdir -p "$ARTIFACT_DIR"
cargo run -q -p emath-cli -- build implementation/tests/valid/stateful.emath \
    --out "$ARTIFACT_DIR" --verify >/dev/null
LIB="$(find "$ARTIFACT_DIR/emath" -name lib.rs -path '*/src/lib.rs' | head -n1)"
if ! diff -u examples/generated/affine-policy-rs/src/lib.rs "$LIB" >/dev/null; then
    echo "FAIL: regenerated src/lib.rs differs from the committed generated crate" >&2
    diff -u examples/generated/affine-policy-rs/src/lib.rs "$LIB" >&2 || true
    rm -rf "$ARTIFACT_DIR"
    exit 1
fi
rm -rf "$ARTIFACT_DIR"
echo "generated crate is byte-identical to committed copy"

echo "== negative controls =="
for fixture in implementation/tests/invalid/*.emath; do
    if cargo run -q -p emath-cli -- check "$fixture" >/dev/null 2>&1; then
        echo "FAIL: invalid fixture admitted: $fixture" >&2
        exit 1
    fi
done
echo "all invalid fixtures refused"

echo "validate.sh: ok"
