#!/usr/bin/env python3
"""Hashed-doc contract loader (gauntlet-d1 closure predicate).

Pins the contract doc set named in `implementation/contract-pins.json`
with SHA-256. A pinned doc that changed without its pin being bumped
fails the gate: contract-doc drift must be a *named bump* (a pin update
with a `bumps` note in the pins file), never a silent rewrite.

Usage:
    check_doc_pins.py [--root ROOT] [--pins FILE]

Defaults target the repository tree. `validate.sh` runs this lane after
the CRATE_MAP / inventory gate and the ERROR_CODES annex check.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys


def sha256_hex(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    root = pathlib.Path(__file__).resolve().parent.parent
    parser.add_argument("--root", type=pathlib.Path, default=root)
    parser.add_argument("--pins", type=pathlib.Path)
    args = parser.parse_args()

    root = args.root.resolve()
    pins_path = (args.pins or root / "implementation/contract-pins.json").resolve()
    pins = json.loads(pins_path.read_text(encoding="utf-8"))

    if pins.get("algorithm") != "sha256":
        print(f"FAIL: pins algorithm must be sha256, got {pins.get('algorithm')!r}", file=sys.stderr)
        return 1

    violations = []
    checked = 0
    for rel, pin in pins["files"].items():
        path = root / rel
        if not path.is_file():
            violations.append(f"pinned doc missing: {rel}")
            continue
        actual = sha256_hex(path)
        checked += 1
        if actual != pin:
            violations.append(
                f"pinned doc drifted without a named bump: {rel}\n"
                f"    pinned {pin}\n"
                f"    actual {actual}\n"
                f"    update implementation/contract-pins.json AND record the bump in `bumps`"
            )

    if violations:
        print(f"doc pins: FAIL ({len(violations)} drifted)", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1

    bumps = pins.get("bumps", [])
    print(
        f"contract pins: {checked} hashed docs match "
        f"(schema v{pins.get('schema_version')}, {len(bumps)} named bumps)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
