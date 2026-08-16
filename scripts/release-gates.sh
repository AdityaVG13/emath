#!/usr/bin/env bash
# emath 1.0 release gates — deterministic local gate runner.
#
# Runs the four standard lanes and prints a per-lane report. Exits 0 only
# when every lane passes. Std-only: requires a Rust toolchain (rustfmt,
# clippy) and Python 3 (used by scripts/validate.sh).
set -u

cd "$(dirname "$0")/.." || exit 1

lane() {
    local name="$1"
    shift
    if "$@" >/tmp/emath-gate-$$.log 2>&1; then
        echo "gate: ${name}: PASS"
    else
        echo "gate: ${name}: FAIL"
        sed -n '1,20p' /tmp/emath-gate-$$.log
        return 1
    fi
}

failures=0

if ! lane "fmt" cargo fmt --all -- --check; then
    failures=1
fi
if ! lane "test" cargo test --workspace; then
    failures=1
fi
if ! lane "clippy" cargo clippy --workspace --all-targets -- -D warnings; then
    failures=1
fi
if ! lane "validate" bash scripts/validate.sh; then
    failures=1
fi

rm -f /tmp/emath-gate-$$.log
if [ "$failures" -eq 0 ]; then
    echo "release-gates: all lanes PASS"
    exit 0
fi
echo "release-gates: FAILED"
exit 1
