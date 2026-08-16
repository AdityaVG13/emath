#!/usr/bin/env bash
# emath 1.0 release gates: deterministic LOCAL gate runner.
#
# Optional convenience aggregate over the four standard lanes; CI runs those
# lanes as separate jobs, so this script is not wired into CI.
# Std-only: requires a Rust toolchain (rustfmt, clippy). Python is only used
# by scripts/reproducible_lane.sh (make_sbom.py), never by validate.sh.
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
