#!/usr/bin/env python3
"""Validate the Far Manager parity specification and its evidence links."""

from __future__ import annotations

import re
import sys
import tomllib
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "project/far-parity.toml"


def main() -> int:
    data = tomllib.loads(SPEC.read_text())
    errors: list[str] = []
    if data.get("schema") != 1:
        errors.append("project/far-parity.toml must declare schema = 1")

    areas = data.get("area", [])
    items = data.get("item", [])
    area_ids = {area.get("id") for area in areas}
    if len(area_ids) != len(areas):
        errors.append("Far parity area IDs must be unique")
    for area in areas:
        if not area.get("source_topics"):
            errors.append(f"area {area.get('id')} has no authoritative source topics")

    allowed_statuses = {"verified", "partial", "missing", "out-of-scope"}
    allowed_priorities = {"must", "should", "may"}
    item_ids: set[str] = set()
    coverage: defaultdict[str, int] = defaultdict(int)
    for item in items:
        item_id = item.get("id")
        if not isinstance(item_id, str) or not re.fullmatch(r"FAR-[A-Z]+-\d{3}", item_id):
            errors.append(f"invalid Far parity item ID: {item_id!r}")
            continue
        if item_id in item_ids:
            errors.append(f"duplicate Far parity item ID: {item_id}")
        item_ids.add(item_id)
        area = item.get("area")
        if area not in area_ids:
            errors.append(f"{item_id} references unknown area {area!r}")
        coverage[area] += 1
        if item.get("status") not in allowed_statuses:
            errors.append(f"{item_id} has invalid status {item.get('status')!r}")
        if item.get("priority") not in allowed_priorities:
            errors.append(f"{item_id} has invalid priority {item.get('priority')!r}")
        acceptance = item.get("acceptance", [])
        if not acceptance or any(not isinstance(value, str) or len(value) < 10 for value in acceptance):
            errors.append(f"{item_id} needs measurable acceptance criteria")
        evidence = item.get("evidence", [])
        if item.get("status") == "verified" and not evidence:
            errors.append(f"verified item {item_id} has no evidence")
        for relative in evidence:
            if not (ROOT / relative).exists():
                errors.append(f"{item_id} evidence does not exist: {relative}")

    for area_id in area_ids:
        if coverage[area_id] == 0:
            errors.append(f"Far parity area {area_id} has no items")

    counts = Counter(item.get("status") for item in items)
    print(f"Far parity validation: {len(areas)} areas, {len(items)} items, {dict(counts)}")
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
