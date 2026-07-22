#!/usr/bin/env python3
"""Validate Near's release and supply-chain policy files."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/release.yml"


def main() -> int:
    errors: list[str] = []
    workflow = WORKFLOW.read_text()
    required_workflow_fragments = [
        'tags: ["v*"]',
        "fetch-tags: true",
        'git fetch --force origin "refs/tags/$GITHUB_REF_NAME:refs/tags/$GITHUB_REF_NAME"',
        "gpg.ssh.allowedSignersFile",
        "project/security/release-signers",
        "git verify-tag",
        "cargo build --release --bins --locked",
        "tools/package_release.py create",
        '--source-revision "$GITHUB_SHA"',
        "cargo audit --json",
        "cargo license --json",
        "anchore/sbom-action@v0",
        "file: dist/${{ matrix.archive }}",
        "output-file: dist/${{ matrix.artifact }}.spdx.json",
        "actions/attest@v4",
        "subject-path: dist/${{ matrix.archive }}",
        "sbom-path: dist/${{ matrix.artifact }}.spdx.json",
        "sha256sum",
        "tools/package_release.py create",
        "gh release create",
        "docs/releases/${GITHUB_REF_NAME}.md",
    ]
    for fragment in required_workflow_fragments:
        if fragment not in workflow:
            errors.append(f"release workflow is missing {fragment}")

    release_signers = ROOT / "project/security/release-signers"
    if not release_signers.is_file() or "namespaces=\"git\" ssh-" not in release_signers.read_text():
        errors.append("trusted SSH release signers are missing")

    changelog = ROOT / "CHANGELOG.md"
    if not changelog.is_file() or "## Unreleased" not in changelog.read_text():
        errors.append("CHANGELOG.md must contain an Unreleased section")

    release_policy = ROOT / "docs/releases/README.md"
    if not release_policy.is_file():
        errors.append("docs/releases/README.md is missing")

    for license_name in ("LICENSE-APACHE", "LICENSE-MIT"):
        if not (ROOT / license_name).is_file():
            errors.append(f"{license_name} is missing")

    forbidden_public_media = [
        *ROOT.glob("Far*.7z"),
        *(ROOT / "assets/farmanager-ux/screenshots").glob("**/*"),
        *(ROOT / "assets/farmanager-ux/contact-sheets").glob("**/*"),
    ]
    forbidden_public_media = [path for path in forbidden_public_media if path.is_file()]
    if forbidden_public_media:
        rendered = ", ".join(str(path.relative_to(ROOT)) for path in forbidden_public_media[:5])
        errors.append(f"third-party research media must not ship in the public tree: {rendered}")

    workspace = __import__("tomllib").loads((ROOT / "Cargo.toml").read_text())
    version = workspace["workspace"]["package"]["version"]
    release_notes = ROOT / "docs" / "releases" / f"v{version}.md"
    if not release_notes.is_file():
        errors.append(f"release notes for v{version} are missing")

    packager = ROOT / "tools/package_release.py"
    if not packager.is_file() or "source_tree_sha256" not in packager.read_text():
        errors.append("deterministic release packager with source provenance is missing")

    exceptions = ROOT / "docs/security/vulnerability-exceptions.md"
    if not exceptions.is_file() or "Expiry" not in exceptions.read_text():
        errors.append("vulnerability exceptions must document expiry")

    if errors:
        print("Near release validation: FAIL")
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("Near release validation: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
