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

# End a lane with a JSONL record. <status> is one of
# passed/failed/skipped/xfail (xfail = expected failure bound to a
# known discrepancy). An optional 5th argument is the discrepancy id,
# emitted as the `discrepancy_id` field so ail-style compliance
# consumers can distinguish acknowledged divergences from skips.
lane_done() {
    local suite="$1" phase="$2" status="$3" detail="${4:-}" disc="${5:-}"
    local ms=$(( (SECONDS - LANE_START_SECONDS) * 1000 ))
    LANE_START_SECONDS="$SECONDS"
    if [ -n "$disc" ]; then
        jsonl_write "suite" "$suite" "phase" "$phase" "status" "$status" \
            "detail" "$detail" "discrepancy_id" "$disc" "duration_ms" "$ms"
    else
        jsonl_write "suite" "$suite" "phase" "$phase" "status" "$status" "detail" "$detail" "duration_ms" "$ms"
    fi
}

# Durable lane-count summary (retained, minimal): one JSONL record with
# pass/fail/skip/xfail counts for the whole run, also emitted on success
# so compliance consumers keep a summary even though lane log lines are
# removed with the workdir. Written next to the lane log.
summary_done() {
    local passed=0 failed=0 skipped=0 xfailed=0
    while IFS= read -r rec; do
        case "$rec" in
            *'"status":"passed"'*) passed=$((passed + 1)) ;;
            *'"status":"failed"'*) failed=$((failed + 1)) ;;
            *'"status":"skipped"'*) skipped=$((skipped + 1)) ;;
            *'"status":"xfail"'*) xfailed=$((xfailed + 1)) ;;
        esac
    done <"$JSONL"
    printf '%s\n' "{\"summary\":\"validate.sh\",\"passed\":$passed,\"failed\":$failed,\"skipped\":$skipped,\"xfailed\":$xfailed}" >>"$TMP_DIR/summary.jsonl"
}

on_exit() {
    local code=$?
    if [ "$code" -eq 0 ]; then
        # Success still surfaces the compliance summary: printed to
        # stdout (ail-style printed-markdown variant) and, when
        # EMATH_VALIDATE_RETAIN_SUMMARY names a path, copied there so a
        # consumer keeps it even though lane log lines are cleaned up.
        if [ -f "$TMP_DIR/summary.jsonl" ]; then
            cat "$TMP_DIR/summary.jsonl"
            if [ -n "${EMATH_VALIDATE_RETAIN_SUMMARY:-}" ]; then
                cp "$TMP_DIR/summary.jsonl" "$EMATH_VALIDATE_RETAIN_SUMMARY"
            fi
        fi
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
    summary_done
    echo "logger self-test: forced mismatch records JSONL, keeps the diff, retains the workdir"
    exit 0
fi

echo "== compile/lint lanes run via DSR, not here =="
echo "(per AGENTS.md, NEVER run full cargo tests; fmt/clippy run through DSR
when available or as narrow, labeled cargo checks; this gate proves only the
artifact, negative-control, capstone, and doc lanes.)"

echo "== fork-type identity gate (AGENTS.md rule 1) =="
# The firewall forbids fork-NATIVE TYPES/symbols in crates — not
# identity STRINGS. `crates/emath-provider-api/src/constellation.rs` is the
# neutral provider census (the declared single registration point):
# `upstream_id` there is the repository id in forks/UPSTREAM_LOCK.json, and
# plans reference providers by id strings only (module doc). CONTRACT.md
# prose describes the same census. Both are exempt as identity DATA; every
# other line of every scanned crate still refuses fork identifiers.
if grep -rniE '(^|[^a-z0-9_.-])(dew|rumoca|wrenfold|franken|modelica)([^a-z0-9_.-]|$)' \
    crates/emath-core crates/emath-ir crates/emath-plan \
    crates/emath-sema crates/emath-rt crates/emath-provider-api \
    crates/emath-artifact examples/provider-skeleton/src/main.rs \
    --exclude=constellation.rs --exclude=CONTRACT.md \
    >"$TMP_DIR/fork-grep.txt"; then
    echo "FAIL: upstream fork-type identifier leaked into a native crate or schema:" >&2
    cat "$TMP_DIR/fork-grep.txt" >&2
    exit 1
fi
echo "no fork-type identifiers in native crates or durable schemas (census identity strings exempt by design)"

# Cut with tests/valid/affine_scorer.emath (dead-lane cut a9b5c3d) and the
# `build --verify` flag (fit-goal surface cut bb38f0d). The generated-crate
# host lane (demo-host-independent) went with the affine-scorer artifact
# (2026-09-23).

# Cut: the `emath-lab provider` command was removed by the constructor
# cutover; the provider census (constellation.rs) is pinned by
# tests/emath-provider-api (provider_constellation).

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
assert_invalid tests/invalid/model_decl.emath "E-KIND-GONE"
assert_invalid tests/invalid/lagrangian_action_fence.emath "E-SYN-101"
assert_invalid tests/invalid/unknown_section.emath "E-SEC-101"
assert_invalid tests/invalid/l3_outputs_without_inputs.emath "E-SEC-130"
assert_invalid tests/invalid/l3_definitions_shadow_input.emath "E-NAME-020"
assert_invalid tests/invalid/exports_junk.emath "E-SYN-101"
assert_invalid tests/invalid/compile_junk.emath "E-SYN-101"
assert_invalid tests/invalid/function_type.emath "E-TYPE-110"
# Unicode honesty lane: a declaration spelled with a Cyrillic lookalike
# of an already-seen Latin name is refused (E-NAME-024), and an
# identifier built from a combining mark (non-NFC by construction) is
# refused at the lexer (E-SYN-115).
assert_invalid tests/invalid/confusable_decl.emath "E-NAME-024"
assert_invalid tests/invalid/combining_mark.emath "E-SYN-115"
# Fit goals are gone with their Rust lane: `fit <params> to
# <observable>:` now refuses as an out-of-subset request kind
# (E-GOAL-043); least-squares fitting is authored `.emath`
# (numerics.levenberg), not compiler machinery.
assert_invalid tests/invalid/empty.emath "E-PKG-081"

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

# Positive control: current-syntax `emath function` is the current
# function lane (a real admit), not a hollow acceptance.
if ! cargo run -q -p emath-cli -- check tests/valid/formatter_square.emath >/dev/null 2>&1; then
    echo "FAIL: formatter_square.emath (function lane) refused" >&2
    exit 1
fi
echo "square.emath function lane admits"

echo "== lossless fmt gate =="
# Every valid corpus file must be byte-canonical under the lossless
# formatter (fmt(file) == file); a drift is a real round-trip break.
for FIXTURE in tests/valid/formatter_square.emath tests/valid/formatter_rationals.emath; do
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
awk 'NR==7 { print "" } { print }' tests/valid/formatter_square.emath >"$UNFORMATTED"
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

echo "== conformance register =="
# Language spec pin: language/reference/** + language/grammar/** are
# SHA-pinned under an edition id in implementation/SPEC_PIN.json; drift
# without a named bump fails the gate (RULE 0.3: unpinned language
# claims are bugs).
lane_begin
if ! SPEC_OUT="$("$PYTHON" scripts/check_spec_pin.py 2>&1)"; then
    echo "FAIL: a language spec file drifted without a named edition bump" >&2
    printf '%s\n' "$SPEC_OUT" >&2
    lane_done "conformance-register" "spec-pin" "failed" "spec file drifted without a named bump"
    exit 1
fi
printf '%s\n' "$SPEC_OUT"
lane_done "conformance-register" "spec-pin" "passed" "language spec files match the pinned edition"
# Upstream lock honesty: schema validation + adapter-seam commit binding
# (dew seam.rs const, rumoca CONTRACT.md no-claim fence).
lane_begin
if ! LOCK_OUT="$("$PYTHON" scripts/check_upstream_lock.py 2>&1)"; then
    echo "FAIL: upstream lock failed schema or seam-binding honesty" >&2
    printf '%s\n' "$LOCK_OUT" >&2
    lane_done "conformance-register" "upstream-lock" "failed" "lock schema or seam binding drifted"
    exit 1
fi
printf '%s\n' "$LOCK_OUT"
lane_done "conformance-register" "upstream-lock" "passed" "lock schema-valid; adapter seams bound to lock commits"
echo "conformance register: spec pin + upstream lock green"

echo "== ELP pipeline gates =="
# ELP document-shape gate: every elps/ELP-NNNN-<slug>.md must carry the
# seven canonical sections and a four-artifact plan; the template's
# seven-section shape is validated too.
lane_begin
if ! ELP_OUT="$("$PYTHON" scripts/check_elp.py 2>&1)"; then
    echo "FAIL: an ELP document failed the shape gate" >&2
    printf '%s\n' "$ELP_OUT" >&2
    lane_done "elp" "shape" "failed" "ELP document shape drift"
    exit 1
fi
printf '%s\n' "$ELP_OUT"
lane_done "elp" "shape" "passed" "ELP documents match the seven-section shape"
# Negative control: a copy with a placeholder section must fail.
lane_begin
BAD_ELP_DIR="$TMP_DIR/bad-elp"
mkdir -p "$BAD_ELP_DIR"
cp elps/ELP-TEMPLATE.md "$BAD_ELP_DIR/"
printf '# ELP-0001-neg-control\n\n## Motivation and coverage claim\n\nTODO placeholder\n\n## Grammar delta\n\n...\n\n## Lowering and world interactions\n\n...\n\n## Meaning-preservation analysis\n\n...\n\n## Migration\n\n...\n\n## Four-artifact plan\n\n...\n\n## Refusals\n\n...\n' >"$BAD_ELP_DIR/ELP-0001-neg-control.md"
if "$PYTHON" scripts/check_elp.py "$BAD_ELP_DIR" >/dev/null 2>&1; then
    echo "FAIL: placeholder ELP admitted by the shape gate" >&2
    lane_done "elp" "negative-control" "failed" "placeholder ELP admitted"
    exit 1
fi
lane_done "elp" "negative-control" "passed" "placeholder ELP refused"
echo "elp: shape gate green; placeholder ELP refused"

echo "== Grammar audit battery =="
GATE_GRAMMARS="language/grammar/surface.ebnf language/grammar/genesis.ebnf"
AMBIGUITY_BASELINE="tests/language-gates/fixtures/ambiguity-baseline.json"
# Ambiguity: the shipped grammar is pinned (31 design decisions); only
# NEW conflict signatures fail the gate.
lane_begin
if ! AMBIGUITY_OUT="$("$PYTHON" scripts/ambiguity_scan.py --baseline "$AMBIGUITY_BASELINE" $GATE_GRAMMARS 2>&1)"; then
    echo "FAIL: ambiguity-scan found NEW grammar conflicts" >&2
    printf '%s\n' "$AMBIGUITY_OUT" >&2
    lane_done "ambiguity-scan" "baseline" "failed" "new ambiguity signatures"
    exit 1
fi
lane_done "ambiguity-scan" "baseline" "passed" "no new ambiguity signatures"
# Negative control: an identical-alternative delta must be caught.
lane_begin
if "$PYTHON" scripts/ambiguity_scan.py --delta tests/language-gates/diffs/ambiguity-identical.diff \
    --baseline "$AMBIGUITY_BASELINE" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: ambiguity-scan admitted identical alternatives" >&2
    lane_done "ambiguity-scan" "negative-control" "failed" "identical alternatives admitted"
    exit 1
fi
# Positive control: an additive delta with a disjoint first set passes.
if ! "$PYTHON" scripts/ambiguity_scan.py --delta tests/language-gates/diffs/ambiguity-clean.diff \
    --baseline "$AMBIGUITY_BASELINE" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: ambiguity-scan refused an ambiguity-free additive delta" >&2
    lane_done "ambiguity-scan" "positive-control" "failed" "clean delta refused"
    exit 1
fi
lane_done "ambiguity-scan" "negative-control" "passed" "identical alternatives refused; clean delta passes"
echo "ambiguity-scan: baseline pinned; bad delta refused, clean delta passes"

# Confusable scan: NFC + sema-aligned fold over every grammar literal.
lane_begin
if ! CONFUSABLE_OUT="$("$PYTHON" scripts/confusable_scan.py $GATE_GRAMMARS 2>&1)"; then
    echo "FAIL: confusable-scan found glyph collisions" >&2
    printf '%s\n' "$CONFUSABLE_OUT" >&2
    lane_done "confusable-scan" "scan" "failed" "glyph collision or NFC drift"
    exit 1
fi
printf '%s\n' "$CONFUSABLE_OUT"
lane_done "confusable-scan" "scan" "passed" "no confusable collisions"
# Negative control: a Cyrillic lookalike of an existing glyph must be
# refused.
lane_begin
if "$PYTHON" scripts/confusable_scan.py --delta tests/language-gates/diffs/confusable-lookalike.diff \
    $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: confusable-scan admitted a Cyrillic lookalike glyph" >&2
    lane_done "confusable-scan" "negative-control" "failed" "lookalike glyph admitted"
    exit 1
fi
lane_done "confusable-scan" "negative-control" "passed" "lookalike delta refused"
echo "confusable-scan: shipped glyphs clean; lookalike delta refused"

# Precedence boundary corpus: the regenerated fixture must equal the
# committed one (drift gate); a tampered fixture must fail --check.
PRECEDENCE_FIXTURE="tests/language-gates/fixtures/precedence-boundary.corpus"
lane_begin
if ! PRECEDENCE_OUT="$("$PYTHON" scripts/precedence_battery.py --check --out "$PRECEDENCE_FIXTURE" $GATE_GRAMMARS 2>&1)"; then
    echo "FAIL: precedence-battery fixture drifted" >&2
    printf '%s\n' "$PRECEDENCE_OUT" >&2
    lane_done "precedence-battery" "drift" "failed" "boundary corpus drifted"
    exit 1
fi
printf '%s\n' "$PRECEDENCE_OUT"
lane_done "precedence-battery" "drift" "passed" "boundary corpus current"
lane_begin
TAMPERED_P="$TMP_DIR/tampered.corpus"
cp "$PRECEDENCE_FIXTURE" "$TAMPERED_P"
printf 'boundary: a + a + a\n' >>"$TAMPERED_P"
if "$PYTHON" scripts/precedence_battery.py --check --out "$TAMPERED_P" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: precedence-battery accepted a tampered corpus" >&2
    lane_done "precedence-battery" "negative-control" "failed" "tampered corpus admitted"
    exit 1
fi
lane_done "precedence-battery" "negative-control" "passed" "tampered corpus refused"
echo "precedence-battery: fixture current; tampered corpus refused"
# Hidden-interpretation scan (grammar audit sub-test 4): every multi-role glyph in
# the shipped grammar must be registered with the policy that pins its
# meaning (parser-context | worlds-machinery | typed-refusal). A delta
# that gives a glyph a second role without registering it — or drifts a
# registered glyph's role set — is the "one glyph, many meanings" class.
HIDDEN_BASELINE="tests/language-gates/fixtures/hidden-interpretation-baseline.json"
lane_begin
if ! HIDDEN_OUT="$("$PYTHON" scripts/hidden_interpretation_scan.py --baseline "$HIDDEN_BASELINE" $GATE_GRAMMARS 2>&1)"; then
    echo "FAIL: hidden-interpretation-scan found unregistered multi-role glyphs" >&2
    printf '%s\n' "$HIDDEN_OUT" >&2
    lane_done "hidden-interpretation-scan" "baseline" "failed" "unregistered role drift"
    exit 1
fi
lane_done "hidden-interpretation-scan" "baseline" "passed" "every multi-role glyph registered"
# Negative control: a glyph introduced in two productions without a
# registry entry is refused.
if "$PYTHON" scripts/hidden_interpretation_scan.py \
    --delta tests/language-gates/diffs/hidden-interpretation-newrole.diff \
    --baseline "$HIDDEN_BASELINE" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: hidden-interpretation-scan admitted an unregistered multi-role glyph" >&2
    lane_done "hidden-interpretation-scan" "negative-control" "failed" "multi-role glyph admitted"
    exit 1
fi
# Negative control: a registered glyph gaining a new role is refused.
if "$PYTHON" scripts/hidden_interpretation_scan.py \
    --delta tests/language-gates/diffs/hidden-interpretation-drift.diff \
    --baseline "$HIDDEN_BASELINE" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: hidden-interpretation-scan admitted a registered glyph's role drift" >&2
    lane_done "hidden-interpretation-scan" "negative-control" "failed" "role drift admitted"
    exit 1
fi
# Positive control: a single-role glyph addition needs no registration.
if ! "$PYTHON" scripts/hidden_interpretation_scan.py \
    --delta tests/language-gates/diffs/hidden-interpretation-clean.diff \
    --baseline "$HIDDEN_BASELINE" $GATE_GRAMMARS >/dev/null 2>&1; then
    echo "FAIL: hidden-interpretation-scan refused a single-role glyph addition" >&2
    lane_done "hidden-interpretation-scan" "positive-control" "failed" "clean delta refused"
    exit 1
fi
lane_done "hidden-interpretation-scan" "controls" "passed" "multi-role and drift refused; clean delta passes"
echo "hidden-interpretation-scan: registry current; multi-role/drift deltas refused"

echo "== four-artifact rule =="
# The four-artifact rule applies to the change UNDER GATE, not the last
# commit: the current tree delta = staged + unstaged + untracked paths
# (excluding the .rch/ daemon dir), so an uncommitted grammar-only
# change fails here.
lane_begin
FOUR_ART_LIST="$TMP_DIR/four-art-files.txt"
"$PYTHON" - "$FOUR_ART_LIST" <<'PY'
import subprocess, sys
out = subprocess.run(
    ["git", "status", "--porcelain", "-z"],
    check=True, capture_output=True, text=True,
).stdout
entries = out.split("\0")
paths = []
i = 0
while i < len(entries):
    entry = entries[i]
    i += 1
    if not entry:
        continue
    status = entry[:2]
    if status[0] in ("R", "C"):
        # The destination path is the next NUL-separated entry.
        path = entries[i] if i < len(entries) else ""
        i += 1
    else:
        path = entry[3:]
    if status[0] == "D" or status[1] == "D":
        continue
    if path.startswith(".rch/"):
        continue
    paths.append(path)
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    fh.write("\n".join(sorted(set(paths))) + "\n")
PY
: >"$TMP_DIR/four-art-msg.txt"
if ! "$PYTHON" scripts/check_four_artifact.py --files-from "$FOUR_ART_LIST" --head-msg "$TMP_DIR/four-art-msg.txt" >/dev/null 2>&1; then
    echo "FAIL: the working-tree change violates the four-artifact rule" >&2
    "$PYTHON" scripts/check_four_artifact.py --files-from "$FOUR_ART_LIST" --head-msg "$TMP_DIR/four-art-msg.txt" >&2 || true
    lane_done "four-artifact" "tree-delta" "failed" "grammar change without reference/examples/tests companions"
    exit 1
fi
lane_done "four-artifact" "tree-delta" "passed" "tree delta satisfies the four-artifact rule"
# Negative control: a synthetic grammar-only change must fail the gate.
lane_begin
printf 'language/grammar/surface.ebnf\n' >"$TMP_DIR/grammar-only.txt"
if "$PYTHON" scripts/check_four_artifact.py --files-from "$TMP_DIR/grammar-only.txt" --head-msg "$TMP_DIR/four-art-msg.txt" >/dev/null 2>&1; then
    echo "FAIL: grammar-only change passed the four-artifact gate" >&2
    lane_done "four-artifact" "negative-control" "failed" "grammar-only change admitted"
    exit 1
fi
lane_done "four-artifact" "negative-control" "passed" "grammar-only change refused"
echo "four-artifact: tree delta compliant; grammar-only change refused"

# Cut: the `planner` command was removed by the constructor cutover.

# Cut: the `emath-lab bench` and `agent` commands were removed by the
# constructor cutover; the E-TLT-004 refusal text is pinned by the
# cli-lab source and its tests.

echo "== language examples admit-or-refuse loop =="
# Every language example must either admit (then build honestly) or be
# refused with a documented E-code: nothing in the corpus is silently
# accepted or refused without a stable code.
lane_begin
for FIXTURE in language/examples/*/*.emath; do
    if cargo run -q -p emath-cli -- check "$FIXTURE" >/dev/null 2>&1; then
        # Admitted examples must build honestly OR refuse with a typed
        # code (multi-function teaching files build with one named
        # entry per function); a silent build failure is the escape
        # this catches.
        if ! cargo run -q -p emath-cli -- build "$FIXTURE" --out "$TMP_DIR/examples" >/dev/null 2>&1; then
            BUILD_OUT="$(cargo run -q -p emath-cli -- build "$FIXTURE" --out "$TMP_DIR/examples" 2>&1 || true)"
            if ! printf '%s\n' "$BUILD_OUT" | grep -qE "E-[A-Z]+-[0-9A-Z]+"; then
                echo "FAIL: admitted example failed to build without a typed code: $FIXTURE" >&2
                printf '%s\n' "$BUILD_OUT" >&2
                lane_done "examples" "admit-or-refuse" "failed" "$FIXTURE built without a code"
                exit 1
            fi
        fi
    else
        EX_OUT="$(cargo run -q -p emath-cli -- check "$FIXTURE" 2>&1 || true)"
        if ! printf '%s\n' "$EX_OUT" | grep -qE "E-[A-Z]+-[0-9A-Z]+"; then
            echo "FAIL: refused example emitted no documented code: $FIXTURE" >&2
            printf '%s\n' "$EX_OUT" >&2
            lane_done "examples" "admit-or-refuse" "failed" "$FIXTURE refused without code"
            exit 1
        fi
    fi
done
lane_done "examples" "admit-or-refuse" "passed" "every example admits+builds or refuses with a code"
echo "language examples: admitted or refused with documented codes"

# Cut: the numerical corpus oracles (spring/heat-plate final rows, the
# scratch E-GOAL-043 run) died with their fixtures when
# tests/fixtures/language was removed in the restructure. The
# admit-or-refuse loop above still governs the teaching corpus.

# Causalized residual models are now fully codegen-able: the example is
# admitted by the loop above and builds a crate whose step_euler/step_rk4
# embed the causalized Newton solve (parity with `emath simulate` is
# pinned by tests/emath-rust-backend + the examples lane).

# Cut: produce_dew_jit.emath was deleted with the other dead-lane fixtures.

# Cut: the `emath artifact` command (battery/check) was removed by the
# constructor cutover.

# Cut: both xtask capstone demos drive fixtures and flags that were
# removed (affine_scorer.emath, `build --verify`, `compile --parametric`).

# Cut: the demo-host-independent host consumer was removed with its
# committed generated crate (affine-scorer, 2026-09-23) - the crate was
# orphaned once its source fixture and the byte-identical regeneration
# lane went with the a9b5c3d dead-lane cut. No generated-crate host
# lane remains.

# Cut: the semantic-genesis capstone family (`compile --parametric`, `genesis`,
# arbitrary-glyphs.emath) was removed by the constructor cutover and the
# dead-lane fixture cut; the committed generated crate stays pinned by the
# demo-host-independent behavioral asserts above.

lane_done "validate" "gate" "passed" "all lanes green"
summary_done
echo "validate.sh: ok"
