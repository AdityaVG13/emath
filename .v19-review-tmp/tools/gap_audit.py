from pathlib import Path
from collections import Counter
import json

ROOT=Path(__file__).resolve().parents[1]
features=json.loads((ROOT/"GENERATED/feature-index.json").read_text(encoding="utf-8"))["features"]
gaps=json.loads((ROOT/"CURRENT_REPO/current-gaps.json").read_text(encoding="utf-8"))["gaps"]
promotion=json.loads((ROOT/"CURRENT_REPO/v16-promotion-ledger.json").read_text(encoding="utf-8"))
holes=json.loads((ROOT/"SPEC_HOLES.json").read_text(encoding="utf-8"))["holes"]
closures=json.loads((ROOT/"GENERATED/projection-closures.json").read_text(encoding="utf-8"))["features"]

report={
    "schema":"emath.v19.gap-report.v1",
    "feature_specs":len(features),
    "prototype_features_missing_realization":sum(1 for x in closures if x["missing"]),
    "current_repo_gaps":len(gaps),
    "current_repo_gap_severity":dict(Counter(g["severity"] for g in gaps)),
    "v16_catalog_entries":promotion["count"],
    "v16_promotion_status":dict(Counter(r["promotion_status"] for r in promotion["rows"])),
    "spec_holes":len(holes),
}
(ROOT/"GENERATED/gap-report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
print(json.dumps(report,indent=2))
