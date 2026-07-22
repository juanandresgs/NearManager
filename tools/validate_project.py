#!/usr/bin/env python3
"""Validate Near's docs-as-code project definition using only the Python stdlib."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "project"
ID_PATTERN = re.compile(r"^[A-Z][A-Z0-9]+(?:-[A-Z0-9]+)+$")
ADR_REQUIREMENTS = re.compile(r"^- Requirements:\s*(.+)$", re.MULTILINE)
ADR_STATUS = re.compile(r"^- Status:\s*(\S+)$", re.MULTILINE)


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def validate_id(errors: list[str], kind: str, value: object) -> None:
    if not isinstance(value, str) or not ID_PATTERN.fullmatch(value):
        fail(errors, f"{kind} has invalid ID: {value!r}")


def validate_required_fields(
    errors: list[str], kind: str, record: dict, required: set[str], allowed: set[str]
) -> None:
    missing = sorted(required - record.keys())
    unknown = sorted(record.keys() - allowed)
    if missing:
        fail(errors, f"{kind} {record.get('id', '<unknown>')} missing fields: {', '.join(missing)}")
    if unknown:
        fail(errors, f"{kind} {record.get('id', '<unknown>')} unknown fields: {', '.join(unknown)}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit summary as JSON")
    args = parser.parse_args()

    errors: list[str] = []
    warnings: list[str] = []

    capabilities_data = load_toml(PROJECT / "capabilities.toml")
    requirements_data = load_toml(PROJECT / "requirements.toml")
    milestones_data = load_toml(PROJECT / "milestones.toml")
    workflows_data = load_toml(PROJECT / "workflows.toml")
    with (PROJECT / "schemas/project-records.schema.json").open() as handle:
        json.load(handle)

    for name, data in [
        ("capabilities", capabilities_data),
        ("requirements", requirements_data),
        ("milestones", milestones_data),
        ("workflows", workflows_data),
    ]:
        if data.get("schema") != 1:
            fail(errors, f"{name}.toml must declare schema = 1")

    capabilities = capabilities_data.get("capability", [])
    requirements = requirements_data.get("requirement", [])
    milestones = milestones_data.get("milestone", [])
    workflows = workflows_data.get("workflow", [])

    capability_ids: set[str] = set()
    for capability in capabilities:
        validate_required_fields(
            errors,
            "capability",
            capability,
            {"id", "name", "description"},
            {"id", "name", "description"},
        )
        capability_id = capability.get("id")
        validate_id(errors, "capability", capability_id)
        if capability_id in capability_ids:
            fail(errors, f"duplicate capability ID: {capability_id}")
        capability_ids.add(capability_id)

    milestone_ids: set[str] = set()
    milestone_by_id: dict[str, dict] = {}
    for milestone in milestones:
        validate_required_fields(
            errors,
            "milestone",
            milestone,
            {"id", "name", "goal", "exit_requirements"},
            {"id", "name", "goal", "exit_requirements"},
        )
        milestone_id = milestone.get("id")
        if not isinstance(milestone_id, str) or not re.fullmatch(r"M\d+", milestone_id):
            fail(errors, f"milestone has invalid ID: {milestone_id!r}")
        if milestone_id in milestone_ids:
            fail(errors, f"duplicate milestone ID: {milestone_id}")
        milestone_ids.add(milestone_id)
        milestone_by_id[milestone_id] = milestone

    required_requirement_fields = {
        "id",
        "title",
        "type",
        "priority",
        "status",
        "statement",
        "capabilities",
        "milestone",
        "rationale",
        "acceptance",
        "verification",
        "planned_evidence",
        "design_refs",
    }
    requirement_ids: set[str] = set()
    requirement_by_id: dict[str, dict] = {}
    capability_coverage: defaultdict[str, list[str]] = defaultdict(list)
    milestone_coverage: defaultdict[str, list[str]] = defaultdict(list)
    priority_words = {"must": "MUST", "should": "SHOULD", "may": "MAY"}
    allowed_types = {"functional", "quality", "constraint", "security", "governance"}
    allowed_statuses = {"draft", "accepted", "implemented", "verified", "deprecated"}
    allowed_verification = {"test", "analysis", "inspection", "demonstration"}

    for requirement in requirements:
        validate_required_fields(
            errors,
            "requirement",
            requirement,
            required_requirement_fields,
            required_requirement_fields,
        )
        requirement_id = requirement.get("id")
        validate_id(errors, "requirement", requirement_id)
        if not isinstance(requirement_id, str) or not requirement_id.startswith("REQ-"):
            fail(errors, f"requirement ID must start with REQ-: {requirement_id}")
        if requirement_id in requirement_ids:
            fail(errors, f"duplicate requirement ID: {requirement_id}")
        requirement_ids.add(requirement_id)
        requirement_by_id[requirement_id] = requirement

        requirement_type = requirement.get("type")
        priority = requirement.get("priority")
        status = requirement.get("status")
        verification = requirement.get("verification")
        if requirement_type not in allowed_types:
            fail(errors, f"{requirement_id} has invalid type: {requirement_type}")
        if priority not in priority_words:
            fail(errors, f"{requirement_id} has invalid priority: {priority}")
        elif priority_words[priority] not in requirement.get("statement", ""):
            fail(errors, f"{requirement_id} statement must contain {priority_words[priority]}")
        if status not in allowed_statuses:
            fail(errors, f"{requirement_id} has invalid status: {status}")
        if verification not in allowed_verification:
            fail(errors, f"{requirement_id} has invalid verification method: {verification}")

        linked_capabilities = requirement.get("capabilities", [])
        if not linked_capabilities:
            fail(errors, f"{requirement_id} has no capabilities")
        for capability_id in linked_capabilities:
            if capability_id not in capability_ids:
                fail(errors, f"{requirement_id} references unknown capability {capability_id}")
            capability_coverage[capability_id].append(requirement_id)

        milestone_id = requirement.get("milestone")
        if milestone_id != "continuous" and milestone_id not in milestone_ids:
            fail(errors, f"{requirement_id} references unknown milestone {milestone_id}")
        milestone_coverage[milestone_id].append(requirement_id)

        acceptance = requirement.get("acceptance", [])
        if not acceptance or any(not isinstance(item, str) or len(item.strip()) < 5 for item in acceptance):
            fail(errors, f"{requirement_id} must have non-empty measurable acceptance criteria")
        if not requirement.get("planned_evidence"):
            fail(errors, f"{requirement_id} has no planned evidence")

        for reference in requirement.get("design_refs", []):
            reference_path = ROOT / reference.split("#", 1)[0]
            if not reference_path.exists():
                fail(errors, f"{requirement_id} design reference does not exist: {reference}")

        if status == "verified":
            missing_evidence = [item for item in requirement.get("planned_evidence", []) if not (ROOT / item).exists()]
            if missing_evidence:
                fail(errors, f"{requirement_id} is verified but evidence is missing: {missing_evidence}")

    for capability_id in sorted(capability_ids):
        if not capability_coverage[capability_id]:
            fail(errors, f"capability has no requirements: {capability_id}")

    for milestone in milestones:
        milestone_id = milestone["id"]
        for requirement_id in milestone.get("exit_requirements", []):
            if requirement_id not in requirement_ids:
                fail(errors, f"{milestone_id} references unknown exit requirement {requirement_id}")
            elif requirement_by_id[requirement_id]["milestone"] != milestone_id:
                fail(
                    errors,
                    f"{milestone_id} exit requirement {requirement_id} is assigned to "
                    f"{requirement_by_id[requirement_id]['milestone']}",
                )

    workflow_ids: set[str] = set()
    workflow_coverage: defaultdict[str, list[str]] = defaultdict(list)
    for workflow in workflows:
        validate_required_fields(
            errors,
            "workflow",
            workflow,
            {"id", "name", "requirements", "given", "when", "then"},
            {"id", "name", "requirements", "given", "when", "then"},
        )
        workflow_id = workflow.get("id")
        validate_id(errors, "workflow", workflow_id)
        if workflow_id in workflow_ids:
            fail(errors, f"duplicate workflow ID: {workflow_id}")
        workflow_ids.add(workflow_id)
        for field in ["given", "when", "then"]:
            if not workflow.get(field):
                fail(errors, f"{workflow_id} has empty {field}")
        for requirement_id in workflow.get("requirements", []):
            if requirement_id not in requirement_ids:
                fail(errors, f"{workflow_id} references unknown requirement {requirement_id}")
            workflow_coverage[requirement_id].append(workflow_id)

    adr_files = sorted((PROJECT / "decisions").glob("[0-9][0-9][0-9][1-9]-*.md"))
    accepted_adrs = 0
    adr_requirement_links: defaultdict[str, list[str]] = defaultdict(list)
    for path in adr_files:
        text = path.read_text()
        status_match = ADR_STATUS.search(text)
        if not status_match:
            fail(errors, f"ADR missing status: {path.relative_to(ROOT)}")
        elif status_match.group(1) == "accepted":
            accepted_adrs += 1
        requirements_match = ADR_REQUIREMENTS.search(text)
        if not requirements_match:
            fail(errors, f"ADR missing requirement links: {path.relative_to(ROOT)}")
            continue
        for requirement_id in [item.strip() for item in requirements_match.group(1).split(",")]:
            if requirement_id not in requirement_ids:
                fail(errors, f"ADR {path.name} references unknown requirement {requirement_id}")
            adr_requirement_links[requirement_id].append(path.name)

    workflow_uncovered = [
        requirement_id
        for requirement_id, requirement in requirement_by_id.items()
        if requirement["type"] == "functional" and not workflow_coverage[requirement_id]
    ]
    if workflow_uncovered:
        warnings.append(f"{len(workflow_uncovered)} functional requirements have no workflow coverage yet")

    threat_data = load_toml(PROJECT / "security/threat-model.toml")
    if threat_data.get("schema") != 1:
        fail(errors, "security/threat-model.toml must declare schema = 1")
    asset_ids = {item["id"] for item in threat_data.get("asset", [])}
    boundary_ids = {item["id"] for item in threat_data.get("boundary", [])}
    threat_ids: set[str] = set()
    for threat in threat_data.get("threat", []):
        threat_id = threat.get("id")
        validate_id(errors, "threat", threat_id)
        if threat_id in threat_ids:
            fail(errors, f"duplicate threat ID: {threat_id}")
        threat_ids.add(threat_id)
        for requirement_id in threat.get("requirements", []):
            if requirement_id not in requirement_ids:
                fail(errors, f"{threat_id} references unknown requirement {requirement_id}")
        for asset_id in threat.get("assets", []):
            if asset_id not in asset_ids:
                fail(errors, f"{threat_id} references unknown asset {asset_id}")
        for boundary_id in threat.get("boundaries", []):
            if boundary_id not in boundary_ids:
                fail(errors, f"{threat_id} references unknown boundary {boundary_id}")
        if not threat.get("mitigations"):
            fail(errors, f"{threat_id} has no mitigations")

    risk_data = load_toml(PROJECT / "risks.toml")
    if risk_data.get("schema") != 1:
        fail(errors, "risks.toml must declare schema = 1")
    risk_ids: set[str] = set()
    for risk in risk_data.get("risk", []):
        risk_id = risk.get("id")
        validate_id(errors, "risk", risk_id)
        if risk_id in risk_ids:
            fail(errors, f"duplicate risk ID: {risk_id}")
        risk_ids.add(risk_id)
        for requirement_id in risk.get("requirements", []):
            if requirement_id not in requirement_ids:
                fail(errors, f"{risk_id} references unknown requirement {requirement_id}")

    summary = {
        "capabilities": len(capabilities),
        "requirements": len(requirements),
        "requirements_by_type": dict(sorted(Counter(item["type"] for item in requirements).items())),
        "requirements_by_priority": dict(sorted(Counter(item["priority"] for item in requirements).items())),
        "requirements_by_milestone": dict(sorted(Counter(item["milestone"] for item in requirements).items())),
        "milestones": len(milestones),
        "workflows": len(workflows),
        "accepted_adrs": accepted_adrs,
        "requirements_with_adrs": len(adr_requirement_links),
        "threats": len(threat_ids),
        "risks": len(risk_ids),
        "functional_requirements_with_workflows": sum(
            1 for item in requirements if item["type"] == "functional" and workflow_coverage[item["id"]]
        ),
        "warnings": warnings,
        "errors": errors,
    }

    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print("Near project-definition validation")
        print(f"  capabilities: {summary['capabilities']}")
        print(f"  requirements: {summary['requirements']} {summary['requirements_by_type']}")
        print(f"  milestones: {summary['milestones']}")
        print(f"  workflows: {summary['workflows']}")
        print(f"  accepted ADRs: {summary['accepted_adrs']}")
        print(f"  threats: {summary['threats']}")
        print(f"  risks: {summary['risks']}")
        print(
            "  functional workflow coverage: "
            f"{summary['functional_requirements_with_workflows']}/"
            f"{summary['requirements_by_type'].get('functional', 0)}"
        )
        for warning in warnings:
            print(f"WARNING: {warning}")
        for error in errors:
            print(f"ERROR: {error}")
        print("PASS" if not errors else "FAIL")

    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
