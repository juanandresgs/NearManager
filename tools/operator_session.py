#!/usr/bin/env python3
"""Prepare and verify reproducible Near operator qualification sessions."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tomllib
import unicodedata
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_MANIFEST = ROOT / "specs" / "operator-workflows.toml"
INTERACTION_MANIFEST = ROOT / "specs" / "interaction-conformance.toml"


def run_version(command: list[str]) -> dict[str, object]:
    executable = shutil.which(command[0])
    if executable is None:
        return {"available": False, "command": command, "output": "not installed"}
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=15,
    )
    return {
        "available": completed.returncode == 0,
        "command": command,
        "exit_code": completed.returncode,
        "output": completed.stdout.strip(),
    }


def git_output(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def current_platform() -> str:
    return {"Darwin": "macos", "Linux": "linux"}.get(platform.system(), platform.system().lower())


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_bytes(path: Path, content: bytes, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    path.chmod(mode)


def add_fixture(
    fixtures: list[dict[str, object]],
    session_root: Path,
    path: Path,
    purpose: str,
    *,
    mutable: bool = False,
) -> None:
    relative = path.relative_to(session_root)
    fixtures.append(
        {
            "path": relative.as_posix(),
            "purpose": purpose,
            "sha256": sha256(path),
            "size": path.stat().st_size,
            "mode": stat.S_IMODE(path.stat().st_mode),
            "name_bytes_hex": os.fsencode(path.name).hex(),
            "mutable": mutable,
        }
    )


def prepare_lookup(
    fixtures_root: Path,
    session_root: Path,
    fixtures: list[dict[str, object]],
) -> dict[str, object]:
    lookup = fixtures_root / "lookup"
    names = [
        ("cargo.toml", b"[package]\nname = \"lookup-cargo\"\n"),
        ("cat.txt", b"cat\n"),
        ("z-no-match.txt", b"no match fixture\n"),
    ]
    for name, content in names:
        path = lookup / name
        write_bytes(path, content)
        add_fixture(fixtures, session_root, path, "Alt lookup and Unicode normalization")
    nfc_name = "caf\N{LATIN SMALL LETTER E WITH ACUTE}.txt"
    nfd_name = unicodedata.normalize("NFD", nfc_name)
    nfc_path = lookup / nfc_name
    write_bytes(nfc_path, "NFC café\n".encode())
    add_fixture(fixtures, session_root, nfc_path, "NFC Alt lookup")
    nfd_path = lookup / nfd_name
    if nfd_path.exists():
        separated = fixtures_root / "lookup-unicode-nfd" / nfd_name
        write_bytes(separated, "NFD café\n".encode())
        add_fixture(fixtures, session_root, separated, "NFD Alt lookup; use provider fixture for co-location")
        return {
            "unicode_normalization_distinct_names": {
                "supported": False,
                "reason": "filesystem aliases NFC and NFD names in one directory",
            }
        }
    write_bytes(nfd_path, "NFD café\n".encode())
    add_fixture(fixtures, session_root, nfd_path, "NFD Alt lookup")
    return {"unicode_normalization_distinct_names": {"supported": True}}


def prepare_viewer_editor(
    fixtures_root: Path,
    session_root: Path,
    fixtures: list[dict[str, object]],
) -> None:
    corpus = fixtures_root / "viewer-editor"
    entries = [
        ("empty.txt", b"", "empty resource", False, 0o644),
        ("unicode.txt", "Near — café — 東京 — 😀\n".encode(), "UTF-8 Unicode", False, 0o644),
        ("utf16le.txt", b"\xff\xfe" + "Near UTF-16LE\r\n".encode("utf-16le"), "UTF-16LE with BOM", False, 0o644),
        ("utf16be.txt", b"\xfe\xff" + "Near UTF-16BE\n".encode("utf-16be"), "UTF-16BE with BOM", False, 0o644),
        ("latin1.txt", "Near café £\r\n".encode("latin-1"), "Latin-1", False, 0o644),
        ("invalid-utf8.bin", b"valid-prefix\xff\xfeinvalid\n", "invalid UTF-8", False, 0o644),
        ("binary.bin", bytes(range(256)) + b"\x00Near\x00", "binary and NUL bytes", False, 0o644),
        ("huge-line.txt", b"x" * (1024 * 1024) + b"\n", "one MiB logical line", False, 0o644),
        ("huge-file.txt", (b"0123456789abcdef" * 4096 + b"\n") * 128, "eight MiB streaming resource", False, 0o644),
        ("mixed-eol.txt", b"lf\ncrlf\r\ncr\rfinal", "mixed line endings", False, 0o644),
        ("tabs.txt", b"one\ttwo\tthree\n\tindented\n", "tab rendering and expansion", False, 0o644),
        ("read-only.txt", b"read only\n", "read-only denial", False, 0o444),
        ("external-change.txt", b"baseline external-change content\n", "external replacement and reload", True, 0o644),
        ("provider-resource.txt", b"mirror this through a non-local test provider\n", "provider-backed viewer/editor", False, 0o644),
    ]
    for name, content, purpose, mutable, mode in entries:
        path = corpus / name
        write_bytes(path, content, mode)
        add_fixture(fixtures, session_root, path, purpose, mutable=mutable)


def prepare_operations(
    fixtures_root: Path,
    session_root: Path,
    fixtures: list[dict[str, object]],
) -> dict[str, object]:
    operations = fixtures_root / "operations"
    ordinary = operations / "ordinary.txt"
    write_bytes(ordinary, b"ordinary trash fixture\n")
    add_fixture(fixtures, session_root, ordinary, "ordinary Trash and restoration")
    nested = operations / "tree" / "nested" / "payload.txt"
    write_bytes(nested, b"recursive payload\n")
    add_fixture(fixtures, session_root, nested, "recursive Trash and cancellation")
    readonly = operations / "read-only.txt"
    write_bytes(readonly, b"read-only operation fixture\n", 0o444)
    add_fixture(fixtures, session_root, readonly, "read-only mutation diagnostics")
    collision_a = operations / "collisions" / "source-a" / "same-name.txt"
    collision_b = operations / "collisions" / "source-b" / "same-name.txt"
    write_bytes(collision_a, b"collision A\n")
    write_bytes(collision_b, b"collision B\n")
    add_fixture(fixtures, session_root, collision_a, "Trash collision source A")
    add_fixture(fixtures, session_root, collision_b, "Trash collision source B")
    symlink = operations / "ordinary-link"
    broken = operations / "broken-link"
    symlink_capability: dict[str, object]
    try:
        symlink.symlink_to(ordinary.name)
        broken.symlink_to("missing-target")
    except OSError as error:
        symlink_capability = {"supported": False, "reason": str(error)}
        marker = operations / "symlink-capability.json"
        write_bytes(marker, json.dumps(symlink_capability, sort_keys=True).encode() + b"\n")
        add_fixture(fixtures, session_root, marker, "explicit symlink capability result")
    else:
        symlink_capability = {"supported": True}
        for path, purpose in ((symlink, "symlink mutation"), (broken, "broken symlink mutation")):
            fixtures.append(
                {
                    "path": path.relative_to(session_root).as_posix(),
                    "purpose": purpose,
                    "symlink_target": os.readlink(path),
                    "name_bytes_hex": os.fsencode(path.name).hex(),
                    "mutable": False,
                }
            )
    exact_name_capability: dict[str, object] = {"supported": False, "reason": "non-POSIX host"}
    if os.name == "posix":
        exact_name = os.fsencode(operations) + b"/exact-name-\xff.bin"
        try:
            descriptor = os.open(exact_name, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
            try:
                os.write(descriptor, b"exact filename bytes\n")
            finally:
                os.close(descriptor)
            exact_path = Path(os.fsdecode(exact_name))
            add_fixture(fixtures, session_root, exact_path, "exact non-UTF-8 filename bytes")
            exact_name_capability = {"supported": True, "name_bytes_hex": b"exact-name-\xff.bin".hex()}
        except OSError as error:
            exact_name_capability = {"supported": False, "reason": str(error)}
            marker = operations / "exact-name-capability.json"
            write_bytes(marker, json.dumps(exact_name_capability, sort_keys=True).encode() + b"\n")
            add_fixture(fixtures, session_root, marker, "explicit exact-filename-byte capability result")
    return {
        "exact_non_utf8_filename": exact_name_capability,
        "symlinks": symlink_capability,
    }


def prepare_shell(fixtures_root: Path, session_root: Path, fixtures: list[dict[str, object]]) -> None:
    shell = fixtures_root / "shell"
    marker = shell / "rc-marker.sh"
    write_bytes(marker, b"export NEAR_OPERATOR_RC_MARKER=loaded\nprintf 'near-operator-rc-loaded\\n'\n")
    add_fixture(fixtures, session_root, marker, "login/interactive startup marker")
    custom = shell / "custom-command.sh"
    write_bytes(
        custom,
        b"#!/bin/sh\nprintf 'near-custom-shell cwd=%s marker=%s\\n' \"$PWD\" \"${NEAR_OPERATOR_RC_MARKER:-missing}\"\nexec \"${SHELL:-/bin/sh}\" -i\n",
        0o755,
    )
    add_fixture(fixtures, session_root, custom, "custom shell program and environment")


def workflow_entries(platform_id: str) -> list[dict[str, str]]:
    with WORKFLOW_MANIFEST.open("rb") as source:
        manifest = tomllib.load(source)
    terminals = manifest["terminals"].get(platform_id, [])
    entries = []
    for scenario in manifest["scenario"]:
        if platform_id not in scenario["platforms"]:
            continue
        selected = terminals if scenario["terminal_matrix"] else ["headless/operator"]
        entries.extend({"scenario": scenario["id"], "terminal": terminal} for terminal in selected)
    return entries


def workflow_checklists(platform_id: str) -> list[dict[str, object]]:
    with WORKFLOW_MANIFEST.open("rb") as source:
        manifest = tomllib.load(source)
    checklists = []
    for scenario in manifest["scenario"]:
        if platform_id not in scenario["platforms"]:
            continue
        steps = scenario.get("steps", [])
        artifacts = scenario.get("artifacts", [])
        if not steps or not artifacts:
            raise SystemExit(f"operator workflow lacks steps or artifacts: {scenario['id']}")
        terminals = (
            manifest["terminals"][platform_id]
            if scenario["terminal_matrix"]
            else ["headless/operator"]
        )
        checklists.append(
            {
                "id": scenario["id"],
                "title": scenario["title"],
                "requirements": scenario["requirements"],
                "parity": scenario["parity"],
                "terminals": terminals,
                "steps": steps,
                "artifacts": artifacts,
            }
        )
    return checklists


def interaction_checklists(platform_id: str) -> list[dict[str, object]]:
    with INTERACTION_MANIFEST.open("rb") as source:
        interactions = tomllib.load(source)
    with WORKFLOW_MANIFEST.open("rb") as source:
        workflows = tomllib.load(source)
    scenarios = {
        scenario["id"]: scenario
        for scenario in workflows["scenario"]
        if platform_id in scenario["platforms"]
    }
    cases_by_suite: dict[str, list[dict[str, object]]] = {}
    for case in interactions.get("case", []):
        cases_by_suite.setdefault(case["suite"], []).append(
            {
                "id": case["id"],
                "title": case["title"],
                "keys": case["keys"],
                "precondition": case["precondition"],
                "expected": case["expected"],
            }
        )
    checklists = []
    for suite in interactions.get("suite", []):
        scenario_id = suite["operator_scenario"]
        if scenario_id not in scenarios:
            continue
        terminals = (
            workflows["terminals"][platform_id]
            if scenarios[scenario_id]["terminal_matrix"]
            else ["headless/operator"]
        )
        checklists.append(
            {
                "suite": suite["id"],
                "title": suite["title"],
                "scenario": scenario_id,
                "terminals": terminals,
                "cases": cases_by_suite.get(suite["id"], []),
            }
        )
    return checklists


def write_operator_checklist(
    path: Path,
    platform_id: str,
    workflows: list[dict[str, object]],
    interactions: list[dict[str, object]],
) -> None:
    lines = [
        "# Near Operator Interaction Checklist",
        "",
        f"Platform: `{platform_id}`",
        "",
        "This file is generated from the versioned workflow and interaction manifests. Complete",
        "each scenario in every named terminal, preserving the listed artifacts and concise notes.",
        "",
        "## Workflow Runbook",
        "",
    ]
    for workflow in workflows:
        requirements = ", ".join(f"`{item}`" for item in workflow["requirements"])
        parity = ", ".join(f"`{item}`" for item in workflow["parity"]) or "none"
        lines.extend(
            [
                f"### {workflow['id']} — {workflow['title']}",
                "",
                "Terminals: " + ", ".join(f"`{terminal}`" for terminal in workflow["terminals"]),
                "",
                f"Requirements: {requirements}",
                "",
                f"Parity: {parity}",
                "",
                "Steps:",
                "",
            ]
        )
        lines.extend(f"{index}. {step}" for index, step in enumerate(workflow["steps"], 1))
        lines.extend(["", "Required artifacts:", ""])
        lines.extend(f"- {artifact}" for artifact in workflow["artifacts"])
        lines.extend(["", "Record command template:", "", "```sh"])
        lines.extend(
            [
                "python3 tools/workflow_evidence.py record \\",
                f"  --evidence .near/qualification/operator/{platform_id}/evidence.json \\",
                f"  --scenario {workflow['id']} \\",
                "  --terminal '<terminal>' \\",
                "  --status passed \\",
                "  --notes '<what was observed>' \\",
                "  --artifact '<artifact-path>'",
                "```",
                "",
            ]
        )

    lines.extend(["## Interaction Conformance Details", ""])
    for checklist in interactions:
        lines.extend(
            [
                f"## {checklist['suite']} — {checklist['title']}",
                "",
                f"Scenario: `{checklist['scenario']}`",
                "",
                "Terminals: " + ", ".join(f"`{terminal}`" for terminal in checklist["terminals"]),
                "",
            ]
        )
        for case in checklist["cases"]:
            keys = ", ".join(f"`{key}`" for key in case["keys"]) or "mouse or resize input"
            lines.extend(
                [
                    f"### {case['id']} — {case['title']}",
                    "",
                    f"- Input: {keys}",
                    f"- Precondition: {case['precondition']}",
                ]
            )
            lines.extend(f"- Expected: {expected}" for expected in case["expected"])
            lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def terminal_inventory(platform_id: str) -> dict[str, dict[str, object]]:
    if platform_id == "macos":
        return {
            "Terminal.app": run_version([
                "/usr/libexec/PlistBuddy", "-c", "Print:CFBundleShortVersionString",
                "/System/Applications/Utilities/Terminal.app/Contents/Info.plist",
            ]),
            "iTerm2": run_version([
                "/usr/libexec/PlistBuddy", "-c", "Print:CFBundleShortVersionString",
                "/Applications/iTerm.app/Contents/Info.plist",
            ]),
            "Ghostty": run_version(["/Applications/Ghostty.app/Contents/MacOS/ghostty", "+version"]),
            "tmux": run_version(["tmux", "-V"]),
        }
    if platform_id == "linux":
        return {
            "GNOME Terminal": run_version(["gnome-terminal", "--version"]),
            "Konsole": run_version(["konsole", "--version"]),
            "tmux": run_version(["tmux", "-V"]),
        }
    return {
        "Windows Terminal": run_version(["wt.exe", "--version"]),
        "PowerShell": run_version(["pwsh.exe", "--version"]),
        "Windows PowerShell": run_version(["powershell.exe", "-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]),
        "Command Prompt": run_version(["cmd.exe", "/d", "/c", "ver"]),
    }


def reset_session_root(session_root: Path) -> None:
    forbidden = {Path("/").resolve(), ROOT.resolve(), Path.home().resolve()}
    if session_root in forbidden:
        raise SystemExit(f"refusing to replace unsafe operator session root: {session_root}")
    session_root.mkdir(parents=True, exist_ok=True)
    for child in session_root.iterdir():
        if child.is_symlink() or child.is_file():
            child.unlink()
        else:
            shutil.rmtree(child)


def prepare(output: Path, platform_id: str) -> Path:
    if platform_id not in {"macos", "linux", "windows"}:
        raise SystemExit(f"unsupported operator platform: {platform_id}")
    session_root = output.resolve()
    reset_session_root(session_root)
    fixtures_root = session_root / "fixtures"
    fixtures: list[dict[str, object]] = []
    filesystem_capabilities = prepare_lookup(fixtures_root, session_root, fixtures)
    prepare_viewer_editor(fixtures_root, session_root, fixtures)
    filesystem_capabilities.update(prepare_operations(fixtures_root, session_root, fixtures))
    prepare_shell(fixtures_root, session_root, fixtures)
    revision = git_output("rev-parse", "HEAD")
    workflows = workflow_checklists(platform_id)
    checklists = interaction_checklists(platform_id)
    checklist_path = session_root / "operator-checklist.md"
    write_operator_checklist(checklist_path, platform_id, workflows, checklists)
    document = {
        "schema_version": 1,
        "kind": "operator-session-pack",
        "operator_observation": False,
        "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "revision": revision,
        "dirty": bool(git_output("status", "--porcelain")),
        "platform": platform_id,
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
        },
        "toolchain": {
            "python": sys.version,
            "rustc": run_version(["rustc", "--version", "--verbose"]),
            "cargo": run_version(["cargo", "--version", "--verbose"]),
        },
        "terminals": terminal_inventory(platform_id),
        "environment": {
            "TERM": os.environ.get("TERM"),
            "COLORTERM": os.environ.get("COLORTERM"),
            "SHELL": os.environ.get("SHELL"),
            "TMUX": os.environ.get("TMUX"),
            "LANG": os.environ.get("LANG"),
            "LC_ALL": os.environ.get("LC_ALL"),
        },
        "filesystem_capabilities": filesystem_capabilities,
        "fixtures": sorted(fixtures, key=lambda fixture: str(fixture["path"])),
        "expected_evidence": workflow_entries(platform_id),
        "workflow_checklists": workflows,
        "interaction_checklists": checklists,
        "operator_checklist": {
            "path": checklist_path.relative_to(session_root).as_posix(),
            "sha256": sha256(checklist_path),
        },
    }
    manifest_path = session_root / "operator-session.json"
    manifest_path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return manifest_path


def verify(manifest_path: Path) -> list[str]:
    document = json.loads(manifest_path.read_text(encoding="utf-8"))
    errors = []
    if document.get("schema_version") != 1 or document.get("kind") != "operator-session-pack":
        errors.append("unsupported operator session schema")
    if document.get("revision") != git_output("rev-parse", "HEAD"):
        errors.append("operator session revision does not match the checkout")
    session_root = manifest_path.parent
    checklist = document.get("operator_checklist", {})
    checklist_path = session_root / checklist.get("path", "")
    if not checklist_path.is_file():
        errors.append(f"missing operator checklist: {checklist_path}")
    elif sha256(checklist_path) != checklist.get("sha256"):
        errors.append(f"operator checklist digest mismatch: {checklist_path}")
    for fixture in document.get("fixtures", []):
        path = session_root / fixture["path"]
        if "symlink_target" in fixture:
            if not path.is_symlink():
                errors.append(f"missing symlink fixture: {path}")
            elif os.readlink(path) != fixture["symlink_target"]:
                errors.append(f"symlink target mismatch: {path}")
            continue
        if not path.is_file():
            errors.append(f"missing fixture: {path}")
            continue
        if not fixture.get("mutable") and sha256(path) != fixture.get("sha256"):
            errors.append(f"fixture digest mismatch: {path}")
        if os.fsencode(path.name).hex() != fixture.get("name_bytes_hex"):
            errors.append(f"fixture filename bytes mismatch: {path}")
    return errors


def evidence_status(evidence_path: Path, show_all: bool = False) -> int:
    document = json.loads(evidence_path.read_text(encoding="utf-8"))
    pending = [entry for entry in document.get("entries", []) if entry.get("status") != "passed"]
    with WORKFLOW_MANIFEST.open("rb") as source:
        workflow_manifest = tomllib.load(source)
    scenarios = {scenario["id"]: scenario for scenario in workflow_manifest["scenario"]}
    grouped: dict[str, list[str]] = {}
    for entry in pending:
        grouped.setdefault(entry.get("scenario", "unknown"), []).append(
            entry.get("terminal", "unknown")
        )
    print(f"revision: {document.get('revision')}")
    print(f"platform: {document.get('platform')}")
    print(f"passed: {len(document.get('entries', [])) - len(pending)}")
    print(f"pending: {len(pending)}")
    for scenario_id, terminals in grouped.items():
        title = scenarios.get(scenario_id, {}).get("title", scenario_id)
        print(f"  {scenario_id}: {len(terminals)} terminal(s) — {title}")
        if show_all:
            for terminal in terminals:
                print(f"    - {terminal}")
    if pending:
        next_entry = pending[0]
        platform_id = document.get("platform")
        checklist = (
            ROOT
            / ".near"
            / "qualification"
            / "operator"
            / str(platform_id)
            / "session"
            / "operator-checklist.md"
        )
        print("next:")
        print(f"  scenario: {next_entry.get('scenario')}")
        print(f"  terminal: {next_entry.get('terminal')}")
        print(f"  checklist: {checklist}")
        print("  after capture: use the record command template in that scenario section")
    return int(bool(pending))


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--platform", choices=("macos", "linux", "windows"), default=current_platform())
    prepare_parser.add_argument("--output", type=Path)
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("--session", type=Path, required=True)
    status_parser = commands.add_parser("status")
    status_parser.add_argument("--evidence", type=Path, required=True)
    status_parser.add_argument("--all", action="store_true")
    args = parser.parse_args()
    if args.command == "status":
        return evidence_status(args.evidence, args.all)
    if args.command == "verify":
        errors = verify(args.session)
        if errors:
            print("operator session pack: FAIL", file=sys.stderr)
            for error in errors:
                print(f"  - {error}", file=sys.stderr)
            return 1
        print("operator session pack: PASS")
        return 0
    output = args.output or ROOT / ".near" / "qualification" / "operator" / args.platform / "session"
    manifest_path = prepare(output, args.platform)
    errors = verify(manifest_path)
    if errors:
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    try:
        displayed_path = manifest_path.relative_to(ROOT)
    except ValueError:
        displayed_path = manifest_path
    print(f"operator session pack: PASS ({displayed_path})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
