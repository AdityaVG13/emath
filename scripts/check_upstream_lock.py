#!/usr/bin/env python3
"""Upstream-lock honesty gate.

Checks the shipped upstream lock:

1. `forks/UPSTREAM_LOCK.json` validates against
   `implementation/schemas/upstream-lock.schema.json` (jsonschema when the
   package is importable, otherwise a structural check of the same
   constraints).
2. No repository row may claim `required: "conformance"` without a
   consuming gate (the schema enum refuses `conformance` outright; MSL
   is a `future` row under the no-MSL-CI fence, DISC-005).

Usage:
    check_upstream_lock.py [--root ROOT]

Wired into `validate.sh` as the conformance-register lane.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

def structural_validate(lock: dict, schema: dict) -> list[str]:
    problems: list[str] = []
    props = schema["properties"]
    for key in schema.get("required", []):
        if key not in lock:
            problems.append(f"lock missing required key {key!r}")
    if "schema" in props and "const" in props["schema"] and lock.get("schema") != props["schema"]["const"]:
        problems.append(
            f'lock schema {lock.get("schema")!r} != schema const {props["schema"]["const"]!r}'
        )
    if "schema_version" in props and lock.get("schema_version") != props["schema_version"]["const"]:
        problems.append(f"lock schema_version drift: {lock.get('schema_version')!r}")
    if "generated_at" in props and "pattern" in props["generated_at"]:
        if not re.match(props["generated_at"]["pattern"], lock.get("generated_at", "")):
            problems.append(f"generated_at not a date: {lock.get('generated_at')!r}")
    commit_re = re.compile(props["repositories"]["items"]["properties"]["commit"]["pattern"])
    required_enum = set(props["repositories"]["items"]["properties"]["required"]["enum"])
    for row in lock.get("repositories", []):
        rid = row.get("id", "<missing>")
        for key in props["repositories"]["items"].get("required", []):
            if key not in row:
                problems.append(f"repo {rid} missing required key {key!r}")
        if not commit_re.match(row.get("commit", "")):
            problems.append(f"repo {rid} commit not a 40-hex sha: {row.get('commit')!r}")
        if row.get("required") not in required_enum:
            problems.append(
                f"repo {rid} required {row.get('required')!r} not in {sorted(required_enum)}"
            )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    root = pathlib.Path(__file__).resolve().parent.parent
    parser.add_argument("--root", type=pathlib.Path, default=root)
    args = parser.parse_args()
    root = args.root.resolve()

    lock_path = root / "forks/UPSTREAM_LOCK.json"
    schema_path = root / "implementation/schemas/upstream-lock.schema.json"
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    schema = json.loads(schema_path.read_text(encoding="utf-8"))

    problems: list[str] = []
    try:
        import jsonschema  # type: ignore

        validator_cls = getattr(jsonschema, "Draft202012Validator", None)
        if validator_cls is not None:
            for error in sorted(validator_cls(schema).iter_errors(lock), key=str):
                problems.append(f"schema: {error.message}")
        else:
            problems.extend(structural_validate(lock, schema))
    except ImportError:
        problems.extend(structural_validate(lock, schema))

    repos = {row["id"]: row for row in lock.get("repositories", [])}

    if problems:
        print(f"upstream lock: FAIL ({len(problems)} problems)", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"upstream lock: {len(repos)} repos schema-valid (schema {lock['schema']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
