#!/usr/bin/env python3
from __future__ import annotations

import unittest
import tomllib
from pathlib import Path

from qualify import has_mandatory_failure
from run_workflow_prechecks import PRECHECKS


class QualificationStatusTests(unittest.TestCase):
    def test_filtered_near_ui_commands_select_exact_test_targets(self) -> None:
        manifest_path = Path(__file__).resolve().parents[1] / "specs" / "qualification.toml"
        with manifest_path.open("rb") as handle:
            manifest = tomllib.load(handle)
        commands = [
            gate["command"]
            for gate in manifest["gates"]
            if gate.get("command", [])[:4] == ["cargo", "test", "-p", "near-ui"]
        ]
        commands.extend(
            command
            for scenario in PRECHECKS.values()
            for command in scenario
            if command[:4] == ["cargo", "test", "-p", "near-ui"]
        )
        for command in commands:
            self.assertTrue(
                any(target in command for target in ("--lib", "--test", "--tests")),
                f"filtered Near UI command launches unrelated targets: {command}",
            )

    def test_capability_artifact_gate_allows_two_cold_windows_release_builds(self) -> None:
        manifest_path = Path(__file__).resolve().parents[1] / "specs" / "qualification.toml"
        with manifest_path.open("rb") as handle:
            manifest = tomllib.load(handle)
        gate = next(gate for gate in manifest["gates"] if gate["id"] == "capability-artifacts")
        self.assertGreaterEqual(gate["timeout_seconds"], 1_800)

    def test_unsupported_high_severity_gate_is_an_explicit_degradation(self) -> None:
        results = [{"status": "unsupported", "severity": "high"}]
        self.assertFalse(has_mandatory_failure(results))

    def test_failed_or_blocked_non_waivable_gate_is_fatal(self) -> None:
        self.assertTrue(has_mandatory_failure([{"status": "failed", "severity": "high"}]))
        self.assertTrue(has_mandatory_failure([{"status": "blocked", "severity": "critical"}]))

    def test_low_severity_failure_is_waivable(self) -> None:
        results = [{"status": "failed", "severity": "medium"}]
        self.assertFalse(has_mandatory_failure(results))


if __name__ == "__main__":
    unittest.main()
