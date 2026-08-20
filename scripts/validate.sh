#!/usr/bin/env bash
# emath repository gate (AGENTS.md): artifacts, negative controls,
# capstones, doc gates. Per AGENTS.md, NEVER run full cargo tests: the
# compile/lint lanes run via DSR when available (or as narrow, clearly
# labeled cargo checks), and this gate never re-runs them.
#
# Every lane writes a JSON-lines record (suite/phase/status/duration) to
# validate.jsonl. On failure the workdir is RETAINED (never a silent
# `rm -rf`): the retained directory holds the JSONL stream, the failing
# lane's diff, and any staged artifacts. `EMATH_VALIDATE_SELF_TEST=2`
# proves the failure path by running a nested forced-failure gate and
# inspecting its retained artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TMP_DIR="${EMATH_VALIDATE_TMP_DIR:-${TMPDIR:-/tmp}/emath-validate-$$}"
mkdir -p "$TMP_DIR"
LOG_DIR="$TMP_DIR/logs"
mkdir -p "$LOG_DIR"
JSONL="$LOG_DIR/validate.jsonl"
: >"$JSONL"
LANE_START_SECONDS="$SECONDS"

# JSONL record writer (positional key/value pairs; values are escaped).
jsonl_write() {
    local out="{" first=1
    while [ $# -gt 0 ]; do
        local key="$1" value="$2"
        shift 2
        if [ "$first" -ne 1 ]; then out="$out,"; fi
        first=0
        value=$(printf '%s' "$value" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n' ' ')
        out="$out\"$key\":\"$value\""
    done
    printf '%s\n' "$out" >>"$JSONL"
}

lane_begin() {
    LANE_START_SECONDS="$SECONDS"
}

# End a lane with a JSONL record. <status> is one of passed/failed/skipped.
lane_done() {
    local suite="$1" phase="$2" status="$3" detail="${4:-}"
    local ms=$(( (SECONDS - LANE_START_SECONDS) * 1000 ))
    LANE_START_SECONDS="$SECONDS"
    jsonl_write "suite" "$suite" "phase" "$phase" "status" "$status" "detail" "$detail" "duration_ms" "$ms"
}

on_exit() {
    local code=$?
    if [ "$code" -eq 0 ]; then
        rm -rf "$TMP_DIR"
    else
        jsonl_write "suite" "validate" "phase" "gate" "status" "failed" "detail" "gate aborted" "duration_ms" "$(( (SECONDS - LANE_START_SECONDS) * 1000 ))"
        echo "FAIL: validation gate failed ($code); workdir and JSONL log retained at:" >&2
        echo "  $TMP_DIR" >&2
        echo "  $JSONL" >&2
    fi
    exit "$code"
}
trap on_exit EXIT

PYTHON="$(command -v python3 || command -v python || printf 'python3')"

# Self-test mode 3: a forced-failure lane used by the mode-2 harness to
# prove that failures write JSONL, print a diff where one exists, and
# retain the workdir.
if [ "${EMATH_VALIDATE_SELF_TEST:-}" = "3" ]; then
    lane_begin
    printf 'honest line\n' >"$TMP_DIR/honest.txt"
    printf 'tampered line\n' >"$TMP_DIR/tampered.txt"
    if diff -u "$TMP_DIR/honest.txt" "$TMP_DIR/tampered.txt" >"$TMP_DIR/forced-diff.txt"; then
        echo "FAIL: self-test fixtures unexpectedly identical" >&2
        exit 1
    fi
    lane_done "self-test" "forced-fail" "failed" "intentionally failing lane"
    echo "FAIL: forced self-test failure (expected)" >&2
    exit 1
fi

# Self-test mode 2: nested run must fail, its JSONL stream must carry the
# failed record, its workdir must be retained with the diff, and the
# stderr must name the retained locations.
if [ "${EMATH_VALIDATE_SELF_TEST:-}" = "2" ]; then
    echo "== logger self-test =="
    lane_begin
    NESTED="$TMP_DIR/nested"
    if env EMATH_VALIDATE_TMP_DIR="$NESTED" EMATH_VALIDATE_SELF_TEST=3 "$0" \
        >"$NESTED.stdout" 2>"$NESTED.stderr"; then
        echo "FAIL: nested forced-failure gate succeeded" >&2
        exit 1
    fi
    if [ ! -f "$NESTED/logs/validate.jsonl" ]; then
        echo "FAIL: nested run kept no JSONL stream" >&2
        exit 1
    fi
    if ! grep -q '"status":"failed"' "$NESTED/logs/validate.jsonl"; then
        echo "FAIL: nested JSONL stream has no failed record" >&2
        cat "$NESTED/logs/validate.jsonl" >&2
        exit 1
    fi
    if [ ! -f "$NESTED/forced-diff.txt" ] || ! grep -q "^-honest line" "$NESTED/forced-diff.txt"; then
        echo "FAIL: nested run did not retain the unified diff" >&2
        exit 1
    fi
    if ! grep -q "workdir and JSONL log retained" "$NESTED.stderr"; then
        echo "FAIL: nested stderr does not name the retained workdir" >&2
        cat "$NESTED.stderr" >&2
        exit 1
    fi
    lane_done "logger" "self-test" "passed" "forced mismatch: JSONL + unified diff + retained workdir"
    echo "logger self-test: forced mismatch records JSONL, keeps the diff, retains the workdir"
    exit 0
fi

echo "== compile/lint lanes run via DSR, not here =="
echo "(per AGENTS.md, NEVER run full cargo tests; fmt/clippy run through DSR
when available or as narrow, labeled cargo checks; this gate proves only the
artifact, negative-control, capstone, and doc lanes.)"

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
cargo run -q -p emath-cli -- build tests/valid/affine_scorer.emath \
    --out "$ARTIFACT_DIR" --verify >/dev/null
LIB="$(find "$ARTIFACT_DIR/emath" -name lib.rs -path '*/src/lib.rs' | head -n1)"
if ! diff -u examples/generated/affine-scorer/src/lib.rs "$LIB" >/dev/null; then
    lane_begin
    diff -u examples/generated/affine-scorer/src/lib.rs "$LIB" >"$TMP_DIR/affine-diff.txt" || true
    echo "FAIL: regenerated src/lib.rs differs from the committed generated crate" >&2
    cat "$TMP_DIR/affine-diff.txt" >&2
    lane_done "affine-scorer" "artifact-identity" "failed" "regenerated lib.rs differs; diff retained at affine-diff.txt"
    exit 1
fi
lane_begin
lane_done "affine-scorer" "artifact-identity" "passed" "generated lib.rs byte-identical"

echo "== provider reality gate =="
# No numeric-placeholder stub may survive in the Dew scalar backends, and
# the same-eval "differential oracle" (a tautology that could never fail)
# must not come back under any name.
if grep -rn "0.0; // refused" crates/emath-adapter-dew/src >"$TMP_DIR/stub-grep.txt" 2>/dev/null; then
    echo "FAIL: Dew scalar backend still emits a numeric placeholder stub:" >&2
    cat "$TMP_DIR/stub-grep.txt" >&2
    exit 1
fi
if grep -rn "differential_scan" crates/emath-adapter-dew/src >"$TMP_DIR/oracle-grep.txt" 2>/dev/null; then
    echo "FAIL: the same-eval differential oracle came back:" >&2
    cat "$TMP_DIR/oracle-grep.txt" >&2
    exit 1
fi
# The CLI provider status table must agree with the in-tree reality: the
# std-only native lanes are implemented, upstream engines are planned.
PROVIDER_LIST="$(cargo run -q -p emath-cli -- provider list)"
for entry in "dew.scalar" "native.causal" "native.euler" "rumoca.subset-import"; do
    if ! printf '%s\n' "$PROVIDER_LIST" | grep -q "^provider $entry: .*\[implemented\]"; then
        echo "FAIL: provider list does not show $entry as implemented" >&2
        printf '%s\n' "$PROVIDER_LIST" >&2
        exit 1
    fi
done
for entry in "phase2.expression" "phase3.structural" "phase4.symbolic"; do
    if ! printf '%s\n' "$PROVIDER_LIST" | grep -q "^provider $entry: .*\[planned\]"; then
        echo "FAIL: provider list does not show $entry as planned" >&2
        printf '%s\n' "$PROVIDER_LIST" >&2
        exit 1
    fi
done
echo "provider table agrees with in-tree adapter reality"

echo "== negative controls =="
# Each invalid fixture must be refused AND carry its documented code, so a
# regression that swaps the diagnostic (or admits the fixture) fails here.
assert_invalid() {
    local fixture="$1"
    local expected="$2"
    local output
    lane_begin
    if output="$(cargo run -q -p emath-cli -- check "$fixture" 2>&1)"; then
        echo "FAIL: invalid fixture admitted: $fixture" >&2
        lane_done "negative-controls" "check" "failed" "admitted $fixture (expected $expected)"
        exit 1
    fi
    if ! printf '%s\n' "$output" | grep -q -- "$expected"; then
        lane_done "negative-controls" "check" "failed" "$fixture emitted a different code than $expected"
        echo "FAIL: $fixture did not emit the documented code $expected" >&2
        printf '%s\n' "$output" >&2
        exit 1
    fi
    lane_done "negative-controls" "check" "passed" "$fixture -> $expected"
}
assert_invalid tests/invalid/duplicate_output.emath "E-NAME-020"
assert_invalid tests/invalid/missing_state_assignment.emath "E-CTOR-030"
assert_invalid tests/invalid/recursive_kind.emath "E-KIND-100"
assert_invalid tests/invalid/unit_mismatch.emath "E-UNIT-101"
assert_invalid tests/invalid/model_decl.emath "E-KIND-100"
assert_invalid tests/invalid/unknown_section.emath "E-SEC-101"
assert_invalid tests/invalid/exports_junk.emath "E-SYN-101"
assert_invalid tests/invalid/compile_junk.emath "E-SYN-101"
assert_invalid tests/invalid/function_type.emath "E-TYPE-110"
# Unicode honesty lane: a declaration spelled with a Cyrillic lookalike
# of an already-seen Latin name is refused (E-NAME-024), and an
# identifier built from a combining mark (non-NFC by construction) is
# refused at the lexer (E-SYN-115).
assert_invalid tests/invalid/confusable_decl.emath "E-NAME-024"
assert_invalid tests/invalid/combining_mark.emath "E-SYN-115"
assert_invalid tests/invalid/named_call_arg.emath "E-SYN-121"

# `check --json` must carry codes and messages, not counts: an outer
# diagnostic line is fine, but the JSON document itself has to name the
# refused code so gate lanes can assert the exact diagnostic.
lane_begin
JSON_OUT="$(cargo run -q -p emath-cli -- check tests/invalid/duplicate_output.emath --json 2>/dev/null || true)"
if ! printf '%s' "$JSON_OUT" | grep -q '"code": "E-NAME-020"'; then
    echo "FAIL: check --json omitted the diagnostic code" >&2
    printf '%s\n' "$JSON_OUT" >&2
    lane_done "negative-controls" "check-json" "failed" "no E-NAME-020 code in JSON document"
    exit 1
fi
if ! printf '%s' "$JSON_OUT" | grep -q '"message":'; then
    echo "FAIL: check --json omitted the diagnostic message" >&2
    printf '%s\n' "$JSON_OUT" >&2
    lane_done "negative-controls" "check-json" "failed" "no message in JSON document"
    exit 1
fi
lane_done "negative-controls" "check-json" "passed" "E-NAME-020 code + message in JSON document"
echo "check --json carries diagnostic codes and messages"

# Positive control: current-syntax `emath function` is the Phase 1
# function lane (a real admit), not a hollow acceptance.
if ! cargo run -q -p emath-cli -- check tests/valid/square.emath >/dev/null 2>&1; then
    echo "FAIL: square.emath (function lane) refused" >&2
    exit 1
fi
echo "square.emath function lane admits"

echo "== lossless fmt gate =="
# Every valid corpus file must be byte-canonical under the lossless
# formatter (fmt(file) == file); a drift is a real round-trip break.
for FIXTURE in tests/valid/square.emath tests/valid/affine_scorer.emath; do
    lane_begin
    if ! FMT_OUT="$(cargo run -q -p emath-cli -- fmt "$FIXTURE" 2>&1)"; then
        echo "FAIL: $FIXTURE is not lossless-canonical" >&2
        printf '%s\n' "$FMT_OUT" >&2
        lane_done "fmt" "canonical" "failed" "$FIXTURE not canonical"
        exit 1
    fi
    if ! printf '%s\n' "$FMT_OUT" | grep -q "canonical form"; then
        echo "FAIL: $FIXTURE fmt did not confirm canonical form" >&2
        lane_done "fmt" "canonical" "failed" "$FIXTURE no canonical confirmation"
        exit 1
    fi
    lane_done "fmt" "canonical" "passed" "$FIXTURE round-trips"
done
# Negative: a parses-but-non-canonical file must be refused, not
# silently accepted (extra blank line inside a suite).
lane_begin
UNFORMATTED="$TMP_DIR/unformatted.emath"
awk 'NR==15 { print "" } { print }' tests/valid/square.emath >"$UNFORMATTED"
if FMT_OUT="$(cargo run -q -p emath-cli -- fmt "$UNFORMATTED" 2>&1)"; then
    echo "FAIL: non-canonical file admitted by fmt" >&2
    lane_done "fmt" "refusal" "failed" "non-canonical file admitted"
    exit 1
fi
if ! printf '%s\n' "$FMT_OUT" | grep -q "NOT canonical"; then
    echo "FAIL: fmt refusal did not explain NOT canonical" >&2
    printf '%s\n' "$FMT_OUT" >&2
    lane_done "fmt" "refusal" "failed" "no NOT canonical explanation"
    exit 1
fi
lane_done "fmt" "refusal" "passed" "non-canonical file refused"
echo "fmt: corpus canonical; non-canonical refused"

echo "== crate map + API inventory gate (gauntlet-08) =="
# CRATE_MAP must map every workspace member and every non-hidden crates/
# directory to an existing path (SURF-0003); PUBLIC_API_INVENTORY must carry the
# exact CompilerSession signatures (name + receiver) from session.rs
# (SURF-0001). Request-typed surface stays honestly Partial.
lane_begin
if ! DOC_GATE_OUT="$("$PYTHON" scripts/check_doc_gates.py)"; then
    echo "FAIL: CRATE_MAP / PUBLIC_API_INVENTORY drift from HEAD" >&2
    printf '%s\n' "$DOC_GATE_OUT" >&2
    lane_done "doc-gates" "crate-map-inventory" "failed" "map or inventory drifted from HEAD"
    exit 1
fi
printf '%s\n' "$DOC_GATE_OUT"
lane_done "doc-gates" "crate-map-inventory" "passed" "CRATE_MAP + inventory pinned to HEAD"

# Negative controls: a mutated map name+path and a mutated session
# signature on COPIES of the docs must make the same gate fail.
lane_begin
NEG_DIR="$TMP_DIR/doc-negative"
mkdir -p "$NEG_DIR"
sed -e 's/`emath-core` | `crates\/emath-core`/`emath-coreX` | `crates\/emath-core`/' \
    -e 's/`crates\/emath-core`/`crates\/emath-coreX`/' \
    implementation/CRATE_MAP.md >"$NEG_DIR/CRATE_MAP.md"
sed 's/pub fn load_package(&mut self/pub fn load_package(\&self/' implementation/PUBLIC_API_INVENTORY.md >"$NEG_DIR/PUBLIC_API_INVENTORY.md"
if "$PYTHON" scripts/check_doc_gates.py \
    --crate-map "$NEG_DIR/CRATE_MAP.md" \
    --inventory "$NEG_DIR/PUBLIC_API_INVENTORY.md" >/dev/null 2>&1; then
    echo "FAIL: mutated CRATE_MAP/inventory copies passed the gate" >&2
    lane_done "doc-gates" "negative-control" "failed" "mutated docs admitted"
    exit 1
fi
lane_done "doc-gates" "negative-control" "passed" "mutated map path and mutated signature refused"
# Policy: the pinned inventory must never be gitignored.
if git check-ignore -q implementation/PUBLIC_API_INVENTORY.md; then
    echo "FAIL: PUBLIC_API_INVENTORY.md is gitignored" >&2
    lane_done "doc-gates" "policy" "failed" "inventory gitignored"
    exit 1
fi
lane_done "doc-gates" "policy" "passed" "inventory not gitignored"
echo "doc gates: map + inventory pinned; negative controls refuse; inventory not ignored"

# Annex currency: ERROR_CODES.md must be byte-current with the emitted
# code set (regenerate via scripts/dump_error_codes.py, never hand-edit
# the generated annex).
lane_begin
if ! ANNEX_OUT="$("$PYTHON" scripts/dump_error_codes.py --check 2>&1)"; then
    echo "FAIL: ERROR_CODES.md annex is not current" >&2
    printf '%s\n' "$ANNEX_OUT" >&2
    lane_done "doc-gates" "annex-currency" "failed" "annex drifted from the emitted code set"
    exit 1
fi
printf '%s\n' "$ANNEX_OUT"
lane_done "doc-gates" "annex-currency" "passed" "annex current; issued list names every code once"
echo "ERROR_CODES: issued list complete and unique; annex current"

echo "== contract doc pins (gauntlet-d1) =="
# The hashed-doc contract loader: a pinned contract doc that changed
# without a named bump (pin update + `bumps` note in
# implementation/contract-pins.json) fails the gate.
lane_begin
if ! PIN_OUT="$("$PYTHON" scripts/check_doc_pins.py)"; then
    echo "FAIL: a pinned contract doc changed without a named bump" >&2
    printf '%s\n' "$PIN_OUT" >&2
    lane_done "doc-pins" "contract-pins" "failed" "hashed contract doc drifted"
    exit 1
fi
printf '%s\n' "$PIN_OUT"
lane_done "doc-pins" "contract-pins" "passed" "contract doc hashes match pins"
echo "contract pins: hashed docs match the named-bump pins"

echo "== planner gate =="
# Planned goals must be selected (native, sir-checker bound); a goal that
# cannot be planned (or refused elaboration) must exit 1, never 0.
lane_begin
if ! PLAN_OUT="$(cargo run -q -p emath-cli -- planner tests/valid/square.emath 2>&1)"; then
    echo "FAIL: planner refused a plan-able file" >&2
    printf '%s\n' "$PLAN_OUT" >&2
    lane_done "planner" "planning" "failed" "square not planned"
    exit 1
fi
if ! printf '%s\n' "$PLAN_OUT" | grep -q "plan goal=y disposition=native"; then
    echo "FAIL: planner did not select the native plan for square" >&2
    printf '%s\n' "$PLAN_OUT" >&2
    lane_done "planner" "planning" "failed" "no native plan for square"
    exit 1
fi
if ! printf '%s\n' "$PLAN_OUT" | grep -q "checks=sir-checker"; then
    echo "FAIL: planner plan lacks the checker binding" >&2
    printf '%s\n' "$PLAN_OUT" >&2
    lane_done "planner" "planning" "failed" "no checker binding on square plan"
    exit 1
fi
lane_done "planner" "planning" "passed" "square planned native + checker"
lane_begin
if PLAN_OUT="$(cargo run -q -p emath-cli -- planner tests/invalid/produce_dew_jit.emath 2>&1)"; then
    echo "FAIL: planner admitted an unplannable produce target" >&2
    lane_done "planner" "refusal" "failed" "produce dew.jit planned"
    exit 1
fi
if ! printf '%s\n' "$PLAN_OUT" | grep -q "E-GOAL-042"; then
    echo "FAIL: planner refusal lacks E-GOAL-042" >&2
    printf '%s\n' "$PLAN_OUT" >&2
    lane_done "planner" "refusal" "failed" "no E-GOAL-042 on dew.jit"
    exit 1
fi
lane_done "planner" "refusal" "passed" "produce dew.jit refused E-GOAL-042"
echo "planner: supported goals plan; unplannable goals refuse"

echo "== typed tooling refusals =="
lane_begin
if BENCH_OUT="$(cargo run -q -p emath-cli -- bench tests/valid/square.emath 2>&1)"; then
    echo "FAIL: bench command succeeded" >&2
    lane_done "tooling" "bench" "failed" "bench admitted"
    exit 1
fi
if ! printf '%s\n' "$BENCH_OUT" | grep -q "E-TLT-004"; then
    echo "FAIL: bench refusal lacks E-TLT-004" >&2
    printf '%s\n' "$BENCH_OUT" >&2
    lane_done "tooling" "bench" "failed" "no E-TLT-004 on bench"
    exit 1
fi
lane_done "tooling" "bench" "passed" "bench refused E-TLT-004"

# CONF-0026: a load failure (unreadable source) must be a typed refusal
# in the agent envelope too, never an empty-diagnostics admit.
lane_begin
if AG_OUT="$(cargo run -q -p emath-cli -- agent check /nonexistent.emath 2>&1)"; then
    echo "FAIL: agent check on a missing file admitted" >&2
    lane_done "tooling" "agent-load" "failed" "missing file admitted"
    exit 1
fi
if ! printf '%s\n' "$AG_OUT" | grep -q '"admitted": false'; then
    echo "FAIL: agent envelope claims admission for a missing file" >&2
    printf '%s\n' "$AG_OUT" >&2
    lane_done "tooling" "agent-load" "failed" "admitted true on missing file"
    exit 1
fi
if ! printf '%s\n' "$AG_OUT" | grep -q "E-PKG-080"; then
    echo "FAIL: agent check on missing file lacks E-PKG-080" >&2
    printf '%s\n' "$AG_OUT" >&2
    lane_done "tooling" "agent-load" "failed" "no E-PKG-080 on missing file"
    exit 1
fi
lane_done "tooling" "agent-load" "passed" "missing file refused E-PKG-080"
echo "bench refuses E-TLT-004; agent check on missing file refuses E-PKG-080"

echo "== language examples admit-or-refuse loop =="
# Every language example must either admit (then build honestly) or be
# refused with a documented E-code: nothing in the corpus is silently
# accepted or refused without a stable code.
lane_begin
for FIXTURE in language/examples/*.emath; do
    if cargo run -q -p emath-cli -- check "$FIXTURE" >/dev/null 2>&1; then
        if ! cargo run -q -p emath-cli -- build "$FIXTURE" --out "$TMP_DIR/examples" >/dev/null 2>&1; then
            echo "FAIL: admitted example failed to build: $FIXTURE" >&2
            lane_done "examples" "admit-or-refuse" "failed" "$FIXTURE admitted but not built"
            exit 1
        fi
    else
        EX_OUT="$(cargo run -q -p emath-cli -- check "$FIXTURE" 2>&1 || true)"
        if ! printf '%s\n' "$EX_OUT" | grep -qE "E-[A-Z]+-[0-9]{3}"; then
            echo "FAIL: refused example emitted no documented code: $FIXTURE" >&2
            printf '%s\n' "$EX_OUT" >&2
            lane_done "examples" "admit-or-refuse" "failed" "$FIXTURE refused without code"
            exit 1
        fi
    fi
done
lane_done "examples" "admit-or-refuse" "passed" "every example admits+builds or refuses with a code"
echo "language examples: admitted or refused with documented codes"

# Produce refusal lives in the build lane (goal elaboration), not in
# `check`, so it gets its own assertion here.
assert_build_refused() {
    local fixture="$1"
    local expected="$2"
    local output
    lane_begin
    if output="$(cargo run -q -p emath-cli -- build "$fixture" --out "$TMP_DIR/refused" 2>&1)"; then
        echo "FAIL: build admitted: $fixture" >&2
        lane_done "negative-controls" "build" "failed" "admitted $fixture (expected $expected)"
        exit 1
    fi
    if ! printf '%s\
' "$output" | grep -q -- "$expected"; then
        echo "FAIL: $fixture build did not emit the documented code $expected" >&2
        lane_done "negative-controls" "build" "failed" "$fixture emitted different code than $expected"
        printf '%s\
' "$output" >&2
        exit 1
    fi
    lane_done "negative-controls" "build" "passed" "$fixture -> $expected"
}
assert_build_refused tests/invalid/produce_dew_jit.emath "E-GOAL-042"
echo "produce dew.jit refused at build with E-GOAL-042"

echo "== artifact battery lane =="
# The seeded negative-control battery runs against a REAL staged build
# output (never a fixture): tampered, stale, wrong-goal, incomplete and
# unsupported seeds must all be refused with the checker's expected
# E-EVID-* codes. An escape fails the gate with the code printed, not a
# generic FAILED string.
lane_begin
BATTERY_DIR="$TMP_DIR/battery-art"
mkdir -p "$BATTERY_DIR"
cargo run -q -p emath-cli -- build tests/valid/affine_scorer.emath \
    --out "$BATTERY_DIR" >/dev/null
BATT_OUT="$(cargo run -q -p emath-cli -- artifact battery "$BATTERY_DIR")"
if ! printf '%s\n' "$BATT_OUT" | grep -q "battery clean (5 controls"; then
    echo "FAIL: artifact battery did not pass all five controls" >&2
    printf '%s\n' "$BATT_OUT" >&2
    lane_done "artifact-battery" "battery" "failed" "standard battery incomplete on real artifact"
    exit 1
fi
for control in tampered-content stale-certificate wrong-goal incomplete-artifact unsupported-claim; do
    if ! printf '%s\n' "$BATT_OUT" | grep -q "control refused ($control)"; then
        echo "FAIL: battery missing refused control $control" >&2
        printf '%s\n' "$BATT_OUT" >&2
        lane_done "artifact-battery" "battery" "failed" "control $control escaped on real artifact"
        exit 1
    fi
done
lane_done "artifact-battery" "battery" "passed" "five seeded controls refused on a real build"
echo "artifact battery: five seeded controls refused on a real build"

# Negative: a mutated byte in a real artifact's lib.rs must be refused
# with E-EVID-101 by the independent checker (code, not exit code).
TAMPER_DIR="$TMP_DIR/tampered-art"
cp -R "$BATTERY_DIR" "$TAMPER_DIR"
TAMPERED_LIB="$(find "$TAMPER_DIR/emath" -path '*/src/lib.rs' | head -n1)"
if [ -z "$TAMPERED_LIB" ]; then
    echo "FAIL: tamper setup found no src/lib.rs under $TAMPER_DIR" >&2
    exit 1
fi
"$PYTHON" - "$TAMPERED_LIB" <<'PY'
import sys
path = sys.argv[1]
data = bytearray(open(path, "rb").read())
data[0] ^= 0x01
open(path, "wb").write(bytes(data))
PY
TAMPER_OUT="$(cargo run -q -p emath-cli -- artifact check "$TAMPER_DIR" 2>&1 || true)"
if ! printf '%s\n' "$TAMPER_OUT" | grep -q "E-EVID-101"; then
    echo "FAIL: tampered artifact not refused with E-EVID-101" >&2
    printf '%s\n' "$TAMPER_OUT" >&2
    lane_done "artifact-battery" "tamper-negative" "failed" "mutated lib.rs admitted"
    exit 1
fi
lane_done "artifact-battery" "tamper-negative" "passed" "mutated lib.rs refused with E-EVID-101"
echo "tampered artifact refused with E-EVID-101 by the independent checker"

echo "== semantic genesis capstone =="
cargo run -q -p xtask -- demo semantic-genesis >/dev/null
echo "semantic-genesis: determinism, wrong-world rejection ok"

echo "== affine-scorer capstone =="
cargo run -q -p xtask -- demo affine-scorer >/dev/null
echo "affine-scorer: build + host promotion + negative control ok"

echo "== independent host consumer =="
# The fingerprint-free host consumer runs real behavioral asserts against
# the committed generated crate (constructor invariants + a known score);
# it is gated here, not just documented.
lane_begin
if ! IND_OUT="$(cargo run -q -p demo-host-independent 2>&1)"; then
    echo "FAIL: demo-host-independent did not pass its behavioral asserts" >&2
    printf '%s\n' "$IND_OUT" >&2
    lane_done "demo-host-independent" "run" "failed" "independent host consumer failed"
    exit 1
fi
if ! printf '%s\n' "$IND_OUT" | grep -q "independent host ok"; then
    echo "FAIL: demo-host-independent did not print its ok line" >&2
    printf '%s\n' "$IND_OUT" >&2
    lane_done "demo-host-independent" "run" "failed" "independent host ok line missing"
    exit 1
fi
lane_done "demo-host-independent" "run" "passed" "behavioral asserts on committed generated crate"
echo "demo-host-independent: behavioral asserts gated"

echo "== semantic genesis generated crate identity =="
SG_DIR="$TMP_DIR/sg"
mkdir -p "$SG_DIR"
cargo run -q -p emath-cli -- compile --parametric language/examples/arbitrary-glyphs.emath \
    --out "$SG_DIR" >/dev/null
# Identity include-set is documented, never silent: `manifest.json`,
# `source-map.json`, and `hole-manifest.json` are EXCLUDED from the byte
# diff because (a) the committed copy predates them and (b) the
# world-codegen provenance map embeds the absolute source path. They are
# pinned instead by the per-shape parse-back lane below (one schema id
# per writer shape: emath.generated-crate-manifest /
# emath.generated-crate-source-map / emath.hole-manifest, never the
# durable artifact ids). Everything else under the diff is
# byte-compared.
if ! diff -r --exclude=Cargo.lock --exclude=target --exclude=manifest.json --exclude=source-map.json --exclude=hole-manifest.json \
    "$SG_DIR" examples/generated/semantic-genesis-worlds >"$TMP_DIR/sg-diff.txt" 2>&1; then
    echo "FAIL: regenerated semantic-genesis crate differs from the committed copy" >&2
    cat "$TMP_DIR/sg-diff.txt" >&2
    lane_done "semantic-genesis" "crate-identity" "failed" "regenerated crate differs; diff retained at sg-diff.txt"
    exit 1
fi
lane_done "semantic-genesis" "crate-identity" "passed" "regenerated crate byte-identical"

echo "== semantic genesis provenance docs pinned by parse-back =="
lane_begin
# Per-shape parse-back pin for the two documents excluded from the byte
# diff above. The schema-id honest contract: each document claims exactly
# its own id, carries exactly its own field shape, and every `generated`
# path actually exists in the regenerated crate. Regenerated and
# committed provenance surfaces (schema id, kind, generated-path list)
# must agree.
"$PYTHON" - "$SG_DIR" examples/generated/semantic-genesis-worlds <<'PY'
import json, os, sys

GEN = sys.argv[1]
COMMITTED = sys.argv[2]

manifest = json.load(open(os.path.join(GEN, "manifest.json")))
source_map = json.load(open(os.path.join(GEN, "source-map.json")))
hole_manifest = json.load(open(os.path.join(GEN, "hole-manifest.json")))

assert manifest["schema"] == "emath.generated-crate-manifest", (
    f"manifest schema id drifted: {manifest['schema']}"
)
assert source_map["schema"] == "emath.generated-crate-source-map", (
    f"source-map schema id drifted: {source_map['schema']}"
)
assert "emath.source-map" not in json.dumps(source_map), (
    "genesis provenance must never claim the durable artifact source-map id"
)
assert hole_manifest["schema"] == "emath.hole-manifest", (
    f"hole-manifest schema id drifted: {hole_manifest['schema']}"
)
assert hole_manifest["schema_version"] == 1, (
    f"hole-manifest schema_version drifted: {hole_manifest['schema_version']}"
)
assert hole_manifest["term_id"] == manifest["term_id"], (
    "hole-manifest term_id disagrees with the crate manifest"
)
assert hole_manifest["signature_id"] == manifest["signature_id"], (
    "hole-manifest signature_id disagrees with the crate manifest"
)
holes = hole_manifest["holes"]
assert isinstance(holes, list) and len(holes) > 0, f"expected open holes, got {holes!r}"
hole_ids = [hole["hole_id"] for hole in holes]
assert len(set(hole_ids)) == len(hole_ids), "hole_ids must be unique"
for hole in holes:
    assert set(hole) == {"hole_id", "symbol", "arity", "kind", "state", "constraint"}, (
        f"bad hole shape: {hole!r}"
    )
    assert hole["state"] == "open", f"parametric-lane hole must be open: {hole!r}"
    assert hole["kind"] in {"constant-definition", "operator-definition"}, (
        f"bad hole kind: {hole!r}"
    )
symbols = [hole["symbol"] for hole in holes]
assert symbols == sorted(symbols), "hole entries must be symbol-sorted (deterministic order)"
assert "emath.resolution-plan" not in json.dumps(manifest), (
    "genesis manifest must never claim the durable resolution-plan id"
)

files = manifest["files"]
entries = source_map["entries"]
assert isinstance(files, list) and len(files) > 0, f"expected a crate file list, got {files!r}"
assert len(entries) == len(files), (
    f"entry count {len(entries)} != manifest file count {len(files)}"
)
for entry in entries:
    assert set(entry) == {"generated", "source", "kind"}, f"bad entry shape: {entry!r}"
    assert entry["kind"] == "parametric-world", f"bad kind: {entry!r}"
    assert os.path.basename(entry["source"]) == "arbitrary-glyphs.emath", (
        f"provenance source basename drifted: {entry['source']!r}"
    )
    target = os.path.join(GEN, entry["generated"])
    assert os.path.isfile(target), f"provenance entry points at a missing file: {target}"

# The committed copy carries no manifest.json/source-map.json; the
# pinable surface is the file list embedded in the regenerated manifest,
# which must equal the committed crate tree (the byte diff above already
# proves the regenerated tree matches, minus the two pinned docs).
import subprocess
tree = sorted(
    os.path.relpath(p, COMMITTED)
    for p in subprocess.run(
        ["find", COMMITTED, "-type", "f", "-not", "-path", "*/target/*"],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()
)
assert tree == sorted(manifest["files"]), (
    f"manifest file list {manifest['files']} does not match committed tree {tree}"
)
PY
lane_done "semantic-genesis" "provenance-pin" "passed" "manifest/source-map/hole-manifest pinned by per-shape parse-back"
echo "genesis provenance docs pinned: schema ids, entry shape, file lists agree"

echo "== semantic genesis generated crate fmt =="
cargo fmt --manifest-path examples/generated/semantic-genesis-worlds/Cargo.toml -- --check
echo "generated crate is rustfmt-stable"

# Oracle derivation note: the capstone constants asserted below are not
# magic. `modular_numeric:6` and `score(3.0)=7` are derived values of the
# same fixtures run through the real pipeline: lane 3 cross-checks the
# receipt answers against the printed output of the parametric lane
# (`compile --parametric` compiles the identical fixture-valued
# evaluators), and the affine-scorer capstone checks the demo host's
# print of `AffineScorer::new(2.0, 1.0).score(3.0) = 2.0*3.0+1.0 = 7.0`
# after the artifact-identity lane above proved the generated crate is
# byte-identical to the committed copy. The factories, not the
# constants, are the contract.
echo "== genesis honesty lane =="
GEN_DIR="$TMP_DIR/genesis"
mkdir -p "$GEN_DIR"
cargo run -q -p emath-cli -- genesis language/examples/arbitrary-glyphs.emath \
    --out "$GEN_DIR" >/dev/null

# 1. No invented `tested` stamp: authority is structural and the receipt's
#    checker_receipts stay empty (no checker produced the answers).
if grep -q '"authority"[[:space:]]*:[[:space:]]*"tested"' "$GEN_DIR/answer-receipt.json"; then
    echo "FAIL: genesis stamped a tested answer with empty checker_receipts" >&2
    cat "$GEN_DIR/answer-receipt.json" >&2
    exit 1
fi
if ! grep -q '"authority"[[:space:]]*:[[:space:]]*"structural"' "$GEN_DIR/answer-receipt.json"; then
    echo "FAIL: genesis receipt did not disclose structural authority" >&2
    exit 1
fi
if ! grep -q '"checker_receipts"[[:space:]]*:[[:space:]]*\[\]' "$GEN_DIR/answer-receipt.json"; then
    echo "FAIL: genesis receipt claims checker receipts that do not exist" >&2
    exit 1
fi

# 2. `answer: return interpretation_portfolio` is honored: the result
#    carries one entry per kept candidate, and `keep: pareto 8` keeps all
#    five admitted worlds (three fixture worlds plus the csa_seeded and
#    one_point builtin seeds; no single-winner collapse).
RESULT="$(sed -n 's/.*"result": "\(.*\)".*/\1/p' "$GEN_DIR/answer-receipt.json")"
for entry in "free_symbolic:apply" "Boolean_algebra:false" "modular_numeric:6" "csa_seeded:" "one_point:"; do
    case "$RESULT" in
        *"$entry"*) ;;
        *)
            echo "FAIL: portfolio answer missing $entry: $RESULT" >&2
            exit 1
            ;;
    esac
done
CANDIDATE_COUNT="$("$PYTHON" -c "import json,sys;print(len(json.load(open(sys.argv[1]))['candidates']))" "$GEN_DIR/interpretation-portfolio.json")"
if [ "$CANDIDATE_COUNT" != "5" ]; then
    echo "FAIL: keep: pareto 8 must keep all five candidates, kept $CANDIDATE_COUNT" >&2
    exit 1
fi

# 3. Genesis answers agree with the parametric lane (`compile --parametric`
#    compiles the identical fixture-valued evaluators): run the generated
#    crate and diff its printed values against the receipt answers.
PARAM_DIR="$TMP_DIR/param"
mkdir -p "$PARAM_DIR"
cargo run -q -p emath-cli -- compile --parametric language/examples/arbitrary-glyphs.emath \
    --out "$PARAM_DIR" >/dev/null
PARAM_OUT="$TMP_DIR/param-out.txt"
(cd "$PARAM_DIR" && cargo run -q) >"$PARAM_OUT"
"$PYTHON" - "$GEN_DIR/answer-receipt.json" "$PARAM_OUT" <<'PY'
import json, re, sys
receipt = json.load(open(sys.argv[1]))
answers = dict(item.split(":", 1) for item in receipt["result"].split(";"))
param = {}
for line in open(sys.argv[2]):
    m = re.match(r"^(free|boolean|modular-17): (.*)$", line.strip())
    if m:
        param[m.group(1)] = m.group(2)
expect = {
    "free_symbolic": "free",
    "Boolean_algebra": "boolean",
    "modular_numeric": "modular-17",
}
ok = True
for name, param_key in expect.items():
    if answers.get(name) != param.get(param_key):
        print(
            f"FAIL: genesis answer {name}={answers.get(name)!r} != "
            f"parametric {param_key}={param.get(param_key)!r}",
            file=sys.stderr,
        )
        ok = False
sys.exit(0 if ok else 1)
PY
echo "genesis receipts agree with compile --parametric values"

# 4. Negative: `keep: pareto 1` must keep one candidate and never present
#    three equally-scored winners as a single tested answer.
GEN1_DIR="$TMP_DIR/genesis-keep1"
mkdir -p "$GEN1_DIR"
sed 's/pareto 8/pareto 1/' language/examples/arbitrary-glyphs.emath >"$TMP_DIR/keep1.emath"
cargo run -q -p emath-cli -- genesis "$TMP_DIR/keep1.emath" --out "$GEN1_DIR" >/dev/null
KEEP1_COUNT="$("$PYTHON" -c "import json,sys;print(len(json.load(open(sys.argv[1]))['candidates']))" "$GEN1_DIR/interpretation-portfolio.json")"
if [ "$KEEP1_COUNT" != "1" ]; then
    echo "FAIL: keep: pareto 1 must keep exactly one candidate, kept $KEEP1_COUNT" >&2
    exit 1
fi
if grep -q '"authority"[[:space:]]*:[[:space:]]*"tested"' "$GEN1_DIR/answer-receipt.json"; then
    echo "FAIL: keep: pareto 1 receipt still claims tested authority" >&2
    exit 1
fi
KEEP1_RESULT="$(sed -n 's/.*"result": "\(.*\)".*/\1/p' "$GEN1_DIR/answer-receipt.json")"
if printf '%s' "$KEEP1_RESULT" | grep -q ';'; then
    echo "FAIL: keep: pareto 1 answer must be a single candidate, got $KEEP1_RESULT" >&2
    exit 1
fi

# 5. `keep: pareto 0` keeps nothing and is a typed refusal, not an empty
#    or winner-less artifact.
sed 's/pareto 8/pareto 0/' language/examples/arbitrary-glyphs.emath >"$TMP_DIR/keep0.emath"
if KEEP0_OUT="$(cargo run -q -p emath-cli -- genesis "$TMP_DIR/keep0.emath" --out "$TMP_DIR/genesis-keep0" 2>&1)"; then
    echo "FAIL: keep: pareto 0 genesis succeeded" >&2
    exit 1
fi
if ! printf '%s\n' "$KEEP0_OUT" | grep -q "E-GEN-093"; then
    echo "FAIL: keep: pareto 0 must refuse with E-GEN-093" >&2
    printf '%s\n' "$KEEP0_OUT" >&2
    exit 1
fi
if [ ! -f "$GEN_DIR/g7-portfolio-receipt.txt" ]; then
    echo "FAIL: genesis must write g7-portfolio-receipt.txt" >&2
    exit 1
fi
if ! grep -q "policy=portfolio" "$GEN_DIR/g7-portfolio-receipt.txt"; then
    echo "FAIL: G7 receipt must record the explicit portfolio policy" >&2
    cat "$GEN_DIR/g7-portfolio-receipt.txt" >&2
    exit 1
fi
sed 's/interpretation_portfolio/best/' language/examples/arbitrary-glyphs.emath >"$TMP_DIR/hidden-winner.emath"
if HIDDEN_OUT="$(cargo run -q -p emath-cli -- genesis "$TMP_DIR/hidden-winner.emath" --out "$TMP_DIR/genesis-hidden" 2>&1)"; then
    echo "FAIL: genesis without interpretation_portfolio collapsed to a single winner" >&2
    printf '%s\n' "$HIDDEN_OUT" >&2
    exit 1
fi
if ! printf '%s\n' "$HIDDEN_OUT" | grep -q "E-GEN-095"; then
    echo "FAIL: hidden single-winner collapse must refuse with E-GEN-095" >&2
    printf '%s\n' "$HIDDEN_OUT" >&2
    exit 1
fi
echo "genesis honesty: no invented tested meaning, pareto honored, no hidden winner"

lane_done "validate" "gate" "passed" "all lanes green"
echo "validate.sh: ok"
