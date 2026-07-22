#!/usr/bin/env python3
"""Validate versioned Near WIT contracts against immutable release baselines."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CURRENT = ROOT / "specs/plugin.wit"
PACKAGE = re.compile(r"^package near:plugin@(\d+\.\d+\.\d+);", re.MULTILINE)


def main() -> int:
    document = CURRENT.read_text()
    match = PACKAGE.search(document)
    errors: list[str] = []
    if not match:
        errors.append("plugin.wit must declare package near:plugin with full SemVer")
    else:
        version = match.group(1)
        baseline = ROOT / f"specs/plugin-v{version}.wit"
        if not baseline.is_file():
            errors.append(f"missing immutable baseline {baseline.relative_to(ROOT)}")
        elif baseline.read_text() != document:
            errors.append(
                f"plugin.wit changed without changing package version {version}"
            )
    if "@since(version = 0.1.0)" not in document:
        errors.append("stable WIT items must use @since gates")
    if "@unstable(feature = provider-mutations)" not in document:
        errors.append("experimental WIT items must use @unstable feature gates")
    if errors:
        print("Near WIT compatibility: FAIL")
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("Near WIT compatibility: PASS")
    print(f"  package: near:plugin@{match.group(1)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
