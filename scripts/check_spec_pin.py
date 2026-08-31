#!/usr/bin/env python3
"""Language specification pin register (emath-conform-pin-register-1iip).

Pins the normative language surface — `language/reference/**` and
`language/grammar/**` — with SHA-256 under `implementation/SPEC_PIN.json`,
under an edition id (`emath-lang-2026.N`). A pinned spec file that changed
without a named edition bump fails the gate: language drift must be a
named bump, never a silent rewrite (AGENTS RULE 0.3: undocumented or
unpinned language claims are bugs).

Usage:
    check_spec_pin.py [--root ROOT] [--pin FILE]
    check_spec_pin.py --regenerate --note "why the edition moved"

Defaults target the repository tree. `validate.sh` runs this lane after
the contract-doc-pins lane.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import pathlib
import sys

SPEC_GLOBS = ("language/reference/*.md", "language/grammar/*.ebnf")


def sha256_hex(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            digest.update(chunk)
    return digest.hexdigest()


def pinned_files(root: pathlib.Path) -> list[pathlib.Path]:
    files: set[pathlib.Path] = set()
    for pattern in SPEC_GLOBS:
        files.update(sorted(root.glob(pattern)))
    return sorted(files)


def main() -> int:
    parser = argparse.ArgumentParser()
    root = pathlib.Path(__file__).resolve().parent.parent
    parser.add_argument("--root", type=pathlib.Path, default=root)
    parser.add_argument("--pin", type=pathlib.Path)
    parser.add_argument("--regenerate", action="store_true",
                        help="recompute hashes into SPEC_PIN.json (requires --note)")
    parser.add_argument("--note", type=str,
                        help="named-bump note recorded with a regenerate")
    args = parser.parse_args()

    root = args.root.resolve()
    pin_path = (args.pin or root / "implementation/SPEC_PIN.json").resolve()

    if args.regenerate:
        if not args.note:
            print("FAIL: --regenerate requires --note (named-bump protocol)", file=sys.stderr)
            return 1
        pin = json.loads(pin_path.read_text(encoding="utf-8")) if pin_path.is_file() else {
            "schema": "emath.spec-pin",
            "schema_version": 1,
            "algorithm": "sha256",
            "bumps": [],
        }
        previous_files = pin.get("files", {})
        new_files = {str(p.relative_to(root)): sha256_hex(p) for p in pinned_files(root)}
        drift = new_files != previous_files
        pin["files"] = new_files
        if not pin.get("edition"):
            # First pin: stamp the edition from today's date.
            pin["edition"] = f"emath-lang-{datetime.date.today():%Y.%m}"
        pin.setdefault("bumps", []).insert(
            0,
            {
                "date": f"{datetime.date.today():%Y-%m-%d}",
                "note": args.note + ("" if drift else " (hash set unchanged)"),
            },
        )
        pin_path.write_text(json.dumps(pin, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(
            f"spec pin: regenerated {len(new_files)} files under edition {pin['edition']} "
            f"(drift={drift})"
        )
        return 0

    if not pin_path.is_file():
        print(f"FAIL: spec pin missing: {pin_path}", file=sys.stderr)
        return 1
    pin = json.loads(pin_path.read_text(encoding="utf-8"))
    if pin.get("algorithm") != "sha256":
        print(f"FAIL: spec pin algorithm must be sha256, got {pin.get('algorithm')!r}", file=sys.stderr)
        return 1

    violations = []
    actual_files = {str(p.relative_to(root)) for p in pinned_files(root)}
    for rel, expected in sorted(pin["files"].items()):
        path = root / rel
        if not path.is_file():
            violations.append(f"pinned spec file missing: {rel}")
            continue
        actual = sha256_hex(path)
        if actual != expected:
            violations.append(
                f"spec file drifted without a named edition bump: {rel}\n"
                f"    pinned {expected}\n"
                f"    actual {actual}\n"
                f"    run: check_spec_pin.py --regenerate --note \"...\""
            )
    for rel in sorted(actual_files - set(pin["files"])):
        violations.append(f"spec file is not pinned (add to SPEC_PIN.json): {rel}")

    if violations:
        print(f"spec pin: FAIL ({len(violations)} violations)", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1

    print(
        f"spec pin: {len(pin['files'])} language spec files match "
        f"edition {pin.get('edition')!r} ({len(pin.get('bumps', []))} named bumps)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
