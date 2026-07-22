#!/usr/bin/env python3
"""Run resumable, manifest-driven Near qualification gates."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import time
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "specs" / "qualification.toml"
STATE_ROOT = ROOT / ".near" / "qualification"
RESULT_PATH = STATE_ROOT / "qualification.json"
STATE_PATH = STATE_ROOT / "state.json"

TRANSIENT_FAILURES = (
    "spurious network error",
    "failed to get `",
    "unable to update registry",
    "http2 framing layer",
    "failed to download from",
    "operation timed out",
    "connection reset by peer",
    "temporary failure in name resolution",
)
NON_WAIVABLE = {"critical", "high"}


def has_mandatory_failure(results: list[dict[str, Any]]) -> bool:
    return any(
        result["status"] in {"failed", "blocked"}
        and result["severity"] in NON_WAIVABLE
        for result in results
    )


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def command_output(command: list[str]) -> str:
    try:
        return subprocess.check_output(command, cwd=ROOT, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def platform_id() -> str:
    names = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}
    return names.get(platform.system(), platform.system().lower())


def source_digest() -> str:
    tracked = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT,
    )
    hasher = hashlib.sha256()
    for encoded in sorted(item for item in tracked.split(b"\0") if item):
        relative = os.fsdecode(encoded)
        path = ROOT / relative
        if path.is_symlink():
            content = b"symlink\0" + os.fsencode(os.readlink(path))
        elif path.is_file():
            content = path.read_bytes()
        else:
            content = b"missing\0"
        hasher.update(len(encoded).to_bytes(8, "big"))
        hasher.update(encoded)
        hasher.update(hashlib.sha256(content).digest())
    return hasher.hexdigest()


def load_json(path: Path, default: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return default


def gate_fingerprint(gate: dict[str, Any], context: dict[str, Any]) -> str:
    payload = {
        "gate": gate,
        "revision": context["revision"],
        "dirty": context["dirty"],
        "toolchain": context["toolchain"],
        "platform": context["platform"],
        "lockfile_digest": context["lockfile_digest"],
        "manifest_digest": context["manifest_digest"],
        "source_digest": context["source_digest"],
    }
    return hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()


def artifact_hashes(paths: list[str]) -> tuple[dict[str, str], list[str]]:
    hashes: dict[str, str] = {}
    missing: list[str] = []
    for relative in paths:
        path = ROOT / relative
        if path.is_file():
            hashes[relative] = digest(path)
        else:
            missing.append(relative)
    return hashes, missing


def run_gate(
    gate: dict[str, Any],
    log_path: Path,
    dirty: bool,
    current_platform: str,
    source_revision: str,
) -> tuple[str, int, str]:
    started = time.monotonic()
    if gate.get("builtin") == "clean-tree":
        output = "working tree is clean\n" if not dirty else "working tree has changes\n"
        log_path.write_text(output, encoding="utf-8")
        return ("passed" if not dirty else "failed", 0 if not dirty else 1, output)
    if gate.get("builtin") == "all-parity-verified":
        with (ROOT / "project" / "far-parity.toml").open("rb") as source:
            parity = tomllib.load(source)
        incomplete = [
            (item["id"], item["status"])
            for item in parity.get("item", [])
            if item.get("status") != "verified"
        ]
        output = "\n".join(f"{item}: {status}" for item, status in incomplete)
        if incomplete:
            output = "Incomplete Far parity items:\n" + output + "\n"
            log_path.write_text(output, encoding="utf-8")
            return "failed", 1, output
        output = "All Far parity items are verified\n"
        log_path.write_text(output, encoding="utf-8")
        return "passed", 0, output
    if gate.get("builtin") == "operator-evidence":
        evidence = STATE_ROOT / "operator" / current_platform / "evidence.json"
        command = [
            "python3",
            "tools/workflow_evidence.py",
            "validate",
            "--platform",
            current_platform,
            "--evidence",
            str(evidence),
        ]
    elif gate.get("builtin") == "release-package":
        architecture = platform.machine().lower()
        archive = f"near-{current_platform}-{architecture}.tar.gz"
        command = [
            "python3",
            "tools/package_release.py",
            "create",
            "--skip-build",
            "--output",
            "dist",
            "--platform-id",
            f"{current_platform}-{architecture}",
            "--archive-name",
            archive,
            "--source-revision",
            source_revision,
        ]
    elif gate.get("builtin") == "tui-proof":
        proof = os.environ.get("NEAR_TUI_PROOF_DIR")
        if not proof:
            output = "NEAR_TUI_PROOF_DIR is required for production qualification\n"
            log_path.write_text(output, encoding="utf-8")
            return "failed", 2, output
        command = [
            "python3",
            "tools/validate_tui_proof.py",
            "--repo",
            proof,
            "--revision",
            source_revision,
        ]
    else:
        command = gate.get("command")
    if not command:
        output = "gate has neither command nor supported builtin\n"
        log_path.write_text(output, encoding="utf-8")
        return "failed", 2, output
    command = [
        str(part).replace("{revision}", source_revision).replace("{platform}", current_platform)
        for part in command
    ]
    attempts = max(1, int(gate.get("transient_attempts", 3)))
    outputs = []
    for attempt in range(1, attempts + 1):
        try:
            completed = subprocess.run(
                command,
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                timeout=int(gate.get("timeout_seconds", 600)),
                check=False,
            )
            output = completed.stdout
            status = "passed" if completed.returncode == 0 else "failed"
            exit_code = completed.returncode
        except subprocess.TimeoutExpired as error:
            partial = error.stdout.decode() if isinstance(error.stdout, bytes) else error.stdout or ""
            output = partial + f"\nTimed out after {gate.get('timeout_seconds')} seconds\n"
            status, exit_code = "failed", 124
        except OSError as error:
            output = f"Unable to execute gate: {error}\n"
            status, exit_code = "failed", 127
        outputs.append(f"--- attempt {attempt}/{attempts} ---\n{output}")
        transient = exit_code != 0 and any(marker in output.lower() for marker in TRANSIENT_FAILURES)
        if not transient or attempt == attempts:
            break
        time.sleep(min(2**attempt, 10))
    output = "\n".join(outputs)
    log_path.write_text(output, encoding="utf-8")
    print(f"[{status}] {gate['id']} ({time.monotonic() - started:.1f}s)")
    return status, exit_code, output


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("profile", choices=("developer", "wave", "production"))
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--no-resume", action="store_true")
    args = parser.parse_args()

    manifest_path = args.manifest.resolve()
    with manifest_path.open("rb") as source:
        manifest = tomllib.load(source)
    if manifest.get("schema_version") != 1:
        raise SystemExit("unsupported qualification manifest schema")

    STATE_ROOT.mkdir(parents=True, exist_ok=True)
    logs = STATE_ROOT / "logs"
    logs.mkdir(exist_ok=True)
    current_platform = platform_id()
    dirty = bool(command_output(["git", "status", "--porcelain"]))
    context = {
        "revision": command_output(["git", "rev-parse", "HEAD"]),
        "dirty": dirty,
        "toolchain": command_output(["rustc", "--version", "--verbose"]),
        "platform": {
            "id": current_platform,
            "system": platform.system(),
            "release": platform.release(),
            "architecture": platform.machine(),
            "capabilities": {
                "disposable_mount_fixtures": current_platform in {"macos", "linux"},
                "operator_terminal_matrix": current_platform in {"macos", "linux"},
            },
        },
        "lockfile_digest": digest(ROOT / "Cargo.lock"),
        "manifest_digest": digest(manifest_path),
        "source_digest": source_digest(),
    }
    previous = {} if args.no_resume else load_json(STATE_PATH, {})
    state: dict[str, Any] = {"gates": {}}
    results: list[dict[str, Any]] = []
    statuses: dict[str, str] = {}
    all_hashes: dict[str, str] = {}
    test_counts = {"passed": 0, "failed": 0, "ignored": 0}
    degradations: list[dict[str, str]] = []

    for gate in manifest.get("gates", []):
        if args.profile not in gate.get("profiles", []):
            continue
        gate_id = gate["id"]
        fingerprint = gate_fingerprint(gate, context)
        blocked_by = [item for item in gate.get("dependencies", []) if statuses.get(item) != "passed"]
        record: dict[str, Any] = {
            "id": gate_id,
            "severity": gate.get("severity", "high"),
            "requirement": gate.get("requirement"),
            "fingerprint": fingerprint,
            "evidence": str((logs / f"{gate_id}.log").relative_to(ROOT)),
        }
        if current_platform not in gate.get("platforms", []):
            record.update(status="unsupported", reason=f"unsupported on {current_platform}")
            degradations.append({"gate": gate_id, "reason": record["reason"]})
        elif blocked_by:
            record.update(status="blocked", reason=f"failed dependencies: {', '.join(blocked_by)}")
        else:
            cached = previous.get("gates", {}).get(gate_id, {})
            resumable = gate.get("deterministic", False) and cached.get("fingerprint") == fingerprint
            if resumable and cached.get("status") == "passed":
                record.update(status="passed", resumed=True, exit_code=0)
                output = (logs / f"{gate_id}.log").read_text(encoding="utf-8")
                print(f"[resumed] {gate_id}")
            else:
                status, exit_code, output = run_gate(
                    gate,
                    logs / f"{gate_id}.log",
                    dirty,
                    current_platform,
                    context["revision"],
                )
                record.update(status=status, resumed=False, exit_code=exit_code)
            pattern = r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored"
            for passed, failed, ignored in re.findall(pattern, output):
                test_counts["passed"] += int(passed)
                test_counts["failed"] += int(failed)
                test_counts["ignored"] += int(ignored)
        hashes, missing = artifact_hashes(gate.get("artifacts", []))
        all_hashes.update(hashes)
        if missing and record["status"] == "passed":
            record.update(status="failed", reason=f"missing artifacts: {', '.join(missing)}")
        statuses[gate_id] = record["status"]
        results.append(record)
        state["gates"][gate_id] = {"fingerprint": fingerprint, "status": record["status"]}
        STATE_PATH.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    mandatory_failure = has_mandatory_failure(results)
    far_path = ROOT / "project" / "far-parity.toml"
    parity_states = re.findall(
        r'^status\s*=\s*"([^"]+)"', far_path.read_text(encoding="utf-8"), re.MULTILINE
    )
    release_hashes = {
        str(path.relative_to(ROOT)): digest(path)
        for path in sorted((ROOT / "dist").glob("**/*"))
        if path.is_file()
    } if (ROOT / "dist").is_dir() else {}
    all_hashes.update(release_hashes)
    result_document = {
        "schema_version": 1,
        "profile": args.profile,
        **context,
        "gates": results,
        "test_counts": test_counts,
        "performance_results": {},
        "known_degradations": degradations,
        "artifact_hashes": all_hashes,
        "far_parity": {
            "digest": digest(far_path),
            "status_counts": {state: parity_states.count(state) for state in sorted(set(parity_states))},
        },
        "final_status": "failed" if mandatory_failure else "passed",
    }
    RESULT_PATH.write_text(json.dumps(result_document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"qualification: {result_document['final_status']} ({RESULT_PATH.relative_to(ROOT)})")
    return 1 if mandatory_failure else 0


if __name__ == "__main__":
    sys.exit(main())
