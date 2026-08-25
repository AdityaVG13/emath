#!/usr/bin/env python3
"""Four-artifact rule gate (ELP governance; see elps/README.md).

A commit touching `language/grammar/*.ebnf` must also touch at least one
reference chapter, one example/fixture, and one test. A reference chapter
change with no grammar delta and no example/test companions is accepted
only with the `Reference-Only: true` trailer in the commit message.

File list comes from a git range by default (`--base HEAD~1 --head HEAD`);
`--files-from` overrides with a plain list (one path per line, `-` for
stdin) for unit-style checks, and `--head-msg` supplies the commit message
used for trailer checks instead of `git log`.

Usage:
    check_four_artifact.py [--base REF] [--head REF]
                           [--files-from FILE] [--head-msg FILE]

Exit: 0 pass, 1 violation, 2 usage error.
Stdlib only; deterministic.
"""

import argparse
import subprocess
import sys

REFERENCE = "language/reference/"
GRAMMAR = "language/grammar/"
EXAMPLES = "language/examples/"
TESTS = "tests/"

REFERENCE_ONLY_TRAILER = "Reference-Only: true"


def git_changed_files(base: str, head: str) -> list:
    return (
        subprocess.run(
            ["git", "diff", "--name-only", f"{base}..{head}"],
            check=True,
            capture_output=True,
            text=True,
        )
        .stdout.splitlines()
    )


def git_head_message(base: str, head: str) -> str:
    return subprocess.run(
        ["git", "log", "-1", "--format=%B", head],
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def has_trailer(message: str) -> bool:
    return any(
        line.strip() == REFERENCE_ONLY_TRAILER
        for line in message.splitlines()
    )


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="HEAD~1", help="git base ref (default HEAD~1)")
    parser.add_argument("--head", default="HEAD", help="git head ref (default HEAD)")
    parser.add_argument("--files-from", help="changed-path list file (`-` for stdin)")
    parser.add_argument("--head-msg", help="commit-message file (trailer checks)")
    args = parser.parse_args(argv)

    if args.files_from:
        if args.files_from == "-":
            changed = sys.stdin.read().splitlines()
        else:
            with open(args.files_from, encoding="utf-8") as fh:
                changed = fh.read().splitlines()
    else:
        changed = git_changed_files(args.base, args.head)

    try:
        if args.head_msg:
            with open(args.head_msg, encoding="utf-8") as fh:
                message = fh.read()
        else:
            message = git_head_message(args.base, args.head)
    except subprocess.CalledProcessError:
        message = ""

    grammar_changed = any(p.startswith(GRAMMAR) and p.endswith(".ebnf") for p in changed)
    reference_changed = any(p.startswith(REFERENCE) and p.endswith(".md") for p in changed)
    examples_changed = any(p.startswith(EXAMPLES) for p in changed)
    tests_changed = any(
        p.startswith(TESTS) or (p.startswith("tests/") and p.endswith((".rs", ".emath")))
        for p in changed
    )

    problems = []
    if grammar_changed:
        missing = []
        if not reference_changed:
            missing.append("reference chapter")
        if not examples_changed:
            missing.append("example or fixture")
        if not tests_changed:
            missing.append("test")
        if missing:
            problems.append(
                "grammar change without " + ", ".join(missing) + " — "
                "the four-artifact rule requires all four (see elps/README.md)"
            )
    elif reference_changed and not examples_changed and not tests_changed:
        if not has_trailer(message):
            problems.append(
                f"reference-only change must carry the `{REFERENCE_ONLY_TRAILER}` "
                "trailer in the commit message"
            )

    summary = " | ".join(
        [
            f"grammar {'Y' if grammar_changed else 'n'}",
            f"reference {'Y' if reference_changed else 'n'}",
            f"examples {'Y' if examples_changed else 'n'}",
            f"tests {'Y' if tests_changed else 'n'}",
        ]
    )
    print(f"check-four-artifact: {summary}")
    for problem in problems:
        print(f"check-four-artifact: error: {problem}")
    print(
        f"check-four-artifact: {'FAIL' if problems else 'pass'} "
        f"({len(changed)} changed files)"
    )
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
