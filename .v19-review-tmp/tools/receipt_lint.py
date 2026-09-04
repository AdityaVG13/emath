from pathlib import Path
import argparse, json

p=argparse.ArgumentParser()
p.add_argument("receipt")
args=p.parse_args()
d=json.loads(Path(args.receipt).read_text(encoding="utf-8"))
required=["schema","receipt_id","baseline","features","files","commands","gates","decisions","new_holes","rollback"]
missing=[x for x in required if x not in d]
if missing:
    raise SystemExit("missing receipt fields: "+", ".join(missing))
if d["schema"]!="emath.change-receipt.v1":
    raise SystemExit("wrong receipt schema")
print(json.dumps({"status":"PASS","receipt":d["receipt_id"]},indent=2))
