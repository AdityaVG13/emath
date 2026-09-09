"""Scan family-slice examples for leftover x=N / pair-count shapes."""
from pathlib import Path
import re

cap = Path("/Users/aditya/Developer/emath/language/CAPABILITY.md").read_text()
pre, _, _ = cap.partition("## Catalog heading coverage")
row = re.compile(r"^\| ([^|]+) \| `([^`]+\.emath)` \|([^|]*)\|", re.M)
family = row.findall(pre)
ex_root = Path("/Users/aditya/Developer/emath/language/examples")

# leftover-looking definition assigns of a literal only
lit_assign = re.compile(
    r"^\s+\w+ = (-?\d+(?:\.\d+)?|true|false|rat\(\d+, \d+\))\s*$", re.M
)
# first-line leftover markers
leftover_words = re.compile(r"leftover|pair-count|x=N|1\+1|1 \* 1|1\*1", re.I)

suspect = []
named_skip = []
for name, rel, formula in family:
    rel = rel.strip()
    ex = ex_root / rel
    if not ex.exists():
        continue
    text = ex.read_text()
    first = text.splitlines()[0] if text else ""
    defs = "\n".join(
        ln for ln in text.splitlines() if re.match(r"^\s+\w+ = ", ln)
    )
    lits = lit_assign.findall(defs)
    if leftover_words.search(first) or leftover_words.search(text[:400]):
        named_skip.append((rel, first[:80], formula.strip()))
        continue
    # definition is only a bare literal (no operator) for a primary output
    if lits and all(
        re.match(r"^\s+\w+ = (-?\d+(?:\.\d+)?|true|false|rat\(\d+, \d+\))\s*$", ln)
        for ln in defs.splitlines()
        if ln.strip()
    ):
        suspect.append((rel, first[:80], formula.strip(), lits[:4]))

print(f"family rows: {len(family)}")
print(f"named leftover-skip comments: {len(named_skip)}")
for rel, first, formula in named_skip:
    print(f"  SKIP {rel} | {formula} | {first}")
print(f"bare-literal definition suspects: {len(suspect)}")
for rel, first, formula, lits in suspect:
    print(f"  LIT {rel} | {formula} | defs={lits} | {first}")
