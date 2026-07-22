#!/usr/bin/env python3
"""Build and ratchet compact-core versus full-capability Near artifacts."""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/capability-budgets.toml"
OUTPUT = ROOT / ".near/qualification/capability-artifacts.json"


def run(command: list[str], target: Path | None = None) -> str:
    environment = None
    if target is not None:
        import os

        environment = os.environ.copy()
        environment["CARGO_TARGET_DIR"] = str(target)
    return subprocess.check_output(command, cwd=ROOT, env=environment, text=True)


def package_count(extra: list[str]) -> int:
    output = run(
        [
            "cargo",
            "tree",
            "--locked",
            "--target",
            "all",
            "-p",
            "near-fm",
            *extra,
            "--prefix",
            "none",
        ]
    )
    return len({line.split(maxsplit=1)[0] for line in output.splitlines() if line})


def main() -> int:
    with SPEC.open("rb") as handle:
        spec = tomllib.load(handle)
    if spec.get("schema_version") != 1:
        print("ERROR: capability budget schema must be version 1", file=sys.stderr)
        return 1
    budget = spec["near_fm"]
    artifact_target = ROOT / "target/capability-artifacts"
    binary = "near-fm.exe" if sys.platform == "win32" else "near-fm"
    binary_path = artifact_target / "release" / binary
    run(
        ["cargo", "build", "--offline", "--locked", "--release", "-p", "near-fm"],
        artifact_target,
    )
    full_size = binary_path.stat().st_size
    run(
        [
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
            "-p",
            "near-fm",
            "--no-default-features",
        ],
        artifact_target,
    )
    base_size = binary_path.stat().st_size
    measurements = {
        "base_binary_bytes": base_size,
        "full_binary_bytes": full_size,
        "plugin_binary_delta_bytes": full_size - base_size,
        "base_transitive_packages": package_count(["--no-default-features"]),
        "full_transitive_packages": package_count([]),
    }
    errors: list[str] = []
    for name in [
        "base_binary_bytes",
        "full_binary_bytes",
        "base_transitive_packages",
        "full_transitive_packages",
    ]:
        maximum = budget[f"{name}_max"]
        if measurements[name] > maximum:
            errors.append(f"{name} is {measurements[name]}; maximum is {maximum}")
    minimum_delta = budget["plugin_binary_delta_bytes_min"]
    if measurements["plugin_binary_delta_bytes"] < minimum_delta:
        errors.append(
            "plugin capability is not producing the expected separately accountable artifact "
            f"delta ({measurements['plugin_binary_delta_bytes']} < {minimum_delta})"
        )
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "measurements": measurements,
                "targets": spec["targets"],
                "status": "failed" if errors else "passed",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    for name, value in measurements.items():
        print(f"{name}: {value}")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"capability artifacts: PASS ({OUTPUT.relative_to(ROOT)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
