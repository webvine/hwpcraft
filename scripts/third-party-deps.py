"""Regenerate THIRD-PARTY-DEPENDENCIES.md from `cargo metadata` (resolved
dependency graph of the HwpCraft packages, excluding workspace members).

Usage: python -I scripts/third-party-deps.py [--check]
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-DEPENDENCIES.md"


def main(argv: list[str]) -> int:
    meta = json.loads(
        subprocess.run(
            ["cargo", "+1.98.1", "metadata", "--format-version", "1", "--offline", "--locked"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    members = set(meta["workspace_members"])
    by_id = {p["id"]: p for p in meta["packages"]}
    # Everything reachable from the workspace members in the resolved graph.
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen: set[str] = set()
    stack = [m for m in members]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        stack.extend(dep["pkg"] for dep in nodes[pid]["deps"])
    rows = []
    rhwp_version = ""
    for pid in seen:
        p = by_id[pid]
        if pid in members:
            continue
        if p["name"] == "rhwp":
            rhwp_version = p["version"]
        rows.append((p["name"], p["version"], p.get("license") or "(see repository)", p.get("repository") or ""))
    rows.sort(key=lambda r: (r[0].lower(), r[1]))
    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    lines = [
        "# Third-party Rust dependencies",
        "",
        f"Generated from Cargo.lock with cargo metadata (HwpCraft {version}, RHWP {rhwp_version}).",
        "Crates offered under a choice of licenses are used under MIT where available, otherwise Apache-2.0.",
        "",
        "| Crate | Version | License | Repository |",
        "|---|---|---|---|",
    ]
    lines += [f"| {n} | {v} | {lic} | {repo} |" for n, v, lic, repo in rows]
    text = "\n".join(lines) + "\n"
    if "--check" in argv:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != text:
            print("THIRD-PARTY-DEPENDENCIES.md is out of date; run scripts/third-party-deps.py")
            return 1
        print("THIRD-PARTY-DEPENDENCIES.md is current")
        return 0
    OUT.write_text(text, encoding="utf-8")
    licenses = sorted({lic for _, _, lic, _ in rows})
    print(f"{len(rows)} crates written; licenses: {licenses}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
