#!/usr/bin/env python3
"""Validate executable interaction conformance and explicit discovery coverage."""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/interaction-conformance.toml"
KEYMAP = ROOT / "specs/keymap.toml"


def load(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def command_id(binding: dict) -> str | None:
    run = binding.get("run")
    if isinstance(run, str):
        return run
    if isinstance(run, dict) and isinstance(run.get("command"), str):
        return run["command"]
    return None


def keymap_bindings() -> dict[str, dict[str, str]]:
    contexts: dict[str, tuple[list[str], dict[str, str]]] = {}
    for context in load(KEYMAP).get("context", []):
        context_id = context.get("id")
        if not isinstance(context_id, str):
            continue
        bindings: dict[str, str] = {}
        for binding in context.get("bindings", []):
            key = binding.get("on")
            command = command_id(binding)
            if isinstance(key, str) and command is not None:
                bindings[key] = command
        parents = [
            parent for parent in context.get("inherits", []) if isinstance(parent, str)
        ]
        contexts[context_id] = (parents, bindings)

    result: dict[str, dict[str, str]] = {}

    def resolve(context_id: str, stack: tuple[str, ...] = ()) -> dict[str, str]:
        if context_id in result:
            return result[context_id]
        if context_id in stack:
            return {}
        parents, own = contexts.get(context_id, ([], {}))
        effective: dict[str, str] = {}
        for parent in parents:
            effective.update(resolve(parent, (*stack, context_id)))
        effective.update(own)
        result[context_id] = effective
        return effective

    for context_id in contexts:
        resolve(context_id)
    return result


def rust_test_symbols() -> set[str]:
    symbols: set[str] = set()
    for path in (ROOT / "crates").rglob("*.rs"):
        source = path.read_text(encoding="utf-8")
        symbols.update(re.findall(r"\bfn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(", source))
    return symbols


def workspace_command_contracts() -> tuple[set[str], set[str]]:
    source = (ROOT / "crates/near-ui/src/workspace.rs").read_text(encoding="utf-8")
    registry_start = source.index("fn register_commands")
    registry_end = source.index("fn focused_panel", registry_start)
    registry = set(re.findall(r'"(near\.[a-z0-9.-]+)"', source[registry_start:registry_end]))
    handlers: set[str] = set()
    for path in (ROOT / "crates/near-ui/src").glob("**/*.rs"):
        handlers.update(
            re.findall(
                r'^\s*"(near\.[a-z0-9.-]+)"(?:\s*\|\s*"near\.[a-z0-9.-]+")*\s*=>',
                path.read_text(encoding="utf-8"),
                re.MULTILINE,
            )
        )
    return registry, handlers


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()

    data = load(SPEC)
    errors: list[str] = []
    if data.get("schema_version") != 1:
        errors.append("interaction conformance must declare schema_version = 1")

    policy = data.get("policy", {})
    required_layers = set(policy.get("required_assertion_layers", []))
    case_statuses = set(policy.get("case_statuses", []))
    discovery_statuses = set(policy.get("discovery_statuses", []))
    production_case_status = policy.get("production_case_status")
    production_discovery_status = policy.get("production_discovery_status")

    evidence_entries: dict[tuple[str, str], dict] = {}
    evidence_revision = None
    if args.require_complete:
        platform_id = {"Darwin": "macos", "Linux": "linux"}.get(
            platform.system(), platform.system().lower()
        )
        evidence_path = args.evidence or (
            ROOT / ".near/qualification/operator" / platform_id / "evidence.json"
        )
        if evidence_path.is_file():
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            evidence_revision = evidence.get("revision")
            evidence_entries = {
                (entry.get("scenario"), entry.get("terminal")): entry
                for entry in evidence.get("entries", [])
            }
        else:
            errors.append(f"operator evidence is missing: {evidence_path}")

    requirements = {item.get("id") for item in load(ROOT / "project/requirements.toml").get("requirement", [])}
    parity = {item.get("id") for item in load(ROOT / "project/far-parity.toml").get("item", [])}
    ownership = {
        item.get("id"): item
        for item in load(ROOT / "specs/abstraction-ownership.toml").get("behavior", [])
    }
    operator_manifest = load(ROOT / "specs/operator-workflows.toml")
    scenario_records = operator_manifest.get("scenario", [])
    scenarios = {item.get("id"): item for item in scenario_records}
    bindings = keymap_bindings()
    tests = rust_test_symbols()
    registered_commands, handled_commands = workspace_command_contracts()

    suites = data.get("suite", [])
    suite_ids: set[str] = set()
    suite_by_id: dict[str, dict] = {}
    for suite in suites:
        suite_id = suite.get("id")
        if not isinstance(suite_id, str) or not re.fullmatch(r"IC-[A-Z]+(?:-[A-Z]+)*", suite_id):
            errors.append(f"invalid interaction suite ID: {suite_id!r}")
            continue
        if suite_id in suite_ids:
            errors.append(f"duplicate interaction suite ID: {suite_id}")
        suite_ids.add(suite_id)
        suite_by_id[suite_id] = suite
        context = suite.get("context")
        if context not in bindings:
            errors.append(f"{suite_id} references unknown keymap context {context!r}")
        owner_behavior = suite.get("owner_behavior")
        owner = ownership.get(owner_behavior)
        if owner is None:
            errors.append(f"{suite_id} references unknown abstraction owner {owner_behavior!r}")
        elif owner.get("layer") == "application-policy":
            errors.append(f"{suite_id} reusable mechanics cannot be owned only by application policy")
        for requirement in suite.get("requirements", []):
            if requirement not in requirements:
                errors.append(f"{suite_id} references unknown requirement {requirement}")
        for item in suite.get("parity", []):
            if item not in parity:
                errors.append(f"{suite_id} references unknown parity item {item}")
        if suite.get("operator_scenario") not in scenarios:
            errors.append(f"{suite_id} references unknown operator scenario {suite.get('operator_scenario')!r}")

    case_ids: set[str] = set()
    operator_suites: set[str] = set()
    for case in data.get("case", []):
        case_id = case.get("id")
        if not isinstance(case_id, str) or not re.fullmatch(r"IC-[A-Z]+-\d{3}", case_id):
            errors.append(f"invalid interaction case ID: {case_id!r}")
            continue
        if case_id in case_ids:
            errors.append(f"duplicate interaction case ID: {case_id}")
        case_ids.add(case_id)
        suite = suite_by_id.get(case.get("suite"))
        if suite is None:
            errors.append(f"{case_id} references unknown suite {case.get('suite')!r}")
            continue
        status = case.get("status")
        if status not in case_statuses:
            errors.append(f"{case_id} has invalid status {status!r}")
        if args.require_complete and status not in {production_case_status, "verified"}:
            errors.append(f"{case_id} is {status}; production requires {production_case_status}")
        expected = case.get("expected", [])
        if not expected or any(not isinstance(value, str) or len(value) < 12 for value in expected):
            errors.append(f"{case_id} needs measurable expected outcomes")
        layers = set(case.get("assertion_layers", []))
        if not {"model", "render"}.issubset(layers):
            errors.append(f"{case_id} must assert both model and render behavior")
        if "boundary" not in layers:
            errors.append(f"{case_id} must assert a boundary or failure condition")
        if case.get("keys") and not {"binding", "command"}.issubset(layers):
            errors.append(f"{case_id} with keys must assert binding and command behavior")
        unknown_layers = layers - required_layers
        if unknown_layers:
            errors.append(f"{case_id} has unknown assertion layers: {sorted(unknown_layers)}")
        test = case.get("test")
        if test not in tests:
            errors.append(f"{case_id} references missing Rust test {test!r}")
        context_bindings = bindings.get(suite.get("context"), {})
        declared_commands = set(case.get("commands", []))
        for command in declared_commands:
            if command not in registered_commands:
                errors.append(f"{case_id} command is not registered: {command}")
            if command not in handled_commands:
                errors.append(f"{case_id} command has no workspace handler: {command}")
        for key in case.get("keys", []):
            command = context_bindings.get(key)
            if command is None:
                errors.append(f"{case_id} key {key} is not bound in {suite.get('context')}")
            elif command not in declared_commands:
                errors.append(f"{case_id} key {key} resolves to {command}, not one of {sorted(declared_commands)}")
        if case.get("operator_required"):
            scenario = scenarios.get(suite.get("operator_scenario"), {})
            if not scenario.get("terminal_matrix"):
                errors.append(f"{case_id} requires an operator terminal matrix scenario")
            operator_suites.add(case.get("suite"))

    if args.require_complete:
        platform_id = {"Darwin": "macos", "Linux": "linux"}.get(
            platform.system(), platform.system().lower()
        )
        expected_terminals = operator_manifest["terminals"].get(platform_id, [])
        for suite_id in sorted(operator_suites):
            scenario_id = suite_by_id[suite_id].get("operator_scenario")
            missing = [
                terminal
                for terminal in expected_terminals
                if evidence_entries.get((scenario_id, terminal), {}).get("status") != "passed"
            ]
            if missing:
                errors.append(
                    f"{suite_id} lacks passed operator evidence for: {', '.join(missing)}"
                )

    discovery_ids: set[str] = set()
    for discovery in data.get("discovery", []):
        discovery_id = discovery.get("id")
        if not isinstance(discovery_id, str) or not re.fullmatch(r"DISC-[A-Z]+-\d{3}", discovery_id):
            errors.append(f"invalid discovery ID: {discovery_id!r}")
            continue
        if discovery_id in discovery_ids:
            errors.append(f"duplicate discovery ID: {discovery_id}")
        discovery_ids.add(discovery_id)
        owner_behavior = discovery.get("owner_behavior")
        if owner_behavior not in ownership:
            errors.append(
                f"{discovery_id} references unknown abstraction owner {owner_behavior!r}"
            )
        status = discovery.get("status")
        if status not in discovery_statuses:
            errors.append(f"{discovery_id} has invalid status {status!r}")
        if args.require_complete and status != production_discovery_status:
            errors.append(f"{discovery_id} is {status}; production requires {production_discovery_status}")
        if not discovery.get("sources"):
            errors.append(f"{discovery_id} has no discovery sources")
        outputs = discovery.get("required_outputs", [])
        if len(outputs) < 3:
            errors.append(f"{discovery_id} needs at least three required outputs")

    print(
        f"Interaction conformance: {len(suites)} suites, {len(case_ids)} cases, "
        f"{len(discovery_ids)} discovery records"
    )
    if args.require_complete and evidence_revision is not None:
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
        if evidence_revision != revision:
            errors.append("operator evidence revision does not match the checked-out revision")
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
