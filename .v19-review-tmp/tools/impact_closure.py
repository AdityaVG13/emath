from pathlib import Path
from collections import deque
import argparse, json

ROOT=Path(__file__).resolve().parents[1]
spine=json.loads((ROOT/"GENERATED/meaning-spine.json").read_text(encoding="utf-8"))
reverse=spine.get("reverse_dependencies",{})

p=argparse.ArgumentParser()
p.add_argument("--feature",required=True)
args=p.parse_args()

seen=set(); q=deque([args.feature])
while q:
    fid=q.popleft()
    if fid in seen: continue
    seen.add(fid)
    q.extend(reverse.get(fid,[]))

print(json.dumps({"schema":"emath.impact-closure.v1","root":args.feature,"affected":sorted(seen)},indent=2))
