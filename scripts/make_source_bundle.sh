#!/usr/bin/env bash
# emath reproducible source bundle (V7 g12 / v0.1.0 release input).
#
# Produces a byte-deterministic source tarball for a git ref plus a SHA-256
# manifest, so the same ref always yields the same bundle:
#
#   emath-<ref>-src.tar.gz        gzip -n over `git archive` (fixed mtime)
#   emath-<ref>-src.sha256        "sha256 (file) = hash" lines
#   emath-<ref>-bundle.json       size, hash, ref, tree, file count
#
# Usage: scripts/make_source_bundle.sh [<ref>] [<out-dir>]
#   <ref>     git ref or commit to bundle (default: HEAD)
#   <out-dir> output directory (default: <repo>/dist)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "error: make_source_bundle.sh must run inside the emath git repository (git archive requires it)" >&2
    exit 1
fi

REF="${1:-HEAD}"
OUT_DIR="${2:-$ROOT/dist}"

# Ref names may contain '/' or '-' (tags/branches); keep only a safe stem.
STEM="$(echo "$REF" | tr '/ ' '-')"

GIT_TREE="$(git rev-parse "$REF^{tree}")"

TARBALL="emath-${STEM}-src.tar.gz"
SHA_FILE="emath-${STEM}-src.sha256"
MANIFEST="emath-${STEM}-bundle.json"

mkdir -p "$OUT_DIR"

# git archive emits entries with fixed (epoch) timestamps; gzip -n stores no
# original name or timestamp, so the tarball bytes depend only on the tree.
git archive --format=tar "$REF" | gzip -n > "$OUT_DIR/$TARBALL"

SIZE_BYTES="$(wc -c < "$OUT_DIR/$TARBALL" | tr -d ' ')"
HASH="$(shasum -a 256 "$OUT_DIR/$TARBALL" | awk '{print $1}')"

printf 'sha256 (%s) = %s\n' "$TARBALL" "$HASH" > "$OUT_DIR/$SHA_FILE"

{
  echo "{"
  echo "  \"schema\": \"emath.source-bundle.v1\","
  echo "  \"bundle\": \"$TARBALL\","
  echo "  \"ref\": \"$REF\","
  echo "  \"tree\": \"$GIT_TREE\","
  echo "  \"size_bytes\": $SIZE_BYTES,"
  echo "  \"sha256\": \"$HASH\","
  echo "  \"sha256_file\": \"$SHA_FILE\","
  echo "  \"method\": \"git archive --format=tar | gzip -n (deterministic mtime)\""
  echo "}"
} > "$OUT_DIR/$MANIFEST"

echo "source bundle: $OUT_DIR/$TARBALL ($SIZE_BYTES bytes)"
echo "sha256:        $HASH"
echo "manifest:      $OUT_DIR/$MANIFEST"
