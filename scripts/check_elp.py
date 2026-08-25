#!/usr/bin/env python3
"""ELP document-shape gate (emath language proposals; see elps/README.md).

Validates every `elps/ELP-NNNN-<slug>.md`:

- filename serial is 4 digits and the slug matches the `# ELP-...` title,
- all seven mandatory `## ` sections are present in canonical order,
- no placeholder text (`TODO`, `TBD`, or a section body that is empty or
  just `...`),
- the four-artifact plan names at least one file under each of the four
  artifact classes (reference, grammar, examples, tests).

The template (`elps/ELP-TEMPLATE.md`) is validated for the seven-section
shape (it is allowed to contain guidance text).

Usage:
    check_elp.py [ELPS_DIR]

Exit: 0 otherwise, 1 when any ELP fails, 2 on usage error.
Stdlib only; deterministic.
"""

import argparse
import re
import sys
from pathlib import Path

SECTIONS = [
    "Motivation and coverage claim",
    "Grammar delta",
    "Lowering and world interactions",
    "Meaning-preservation analysis",
    "Migration",
    "Four-artifact plan",
    "Refusals",
]

# Template headings are numbered (`## 1. Motivation ...`); ELP headings
# may be bare. Compare on the stripped title.
_SECTION_TITLE = re.compile(r"^(?:\d+\.\s*)?(.*)$")


def section_title(heading: str) -> str:
    match = _SECTION_TITLE.match(heading)
    return match.group(1) if match else heading

PLACEHOLDER = re.compile(r"\b(TODO|TBD|FIXME)\b")


def check_elp(path: Path) -> list:
    problems = []
    text = path.read_text(encoding="utf-8")

    match = re.fullmatch(r"ELP-(\d{4})-([a-z0-9-]+)\.md", path.name)
    if not match:
        problems.append(
            f"filename `{path.name}` must match ELP-NNNN-<slug>.md (4-digit serial, kebab slug)"
        )
        return problems
    serial, slug = match.groups()

    title = re.search(r"^# ELP-\d{4}-[a-z0-9-]+", text, re.MULTILINE)
    if not title:
        problems.append("missing `# ELP-NNNN-<slug>` title line")
    else:
        expected = f"# ELP-{serial}-{slug}"
        if not text.startswith(expected + "\n"):
            problems.append(f"title must be `{expected}` (filename/title mismatch)")

    headings = re.findall(r"^## ([^\n]+)", text, re.MULTILINE)
    stripped_headings = [section_title(h) for h in headings]
    if stripped_headings[: len(SECTIONS)] != SECTIONS:
        problems.append(
            "the first seven `## ` headings must be, in order: "
            + "; ".join(SECTIONS[:3])
            + "; ... (see template)"
        )
    else:
        for index, heading in enumerate(SECTIONS, 1):
            start = text.find(f"## {headings[index - 1]}")
            end = (
                text.find("\n## ", start + len(heading))
                if index < len(SECTIONS)
                else len(text)
            )
            if end < 0:
                end = len(text)
            body = text[start:end]
            body_lines = [
                ln.strip() for ln in body.splitlines()[1:] if ln.strip()
            ]
            if not body_lines:
                problems.append(f"section {index} `{heading}` is empty")
                continue
            if len(body_lines) == 1 and body_lines[0] in ("...", ".", "TBD", "TODO"):
                problems.append(f"section {index} `{heading}` is a placeholder")
        if PLACEHOLDER.search(text):
            problems.append("placeholder text (TODO/TBD/FIXME) present")

        if "## 6. Four-artifact plan" in text or "## Four-artifact plan" in text:
            for artifact, patterns in (
                ("reference", ("language/reference/",)),
                ("grammar", ("language/grammar/",)),
                ("examples", ("language/examples/",)),
                ("tests", ("tests/",)),
            ):
                if not any(p in text for p in patterns):
                    problems.append(
                        f"four-artifact plan must name at least one {artifact} path"
                    )
    return problems


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elps_dir", nargs="?", default="elps", help="ELP directory")
    args = parser.parse_args(argv)

    root = Path(args.elps_dir)
    if not root.is_dir():
        print(f"check-elp: error: {root} is not a directory", file=sys.stderr)
        return 2

    template = root / "ELP-TEMPLATE.md"
    elps = sorted(path for path in root.glob("ELP-*.md") if path.name != "ELP-TEMPLATE.md")
    problems = []
    if not template.is_file():
        problems.append(f"missing template {template}")
    else:
        text = template.read_text(encoding="utf-8")
        headings = [section_title(h) for h in re.findall(r"^## ([^\n]+)", text, re.MULTILINE)]
        if headings[: len(SECTIONS)] != SECTIONS:
            problems.append(f"{template}: seven-section shape required")

    for path in elps:
        for problem in check_elp(path):
            problems.append(f"{path}: {problem}")

    for problem in problems:
        print(f"check-elp: error: {problem}")
    print(
        f"check-elp: {len(elps)} ELPs + template checked; "
        f"{len(problems)} problems"
    )
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
