from pathlib import Path
import re

cap = Path("/Users/aditya/Developer/emath/language/CAPABILITY.md").read_text()
pre, _, post = cap.partition("## Catalog heading coverage")
row = re.compile(r"^\| ([^|]+) \| `([^`]+\.emath)` \|([^|]*)\|", re.M)
family = row.findall(pre)
post_body = post.split("\n## ")[0]
headings = re.compile(r"^\| ([^|]+) \| `([^`]+\.emath)` \|", re.M).findall(post_body)
print(f"family-slice rows: {len(family)}")
print(f"heading-coverage rows: {len(headings)}")

ex_root = Path("/Users/aditya/Developer/emath/language/examples")
cap_root = Path("/Users/aditya/Developer/emath/language/spec/capabilities")
missing_ex, missing_cap, check_first = [], [], []
for name, rel, formula in family:
    rel = rel.strip()
    ex = ex_root / rel
    stem = Path(rel).name
    if not ex.exists():
        missing_ex.append(rel)
    if not (cap_root / stem).exists():
        missing_cap.append(rel)
    if ex.exists():
        first = ex.read_text().splitlines()[0]
        if first.startswith("# Check:"):
            check_first.append((rel, formula.strip(), first[:70]))

print(f"missing examples: {len(missing_ex)}")
print(f"missing capsules: {len(missing_cap)}")
print(f"check-first family examples: {len(check_first)}")
for rel, formula, first in check_first:
    print(f"  {rel} | {formula}")
for rel in missing_ex[:20]:
    print(f"missing-ex {rel}")
print("ALL-MISSING-CAP")
for rel in missing_cap:
    print(f"missing-cap {rel}")
