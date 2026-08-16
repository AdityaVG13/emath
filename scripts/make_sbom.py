#!/usr/bin/env python3
"""SBOM generator.

Deterministic SPDX-lite inventory over every workspace package: name,
version, license, edition, repo-relative manifest path, and a sha256 digest
of the package directory tree (excluding target/, .git/, .beads/,
internal/, artifacts/ and Cargo.lock). Emitted sorted and timestamp-free,
so regeneration is byte-identical until the tree changes.

Usage: python3 scripts/make_sbom.py [output.json]
       (default: licenses/SBOM.json)
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXCLUDED_DIRS = {"target", ".git", ".beads", "internal", "artifacts", "licenses"}

def package_sha256(directory):
    """Deterministic directory digest: sorted relative paths + contents."""
    digest = hashlib.sha256()
    for root, dirs, files in os.walk(directory):
        dirs[:] = sorted(d for d in dirs if d not in EXCLUDED_DIRS)
        for name in sorted(files):
            if name == "Cargo.lock":
                continue
            path = os.path.join(root, name)
            with open(path, "rb") as handle:
                data = handle.read()
            digest.update(os.path.relpath(path, directory).encode("utf-8"))
            digest.update(b"\0")
            digest.update(data)
            digest.update(b"\0")
    return digest.hexdigest()

def workspace_version():
    """Root package version (first `version = "..."` in the root manifest)."""
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as handle:
        text = handle.read()
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    if not match:
        raise SystemExit("cannot determine workspace version from root Cargo.toml")
    return match.group(1)

def main():
    out_path = (
        sys.argv[1]
        if len(sys.argv) > 1
        else os.path.join(ROOT, "legal", "SBOM.json")
    )
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=ROOT,
            capture_output=True,
            check=True,
        ).stdout
    )

    packages = []
    for package in metadata["packages"]:
        directory = os.path.dirname(package["manifest_path"])
        packages.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package.get("license"),
                "edition": package.get("edition"),
                "manifest": os.path.relpath(package["manifest_path"], ROOT),
                "sha256": package_sha256(directory),
            }
        )
    packages.sort(key=lambda p: (p["name"], p["version"]))

    sbom = {
        "schema": "emath.sbom.v1",
        "spec": "SPDX-2.3-lite (subset)",
        "document": {
            "name": "emath",
            "version": workspace_version(),
            "generator": "scripts/make_sbom.py",
        },
        "checksum_notes": (
            "sha256 over each package directory; excludes target/, .git/, "
            ".beads/, internal/, artifacts/, licenses/ and Cargo.lock"
        ),
        "packages": packages,
    }
    text = json.dumps(sbom, indent=2, sort_keys=True) + "\n"
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as handle:
        handle.write(text)
    print(
        "wrote {} ({} packages)".format(
            os.path.relpath(out_path, ROOT), len(packages)
        )
    )

if __name__ == "__main__":
    main()
