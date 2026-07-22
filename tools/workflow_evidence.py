#!/usr/bin/env python3
"""Initialize, record, and validate Near operator workflow evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "specs" / "operator-workflows.toml"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def revision() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def current_platform() -> str:
    return {"Darwin": "macos", "Linux": "linux"}.get(platform.system(), platform.system().lower())


def manifest() -> dict:
    with MANIFEST.open("rb") as source:
        return tomllib.load(source)


def expected_entries(data: dict, platform_id: str) -> list[tuple[str, str]]:
    terminals = data["terminals"][platform_id]
    expected = []
    for scenario in data["scenario"]:
        if platform_id not in scenario["platforms"]:
            continue
        scenario_terminals = terminals if scenario["terminal_matrix"] else ["headless/operator"]
        expected.extend((scenario["id"], terminal) for terminal in scenario_terminals)
    return expected


def initialize(path: Path, platform_id: str, operator: str) -> None:
    data = manifest()
    document = {
        "schema_version": 1,
        "revision": revision(),
        "platform": platform_id,
        "operator": operator,
        "entries": [
            {
                "scenario": scenario,
                "terminal": terminal,
                "status": "pending",
                "artifacts": [],
                "notes": "",
            }
            for scenario, terminal in expected_entries(data, platform_id)
        ],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def record(
    path: Path,
    scenario: str,
    terminal: str,
    status: str,
    notes: str,
    artifacts: list[Path],
) -> None:
    document = json.loads(path.read_text(encoding="utf-8"))
    entry = next(
        (
            entry
            for entry in document["entries"]
            if entry["scenario"] == scenario and entry["terminal"] == terminal
        ),
        None,
    )
    if entry is None:
        raise SystemExit(f"unknown evidence entry: {scenario} / {terminal}")
    recorded = []
    artifact_store = path.resolve().parent / "artifacts"
    artifact_store.mkdir(parents=True, exist_ok=True)
    for artifact in artifacts:
        resolved = artifact.resolve()
        if not resolved.is_file():
            raise SystemExit(f"artifact does not exist: {artifact}")
        artifact_digest = digest(resolved)
        stored = artifact_store / f"{artifact_digest}-{resolved.name}"
        if resolved != stored:
            shutil.copyfile(resolved, stored)
        try:
            relative = stored.relative_to(ROOT)
        except ValueError as error:
            raise SystemExit(f"evidence path must be inside the repository: {path}") from error
        recorded.append({"path": str(relative), "sha256": artifact_digest})
    entry.update(status=status, notes=notes, artifacts=recorded)
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def validate(path: Path, platform_id: str | None = None) -> list[str]:
    data = manifest()
    document = json.loads(path.read_text(encoding="utf-8"))
    errors = []
    selected_platform = platform_id or document.get("platform")
    if document.get("schema_version") != 1:
        errors.append("unsupported evidence schema")
    if document.get("revision") != revision():
        errors.append("evidence revision does not match the checked-out revision")
    if document.get("platform") != selected_platform:
        errors.append("evidence platform does not match the selected platform")
    if not document.get("operator"):
        errors.append("operator identity is required")
    entries = {
        (entry.get("scenario"), entry.get("terminal")): entry
        for entry in document.get("entries", [])
    }
    for key in expected_entries(data, selected_platform):
        entry = entries.get(key)
        if entry is None:
            errors.append(f"missing workflow entry: {key[0]} / {key[1]}")
            continue
        if entry.get("status") != "passed":
            errors.append(f"workflow did not pass: {key[0]} / {key[1]}")
        if not entry.get("notes"):
            errors.append(f"workflow notes are required: {key[0]} / {key[1]}")
        if not entry.get("artifacts"):
            errors.append(f"workflow artifacts are required: {key[0]} / {key[1]}")
        for artifact in entry.get("artifacts", []):
            artifact_path = ROOT / artifact.get("path", "")
            if not artifact_path.is_file():
                errors.append(f"missing artifact: {artifact_path}")
            elif digest(artifact_path) != artifact.get("sha256"):
                errors.append(f"artifact digest mismatch: {artifact_path}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init")
    init.add_argument("--output", type=Path, required=True)
    init.add_argument("--platform", choices=("macos", "linux"), default=current_platform())
    init.add_argument("--operator", required=True)
    add = commands.add_parser("record")
    add.add_argument("--evidence", type=Path, required=True)
    add.add_argument("--scenario", required=True)
    add.add_argument("--terminal", required=True)
    add.add_argument("--status", choices=("passed", "failed"), required=True)
    add.add_argument("--notes", required=True)
    add.add_argument("--artifact", type=Path, action="append", default=[])
    check = commands.add_parser("validate")
    check.add_argument("--evidence", type=Path, required=True)
    check.add_argument("--platform", choices=("macos", "linux"))
    args = parser.parse_args()
    if args.command == "init":
        initialize(args.output, args.platform, args.operator)
        return 0
    if args.command == "record":
        record(args.evidence, args.scenario, args.terminal, args.status, args.notes, args.artifact)
        return 0
    errors = validate(args.evidence, args.platform)
    if errors:
        print("Operator workflow evidence: FAIL", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("Operator workflow evidence: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
