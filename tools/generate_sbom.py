#!/usr/bin/env python3
"""Generate a deterministic SPDX JSON SBOM from the locked Cargo graph."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import tomllib
import uuid
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def command_output(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_key(package: dict[str, Any]) -> str:
    return f"{package['name']}@{package['version']}:{package.get('source', 'workspace')}"


def spdx_id(key: str) -> str:
    return f"SPDXRef-Package-{hashlib.sha256(key.encode()).hexdigest()[:20]}"


def creation_time(revision: str) -> str:
    value = command_output(["git", "show", "-s", "--format=%cI", revision])
    parsed = dt.datetime.fromisoformat(value).astimezone(dt.timezone.utc)
    return parsed.isoformat(timespec="seconds").replace("+00:00", "Z")


def dependency_parts(value: str) -> tuple[str, str | None, str | None]:
    match = re.fullmatch(r"([^ ]+)(?: ([^ ]+))?(?: \((.+)\))?", value)
    if match is None:
        raise ValueError(f"unsupported Cargo.lock dependency: {value}")
    return match.group(1), match.group(2), match.group(3)


def resolve_dependency(
    value: str, packages: list[dict[str, Any]]
) -> dict[str, Any]:
    name, version, source = dependency_parts(value)
    candidates = [package for package in packages if package["name"] == name]
    if version is not None:
        candidates = [package for package in candidates if package["version"] == version]
    if source is not None:
        candidates = [package for package in candidates if package.get("source") == source]
    if len(candidates) != 1:
        raise ValueError(f"ambiguous Cargo.lock dependency {value!r}: {len(candidates)} matches")
    return candidates[0]


def package_document(
    package: dict[str, Any], workspace: dict[tuple[str, str], dict[str, Any]]
) -> dict[str, Any]:
    source = package.get("source") or ""
    metadata = workspace.get((package["name"], package["version"]), {})
    document: dict[str, Any] = {
        "SPDXID": spdx_id(package_key(package)),
        "name": package["name"],
        "versionInfo": package["version"],
        "downloadLocation": source or "NOASSERTION",
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": metadata.get("license") or "NOASSERTION",
        "filesAnalyzed": False,
        "externalRefs": [
            {
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": f"pkg:cargo/{package['name']}@{package['version']}",
            }
        ],
    }
    if package.get("checksum"):
        document["checksums"] = [
            {"algorithm": "SHA256", "checksumValue": package["checksum"]}
        ]
    if metadata.get("homepage"):
        document["homepage"] = metadata["homepage"]
    return document


def generate(revision: str) -> dict[str, Any]:
    head = command_output(["git", "rev-parse", "HEAD"])
    if revision != head:
        raise SystemExit(f"SBOM revision {revision} does not match checkout HEAD {head}")
    metadata = json.loads(
        command_output(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--locked",
                "--offline",
                "--format-version",
                "1",
            ]
        )
    )
    workspace = {
        (package["name"], package["version"]): package
        for package in metadata["packages"]
        if package["id"] in metadata["workspace_members"]
    }
    with (ROOT / "Cargo.lock").open("rb") as source:
        lockfile = tomllib.load(source)
    locked_packages = lockfile["package"]
    identifiers = {package_key(package): spdx_id(package_key(package)) for package in locked_packages}
    packages = [
        package_document(package, workspace)
        for package in sorted(
            locked_packages,
            key=lambda item: (item["name"], item["version"], item.get("source", "")),
        )
    ]
    workspace_keys = {
        package_key(package)
        for package in locked_packages
        if (package["name"], package["version"]) in workspace and not package.get("source")
    }
    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": identifiers[key],
        }
        for key in sorted(workspace_keys)
    ]
    for package in sorted(locked_packages, key=package_key):
        for dependency in sorted(package.get("dependencies", [])):
            target = resolve_dependency(dependency, locked_packages)
            relationships.append(
                {
                    "spdxElementId": identifiers[package_key(package)],
                    "relationshipType": "DEPENDS_ON",
                    "relatedSpdxElement": identifiers[package_key(target)],
                }
            )
    lock_digest = file_sha256(ROOT / "Cargo.lock")
    namespace = uuid.uuid5(uuid.NAMESPACE_URL, f"Near:{revision}:{lock_digest}")
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"Near-{revision}",
        "documentNamespace": f"urn:uuid:{namespace}",
        "creationInfo": {
            "creators": ["Tool: Near-tools-generate-sbom"],
            "created": creation_time(revision),
        },
        "documentDescribes": sorted(identifiers[key] for key in workspace_keys),
        "packages": packages,
        "relationships": relationships,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "dist" / "near.spdx.json")
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    document = generate(args.revision)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"Near SBOM: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
