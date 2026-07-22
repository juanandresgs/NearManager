#!/usr/bin/env python3
"""Validate Near's agent-facing abstraction ownership and evidence policy."""

from __future__ import annotations

import datetime as dt
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/abstraction-ownership.toml"
AGENT_INSTRUCTIONS = ROOT / "AGENTS.md"
QUALIFICATION = ROOT / "specs/qualification.toml"


def load(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def existing(errors: list[str], record_id: str, field: str, values: object) -> list[str]:
    if not isinstance(values, list):
        errors.append(f"{record_id} field {field} must be a list")
        return []
    result: list[str] = []
    for value in values:
        if not isinstance(value, str) or not value:
            errors.append(f"{record_id} field {field} contains an invalid path")
            continue
        result.append(value)
        if not (ROOT / value).exists():
            errors.append(f"{record_id} {field} path does not exist: {value}")
    return result


def main() -> int:
    errors: list[str] = []
    data = load(SPEC)
    if data.get("schema_version") != 1:
        errors.append("specs/abstraction-ownership.toml must declare schema_version = 1")

    policy = data.get("policy", {})
    allowed_layers = set(policy.get("allowed_layers", []))
    reusable_layers = set(policy.get("reusable_layers", []))
    consumer_values = set(policy.get("public_consumer_values", []))
    exception_statuses = set(policy.get("exception_statuses", []))
    required_evidence = set(policy.get("required_evidence", []))
    if required_evidence != {"model", "render", "application"}:
        errors.append("policy.required_evidence must contain model, render, and application")

    behavior_ids: set[str] = set()
    behavior_by_id: dict[str, dict] = {}
    allowed_statuses = {"partial", "implemented", "verified", "deprecated"}
    for behavior in data.get("behavior", []):
        behavior_id = behavior.get("id")
        if not isinstance(behavior_id, str) or not re.fullmatch(r"ABST-[A-Z]+-\d{3}", behavior_id):
            errors.append(f"invalid behavior ID: {behavior_id!r}")
            continue
        if behavior_id in behavior_ids:
            errors.append(f"duplicate behavior ID: {behavior_id}")
        behavior_ids.add(behavior_id)
        behavior_by_id[behavior_id] = behavior

        layer = behavior.get("layer")
        if layer not in allowed_layers:
            errors.append(f"{behavior_id} has invalid layer {layer!r}")
        if behavior.get("status") not in allowed_statuses:
            errors.append(f"{behavior_id} has invalid status {behavior.get('status')!r}")
        if not isinstance(behavior.get("application_policy"), str) or len(behavior["application_policy"]) < 20:
            errors.append(f"{behavior_id} needs an explicit application policy boundary")

        owner = behavior.get("owner")
        if not isinstance(owner, str) or not (ROOT / owner).exists():
            errors.append(f"{behavior_id} owner does not exist: {owner!r}")

        public_api = existing(errors, behavior_id, "public_api", behavior.get("public_api"))
        for evidence in required_evidence:
            paths = existing(
                errors,
                behavior_id,
                f"evidence_{evidence}",
                behavior.get(f"evidence_{evidence}"),
            )
            if not paths:
                errors.append(f"{behavior_id} has no {evidence} evidence")

        consumer = behavior.get("public_consumer")
        if consumer not in consumer_values:
            errors.append(f"{behavior_id} has invalid public_consumer value {consumer!r}")
        consumer_evidence = existing(
            errors, behavior_id, "consumer_evidence", behavior.get("consumer_evidence")
        )
        if layer in reusable_layers and not public_api:
            errors.append(f"reusable behavior {behavior_id} has no declared public API")
        if consumer == "required" and not consumer_evidence:
            errors.append(f"{behavior_id} requires public-consumer evidence")
        if behavior.get("status") == "verified" and consumer == "pending":
            errors.append(f"verified behavior {behavior_id} still has pending consumer proof")

    today = dt.date.today()
    active_exceptions = 0
    for exception in data.get("exception", []):
        exception_id = exception.get("id")
        if not isinstance(exception_id, str) or not re.fullmatch(r"ABST-EXC-\d{3}", exception_id):
            errors.append(f"invalid exception ID: {exception_id!r}")
            continue
        if exception.get("behavior") not in behavior_by_id:
            errors.append(f"{exception_id} references unknown behavior {exception.get('behavior')!r}")
        status = exception.get("status")
        if status not in exception_statuses:
            errors.append(f"{exception_id} has invalid status {status!r}")
        for field in ("owner", "reason", "review_date", "expires"):
            if not isinstance(exception.get(field), str) or not exception[field]:
                errors.append(f"{exception_id} is missing {field}")
        try:
            review_date = dt.date.fromisoformat(exception.get("review_date", ""))
            expires = dt.date.fromisoformat(exception.get("expires", ""))
            if review_date > expires:
                errors.append(f"{exception_id} review_date is after expiry")
            if status == "active":
                active_exceptions += 1
                if expires < today:
                    errors.append(f"{exception_id} expired on {expires.isoformat()}")
        except ValueError:
            errors.append(f"{exception_id} has invalid ISO dates")

    instructions = AGENT_INSTRUCTIONS.read_text(encoding="utf-8")
    for required in (
        "Mandatory Abstraction Harvest",
        "specs/abstraction-ownership.toml",
        "A `FarWorkspace` test alone cannot prove",
        "validate_abstraction_policy.py",
    ):
        if required not in instructions:
            errors.append(f"AGENTS.md is missing required policy text: {required!r}")

    gates = {gate.get("id"): gate for gate in load(QUALIFICATION).get("gates", [])}
    gate = gates.get("abstraction-policy")
    if gate is None:
        errors.append("qualification manifest has no abstraction-policy gate")
    elif gate.get("command") != ["python3", "tools/validate_abstraction_policy.py"]:
        errors.append("abstraction-policy gate runs the wrong command")
    elif set(gate.get("profiles", [])) != {"developer", "wave", "production"}:
        errors.append("abstraction-policy gate must run in developer, wave, and production")

    print(
        f"Abstraction policy: {len(behavior_ids)} behaviors, "
        f"{active_exceptions} active exceptions"
    )
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
