from pathlib import Path
from collections import deque
import argparse, json

ROOT=Path(__file__).resolve().parents[1]
index=json.loads((ROOT/"GENERATED/agent-index.json").read_text(encoding="utf-8"))
spine=json.loads((ROOT/"GENERATED/meaning-spine.json").read_text(encoding="utf-8"))
lock=json.loads((ROOT/"GENERATED/language.lock.json").read_text(encoding="utf-8"))

parser=argparse.ArgumentParser()
parser.add_argument("--feature",required=True)
parser.add_argument("--max-depth",type=int,default=3)
parser.add_argument("--include-reverse",action="store_true")
args=parser.parse_args()

features=index["features"]
if args.feature not in features:
    raise SystemExit(f"unknown FeatureID: {args.feature}")

seen=set(); queue=deque([(args.feature,0)]); closure=[]
while queue:
    fid,depth=queue.popleft()
    if fid in seen or depth>args.max_depth: continue
    seen.add(fid); closure.append(fid)
    for dep in features.get(fid,{}).get("dependencies",[]):
        if dep in features: queue.append((dep,depth+1))

f=features[args.feature]
read_order=[features[fid]["path"] for fid in closure]
read_order += ["SYSTEM/02_FEATURE_CAPSULE.md","SYSTEM/07_PROJECTION_CLOSURE.md","CONFORMANCE/README.md"]
reverse=spine.get("reverse_dependencies",{}).get(args.feature,[]) if args.include_reverse else []

out={
    "schema":"emath.context-capsule.v1",
    "feature":args.feature,
    "baseline":{"repo_commit":"ffec253ab08e7a40d260798f348634e522a18e66","language_image":lock["image_id"]},
    "authority":lock["features"][args.feature],
    "summary":f["summary"],
    "dependency_closure":closure,
    "reverse_impact":reverse,
    "read_order":read_order,
    "owner":f["owner"],
    "allowed_edit_categories":f.get("edit",[]),
    "required_projections":f["required_projections"],
    "gates":["spec parse","dependency closure","positive","negative","mutation/differential","generated freshness"],
    "hazards":f.get("hazards",[]),
    "next_command":"python tools/validate_all.py"
}
print(json.dumps(out,indent=2,ensure_ascii=False))
