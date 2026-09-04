from pathlib import Path
import argparse, json
from spec_parser import parse_spec

p=argparse.ArgumentParser()
p.add_argument("old")
p.add_argument("new")
args=p.parse_args()
a=parse_spec(Path(args.old)); b=parse_spec(Path(args.new))
keys=sorted((set(a)|set(b))-{"_path"})
changes=[]
for key in keys:
    if a.get(key)!=b.get(key):
        changes.append({"field":key,"old":a.get(key),"new":b.get(key)})

identity_fields={"identity","canonical","semantics","exactness","effects","worlds","reference","artifact"}
classification="meaning-affecting" if any(c["field"] in identity_fields for c in changes) else "presentation-or-metadata"
print(json.dumps({"classification":classification,"changes":changes},indent=2,ensure_ascii=False))
