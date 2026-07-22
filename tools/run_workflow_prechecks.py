#!/usr/bin/env python3
"""Run deterministic prechecks for operator workflows without claiming operator observation."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

PRECHECKS = {
    "WF-TRASH-002": [
        ["cargo", "test", "-p", "near-local-fs", "--locked", "trash_"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "operation_preview"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "protected_filesystem_root_shows_blocking_denial"],
    ],
    "OP-INPUT-ALT-LOOKUP": [
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "filename_lookup"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "enhanced_modifier_hold"],
    ],
    "OP-INPUT-KEYBAR": [
        ["cargo", "test", "-p", "near-ui", "--locked", "--test", "keymap", "function_hints_follow"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "mouse_click_right_click_wheel_and_keybar"],
    ],
    "OP-INPUT-RESTORATION": [
        ["cargo", "test", "-p", "near-terminal", "--locked", "restor"],
        ["cargo", "test", "-p", "near-terminal", "--locked", "suspend_"],
        ["cargo", "test", "-p", "near-pty", "--locked", "nested_vim_and_ssh_client"],
    ],
    "OP-PANEL-NAVIGATION-001": [
        ["python3", "tools/validate_interaction_conformance.py"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "panel_interaction_conformance"],
    ],
    "OP-SETTINGS-001": [
        ["cargo", "test", "-p", "near-config", "--locked", "settings"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "settings_surface"],
    ],
    "OP-SHELL-001": [
        ["cargo", "test", "-p", "near-pty", "--locked", "shell_profile"],
        ["cargo", "test", "-p", "near-pty", "--locked", "interactive_native_shell"],
    ],
    "OP-VIEWER-001": [
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "viewer::tests"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "viewer_and_editor_open_policies"],
    ],
    "OP-EDITOR-001": [
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "editor::tests"],
        ["cargo", "test", "-p", "near-ui", "--locked", "--lib", "editor_save_as_and_external_change"],
    ],
    "OP-FAR-PARITY-001": [
        ["python3", "tools/validate_far_parity.py"],
        ["python3", "tools/validate_project.py"],
        ["python3", "tools/check_public_api.py"],
    ],
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default=".near/qualification/workflow-prechecks.json")
    args = parser.parse_args()
    results = []
    failed = False
    for scenario, commands in PRECHECKS.items():
        command_results = []
        for command in commands:
            completed = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            output = completed.stdout
            command_results.append({
                "command": command,
                "exit_code": completed.returncode,
                "output_sha256": hashlib.sha256(output.encode()).hexdigest(),
                "output": output,
            })
            failed |= completed.returncode != 0
        results.append({
            "scenario": scenario,
            "status": "failed" if any(item["exit_code"] for item in command_results) else "passed",
            "commands": command_results,
        })
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()
    payload = {
        "schema_version": 1,
        "kind": "automated-precheck",
        "operator_observation": False,
        "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "revision": revision,
        "results": results,
        "status": "failed" if failed else "passed",
    }
    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"workflow prechecks: {payload['status'].upper()} ({output_path.relative_to(ROOT)})")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
