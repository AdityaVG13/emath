#!/usr/bin/env python3
"""Gauntlet-08 doc gate: CRATE_MAP <-> workspace, inventory <-> session.

Pins `implementation/CRATE_MAP.md` against the workspace manifest and the
`crates/` directory (SURF-0003), and `implementation/PUBLIC_API_INVENTORY.md`
against the `emath-sema` CompilerSession signatures (SURF-0001). The gate
fails if the docs drift from HEAD; negative controls in `validate.sh` run
this checker against mutated copies to prove the gate refuses drift.

Usage:
    check_doc_gates.py [--root ROOT] [--crate-map FILE] [--inventory FILE]
                       [--session FILE] [--manifest FILE]

Defaults target the repository tree (the script's own parent directory).
`validate.sh` passes mutated copies via the explicit flags.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tomllib

ROW = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|")
PLANNED_ROW = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*planned\s*\|")
PUB_FN = re.compile(r"pub fn (\w+)\(([^)]*)\)")
PLANNED_TOKENS = ("LoadRequest", "GoalRequest", "BuildRequest")

ISSUED_BULLET = re.compile(r"^- `(E-[A-Z]+-[0-9]{3})`")
CODE_RE = re.compile(r"\bE-[A-Z]+-\d{3}\b")
# Files whose claim strings the gate refuses. AGENTS.md is local-only
# (gitignored) and the session docs are not shipped, so each entry is
# skipped when missing on a clean clone.
DOC_SET = [
    "AGENTS.md",
    "implementation/ERROR_CODES.md",
    "implementation/CLI_REFERENCE.md",
    "implementation/CRATE_MAP.md",
    "implementation/PUBLIC_API_INVENTORY.md",
]
# Affirmative phrasings; negated mentions ("not a certifying oracle") do
# not match because the verb is separated from the noun by "not".
FORBIDDEN_PHRASES = [
    "Dew Differential V2",
    "0.687",
    "conformal bound",
]
FORBIDDEN_KEEP_GATE_WIN = re.compile(r"(?i)keep-gate.{0,24}\b(win|won)\b|\b(win|won).{0,24}keep-gate")
AFFIRMATIVE_CERTIFYING_ORACLE = re.compile(
    r"(?i)\b(?:is|as|treated as|used as)\s+(?:a\s+)?certifying oracle\b"
)
FNV_CRYPTO_CLAIM = re.compile(r"\bFNV\b.*cryptograph|.cryptograph.*\bFNV\b", re.IGNORECASE)
FALSE_COMFORTS = ("not", "never", "bootstrap", "replaced")
CLI_STUB_TOKENS = ("stub", "placeholder", "TBD", "TODO")


def method_kind(params: str) -> str:
    p = params.strip()
    if p.startswith("&mut self") or ", &mut self" in p:
        return "&mut"
    if p.startswith("&self") or ", &self" in p:
        return "&"
    if p.startswith("self") or ", self" in p:
        return "self"
    return "assoc"


def impl_block_names(text: str, ident: str) -> set[tuple[str, str]]:
    """(name, receiver-kind) pairs of the `pub fn` entries inside an impl."""
    start = text.index(ident)
    open_idx = text.index("{", start)
    depth = 0
    end = open_idx
    for i in range(open_idx, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    block = text[open_idx : end + 1]
    return {(name, method_kind(params)) for name, params in PUB_FN.findall(block)}


def inventory_fence(text: str) -> str:
    """Content of the first ```rust fence whose block names CompilerSession."""
    fences = text.split("```")
    for i in range(0, len(fences) - 1, 2):
        inner = fences[i + 1]
        if inner.lstrip().startswith("rust\n") and "impl CompilerSession" in inner:
            return inner
    raise SystemExit("FAIL: inventory has no ```rust fence naming impl CompilerSession")


def main() -> int:
    parser = argparse.ArgumentParser()
    root = pathlib.Path(__file__).resolve().parent.parent
    parser.add_argument("--root", type=pathlib.Path, default=root)
    parser.add_argument("--crate-map", type=pathlib.Path)
    parser.add_argument("--inventory", type=pathlib.Path)
    parser.add_argument("--session", type=pathlib.Path)
    parser.add_argument("--manifest", type=pathlib.Path)
    args = parser.parse_args()

    root = args.root.resolve()
    crate_map = (args.crate_map or root / "implementation/CRATE_MAP.md").resolve()
    inventory = (args.inventory or root / "implementation/PUBLIC_API_INVENTORY.md").resolve()
    session = (args.session or root / "crates/emath-sema/src/session.rs").resolve()
    manifest = (args.manifest or root / "Cargo.toml").resolve()

    violations: list[str] = []

    # Workspace members from [workspace] members (tomllib, std only).
    with open(manifest, "rb") as fh:
        members = list(tomllib.load(fh)["workspace"]["members"])

    map_text = crate_map.read_text(encoding="utf-8")
    implemented: dict[str, str] = {}
    planned: set[str] = set()
    for line in map_text.splitlines():
        m = ROW.match(line)
        if m:
            name, path = m.groups()
            if name in implemented or path in implemented.values():
                violations.append(f"duplicate implemented row: {name} -> {path}")
            implemented[name] = path
            continue
        p = PLANNED_ROW.match(line)
        if p:
            planned.add(p.group(1))

    # R1: every implemented row maps to a directory that exists on disk,
    # and its name matches the directory (no name rewritten behind a
    # surviving path).
    for name, path in sorted(implemented.items()):
        if not (root / path).is_dir():
            violations.append(f"implemented row maps to a missing directory: {name} -> {path}")
        elif name != path.rsplit("/", 1)[-1] and name != path:
            violations.append(
                f"implemented row name does not match its directory: {name} -> {path}"
            )

    # R2: every directory under crates/ is mapped (no disk-unmapped crates).
    if (root / "crates").is_dir():
        mapped_paths = set(implemented.values())
        for child in sorted((root / "crates").iterdir()):
            rel = f"crates/{child.name}"
            if child.is_dir() and rel not in mapped_paths:
                violations.append(f"crates directory missing from the map: {rel}")

    # R3: every workspace member is mapped.
    member_paths = set(implemented.values())
    for member in sorted(members):
        if member not in member_paths:
            violations.append(f"workspace member missing from the map: {member}")

    # Planned names must not masquerade as implemented rows.
    for name in sorted(planned & implemented.keys()):
        violations.append(f"name listed as both planned and implemented: {name}")

    # Inventory: CompilerSession block must match session.rs exactly
    # (name + receiver kind). Documenting a request-typed method as on the
    # session when it is not would round PARTIAL to PASSING; refusing it is
    # the gate's honesty job.
    session_text = session.read_text(encoding="utf-8")
    session_names = impl_block_names(session_text, "impl CompilerSession")
    fence = inventory_fence(inventory.read_text(encoding="utf-8"))
    inventory_names = {(name, method_kind(params)) for name, params in PUB_FN.findall(fence)}

    for entry in sorted(session_names - inventory_names):
        violations.append(f"session method missing from inventory: {entry}")
    for entry in sorted(inventory_names - session_names):
        violations.append(f"inventory method not on the session (must be marked [planned]): {entry}")

    # Honest Partial marker: request-typed tokens only appear with [planned].
    for lineno, line in enumerate(
        inventory.read_text(encoding="utf-8").splitlines(), start=1
    ):
        if any(token in line for token in PLANNED_TOKENS) and "[planned]" not in line:
            violations.append(
                f"inventory line {lineno} names a request type without a [planned] marker"
            )

    # ERROR_CODES issued-list contract: every emitted code must have an
    # issued bullet, and no code may be issued twice (one code, one story).
    err_path = root / "implementation/ERROR_CODES.md"
    err_text = err_path.read_text(encoding="utf-8")
    issued: dict[str, int] = {}
    in_issued = False
    for lineno, line in enumerate(err_text.splitlines(), start=1):
        if line.startswith("## Issued codes"):
            in_issued = True
            continue
        if in_issued and line.startswith("## "):
            break
        m = ISSUED_BULLET.match(line)
        if m:
            code = m.group(1)
            if code in issued:
                violations.append(
                    f"issued code listed twice: {code} (lines {issued[code]} and {lineno})"
                )
            issued[code] = lineno
    # The annex and the issued list must name the same codes; the annex
    # is generated, so this re-checks the hand-maintained prose list.
    annex_start = err_text.find("## Completeness annex: every issued code")
    annex_codes = set(CODE_RE.findall(err_text[annex_start:])) if annex_start >= 0 else set()
    for code in sorted(annex_codes - set(issued)):
        violations.append(f"emitted code has no issued-list entry: {code}")
    for code in sorted(set(issued) - annex_codes):
        violations.append(f"issued entry not in the generated annex (stale): {code}")

    # Forbidden-claim scan over the contract doc set: remediated claims
    # must not reappear (Dew certifying, conformal bound, FNV crypto,
    # keep-gate win, BUILD_STATUS oracle, CLI stubs as implemented).
    for rel in DOC_SET:
        path = root / rel
        if not path.is_file():
            continue
        for lineno, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            lowered = line.lower()
            for phrase in FORBIDDEN_PHRASES:
                if phrase.lower() in lowered:
                    violations.append(f"{rel}:{lineno} forbidden claim: {phrase!r}")
            if FORBIDDEN_KEEP_GATE_WIN.search(line):
                violations.append(f"{rel}:{lineno} claims a keep-gate win")
            if AFFIRMATIVE_CERTIFYING_ORACLE.search(line):
                violations.append(f"{rel}:{lineno} presents BUILD_STATUS as a certifying oracle")
            if FNV_CRYPTO_CLAIM.search(line) and not any(
                token in line for token in FALSE_COMFORTS
            ):
                violations.append(
                    f"{rel}:{lineno} asserts FNV as cryptographic identity without a caveat"
                )
            if rel == "implementation/CLI_REFERENCE.md" and any(
                token in lowered for token in CLI_STUB_TOKENS
            ):
                violations.append(f"{rel}:{lineno} documents a stub as surface: {line.strip()[:80]}")

    if violations:
        print("doc gates: FAIL", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1

    print(
        f"crate map: {len(implemented)} implemented rows exist on disk, "
        f"{len(members)} workspace members mapped, "
        f"{len(planned)} planned rows (never certifying)"
    )
    print(
        f"inventory: {len(inventory_names)} CompilerSession methods match "
        f"session.rs (name + receiver); request types stay [planned]"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
