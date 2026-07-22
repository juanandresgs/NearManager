#!/usr/bin/env python3
"""Check Near's published application boundary for backend and domain leaks."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN_PUBLIC_TOKENS = ("ratatui", "crossterm", "tokio::", "PathBuf", "TaskHandle")
FORBIDDEN_APP_DEPENDENCIES = {"near-fm", "ratatui", "crossterm", "tokio"}
PUBLISHED_CRATES = (
    "crates/near-core",
    "crates/near-runtime",
    "crates/near-terminal",
    "crates/near-ui",
    "crates/near-app",
    "crates/near-search",
    "crates/near-handlers",
    "crates/near-config",
    "crates/near-macros",
    "crates/near-pty",
    "crates/near-plugins",
    "crates/near-reference-providers",
    "crates/near-archive",
    "crates/near-sftp",
    "crates/near-testkit",
)
REFERENCE_APPS = ("apps/near-demo", "apps/near-view", "apps/near-proc")


def manifest(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def main() -> int:
    errors: list[str] = []
    facade = (ROOT / "crates/near-app/src/lib.rs").read_text()
    for token in FORBIDDEN_PUBLIC_TOKENS:
        if token in facade:
            errors.append(f"near-app public facade exposes forbidden token {token!r}")

    provider = (ROOT / "crates/near-core/src/provider.rs").read_text()
    for token in ("PathBuf", "std::path", "tokio"):
        if token in provider:
            errors.append(f"provider contract exposes forbidden token {token!r}")
    if "Location" not in provider or "ResourceRef" not in provider:
        errors.append("provider contract must use Location and ResourceRef")

    application = (ROOT / "crates/near-ui/src/application.rs").read_text()
    if "pub fn new(" not in application or "surface: impl Surface" not in application:
        errors.append("single-surface application constructor is missing")

    for relative in REFERENCE_APPS:
        data = manifest(ROOT / relative / "Cargo.toml")
        dependencies = set(data.get("dependencies", {}))
        forbidden = sorted(dependencies & FORBIDDEN_APP_DEPENDENCIES)
        if forbidden:
            errors.append(f"{relative} directly depends on forbidden crates: {', '.join(forbidden)}")
        if "near-app" not in dependencies:
            errors.append(f"{relative} must consume the near-app facade")

    workspace = manifest(ROOT / "Cargo.toml")
    rust_version = workspace.get("workspace", {}).get("package", {}).get("rust-version")
    if not rust_version:
        errors.append("workspace.package.rust-version must be declared")
    for relative in PUBLISHED_CRATES:
        data = manifest(ROOT / relative / "Cargo.toml")
        if data.get("package", {}).get("rust-version") != {"workspace": True}:
            errors.append(f"{relative} must inherit workspace rust-version")

    if errors:
        print("Near public API audit: FAIL")
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("Near public API audit: PASS")
    print(f"  published crates: {len(PUBLISHED_CRATES)}")
    print(f"  reference apps: {len(REFERENCE_APPS)}")
    print(f"  MSRV: {rust_version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
