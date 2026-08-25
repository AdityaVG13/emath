#!/usr/bin/env python3
"""G4 confusable-glyph scan for emath grammar deltas (ELP gate).

Extracts every string-literal terminal from the shipped EBNF grammars
and checks:

- NFC normalization: a glyph stored in decomposed form is reported
  (warn; the canonical spelling must be NFC),
- confusable folding: the same fold table as
  `emath_sema::admit::confusable_fold` (Cyrillic/Greek lookalikes) plus
  punctuation case-folding; two DISTINCT glyphs sharing a fold are a
  confusable collision (fatal),
- new glyphs (added by a unified diff) colliding with an existing
  grammar glyph's fold are fatal.

The Standard Symbol Catalog (bead `emath-r3-ssc-governance-kvuo`) will
extend the fold table from there; this battery is the shipped grammar's
first-pass mechanical check.

Usage:
    g4_confusable.py [--delta DELTA.diff] [grammar.ebnf ...]

Exit: 0 clean, 1 collision found, 2 usage error.
Stdlib only; deterministic.
"""

import argparse
import sys
import unicodedata

_IDENT = r"[A-Za-z_][A-Za-z0-9_]*"

# Same fold table as crates/emath-sema/src/admit.rs `confusable_fold`.
_FOLD = {
    # Cyrillic lowercase lookalikes.
    "\u0430": "a", "\u03b1": "a",
    "\u0435": "e",
    "\u043a": "k", "\u03ba": "k",
    "\u043c": "m", "\u03bc": "m",
    "\u043d": "h",
    "\u043e": "o", "\u03bf": "o",
    "\u0440": "p", "\u03c1": "p",
    "\u0441": "c",
    "\u0442": "t", "\u03c4": "t",
    "\u0443": "y",
    "\u0445": "x", "\u03c7": "x",
    "\u0455": "s",
    "\u0456": "i", "\u03b9": "i",
    "\u0458": "j",
    # Cyrillic uppercase lookalikes.
    "\u0410": "A",
    "\u0415": "E",
    "\u041a": "K", "\u039a": "K",
    "\u041c": "M", "\u039c": "M",
    "\u041d": "H",
    "\u041e": "O", "\u039f": "O",
    "\u0420": "P", "\u03a1": "P",
    "\u0421": "C",
    "\u0422": "T", "\u03a4": "T",
    "\u0423": "Y",
    "\u0425": "X", "\u03a7": "X",
    "\u0405": "S",
    "\u0406": "I", "\u0399": "I",
    "\u0408": "J",
    # Greek lowercase lookalikes.
    "\u03bd": "v",
    # Greek uppercase lookalikes.
    "\u039d": "N",
}


def confusable_fold(glyph: str) -> str:
    return "".join(_FOLD.get(ch, ch) for ch in glyph)


def extract_literals(path: str) -> list:
    """All string-literal terminals in one grammar file with line numbers."""
    literals = []
    with open(path, encoding="utf-8") as fh:
        for line_no, line in enumerate(fh, 1):
            quote = None
            i = 0
            n = len(line)
            while i < n:
                ch = line[i]
                if quote:
                    if ch == quote and (i == 0 or line[i - 1] != "\\"):
                        quote = None
                    elif ch == quote and line[i - 1] == "\\":
                        pass
                elif ch in "\"'":
                    quote = ch
                    start = i + 1
                    i += 1
                    while i < n:
                        c = line[i]
                        if c == quote and (i == 0 or line[i - 1] != "\\"):
                            break
                        if c == quote:
                            pass
                        i += 1
                    if i < n:
                        literals.append((line[start:i], line_no))
                        quote = None
                    continue
                i += 1
    return literals


def new_literals_from_diff(diff_path: str) -> list:
    """Literals appearing on added (`+`) lines of a unified diff."""
    literals = []
    with open(diff_path, encoding="utf-8") as fh:
        for line_no, line in enumerate(fh, 1):
            if not line.startswith("+"):
                continue
            if line.startswith(("+++ ", "--- ")):
                continue
            text = line[1:]
            quote = None
            i = 0
            n = len(text)
            while i < n:
                ch = text[i]
                if quote:
                    if ch == quote and (i == 0 or text[i - 1] != "\\"):
                        quote = None
                elif ch in "\"'":
                    quote = ch
                    start = i + 1
                    i += 1
                    while i < n:
                        c = text[i]
                        if c == quote and (i == 0 or text[i - 1] != "\\"):
                            break
                        i += 1
                    if i < n:
                        literals.append((text[start:i], line_no))
                        quote = None
                    continue
                i += 1
    return literals


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("grammars", nargs="+", help="EBNF grammar files")
    parser.add_argument("--delta", help="unified diff: added-line glyphs are the new surface")
    args = parser.parse_args(argv)

    errors = []
    warnings = []

    existing = {}  # glyph -> (path, line)
    for path in args.grammars:
        for glyph, line in extract_literals(path):
            if unicodedata.is_normalized("NFC", glyph):
                existing.setdefault(glyph, (path, line))
            else:
                warnings.append(
                    f"{path}:{line}: glyph {glyph!r} is not NFC-normalized; "
                    "canonical spelling must be NFC"
                )

    # Collisions among existing glyphs. Folding is case-preserving (the
    # sema table folds Cyrillic/Greek lookalikes to their Latin twin but
    # never merges Latin case variants, which are not confusable).
    by_fold = {}
    for glyph, (path, line) in sorted(existing.items()):
        fold = confusable_fold(glyph)
        by_fold.setdefault(fold, []).append((glyph, path, line))
    for fold, group in sorted(by_fold.items()):
        distinct = sorted({g for g, _, _ in group})
        if len(distinct) > 1:
            errors.append(
                f"confusable collision: {distinct} all fold to {fold!r} "
                f"(first at {group[0][1]}:{group[0][2]})"
            )

    if args.delta:
        for glyph, line in new_literals_from_diff(args.delta):
            fold = confusable_fold(glyph)
            base = by_fold.get(fold, [])
            base_distinct = sorted({g for g, _, _ in base})
            if base_distinct and base_distinct != [glyph] and glyph not in base_distinct:
                errors.append(
                    f"delta:{line}: new glyph {glyph!r} folds to {fold!r}, "
                    f"already used by {base_distinct}; pick a visually distinct glyph"
                )

    for message in warnings:
        print(f"g4-confusable: warning: {message}")
    for message in errors:
        print(f"g4-confusable: error: {message}")
    print(f"g4-confusable: {len(existing)} glyphs; {len(errors)} errors, {len(warnings)} warnings")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
