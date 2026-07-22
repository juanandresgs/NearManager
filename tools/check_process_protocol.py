#!/usr/bin/env python3
"""Validate the process extension protocol against its immutable baseline."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CURRENT = ROOT / "specs/process-plugin-protocol.md"
VERSION = re.compile(r"^# Near Process Extension Protocol (\d+\.\d+\.\d+)$", re.MULTILINE)


def main() -> int:
    document = CURRENT.read_text()
    match = VERSION.search(document)
    errors: list[str] = []
    if not match:
        errors.append("process protocol title must contain a full SemVer version")
    else:
        version = match.group(1)
        baseline = ROOT / f"specs/process-plugin-v{version}.md"
        if not baseline.is_file():
            errors.append(f"missing immutable baseline {baseline.relative_to(ROOT)}")
        elif baseline.read_text() != document:
            errors.append(
                f"process protocol changed without changing protocol version {version}"
            )
    required_sections = ["## Package", "## Invocation", "## Response", "## macOS Sandbox", "## Compatibility"]
    for section in required_sections:
        if section not in document:
            errors.append(f"process protocol is missing {section}")
    if errors:
        print("Near process protocol compatibility: FAIL")
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("Near process protocol compatibility: PASS")
    print(f"  protocol: near-process@{match.group(1)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
