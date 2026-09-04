from __future__ import annotations
from pathlib import Path
from collections import Counter, defaultdict, deque
import hashlib
import json
import re
import sys

from spec_parser import parse_spec, SpecError

ROOT=Path(__file__).resolve().parents[1]
SPEC=ROOT/"SPEC"
OUT=ROOT/"GENERATED"
OUT.mkdir(exist_ok=True)

REQUIRED=[
    "identity","class","status","edition","summary","dependencies","surface","canonical",
    "semantics","exactness","effects","worlds","reference","artifact","projections",
    "diagnostics","conformance","evolution","agent"
]
CLASSES={
    "syntax","syntax-pack","kind","section","symbol","binder","type","constructor",
    "capability","theory","instance","family","method","world","provider","artifact",
    "diagnostic","migration","field-pack","lens"
}
PROJECTIONS={
    "identity","surface","parse","lowering","static-semantics","reference","worlds",
    "execution","artifact","diagnostics","documentation","tooling","conformance",
    "migration","agent-view"
}
ID_RE=re.compile(r"^[a-z][a-z0-9_.-]*@[1-9][0-9]*$")

def canonical(spec):
    data={k:v for k,v in spec.items() if not k.startswith("_")}
    return json.dumps(data,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()

specs=[]
errors=[]
for path in sorted(SPEC.rglob("*.emath")):
    try:
        spec=parse_spec(path)
        spec["_path"]=path.relative_to(ROOT).as_posix()
        missing=[k for k in REQUIRED if k not in spec]
        if missing: errors.append(f"{spec['_path']}: missing {missing}")
        if spec.get("class") not in CLASSES: errors.append(f"{spec['_path']}: unknown class {spec.get('class')}")
        if not ID_RE.match(str(spec.get("identity",""))): errors.append(f"{spec['_path']}: invalid FeatureID")
        unknown=set(spec.get("projections",[]))-PROJECTIONS
        if unknown: errors.append(f"{spec['_path']}: unknown projections {sorted(unknown)}")
        for key in ("positive","negative","mutation"):
            if key not in spec.get("conformance",{}):
                errors.append(f"{spec['_path']}: conformance missing {key}")
        specs.append(spec)
    except SpecError as exc:
        errors.append(str(exc))

by_id={}
for spec in specs:
    fid=spec.get("identity")
    if fid in by_id:
        errors.append(f"duplicate FeatureID {fid}: {by_id[fid]['_path']} / {spec['_path']}")
    by_id[fid]=spec

for spec in specs:
    for dep in spec.get("dependencies",[]):
        if dep not in by_id:
            errors.append(f"{spec['identity']}: unresolved dependency {dep}")

# Reject ordinary dependency cycles.
graph={fid:list(spec.get("dependencies",[])) for fid,spec in by_id.items()}
visiting=set(); visited=set()
def dfs(fid,trail):
    if fid in visiting:
        errors.append("dependency cycle: "+" -> ".join(trail+[fid]))
        return
    if fid in visited: return
    visiting.add(fid)
    for dep in graph.get(fid,[]): dfs(dep,trail+[fid])
    visiting.remove(fid); visited.add(fid)
for fid in graph: dfs(fid,[])

# Surface collision check per role. Same spelling is allowed in distinct roles.
spellings=defaultdict(list)
for spec in specs:
    role=spec.get("surface",{}).get("role","none")
    for spelling in spec.get("surface",{}).get("spellings",[]):
        spellings[(role,spelling)].append(spec["identity"])
for (role,spelling),ids in spellings.items():
    if len(ids)>1:
        errors.append(f"surface collision role={role} spelling={spelling}: {ids}")

if errors:
    print("\n".join(errors),file=sys.stderr)
    raise SystemExit(1)

features=[]
edges=[]
reverse=defaultdict(list)
lock_features={}
agent_features={}
projection_closures=[]
for spec in sorted(specs,key=lambda x:x["identity"]):
    digest=hashlib.sha256(canonical(spec)).hexdigest()
    feature={
        "id":spec["identity"],"name":spec["name"],"class":spec["class"],
        "status":spec["status"],"summary":spec["summary"],"dependencies":spec["dependencies"],
        "path":spec["_path"],"spec_hash":digest,"required_projections":spec["projections"],
        "owner":spec["agent"]["owner"],"surface":spec["surface"],"diagnostics":spec["diagnostics"]
    }
    features.append(feature)
    lock_features[spec["identity"]]={"spec_hash":digest,"path":spec["_path"],"status":spec["status"]}
    agent_features[spec["identity"]]={
        "summary":spec["summary"],"path":spec["_path"],"class":spec["class"],
        "owner":spec["agent"]["owner"],"dependencies":spec["dependencies"],
        "required_projections":spec["projections"],"hazards":spec["agent"].get("hazards",[]),
        "read":spec["agent"].get("read",[]),"edit":spec["agent"].get("edit",[])
    }
    projection_closures.append({
        "schema":"emath.projection-closure.v1","feature_id":spec["identity"],
        "required":spec["projections"],"present":["identity","agent-view"],
        "missing":[x for x in spec["projections"] if x not in {"identity","agent-view"}],
        "status":"prototype-spec-only"
    })
    for dep in spec["dependencies"]:
        edges.append({"source":spec["identity"],"target":dep,"kind":"requires"})
        reverse[dep].append(spec["identity"])

spine={
    "nodes":[f["id"] for f in features],
    "edges":edges,
    "reverse_dependencies":{k:sorted(v) for k,v in sorted(reverse.items())}
}
image_preimage=json.dumps({"features":features,"meaning_spine":spine},sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
image_id="sha256:"+hashlib.sha256(image_preimage).hexdigest()

image={
    "schema":"emath.language-image.v1","image_id":image_id,"feature_count":len(features),
    "features":features,"meaning_spine":spine,
    "partitions":["feature-table","surface","kinds","semantics","worlds","diagnostics","migrations","agent"]
}
lock={
    "schema":"emath.language-lock.v1","image_id":image_id,"features":lock_features,
    "authority":"GENERATED/authority-target.json","spec_holes":"SPEC_HOLES.json",
    "status":"prototype target; not active repository authority"
}
authority={
    "schema":"emath.authority-lock.v1",
    "repo_commit":"ffec253ab08e7a40d260798f348634e522a18e66",
    "status":"target example; feature authority has not been switched in the repository",
    "features":{fid:{"authority":"v19-prototype","source":data["path"]} for fid,data in lock_features.items()},
    "legacy_default":{"authority":"legacy-reference","source":"language/reference/"}
}
agent_index={
    "schema":"emath.agent-index.v1","image_id":image_id,
    "entry":"AGENT_START.json","features":agent_features
}

(OUT/"feature-index.json").write_text(json.dumps({"schema":"emath.feature-index.v1","features":features},indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"meaning-spine.json").write_text(json.dumps(spine,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"language-image.v1.json").write_text(json.dumps(image,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"language.lock.json").write_text(json.dumps(lock,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"authority-target.json").write_text(json.dumps(authority,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"agent-index.json").write_text(json.dumps(agent_index,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
(OUT/"projection-closures.json").write_text(json.dumps({"schema":"emath.projection-closures.v1","features":projection_closures},indent=2)+"\n",encoding="utf-8")

counts=Counter(f["class"] for f in features)
lines=["# Generated Capability Matrix","",f"Prototype Language Image: `{image_id}`","",
       "This is a target-spec view, not a claim that every feature is implemented.","",
       "| Class | Count |","|---|---:|"]
for cls,count in sorted(counts.items()): lines.append(f"| `{cls}` | {count} |")
lines += ["","## Features","",
          "| FeatureID | Class | Maturity | Owner | Summary |","|---|---|---|---|---|"]
for f in features:
    lines.append(f"| `{f['id']}` | {f['class']} | {f['status']} | `{f['owner']}` | {f['summary']} |")
(OUT/"capability-matrix.md").write_text("\n".join(lines)+"\n",encoding="utf-8")

surface_rows=[]
for f in features:
    if f["surface"].get("spellings"):
        surface_rows.append({"feature_id":f["id"],"class":f["class"],"surface":f["surface"]})
(OUT/"surface-registry.json").write_text(json.dumps({"schema":"emath.surface-registry.v1","rows":surface_rows},indent=2,ensure_ascii=False)+"\n",encoding="utf-8")

print(json.dumps({"features":len(features),"classes":dict(counts),"image_id":image_id,"edges":len(edges)},indent=2))
