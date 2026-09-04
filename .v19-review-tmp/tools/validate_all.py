from pathlib import Path
import json, py_compile, subprocess, sys

ROOT=Path(__file__).resolve().parents[1]

def run(args):
    cp=subprocess.run(args,cwd=ROOT,text=True,capture_output=True)
    if cp.returncode:
        sys.stderr.write(cp.stdout+cp.stderr)
        raise SystemExit(cp.returncode)
    return cp.stdout.strip()

for script in sorted((ROOT/"tools").glob("*.py")):
    py_compile.compile(str(script),doraise=True)

build=json.loads(run([sys.executable,"tools/build_language_index.py"]))
authority=json.loads(run([sys.executable,"tools/authority_check.py"]))
gaps=json.loads(run([sys.executable,"tools/gap_audit.py"]))
orientation=json.loads(run([sys.executable,"tools/agent_orient.py","--feature","std.kind.cipher@1","--include-reverse"]))
impact=json.loads(run([sys.executable,"tools/impact_closure.py","--feature","std.capability.math.add@1"]))
receipt=json.loads(run([sys.executable,"tools/receipt_lint.py","AGENT/receipts/CHANGE_RECEIPT_TEMPLATE.json"]))

corpus=json.loads((ROOT/"CONFORMANCE/corpus.json").read_text(encoding="utf-8"))
feature_ids={f["id"] for f in json.loads((ROOT/"GENERATED/feature-index.json").read_text(encoding="utf-8"))["features"]}
for case in corpus["cases"]:
    d=ROOT/case["path"]
    for name in ("source.emath","negative.emath","expected.json","case.json"):
        if not (d/name).exists(): raise SystemExit(f"missing conformance file {d/name}")
    unknown=set(case["features"])-feature_ids
    if unknown: raise SystemExit(f"case {case['id']} has unknown features {sorted(unknown)}")

for path in ROOT.rglob("*.json"):
    json.loads(path.read_text(encoding="utf-8"))

print(json.dumps({
    "status":"PASS","build":build,"authority":authority,"gaps":gaps,
    "conformance_cases":len(corpus["cases"]),
    "orientation_feature":orientation["feature"],
    "impact_features":len(impact["affected"]),
    "receipt_lint":receipt["status"]
},indent=2))
