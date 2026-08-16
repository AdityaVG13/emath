#!/usr/bin/env bash
# reproducible build lane.
#
# Certified-profile determinism, verified end to end:
#   1. the same spec regenerated into two different absolute paths yields
#      byte-identical trees (manifest, source map, sources, verify state);
#   2. generated sources never embed the host absolute path;
#   3. a clean rebuild of the same tree is binary-deterministic (rlib hashes
#      identical across rebuilds);
#   4. tampering with a generated file is refused by the independent
#      artifact checker (`emath artifact check <dir>`);
#   5. publishing artifacts are deterministic (SBOM regenerates
#      byte-identically).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SPEC="tests/valid/stateful.emath"
test -f "$SPEC"

BASE="${TMPDIR:-/tmp}/emath-lane-$$"
trap 'rm -rf "$BASE"' EXIT
A="$BASE/tree-a"
B="$BASE/tree-b"
mkdir -p "$A" "$B"

echo "== 1. regenerate $SPEC into two different absolute paths =="
cargo run -q -p emath-cli -- build "$SPEC" --out "$A" --verify >/dev/null
cargo run -q -p emath-cli -- build "$SPEC" --out "$B" --verify >/dev/null
if ! diff -r --exclude=Cargo.lock --exclude=target "$A" "$B" >/dev/null; then
    echo "FAIL: cross-path regeneration diverged" >&2
    exit 1
fi
echo "ok: cross-path trees byte-identical"

echo "== 2. no absolute host-path leak in generated material =="
if grep -rlF -- "$ROOT" "$A" --exclude-dir=target >/dev/null 2>&1; then
    echo "FAIL: generated tree embeds the host absolute path" >&2
    exit 1
fi
echo "ok: no host-path leak"

echo "== 3. same-tree rebuild binary determinism =="
# The buildable copy is the staged `verify/` tree (the published artifact
# directory is named after its content id, e.g. `fnv1a64:...`, which macOS
# cargo cannot join as a build path).
test -f "$A/verify/Cargo.toml"
test -f "$A/verify/src/lib.rs"
hash_build() {
    rm -rf "$1/target"
    cargo build -q --manifest-path "$1/Cargo.toml" >/dev/null
    find "$1/target" -name '*.rlib' -maxdepth 3 | sort | xargs shasum -a 256
}
H1="$(hash_build "$A/verify")"
H2="$(hash_build "$A/verify")"
if [ "$H1" != "$H2" ]; then
    echo "FAIL: clean rebuild of the same tree is not binary-deterministic" >&2
    diff <(echo "$H1") <(echo "$H2") >&2 || true
    exit 1
fi
echo "ok: clean rebuild hashes identical"

echo "== 4. tamper negative control =="
PUBLISHED_SRC="$(find "$A/emath" -path '*/src/lib.rs' | head -1)"
test -n "$PUBLISHED_SRC"
cp "$PUBLISHED_SRC" "$PUBLISHED_SRC.bak"
echo "// tampered by $0" >>"$PUBLISHED_SRC"
if cargo run -q -p emath-cli -- artifact check "$A" >/dev/null 2>&1; then
    mv "$PUBLISHED_SRC.bak" "$PUBLISHED_SRC"
    echo "FAIL: independent artifact check admitted a tampered crate" >&2
    exit 1
fi
mv "$PUBLISHED_SRC.bak" "$PUBLISHED_SRC"
if ! cargo run -q -p emath-cli -- artifact check "$A" >/dev/null 2>&1; then
    echo "FAIL: artifact check must pass again after restore" >&2
    exit 1
fi
echo "ok: tamper refused by the independent checker; restore re-verified"

echo "== 5. publishing artifact determinism (SBOM) =="
if [ ! -f legal/SBOM.json ]; then
    echo "skip: legal/SBOM.json is not shipped in a fresh clone (maintainer-only lane)"
else
    TMP_SBOM="$BASE/SBOM.json"
    python3 scripts/make_sbom.py "$TMP_SBOM" >/dev/null
    if ! diff -u legal/SBOM.json "$TMP_SBOM" >/dev/null; then
        echo "FAIL: SBOM regeneration diverged from the committed artifact" >&2
        diff -u legal/SBOM.json "$TMP_SBOM" | head -30 >&2 || true
        exit 1
    fi
    echo "ok: SBOM regeneration byte-identical"
fi

echo "reproducible lane: green"
