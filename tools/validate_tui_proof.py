#!/usr/bin/env python3
"""Validate the separate NearTuiProof consumer against an exact Near revision."""

from __future__ import annotations

import argparse
import subprocess
import sys
import tomllib
from pathlib import Path

PUBLIC_CRATES = {"near-app", "near-core", "near-config", "near-terminal", "near-testkit", "near-ui"}
FORBIDDEN_SOURCE = ("ratatui", "crossterm", "near_local_fs", "FarWorkspace", "FocusedPanel")
REQUIRED_SOURCE = (
    "ApplicationWorkflowHarness",
    "CollectionStateSnapshot",
    "CollectionSurface",
    "CollectionTargetScope",
    "CollectionViewport",
    "DialogSurface",
    "DualSurfaceLayout",
    "EditorSurface",
    "HelpSurface",
    "MenuSurface",
    "OperationPresentation",
    "SettingsSurface",
    "TaskSurface",
    "TerminalDiagnostics",
    "TerminalEvent",
    "TerminalSurface",
    "ViewerSurface",
    "near.collection.page",
    "near.collection.scroll-horizontal",
)


def dependency_tables(document: dict) -> list[dict]:
    tables = []
    for name in ("dependencies", "dev-dependencies", "build-dependencies"):
        if isinstance(document.get(name), dict):
            tables.append(document[name])
    for target in document.get("target", {}).values():
        if isinstance(target, dict):
            tables.extend(dependency_tables(target))
    return tables


def validate(repo: Path, revision: str) -> list[str]:
    errors = []
    manifest = repo / "Cargo.toml"
    if not manifest.is_file():
        return ["NearTuiProof Cargo.toml is missing"]
    with manifest.open("rb") as source:
        document = tomllib.load(source)
    seen_near = set()
    for table in dependency_tables(document):
        for name, specification in table.items():
            if isinstance(specification, dict) and "path" in specification:
                errors.append(f"path dependency is forbidden: {name}")
            package = specification.get("package", name) if isinstance(specification, dict) else name
            if package in PUBLIC_CRATES:
                seen_near.add(package)
                if not isinstance(specification, dict) or not specification.get("git"):
                    errors.append(f"Near dependency must use Git: {package}")
                elif specification.get("rev") != revision:
                    errors.append(f"Near dependency is not pinned to {revision}: {package}")
    if "near-app" not in seen_near:
        errors.append("NearTuiProof must consume near-app")
    source_text = ""
    for source in repo.glob("src/**/*.rs"):
        text = source.read_text(encoding="utf-8")
        source_text += text
        for forbidden in FORBIDDEN_SOURCE:
            if forbidden in text:
                errors.append(f"forbidden application-facing assumption {forbidden} in {source}")
    for required in REQUIRED_SOURCE:
        if required not in source_text:
            errors.append(f"NearTuiProof does not exercise required public contract: {required}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--skip-tests", action="store_true")
    args = parser.parse_args()
    repo = args.repo.resolve()
    errors = validate(repo, args.revision)
    if errors:
        print("NearTuiProof validation: FAIL", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    if not args.skip_tests:
        completed = subprocess.run(["cargo", "test", "--locked"], cwd=repo, check=False)
        if completed.returncode:
            return completed.returncode
    print("NearTuiProof validation: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
    "ListNavigation",
