#!/usr/bin/env python3
"""Enforce monotonic architecture and resource-footprint ratchets."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/architecture-budgets.toml"
OUTPUT = ROOT / ".near/qualification/architecture-baseline.json"


def rust_lines(root: Path) -> int:
    return sum(
        len(path.read_text(encoding="utf-8").splitlines())
        for path in root.rglob("*.rs")
    )


def workspace_field_count(source: str) -> int:
    start = source.index("pub struct FarWorkspace")
    brace_depth = 0
    fields = 0
    for line in source[start:].splitlines():
        brace_depth += line.count("{") - line.count("}")
        stripped = line.strip()
        if brace_depth == 1 and ":" in stripped and stripped.endswith(","):
            fields += 1
        if brace_depth == 0 and fields:
            break
    return fields


def transitive_package_count() -> int:
    output = subprocess.check_output(
        [
            "cargo",
            "tree",
            "--locked",
            "--target",
            "all",
            "-p",
            "near-fm",
            "--prefix",
            "none",
        ],
        cwd=ROOT,
        text=True,
    )
    return len({line.split(maxsplit=1)[0] for line in output.splitlines() if line})


def main() -> int:
    with SPEC.open("rb") as handle:
        spec = tomllib.load(handle)
    if spec.get("schema_version") != 1:
        print("ERROR: architecture budget schema must be version 1", file=sys.stderr)
        return 1

    workspace_path = ROOT / "crates/near-ui/src/workspace.rs"
    workspace_source = workspace_path.read_text(encoding="utf-8")
    ui_runtime_source = "\n".join(
        (ROOT / path).read_text(encoding="utf-8")
        for path in ["crates/near-ui/src/application.rs", "crates/near-ui/src/workspace.rs"]
    )

    measurements = {
        "workspace_lines": len(workspace_source.splitlines()),
        "workspace_fields": workspace_field_count(workspace_source),
        "near_ui_lines": rust_lines(ROOT / "crates/near-ui/src"),
        "near_fm_transitive_packages": transitive_package_count(),
        "unix_fixed_idle_poll_sites": ui_runtime_source.count("DEFAULT_IDLE_POLL"),
        "concrete_dependencies": [],
    }
    errors: list[str] = []
    ratchet = spec.get("ratchet", {})
    for name, measured in measurements.items():
        if name == "concrete_dependencies":
            continue
        maximum = ratchet.get(f"{name}_max")
        if not isinstance(maximum, int):
            errors.append(f"missing integer ratchet for {name}")
        elif measured > maximum:
            errors.append(f"{name} grew to {measured}; ratchet is {maximum}")

    for dependency in spec.get("concrete_dependency", []):
        path = ROOT / dependency["source"]
        occurrences = path.read_text(encoding="utf-8").count(dependency["pattern"])
        record = {
            "source": dependency["source"],
            "pattern": dependency["pattern"],
            "occurrences": occurrences,
            "target": dependency["target"],
        }
        measurements["concrete_dependencies"].append(record)
        if occurrences > dependency["occurrences_max"]:
            errors.append(
                f"{dependency['source']} concrete dependency occurrences grew to "
                f"{occurrences}; ratchet is {dependency['occurrences_max']}"
            )

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "measurements": measurements,
                "targets": spec.get("targets", {}),
                "status": "failed" if errors else "passed",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    for name, measured in measurements.items():
        if name != "concrete_dependencies":
            print(f"{name}: {measured} (target {spec['targets'].get(name)})")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"architecture budgets: PASS ({OUTPUT.relative_to(ROOT)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
