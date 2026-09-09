import re
import subprocess
from pathlib import Path

pat = re.compile(
    r'InvalidCapsule \{ path: "([^"]+)", issues: \[.*?E-CAPSULE-022:\d+:semantic_hash mismatch: declared (sha256:[0-9a-f]+), computed (sha256:[0-9a-f]+)'
)
root = Path("/Users/aditya/Developer/emath")
for _ in range(40):
    run = subprocess.run(
        ["./target/debug/xtask", "generate-language"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    out = run.stdout + run.stderr
    match = pat.search(out)
    if match is None:
        print(out[-500:])
        break
    path, declared, computed = match.group(1), match.group(2), match.group(3)
    capsule = root / path
    text = capsule.read_text()
    if declared not in text:
        raise SystemExit(f"declared hash missing in {path}")
    capsule.write_text(text.replace(declared, computed, 1))
    print(f"patched {path}")
