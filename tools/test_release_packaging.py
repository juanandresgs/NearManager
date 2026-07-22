#!/usr/bin/env python3
"""Regression coverage for deterministic release packaging and provenance."""

from __future__ import annotations

import importlib.util
import json
import stat
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "near_package_release", ROOT / "tools/package_release.py"
)
assert SPEC is not None and SPEC.loader is not None
package_release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package_release)


class ReleasePackagingTests(unittest.TestCase):
    def test_source_digest_ignores_qualification_state(self) -> None:
        before = package_release.source_tree_sha256()
        noise = ROOT / ".near/qualification/release-packaging-test-noise"
        noise.parent.mkdir(parents=True, exist_ok=True)
        try:
            noise.write_text("ignored evidence\n", encoding="utf-8")
            self.assertEqual(package_release.source_tree_sha256(), before)
        finally:
            noise.unlink(missing_ok=True)

    def test_archive_smoke_and_clean_provenance_contract(self) -> None:
        with tempfile.TemporaryDirectory(prefix="near-package-contract-") as temporary:
            root = Path(temporary)
            binaries = []
            version = "9.8.7"
            for name in package_release.BINARIES:
                binary = root / name
                binary.write_text(
                    "#!/bin/sh\n"
                    f"if [ \"$1\" = --help ]; then echo 'usage: {name}'; exit 0; fi\n"
                    f"if [ \"$1\" = --version ]; then echo '{name} {version}'; exit 0; fi\n"
                    "exit 2\n",
                    encoding="utf-8",
                )
                binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
                binaries.append((name, binary))
            binaries.extend(package_release.distribution_paths())
            archive = root / "near-test.tar.gz"
            package_release.create_tar(archive, binaries)
            smoke_tests = package_release.smoke_archive(archive, version)
            checksum = package_release.write_checksum(
                archive, package_release.sha256(archive)
            )
            provenance = root / "test.provenance.json"
            document = {
                "schema": 2,
                "project": "Near",
                "version": version,
                "archive": archive.name,
                "archive_sha256": package_release.sha256(archive),
                "archive_members": package_release.expected_archive_members(archive),
                "source_dirty": False,
                "smoke_tests": smoke_tests,
            }
            provenance.write_text(json.dumps(document), encoding="utf-8")
            package_release.verify_archive(archive, provenance, checksum)
            document["source_dirty"] = True
            provenance.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "dirty source tree"):
                package_release.verify_archive(archive, provenance, checksum)
            package_release.verify_archive(
                archive, provenance, checksum, allow_dirty=True
            )

    def test_smoke_rejects_unsafe_tar_members_on_all_supported_python_versions(self) -> None:
        with tempfile.TemporaryDirectory(prefix="near-package-unsafe-") as temporary:
            archive = Path(temporary) / "unsafe.tar.gz"
            payload = Path(temporary) / "payload"
            payload.write_text("unsafe", encoding="utf-8")
            with tarfile.open(archive, "w:gz") as output:
                output.add(payload, arcname="../escape")
            with self.assertRaisesRegex(SystemExit, "unsafe release archive member"):
                package_release.smoke_archive(archive, "0.0.0")


if __name__ == "__main__":
    unittest.main()
