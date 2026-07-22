#!/usr/bin/env python3
"""Validate Near's deterministic SPDX dependency graph contract."""

from __future__ import annotations

import importlib.util
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("near_generate_sbom", ROOT / "tools/generate_sbom.py")
assert SPEC is not None and SPEC.loader is not None
generate_sbom = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generate_sbom)


class SbomTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
        cls.document = generate_sbom.generate(cls.revision)

    def test_document_is_deterministic_and_describes_workspace(self) -> None:
        self.assertEqual(generate_sbom.generate(self.revision), self.document)
        self.assertEqual(self.document["spdxVersion"], "SPDX-2.3")
        self.assertTrue(self.document["documentNamespace"].startswith("urn:uuid:"))
        self.assertNotEqual(self.document["creationInfo"]["created"], "1970-01-01T00:00:00Z")
        self.assertGreater(len(self.document["documentDescribes"]), 0)
        self.assertGreater(len(self.document["relationships"]), len(self.document["packages"]))

    def test_relationships_reference_declared_spdx_elements(self) -> None:
        identifiers = {"SPDXRef-DOCUMENT"}
        identifiers.update(package["SPDXID"] for package in self.document["packages"])
        self.assertEqual(len(identifiers), len(self.document["packages"]) + 1)
        for relationship in self.document["relationships"]:
            self.assertIn(relationship["spdxElementId"], identifiers)
            self.assertIn(relationship["relatedSpdxElement"], identifiers)

    def test_revision_mismatch_fails_closed(self) -> None:
        with self.assertRaisesRegex(SystemExit, "does not match checkout HEAD"):
            generate_sbom.generate("0" * 40)


if __name__ == "__main__":
    unittest.main()
