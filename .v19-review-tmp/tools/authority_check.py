from pathlib import Path
import json

ROOT=Path(__file__).resolve().parents[1]
lock=json.loads((ROOT/"GENERATED/authority-target.json").read_text(encoding="utf-8"))
seen={}
for fid,record in lock["features"].items():
    if fid in seen: raise SystemExit(f"dual authority: {fid}")
    seen[fid]=record
print(json.dumps({"status":"PASS","features":len(seen),"lock_status":lock["status"]},indent=2))
