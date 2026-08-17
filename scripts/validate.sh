#!/usr/bin/env bash
# emath Phase 1 validation gate (AGENTS.md): fmt, test, clippy, artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TMP_DIR="${TMPDIR:-/tmp}/emath-validate-$$"
mkdir -p "$TMP_DIR"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo "== compile/lint lanes run in CI, not here =="
echo "(fmt / cargo test / clippy run as separate CI jobs and via the AGENTS.md
validation commands; re-running cargo test here would double the vacuous
execution that this gate exists to kill.)"

echo "== fork-type identity gate (AGENTS.md rule 1) =="
if grep -rniE '(^|[^a-z0-9_.-])(dew|rumoca|wrenfold|franken|modelica)([^a-z0-9_.-]|$)' \
    crates/emath-core crates/emath-ir crates/emath-goal crates/emath-plan \
    crates/emath-sema crates/emath-runtime crates/emath-provider-api \
    crates/emath-artifact examples/provider-skeleton/src/main.rs \
    >"$TMP_DIR/fork-grep.txt"; then
    echo "FAIL: upstream fork-type identifier leaked into a Phase 1 crate or schema:" >&2
    cat "$TMP_DIR/fork-grep.txt" >&2
    exit 1
fi
echo "no fork-type identifiers in Phase 1 crates or durable schemas"

echo "== artifact determinism =="
ARTIFACT_DIR="$TMP_DIR/artifacts"
mkdir -p "$ARTIFACT_DIR"
cargo run -q -p emath-cli -- build tests/valid/affine_policy.emath \
    --out "$ARTIFACT_DIR" --verify >/dev/null
LIB="$(find "$ARTIFACT_DIR/emath" -name lib.rs -path '*/src/lib.rs' | head -n1)"
if ! diff -u examples/generated/affine-policy-rs/src/lib.rs "$LIB" >/dev/null; then
    echo "FAIL: regenerated src/lib.rs differs from the committed generated crate" >&2
    diff -u examples/generated/affine-policy-rs/src/lib.rs "$LIB" >&2 || true
    exit 1
fi
echo "generated crate is byte-identical to committed copy"

echo "== negative controls =="
# Each invalid fixture must be refused AND carry its documented code, so a
# regression that swaps the diagnostic (or admits the fixture) fails here.
assert_invalid() {
    local fixture="$1"
    local expected="$2"
    local output
    if output="$(cargo run -q -p emath-cli -- check "$fixture" 2>&1)"; then
        echo "FAIL: invalid fixture admitted: $fixture" >&2
        exit 1
    fi
    if ! printf '%s\n' "$output" | grep -q -- "$expected"; then
        echo "FAIL: $fixture did not emit the documented code $expected" >&2
        printf '%s\n' "$output" >&2
        exit 1
    fi
}
assert_invalid tests/invalid/duplicate_output.emath "E-NAME-020"
assert_invalid tests/invalid/missing_state_assignment.emath "E-CTOR-030"
assert_invalid tests/invalid/recursive_kind.emath "E-KIND-100"
assert_invalid tests/invalid/unit_mismatch.emath "E-UNIT-001"
echo "all invalid fixtures refused with the documented codes"

echo "== semantic genesis capstone =="
cargo run -q -p xtask -- demo semantic-genesis >/dev/null
echo "semantic-genesis: determinism, wrong-world rejection ok"

echo "== cache-policy capstone =="
cargo run -q -p xtask -- demo cache-policy >/dev/null
echo "cache-policy: build + host promotion + negative control ok"

echo "== semantic genesis generated crate identity =="
SG_DIR="$TMP_DIR/sg"
mkdir -p "$SG_DIR"
cargo run -q -p emath-cli -- compile --parametric language/examples/01_arbitrary_glyphs.emath \
    --out "$SG_DIR" >/dev/null
if ! diff -r --exclude=Cargo.lock --exclude=target --exclude=manifest.json --exclude=source-map.json \
    "$SG_DIR" examples/generated/semantic-genesis-worlds >/dev/null; then
    echo "FAIL: regenerated semantic-genesis crate differs from the committed copy" >&2
    exit 1
fi
echo "semantic-genesis crate is byte-identical to committed copy"

echo "== semantic genesis generated crate fmt =="
cargo fmt --manifest-path examples/generated/semantic-genesis-worlds/Cargo.toml -- --check
echo "generated crate is rustfmt-stable"

echo "validate.sh: ok"
