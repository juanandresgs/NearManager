#!/usr/bin/env python3
"""Ensure hosted GitHub Actions use only the reviewed trigger policy."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "specs" / "github-actions-policy.toml"


def trigger_block(source: str) -> str:
    lines = source.splitlines()
    start = next(
        (index for index, line in enumerate(lines) if line.rstrip() == "on:"),
        None,
    )
    if start is None:
        raise ValueError("missing top-level on block")
    block = []
    for line in lines[start + 1 :]:
        if line and not line.startswith((" ", "\t")):
            break
        block.append(line)
    return "\n".join(block)


def events(block: str) -> set[str]:
    return {
        match.group(1)
        for line in block.splitlines()
        if (match := re.match(r"^  ([A-Za-z_][A-Za-z0-9_-]*):", line))
    }


def main() -> int:
    with POLICY.open("rb") as source:
        policy = tomllib.load(source)
    errors = []
    if policy.get("schema_version") != 1:
        errors.append("GitHub Actions policy schema must be version 1")
    checked = 0
    for record in policy.get("workflow", []):
        relative = record["path"]
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing workflow: {relative}")
            continue
        try:
            block = trigger_block(path.read_text(encoding="utf-8"))
        except ValueError as error:
            errors.append(f"{relative}: {error}")
            continue
        actual = events(block)
        allowed = set(record.get("allowed_events", []))
        if actual != allowed:
            errors.append(
                f"{relative}: trigger events {sorted(actual)} do not match "
                f"policy {sorted(allowed)}"
            )
        required_tag = record.get("required_tag_pattern")
        if required_tag and f'tags: ["{required_tag}"]' not in block:
            errors.append(
                f"{relative}: release push must be restricted to tag pattern {required_tag}"
            )
        checked += 1
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"GitHub Actions policy: PASS ({checked} workflows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
