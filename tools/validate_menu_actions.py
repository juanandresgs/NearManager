#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/menu-actions.toml"
REPORT = ROOT / "project/evidence/menu-action-assessment.md"
SOURCE_ROOTS = [ROOT / "crates/near-ui/src", ROOT / "apps/near-demo/src"]


def main() -> int:
    document = tomllib.loads(SPEC.read_text(encoding="utf-8"))
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("specs/menu-actions.toml must declare schema_version = 1")

    commands = document.get("static_commands", [])
    if len(commands) != len(set(commands)):
        errors.append("static_commands contains duplicates")
    expected_count = document.get("static_command_count")
    if not isinstance(expected_count, int) or expected_count < 1:
        errors.append("static_command_count must be a positive integer")
    elif len(commands) != expected_count:
        errors.append(
            f"expected {expected_count} unique static commands, found {len(set(commands))}"
        )

    report = REPORT.read_text(encoding="utf-8")
    for command in commands:
        occurrences = report.count(f"`{command}`")
        if occurrences != 1:
            errors.append(
                f"assessment must contain exactly one row for {command}; found {occurrences}"
            )

    nested_commands = document.get("nested_commands", [])
    if len(nested_commands) != len(set(nested_commands)):
        errors.append("nested_commands contains duplicates")
    for command in nested_commands:
        occurrences = report.count(f"`{command}`")
        if occurrences != 1:
            errors.append(
                f"nested assessment must contain exactly one row for {command}; found {occurrences}"
            )

    for catalog in document.get("dynamic_catalogs", []):
        if not (ROOT / catalog).is_file():
            errors.append(f"dynamic menu catalog does not exist: {catalog}")
        if f"`{catalog}`" not in report:
            errors.append(f"assessment does not identify dynamic menu catalog: {catalog}")

    for family in document.get("dynamic_families", []):
        if f"| {family} |" not in report:
            errors.append(f"assessment is missing dynamic family row: {family}")

    source = "\n".join(
        path.read_text(encoding="utf-8")
        for source_root in SOURCE_ROOTS
        for path in source_root.glob("*.rs")
    )
    test_names = set(re.findall(r"(?m)^\s*fn\s+([a-zA-Z0-9_]+)\s*\(", source))
    for test in document.get("required_tests", []):
        if test not in test_names:
            errors.append(f"required menu-action evidence test is missing: {test}")

    required_phrases = [
        "activation through the real menu route",
        "explicit denial",
        "operator-only",
    ]
    lowered = report.lower()
    for phrase in required_phrases:
        if phrase not in lowered:
            errors.append(f"assessment is missing required scope statement: {phrase}")

    if errors:
        print("Menu action assessment: FAIL")
        for error in errors:
            print(f"  - {error}")
        return 1
    print(
        "Menu action assessment: PASS "
        f"({len(commands)} static commands, "
        f"{len(nested_commands)} nested commands, "
        f"{len(document.get('dynamic_families', []))} dynamic families, "
        f"{len(document.get('required_tests', []))} required evidence tests)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
