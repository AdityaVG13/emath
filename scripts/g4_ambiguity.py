#!/usr/bin/env python3
"""G4 ambiguity scan for emath grammar deltas (ELP gate).

Parses the repository's EBNF dialect (productions `name = rhs ;`,
`(* ... *)` block comments, `--` line comments) and detects, on the
merged grammar (base files + optional unified diff):

- duplicate production definitions (the same LHS defined twice),
- first-set conflicts between alternatives of one production — two
  alternatives that can begin with the same literal means a sentence
  with two parses for a predictive parser (fatal),
- nullable alternatives sitting beside non-nullable siblings (warning:
  may hide parses; requires review).

This is the mechanical first-pass battery for the C2-C15 class of
governance bugs; it is not a full ambiguity proof. The ELP procedure
(`elps/README.md`) pairs it with the boundary corpus and human review.

Usage:
    g4_ambiguity.py [--delta DELTA.diff] [grammar.ebnf ...]

Exit: 0 clean, 1 ambiguity/redefinition found, 2 usage error.
Stdlib only; deterministic.
"""

import argparse
import re
import sys
from dataclasses import dataclass, field


@dataclass
class Production:
    name: str
    alternatives: list  # list[list[Term]]
    line: int

    @property
    def nullable(self) -> bool:
        return any(_alt_nullable(alt) for alt in self.alternatives)


@dataclass
class Term:
    kind: str  # literal | ref | optional | repeat | group | special | empty
    value: object = None
    line: int = 0


_COMMENT_BLOCK = re.compile(r"\(\*.*?\*\)", re.DOTALL)
# EBNF identifiers may carry a hyphen (`genesis-section`).
_IDENT = r"[A-Za-z_][A-Za-z0-9_-]*"


def strip_comments(text: str) -> str:
    text = _COMMENT_BLOCK.sub(" ", text)
    lines = []
    for line in text.splitlines():
        # `--` line comments; strip only outside quotes.
        out = []
        quote = None
        i = 0
        while i < len(line):
            ch = line[i]
            if quote:
                out.append(ch)
                if ch == quote and (i == 0 or line[i - 1] != "\\"):
                    quote = None
            elif ch in "\"'":
                quote = ch
                out.append(ch)
            elif ch == "-" and line[i : i + 2] == "--":
                break
            else:
                out.append(ch)
            i += 1
        lines.append("".join(out))
    return "\n".join(lines)


def split_top_level(text: str, sep: str = "|") -> list:
    """Split on a separator outside quotes/brackets/braces/parens."""
    parts = []
    depth = 0
    quote = None
    start = 0
    i = 0
    while i < len(text):
        ch = text[i]
        if quote:
            if ch == quote and (i == 0 or text[i - 1] != "\\"):
                quote = None
        elif ch in "\"'":
            quote = ch
        elif ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth = max(0, depth - 1)
        elif ch == sep and depth == 0:
            parts.append(text[start:i])
            start = i + 1
        i += 1
    parts.append(text[start:])
    return parts


def scan_terms(rhs: str, line: int) -> list:
    """Tokenize one alternative into terms, left to right.

    Handles both `? description ?` terminals and the ISO suffix `?`
    (optionality) that directly follows a term: `":"?`, `( ... )?`.
    """
    terms = []
    i = 0
    n = len(rhs)
    while i < n:
        ch = rhs[i]
        if ch.isspace():
            i += 1
            continue
        if ch == "?":
            # Postfix optionality when it directly abuts the previous term;
            # a `? ... ?` pair is a special terminal otherwise.
            ends_adjacent = i > 0 and not rhs[i - 1].isspace() and bool(terms)
            if ends_adjacent:
                wrapped = terms.pop()
                terms.append(Term("opt", wrapped, line))
                i += 1
                continue
            end = rhs.find("?", i + 1)
            if end < 0:
                raise ValueError(f"unterminated ?description? at line {line}")
            terms.append(Term("special", rhs[i + 1 : end].strip(), line))
            i = end + 1
        elif ch in "\"'":
            end = i + 1
            while end < n:
                if rhs[end] == ch and rhs[end - 1] != "\\":
                    break
                end += 1
            if end >= n:
                raise ValueError(f"unterminated literal at line {line}")
            terms.append(Term("literal", rhs[i + 1 : end], line))
            i = end + 1
        elif ch in "[{(":
            closer = { "[": "]", "{": "}", "(": ")" }[ch]
            depth = 1
            end = i + 1
            quote = None
            while end < n and depth > 0:
                c = rhs[end]
                if quote:
                    if c == quote and rhs[end - 1] != "\\":
                        quote = None
                elif c in "\"'":
                    quote = c
                elif c == ch:
                    depth += 1
                elif c == closer:
                    depth -= 1
                end += 1
            if depth != 0:
                raise ValueError(f"unbalanced {ch}{closer} at line {line}")
            inner = rhs[i + 1 : end - 1]
            kind = { "[": "optional", "{": "repeat", "(": "group" }[ch]
            terms.append(Term(kind, inner, line))
            i = end
        else:
            m = re.match(_IDENT, rhs[i:])
            if not m:
                # Unexpected punctuation inside an alternative: treat as
                # an opaque literal so scanning stays total.
                terms.append(Term("literal", rhs[i], line))
                i += 1
                continue
            terms.append(Term("ref", m.group(0), line))
            i += m.end()
    if not terms:
        terms.append(Term("empty", None, line))
    return terms


def _scan_body(text: str, start: int):
    """Read a production body up to the top-level `;`.

    Returns (body, end_index, end_line). Quote/group aware so a `;` inside
    a literal never terminates the body early.
    """
    depth = 0
    quote = None
    line = 1
    i = start
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "\n":
            line += 1
        if quote:
            if ch == quote and (i == 0 or text[i - 1] != "\\"):
                quote = None
        elif ch in "\"'":
            quote = ch
        elif ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth = max(0, depth - 1)
        elif ch == ";" and depth == 0:
            return text[start:i], i, line
        i += 1
    raise ValueError("unterminated production body")


def parse_grammar(text: str, source: str) -> dict:
    """Char-walking production scanner: `name = body ;`, bodies may span
    lines and contain literals, groups, options, and repeats."""
    stripped = strip_comments(text)
    productions = {}
    i = 0
    n = len(stripped)
    line = 1
    while i < n:
        ch = stripped[i]
        if ch in " \t\r\n":
            if ch == "\n":
                line += 1
            i += 1
            continue
        if ch in "\"'":
            # Standalone literal line (e.g. a comment artifact); skip to its end.
            quote = ch
            i += 1
            while i < n:
                if stripped[i] == quote and stripped[i - 1] != "\\":
                    i += 1
                    break
                if stripped[i] == "\n":
                    line += 1
                i += 1
            continue
        m = re.match(_IDENT, stripped[i:])
        if not m:
            i += 1  # stray punctuation outside a production; skip
            continue
        name = m.group(0)
        j = i + m.end()
        while j < n and stripped[j] in " \t":
            j += 1
        if j >= n or stripped[j] != "=":
            i = j
            continue
        body, end, body_line = _scan_body(stripped, j + 1)
        prod_line = line
        if name in productions:
            raise ValueError(f"{source}: duplicate production `{name}`")
        alternatives = []
        for alt in split_top_level(body):
            alt = alt.strip()
            if not alt:
                alternatives.append([Term("empty", None, prod_line)])
                continue
            alternatives.append(scan_terms(alt, prod_line))
        productions[name] = Production(name, alternatives, prod_line)
        line += body.count("\n")
        i = end + 1
    return productions


def diff_targets(diff: str, path: str) -> bool:
    """Whether a unified diff's `--- ` header names `path`.

    Diffs without a header are treated as legacy insertion-only hunks
    and applied everywhere (the original G4 behavior). Diffs with a
    header are only applied to their named file, so a real EBNF diff
    targeting `surface.ebnf` never gets replayed against `genesis.ebnf`."""
    for raw in diff.splitlines():
        if raw.startswith("--- "):
            target = raw[4:].split("\t")[0].strip()
            if target.startswith(("a/", "b/")):
                target = target[2:]
            if target == path:
                return True
            return target.rsplit("/", 1)[-1] == path.rsplit("/", 1)[-1]
    return True


def apply_diff(base: str, diff: str) -> str:
    """Apply a unified diff to grammar text.

    Hunks use standard unified-diff semantics: `@@ -old,count +new,count @@`
    with context, `-`, and `+` lines; each hunk is walked in order so
    context lines advance both sides and removals/insertions apply at the
    exact original position. Hunks are applied from the end backwards so
    earlier line numbers stay valid.
    """
    lines = base.splitlines(keepends=True)
    hunks = []
    current = None
    for raw in diff.splitlines(keepends=True):
        if raw.startswith(("--- ", "+++ ", "diff ", "index ")):
            continue
        m = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", raw)
        if m:
            current = {"start": int(m.group(1)), "ops": []}
            hunks.append(current)
            continue
        if current is None:
            continue
        if raw.startswith("+"):
            current["ops"].append(("+", raw[1:]))
        elif raw.startswith("-"):
            current["ops"].append(("-", raw[1:]))
        else:
            current["ops"].append((" ", raw[1:]))
    for hunk in reversed(hunks):
        index = hunk["start"] - 1
        for op, text in hunk["ops"]:
            if op == " ":
                index += 1
            elif op == "-":
                if index >= len(lines) or lines[index] != text:
                    raise ValueError(f"diff context mismatch at line {index + 1}")
                del lines[index]
            elif op == "+":
                lines.insert(index, text)
                index += 1
    return "".join(lines)


def first_set(name: str, productions: dict, seen: set) -> set:
    """First terminals of a production (recursion-guarded)."""
    if name not in productions:
        return {f"?{name}?"}
    key = f"{name}:{seen}"
    acc = set()
    for alt in productions[name].alternatives:
        acc |= seq_first(alt, productions, seen)[0]
    return acc


def term_first(term: Term, productions: dict, seen: set) -> tuple:
    """(firsts, nullable) for one term."""
    if term.kind == "literal":
        return ({term.value}, False)
    if term.kind == "special":
        return ({f"?{term.value}?"}, False)
    if term.kind == "ref":
        if term.value in seen:
            return (set(), True)  # recursion: approximate as nullable
        firsts = first_set(term.value, productions, seen | {term.value})
        target = productions.get(term.value)
        nullable = bool(target) and all(
            _alt_nullable(alt) for alt in target.alternatives
        )
        return (firsts, nullable)
    if term.kind == "empty":
        return (set(), True)
    if term.kind == "optional":
        firsts, _ = seq_first(scan_terms(term.value, term.line), productions, seen)
        return (firsts, True)
    if term.kind == "repeat":
        firsts, _ = seq_first(scan_terms(term.value, term.line), productions, seen)
        return (firsts, True)  # zero repetitions always possible
    if term.kind == "group":
        return seq_first(scan_terms(term.value, term.line), productions, seen)
    if term.kind == "opt":
        firsts, _ = term_first(term.value, productions, seen)
        return (firsts, True)
    return (set(), False)


def seq_first(terms: list, productions: dict, seen: set) -> tuple:
    firsts = set()
    nullable = True
    for term in terms:
        f, n = term_first(term, productions, seen)
        firsts |= f
        if not n:
            nullable = False
            break
    return (firsts, nullable)


def _alt_nullable(alt: list) -> bool:
    return any(t.kind in ("empty", "optional", "repeat", "opt") for t in alt)


def strip_common_prefix(alt_a: list, alt_b: list):
    """Skip the exact common leading terms so the decision point after a
    shared prefix (e.g. layout tokens) is what gets compared."""
    i = 0
    while i < len(alt_a) and i < len(alt_b) and alt_a[i] == alt_b[i]:
        i += 1
    return alt_a[i:], alt_b[i:]


def _term_label(term: Term) -> str:
    if term.kind == "literal":
        return term.value
    if term.kind == "special":
        return f"?{term.value}?"
    return term.kind


def conflict_signature(name: str, i: int, j: int, overlap: list) -> str:
    return f"{name}::{i + 1}:{j + 1}::{','.join(overlap)}"


def scan(productions: dict) -> list:
    """Returns [(severity, signature, message)] — severity in {error, warning}.

    Errors are pairwise alternative conflicts after skipping the common
    leading prefix. The shipped grammar is a design contract (lexical
    maximal-munch tokens, layout tokens, LL(2) lookahead), so the gate
    runs against a pinned baseline: only NEW conflict signatures fail.
    """
    findings = []
    for name, production in productions.items():
        nullables = [_alt_nullable(alt) for alt in production.alternatives]
        alternatives = production.alternatives
        for i in range(len(alternatives)):
            for j in range(i + 1, len(alternatives)):
                if alternatives[i] == alternatives[j] and alternatives[i] != [
                    Term("empty", None, production.line)
                ]:
                    findings.append(
                        (
                            "error",
                            f"{name}::{i + 1}:{j + 1}::identical",
                            f"{name} (line {production.line}): alternatives {i + 1} and "
                            f"{j + 1} are identical",
                        )
                    )
                    continue
                rest_a, rest_b = strip_common_prefix(alternatives[i], alternatives[j])
                if not rest_a or not rest_b:
                    # One alternative is a strict prefix of the other: after
                    # the shared prefix, the shorter may be complete or the
                    # longer may continue — a real decision ambiguity.
                    findings.append(
                        (
                            "error",
                            f"{name}::{i + 1}:{j + 1}::prefix",
                            f"{name} (line {production.line}): alternative {i + 1} is a "
                            f"prefix of alternative {j + 1} (or vice versa)",
                        )
                    )
                    continue
                firsts_a = seq_first(rest_a, productions, set())[0]
                firsts_b = seq_first(rest_b, productions, set())[0]
                overlap = sorted(firsts_a & firsts_b)
                if overlap:
                    signature = conflict_signature(name, i, j, overlap)
                    # Overlap restricted to `?special?` terminals (layout
                    # tokens, lexical token shapes) is resolved by the
                    # lexer/layout pass, never by a parse decision:
                    # warning, not an error.
                    if all(token.startswith("?") for token in overlap):
                        findings.append(
                            (
                                "warning",
                                signature,
                                f"{name} (line {production.line}): alternatives {i + 1} and "
                                f"{j + 1} overlap only on special/layout terminals {overlap} "
                                "(lexer/layout-resolved)",
                            )
                        )
                    else:
                        findings.append(
                            (
                                "error",
                                signature,
                                f"{name} (line {production.line}): alternatives {i + 1} and "
                                f"{j + 1} share first-set terminals {overlap} — "
                                "a sentence with two parses; split the alternatives",
                            )
                        )
        if nullables.count(True) > 0 and len(nullables) > 1:
            findings.append(
                (
                    "warning",
                    f"{name}::nullable",
                    f"{name} (line {production.line}): nullable alternative beside "
                    "non-nullable siblings — review for hidden parses",
                )
            )
    return findings


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("grammars", nargs="+", help="EBNF grammar files")
    parser.add_argument("--delta", help="unified diff to apply before scanning")
    parser.add_argument(
        "--baseline",
        help="JSON list of known conflict signatures (shipped grammar); "
        "only NEW signatures fail the gate",
    )
    parser.add_argument(
        "--emit-baseline", help="write the current conflict signatures as JSON and exit 0"
    )
    args = parser.parse_args(argv)

    merged = {}
    try:
        for path in args.grammars:
            with open(path, encoding="utf-8") as fh:
                text = fh.read()
            if args.delta:
                with open(args.delta, encoding="utf-8") as fh:
                    diff_text = fh.read()
                if diff_targets(diff_text, path):
                    text = apply_diff(text, diff_text)
            merged.update(parse_grammar(text, path))
    except ValueError as exc:
        print(f"g4-ambiguity: cannot parse grammar: {exc}", file=sys.stderr)
        return 2

    findings = scan(merged)
    signatures = sorted(sig for severity, sig, _ in findings if severity == "error")

    if args.emit_baseline:
        import json

        with open(args.emit_baseline, "w", encoding="utf-8") as fh:
            json.dump(signatures, fh, indent=2)
            fh.write("\n")
        print(
            f"g4-ambiguity: baseline written to {args.emit_baseline} "
            f"({len(signatures)} signatures)"
        )
        return 0

    baseline = set()
    if args.baseline:
        import json

        with open(args.baseline, encoding="utf-8") as fh:
            baseline = set(json.load(fh))

    new_errors = [f for f in findings if f[0] == "error" and f[1] not in baseline]
    pinned = [f for f in findings if f[0] == "error" and f[1] in baseline]
    warnings = [f for f in findings if f[0] == "warning"]
    for severity, _, message in pinned:
        print(f"g4-ambiguity: info (baseline): {message}")
    for severity, _, message in new_errors:
        print(f"g4-ambiguity: error: {message}")
    for severity, _, message in warnings:
        print(f"g4-ambiguity: warning: {message}")
    print(
        f"g4-ambiguity: {len(merged)} productions; {len(new_errors)} NEW errors, "
        f"{len(pinned)} baseline, {len(warnings)} warnings"
    )
    if new_errors:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
