#!/usr/bin/env python3
"""G4 hidden-interpretation scan for emath grammar deltas (ELP gate).

One glyph, many meanings: the fourth G4 mechanical sub-test. A quoted
literal terminal that the shipped EBNF admits in more than one
production (role) is liable to carry more than one established meaning
(`-` binary minus vs prefix negation, `,` list separator in every
sequence, a contextual keyword active in expression position only).
Silently picking one meaning at parse time is the C2-C15 class of
governance bug this battery prevents.

The registry (`tests/language-gates/fixtures/
g4-hidden-interpretation-baseline.json`) records every currently
multi-role glyph with the policy that pins its meaning:

- `parser-context` — the parser/operator table or lexer position
  disambiguates deterministically (built-in punctuation is in this
  class; `g4_precedence.py` prints the operator-table resolution).
- `worlds-machinery` — the meaning routes through an explicit notation
  pack, a declared world, or a portfolio artifact (the genesis/portfolio
  machinery); never a silent pick.
- `typed-refusal` — the ambiguating use is refused with a documented
  E-code.

The gate fails on:

- a NEW multi-role glyph (a delta adds a literal to a second production
  without a registry entry),
- a registered glyph whose role set drifted from the registry (its
  interpretation surface changed without an explicit registry update),
- a malformed registry entry.

Stale registry entries (glyph no longer multi-role) warn; single-role
glyphs are one meaning by construction and need no entry.

Reuses the EBNF parser from `g4_ambiguity.py` so every G4 battery
measures the same grammar dialect.

Usage:
    g4_hidden_interpretation.py [--delta DELTA.diff]
        [--baseline baseline.json] [--emit-baseline out.json]
        [grammar.ebnf ...]

Exit: 0 clean, 1 hidden interpretation found, 2 usage/parse error.
Stdlib only; deterministic.
"""

import argparse
import json
import re
import sys

from g4_ambiguity import apply_diff, diff_targets, parse_grammar

_RESOLVED_BY = ("parser-context", "worlds-machinery", "typed-refusal")
_CODE_RE = re.compile(r"^E-[A-Z]+-[0-9]{3}$")


def _walk_terms(terms: list, production: str, roles: dict) -> None:
    """Record every literal's role, descending into group/repeat/optional/
    opt wrappers whose payloads are raw EBNF text.

    Group payloads hold inner alternatives separated by a top-level `|`;
    split on the separators before scanning so a bare `|` is never
    mistaken for a quoted glyph (`"|" | "&"` yields two literals, not
    three)."""
    from g4_ambiguity import scan_terms, split_top_level

    for term in terms:
        if term.kind == "literal":
            roles.setdefault(term.value, set()).add(production)
        elif term.kind in ("group", "repeat", "optional"):
            for alt in split_top_level(term.value):
                _walk_terms(scan_terms(alt.strip(), term.line), production, roles)
        elif term.kind == "opt":
            _walk_terms([term.value], production, roles)


def collect_roles(productions: dict) -> dict:
    """Literal terminal -> sorted role production names (nested groups
    included: `{ ("+" | "-") , unary_expr }` counts for both signs)."""
    roles: dict = {}
    for name, production in productions.items():
        for alt in production.alternatives:
            _walk_terms(alt, name, roles)
    return {glyph: sorted(names) for glyph, names in roles.items()}


def load_baseline(path: str) -> dict:
    """Registry file -> {glyph: entry}; raises ValueError on bad shape."""
    with open(path, encoding="utf-8") as fh:
        try:
            entries = json.load(fh)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}: baseline is not valid JSON: {exc}") from exc
    if not isinstance(entries, list):
        raise ValueError(f"{path}: baseline must be a JSON list of entries")
    registry = {}
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError(f"{path}: entry is not an object: {entry!r}")
        glyph = entry.get("glyph")
        if not isinstance(glyph, str) or not glyph:
            raise ValueError(f"{path}: entry missing a non-empty `glyph`: {entry!r}")
        resolved = entry.get("resolved_by")
        if resolved not in _RESOLVED_BY:
            raise ValueError(
                f"{path}: glyph {glyph!r} resolved_by must be one of "
                f"{_RESOLVED_BY}, got {resolved!r}"
            )
        roles = entry.get("roles")
        if not isinstance(roles, list) or not all(
            isinstance(role, str) for role in roles
        ):
            raise ValueError(f"{path}: glyph {glyph!r} `roles` must be a string list")
        if resolved == "typed-refusal" and not (
            isinstance(entry.get("code"), str) and _CODE_RE.match(entry["code"])
        ):
            raise ValueError(
                f"{path}: glyph {glyph!r} typed-refusal requires an E-XXX-NNN `code`"
            )
        if resolved == "worlds-machinery" and not (
            isinstance(entry.get("note"), str) and entry["note"].strip()
        ):
            raise ValueError(
                f"{path}: glyph {glyph!r} worlds-machinery requires a `note` "
                "naming the notation pack / world / portfolio routing"
            )
        registry[glyph] = entry
    return registry


def scan(roles: dict, registry: dict) -> list:
    """Returns [(severity, glyph, message)]."""
    findings = []
    for glyph, names in sorted(roles.items()):
        if len(names) < 2:
            continue
        entry = registry.get(glyph)
        if entry is None:
            findings.append(
                (
                    "error",
                    glyph,
                    f"glyph `{glyph}` now admits {len(names)} roles {names} with no "
                    f"registered interpretation; add it to the baseline with "
                    f"resolved_by in {list(_RESOLVED_BY)}",
                )
            )
            continue
        if set(entry["roles"]) != set(names):
            findings.append(
                (
                    "error",
                    glyph,
                    f"glyph `{glyph}` role set drifted: registry {sorted(entry['roles'])} "
                    f"vs grammar {names}; update the baseline entry (its interpretation "
                    f"surface changed)",
                )
            )
            continue
        suffix = f" — {entry['note']}" if entry.get("note") else ""
        findings.append(
            (
                "info",
                glyph,
                f"glyph `{glyph}` (roles {names}) resolved_by={entry['resolved_by']}{suffix}",
            )
        )
    for glyph, entry in sorted(registry.items()):
        names = roles.get(glyph, [])
        if len(names) < 2:
            findings.append(
                (
                    "warning",
                    glyph,
                    f"stale registry entry for glyph `{glyph}` (no longer multi-role); "
                    "remove it when regenerating the baseline",
                )
            )
    return findings


def emit_baseline(roles: dict, path: str) -> None:
    entries = [
        {"glyph": glyph, "roles": names, "resolved_by": "parser-context", "note": ""}
        for glyph, names in sorted(roles.items())
        if len(names) >= 2
    ]
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(entries, fh, indent=2, ensure_ascii=False)
        fh.write("\n")


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("grammars", nargs="+", help="EBNF grammar files")
    parser.add_argument("--delta", help="unified diff to apply before scanning")
    parser.add_argument(
        "--baseline",
        help="JSON registry of multi-role glyph interpretation policies "
        "(shipped grammar); only NEW glyphs or role drift fail the gate",
    )
    parser.add_argument(
        "--emit-baseline",
        help="write the current multi-role glyphs as a baseline registry and exit 0",
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
        print(f"g4-hidden-interpretation: cannot parse grammar: {exc}", file=sys.stderr)
        return 2

    roles = collect_roles(merged)

    if args.emit_baseline:
        emit_baseline(roles, args.emit_baseline)
        print(
            f"g4-hidden-interpretation: baseline written to {args.emit_baseline} "
            f"({sum(1 for n in roles.values() if len(n) >= 2)} multi-role glyphs)"
        )
        return 0

    registry = {}
    if args.baseline:
        try:
            registry = load_baseline(args.baseline)
        except ValueError as exc:
            print(f"g4-hidden-interpretation: error: {exc}", file=sys.stderr)
            return 1

    findings = scan(roles, registry)
    errors = [f for f in findings if f[0] == "error"]
    warnings = [f for f in findings if f[0] == "warning"]
    infos = [f for f in findings if f[0] == "info"]
    for severity, _, message in infos:
        print(f"g4-hidden-interpretation: info: {message}")
    for severity, _, message in errors:
        print(f"g4-hidden-interpretation: error: {message}")
    for severity, _, message in warnings:
        print(f"g4-hidden-interpretation: warning: {message}")
    multi = sum(1 for names in roles.values() if len(names) >= 2)
    print(
        f"g4-hidden-interpretation: {len(roles)} glyphs; {multi} multi-role; "
        f"{len(errors)} errors, {len(infos)} registered, {len(warnings)} stale"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
