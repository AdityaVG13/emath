from __future__ import annotations
from pathlib import Path
import json
import re
from typing import Any

HEADER = re.compile(r"^emath spec ([A-Za-z][A-Za-z0-9_]*)\s*:$")
FIELD = re.compile(r"^    ([A-Za-z][A-Za-z0-9_-]*)\s*:\s*(.+)$")

class SpecError(ValueError):
    pass

def parse_spec(path: Path) -> dict[str, Any]:
    lines = path.read_text(encoding="utf-8").splitlines()
    significant=[(i+1,line) for i,line in enumerate(lines) if line.strip() and not line.lstrip().startswith("#")]
    if not significant:
        raise SpecError(f"{path}: empty")
    no,head=significant[0]
    m=HEADER.match(head)
    if not m:
        raise SpecError(f"{path}:{no}: expected `emath spec Name:`")
    out={"name":m.group(1),"_path":path.as_posix()}
    for no,line in significant[1:]:
        m=FIELD.match(line)
        if not m:
            raise SpecError(f"{path}:{no}: fields require exactly four-space indentation and JSON value")
        key,raw=m.group(1),m.group(2)
        if key in out:
            raise SpecError(f"{path}:{no}: duplicate field {key}")
        try:
            out[key]=json.loads(raw)
        except json.JSONDecodeError as exc:
            raise SpecError(f"{path}:{no}: invalid JSON value for {key}: {exc}") from exc
    return out
