from pathlib import Path
from collections import defaultdict
import re

root = Path("/Users/aditya/Developer/emath/language/spec/capabilities")
feats = defaultdict(list)
ids = defaultdict(list)
aliases = defaultdict(list)
for p in root.rglob("*.emath"):
    text = p.read_text()
    rel = str(p.relative_to(root))
    for m in re.finditer(r"^emath feature (\w+):", text, re.M):
        feats[m.group(1)].append(rel)
    for m in re.finditer(r'feature_id: "([^"]+)"', text):
        ids[m.group(1)].append(rel)
    for m in re.finditer(r'aliases=([^"]+)"', text):
        for a in m.group(1).split(","):
            aliases[a.strip()].append(rel)

def dump(title, d):
    dups = {k: v for k, v in d.items() if len(v) > 1}
    print(f"{title} dups: {len(dups)}")
    for k, v in sorted(dups.items()):
        print(f"  {k}: {v}")

dump("feature", feats)
dump("feature_id", ids)
dump("alias", aliases)
