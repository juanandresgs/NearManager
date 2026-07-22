#!/usr/bin/env python3
"""Exercise WF-TRASH-002 against a disposable image mounted under /Volumes."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from qualify import source_digest

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs)


def git_revision() -> str:
    return run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.decode().strip()


def reusable_evidence(path: Path, revision: str, digest: str) -> bool:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    return (
        document.get("scenario") == "WF-TRASH-002"
        and document.get("platform") == "macos"
        and document.get("status") == "passed"
        and document.get("revision") == revision
        and document.get("source_digest") == digest
        and bool(document.get("sentinel_sha256"))
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default=".near/qualification/macos-mount-safety.json")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this fixture requires macOS and hdiutil")

    output = Path(args.output)
    if not output.is_absolute():
        output = ROOT / output
    revision = git_revision()
    digest = source_digest()
    root = Path(tempfile.mkdtemp(prefix="near-mount-safety-"))
    image = root / "NearQualification.dmg"
    device = None
    mount = None
    test_output = b""
    try:
        try:
            run([
                "hdiutil", "create", "-size", "32m", "-fs", "HFS+",
                "-volname", "NearQualification", str(image),
            ])
        except subprocess.CalledProcessError as error:
            if reusable_evidence(output, revision, digest):
                print(f"macOS mount safety: PASS (resumed exact evidence: {output})")
                return 0
            detail = error.stdout.decode(errors="replace").strip()
            raise RuntimeError(
                "hdiutil could not create the disposable qualification image; "
                "the macOS mount capability is unavailable in this execution context"
                + (f": {detail}" if detail else "")
            ) from error
        attached = run(["hdiutil", "attach", "-nobrowse", "-plist", str(image)])
        payload = plistlib.loads(attached.stdout)
        for entity in payload.get("system-entities", []):
            if entity.get("mount-point"):
                mount = Path(entity["mount-point"])
                device = entity.get("dev-entry")
                break
        if mount is None or device is None or not str(mount).startswith("/Volumes/"):
            raise RuntimeError("hdiutil did not mount the fixture under /Volumes")
        sentinel = mount / "near-mount-sentinel.txt"
        sentinel.write_text("must survive rejected Trash/delete/wipe\n", encoding="utf-8")
        environment = os.environ.copy()
        environment["NEAR_TEST_MOUNT_ROOT"] = str(mount)
        build = run([
            "cargo", "build", "-p", "near-fm", "--locked",
        ], cwd=ROOT)
        native_environment = environment.copy()
        native_environment["NEAR_NATIVE_TRASH_HELPER"] = str(ROOT / "target" / "debug" / "near-fm")
        native_trash_test = run([
            "cargo", "test", "-p", "near-local-fs", "--locked",
            "macos_native_trash_helper_preserves_colliding_items", "--", "--ignored", "--nocapture",
        ], cwd=ROOT, env=native_environment)
        provider_test = run([
            "cargo", "test", "-p", "near-local-fs", "--locked",
            "mounted_volume_root_is_rejected_before_recording_a_plan", "--", "--ignored", "--nocapture",
        ], cwd=ROOT, env=environment)
        workspace_test = run([
            "cargo", "test", "-p", "near-ui", "--locked",
            "mounted_volume_delete_workflow_shows_denial_before_operation_planning", "--", "--ignored", "--nocapture",
        ], cwd=ROOT, env=environment)
        test_output = (
            build.stdout
            + b"\n"
            + native_trash_test.stdout
            + b"\n"
            + provider_test.stdout
            + b"\n"
            + workspace_test.stdout
        )
        if not sentinel.is_file():
            raise RuntimeError("mounted-volume sentinel was altered")
        evidence = {
            "schema_version": 1,
            "scenario": "WF-TRASH-002",
            "platform": "macos",
            "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "revision": revision,
            "source_digest": digest,
            "mount_root": str(mount),
            "device": device,
            "sentinel_sha256": hashlib.sha256(sentinel.read_bytes()).hexdigest(),
            "tests": [
                "macos_native_trash_helper_preserves_colliding_items",
                "mounted_volume_root_is_rejected_before_recording_a_plan",
                "mounted_volume_delete_workflow_shows_denial_before_operation_planning",
            ],
            "test_output": test_output.decode(errors="replace"),
            "status": "passed",
        }
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"macOS mount safety: PASS ({output})")
        return 0
    finally:
        if device is not None:
            subprocess.run(["hdiutil", "detach", "-force", device], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
