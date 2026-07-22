#!/usr/bin/env python3
"""Validate independently sourced Far behavior contracts and their evidence."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/far-behavior-contract.toml"


def load(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def rust_test_symbols() -> set[str]:
    symbols: set[str] = set()
    for path in (ROOT / "crates").glob("*/**/*.rs"):
        symbols.update(
            re.findall(
                r"\bfn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(",
                path.read_text(encoding="utf-8"),
            )
        )
    return symbols


def external_test_symbols() -> set[str]:
    return {
        "idle_cpu_external_gate",
        "tmux_resource_kind_workflows",
        "registered_menu_actions_are_assessed",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--require-complete", action="store_true")
    args = parser.parse_args()

    data = load(SPEC)
    errors: list[str] = []
    if data.get("schema_version") != 1:
        errors.append("Far behavior contract must declare schema_version = 1")
    if not data.get("reference_build"):
        errors.append("Far behavior contract must identify the reference build")

    policy = data.get("policy", {})
    statuses = set(policy.get("statuses", []))
    production_statuses = set(policy.get("production_statuses", []))
    deviation_kinds = set(policy.get("deviation_kinds", []))
    parity = {
        item.get("id")
        for item in load(ROOT / "project/far-parity.toml").get("item", [])
    }
    tests = rust_test_symbols() | external_test_symbols()
    seen: set[str] = set()

    for behavior in data.get("behavior", []):
        behavior_id = behavior.get("id")
        if not isinstance(behavior_id, str) or not re.fullmatch(
            r"FBC-[A-Z]+-\d{3}", behavior_id
        ):
            errors.append(f"invalid behavior ID: {behavior_id!r}")
            continue
        if behavior_id in seen:
            errors.append(f"duplicate behavior ID: {behavior_id}")
        seen.add(behavior_id)
        if behavior.get("parity") not in parity:
            errors.append(f"{behavior_id} references unknown parity item")
        status = behavior.get("status")
        if status not in statuses:
            errors.append(f"{behavior_id} has invalid status {status!r}")
        if args.require_complete and status not in production_statuses:
            errors.append(f"{behavior_id} remains {status}")
        if len(str(behavior.get("source", ""))) < 12:
            errors.append(f"{behavior_id} needs an independent source")
        deviation = behavior.get("deviation")
        if deviation not in deviation_kinds:
            errors.append(f"{behavior_id} has invalid deviation {deviation!r}")
        if deviation != "none" and len(str(behavior.get("deviation_reason", ""))) < 20:
            errors.append(f"{behavior_id} needs a deviation reason")
        expected = behavior.get("expected", [])
        if len(expected) < 3 or any(len(str(item)) < 12 for item in expected):
            errors.append(f"{behavior_id} needs at least three measurable outcomes")
        evidence = behavior.get("tests", [])
        if not evidence:
            errors.append(f"{behavior_id} has no proving tests")
        for test in evidence:
            if test not in tests:
                errors.append(f"{behavior_id} references missing test {test!r}")

    print(f"Far behavior contract: {len(seen)} independently sourced behaviors")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
