#!/usr/bin/env python3
"""Exercise WF-TRASH-002 against a disposable Linux tmpfs mount."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default=".near/qualification/linux-mount-safety.json")
    args = parser.parse_args()
    if not sys.platform.startswith("linux"):
        parser.error("this fixture requires Linux")
    root = Path(tempfile.mkdtemp(prefix="near-linux-mount-safety-"))
    mount = root / "mounted"
    mount.mkdir()
    mounted = False
    try:
        run(["mount", "-t", "tmpfs", "-o", "size=16m", "near-qualification", str(mount)])
        mounted = True
        sentinel = mount / "near-mount-sentinel.txt"
        sentinel.write_text("must survive rejected Trash/delete/wipe\n", encoding="utf-8")
        environment = os.environ.copy()
        environment["NEAR_TEST_MOUNT_ROOT"] = str(mount)
        provider_test = run([
            "cargo", "test", "-p", "near-local-fs", "--locked",
            "mounted_volume_root_is_rejected_before_recording_a_plan", "--", "--ignored", "--nocapture",
        ], cwd=ROOT, env=environment)
        workspace_test = run([
            "cargo", "test", "-p", "near-ui", "--locked",
            "mounted_volume_delete_workflow_shows_denial_before_operation_planning", "--", "--ignored", "--nocapture",
        ], cwd=ROOT, env=environment)
        if not sentinel.is_file():
            raise RuntimeError("mounted-filesystem sentinel was altered")
        evidence = {
            "schema_version": 1,
            "scenario": "WF-TRASH-002",
            "platform": "linux",
            "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "revision": run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip(),
            "mount_root": str(mount),
            "filesystem": "tmpfs",
            "sentinel_sha256": hashlib.sha256(sentinel.read_bytes()).hexdigest(),
            "tests": [
                "mounted_volume_root_is_rejected_before_recording_a_plan",
                "mounted_volume_delete_workflow_shows_denial_before_operation_planning",
            ],
            "test_output": provider_test.stdout + "\n" + workspace_test.stdout,
            "status": "passed",
        }
        output = ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"Linux mount safety: PASS ({output.relative_to(ROOT)})")
        return 0
    finally:
        if mounted:
            subprocess.run(["umount", str(mount)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
