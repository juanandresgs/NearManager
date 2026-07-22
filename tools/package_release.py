#!/usr/bin/env python3
"""Create and verify deterministic Near binary release archives."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARIES = ("near-fm", "near-view", "near-proc", "near-demo")
DISTRIBUTION_FILES = ("LICENSE-APACHE", "LICENSE-MIT", "README.md")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_tree_sha256() -> str:
    digest = hashlib.sha256()
    listed = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT,
    )
    for encoded in sorted(item for item in listed.split(b"\0") if item):
        relative = os.fsdecode(encoded)
        path = ROOT / relative
        relative_bytes = encoded
        if path.is_symlink():
            content = b"symlink\0" + os.fsencode(os.readlink(path))
        elif path.is_file():
            content = path.read_bytes()
        else:
            content = b"missing\0"
        digest.update(len(relative_bytes).to_bytes(8, "big"))
        digest.update(relative_bytes)
        digest.update(hashlib.sha256(content).digest())
    return digest.hexdigest()


def workspace_version() -> str:
    document = tomllib.loads((ROOT / "Cargo.toml").read_text())
    return document["workspace"]["package"]["version"]


def command_version(command: str) -> str:
    result = subprocess.run(
        [command, "--version"],
        check=True,
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def git_output(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def binary_paths() -> list[tuple[str, Path]]:
    suffix = ".exe" if os.name == "nt" else ""
    paths = [(f"{name}{suffix}", ROOT / "target" / "release" / f"{name}{suffix}") for name in BINARIES]
    missing = [str(path) for _, path in paths if not path.is_file()]
    if missing:
        raise SystemExit("missing release binaries: " + ", ".join(missing))
    return paths


def distribution_paths() -> list[tuple[str, Path]]:
    paths = [(name, ROOT / name) for name in DISTRIBUTION_FILES]
    missing = [str(path) for _, path in paths if not path.is_file()]
    if missing:
        raise SystemExit("missing distribution files: " + ", ".join(missing))
    return paths


def expected_archive_members(path: Path) -> list[str]:
    suffix = ".exe" if path.suffix == ".zip" else ""
    return sorted(
        [f"{name}{suffix}" for name in BINARIES] + list(DISTRIBUTION_FILES)
    )


def create_tar(path: Path, binaries: list[tuple[str, Path]]) -> None:
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(mode="w", fileobj=compressed, format=tarfile.PAX_FORMAT) as archive:
                for name, binary in binaries:
                    info = archive.gettarinfo(str(binary), arcname=name)
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "root"
                    info.mtime = 0
                    executable = name in BINARIES or name.endswith(".exe")
                    info.mode = stat.S_IFREG | (0o755 if executable else 0o644)
                    with binary.open("rb") as handle:
                        archive.addfile(info, handle)


def create_zip(path: Path, binaries: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, binary in binaries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            executable = name in BINARIES or name.endswith(".exe")
            info.external_attr = ((0o755 if executable else 0o644) & 0xFFFF) << 16
            archive.writestr(info, binary.read_bytes())


def archive_members(path: Path) -> list[str]:
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            return sorted(archive.namelist())
    with tarfile.open(path, "r:gz") as archive:
        return sorted(member.name for member in archive.getmembers() if member.isfile())


def write_checksum(path: Path, digest: str) -> Path:
    checksum = path.with_name(f"{path.name}.sha256")
    checksum.write_text(f"{digest}  {path.name}\n")
    return checksum


def smoke_archive(path: Path, version: str) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="near-release-smoke-") as temporary:
        root = Path(temporary)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                archive.extractall(root)
        else:
            with tarfile.open(path, "r:gz") as archive:
                for member in archive.getmembers():
                    relative = Path(member.name)
                    if (
                        not member.isfile()
                        or relative.is_absolute()
                        or ".." in relative.parts
                    ):
                        raise SystemExit(
                            f"unsafe release archive member: {member.name}"
                        )
                    source = archive.extractfile(member)
                    if source is None:
                        raise SystemExit(
                            f"unreadable release archive member: {member.name}"
                        )
                    destination = root / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with source, destination.open("wb") as output:
                        shutil.copyfileobj(source, output)
                    destination.chmod(member.mode & 0o777)
        suffix = ".exe" if path.suffix == ".zip" else ""
        passed = []
        for name in BINARIES:
            binary = root / f"{name}{suffix}"
            for argument, expected in (
                ("--help", f"usage: {name}"),
                ("--version", f"{name} {version}"),
            ):
                result = subprocess.run(
                    [str(binary), argument],
                    check=False,
                    cwd=root,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                output = result.stdout.strip()
                if result.returncode != 0 or expected not in output:
                    raise SystemExit(
                        f"release smoke failed for {name} {argument}: "
                        f"exit={result.returncode} stdout={output!r} stderr={result.stderr.strip()!r}"
                    )
                passed.append(f"{name} {argument}")
        return passed


def package(args: argparse.Namespace) -> None:
    head = git_output("rev-parse", "HEAD")
    if args.source_revision != head:
        raise SystemExit(
            f"source revision {args.source_revision} does not match checkout HEAD {head}"
        )
    dirty = bool(git_output("status", "--porcelain"))
    if dirty and not args.allow_dirty:
        raise SystemExit("release packaging requires a clean source tree")
    if not args.skip_build:
        subprocess.run(
            ["cargo", "build", "--release", "--bins", "--locked"],
            check=True,
            cwd=ROOT,
        )
    version = workspace_version()
    release_notes = ROOT / "docs" / "releases" / f"v{version}.md"
    if not release_notes.is_file():
        raise SystemExit(f"missing release notes: {release_notes}")
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / args.archive_name
    binaries = binary_paths() + distribution_paths()
    if archive.suffix == ".zip":
        create_zip(archive, binaries)
    elif archive.name.endswith(".tar.gz"):
        create_tar(archive, binaries)
    else:
        raise SystemExit("archive name must end in .zip or .tar.gz")
    digest = sha256(archive)
    checksum = write_checksum(archive, digest)
    smoke_tests = smoke_archive(archive, version)
    provenance = output / f"{args.platform_id}.provenance.json"
    provenance.write_text(
        json.dumps(
            {
                "schema": 2,
                "project": "Near",
                "version": version,
                "platform": args.platform_id,
                "source_revision": args.source_revision,
                "source_dirty": dirty,
                "source_tree_sha256": source_tree_sha256(),
                "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
                "archive": archive.name,
                "archive_sha256": digest,
                "archive_members": archive_members(archive),
                "smoke_tests": smoke_tests,
                "rustc": command_version("rustc"),
                "cargo": command_version("cargo"),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    verify_archive(archive, provenance, checksum, allow_dirty=args.allow_dirty)
    print(f"Near release package: {archive}")
    print(f"  sha256: {digest}")


def verify_archive(
    archive: Path, provenance: Path, checksum: Path, *, allow_dirty: bool = False
) -> None:
    document = json.loads(provenance.read_text())
    if document.get("schema") != 2 or document.get("project") != "Near":
        raise SystemExit("unsupported release provenance")
    if document.get("archive") != archive.name:
        raise SystemExit("provenance archive name mismatch")
    if document.get("source_dirty") is not False and not allow_dirty:
        raise SystemExit("release provenance records a dirty source tree")
    actual_digest = sha256(archive)
    expected_line = f"{actual_digest}  {archive.name}"
    if document["archive_sha256"] != actual_digest:
        raise SystemExit("provenance archive checksum mismatch")
    if checksum.read_text().strip() != expected_line:
        raise SystemExit("checksum sidecar mismatch")
    expected_members = expected_archive_members(archive)
    if archive_members(archive) != expected_members:
        raise SystemExit("release archive member mismatch")
    if sorted(document["archive_members"]) != expected_members:
        raise SystemExit("provenance archive member mismatch")
    smoke_tests = smoke_archive(archive, document["version"])
    if document.get("smoke_tests") != smoke_tests:
        raise SystemExit("provenance smoke-test record mismatch")


def verify(args: argparse.Namespace) -> None:
    archive = Path(args.archive).resolve()
    provenance = Path(args.provenance).resolve()
    checksum = Path(args.checksum).resolve()
    verify_archive(archive, provenance, checksum, allow_dirty=args.allow_dirty)
    print(f"Near release package verification: PASS ({archive.name})")


def default_platform() -> str:
    system = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(
        platform.system(), platform.system().lower()
    )
    return f"{system}-{platform.machine().lower()}"


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    subcommands = root.add_subparsers(dest="command", required=True)
    create = subcommands.add_parser("create")
    create.add_argument("--output", default="dist")
    create.add_argument("--platform-id", default=default_platform())
    create.add_argument("--archive-name", required=True)
    create.add_argument("--source-revision", required=True)
    create.add_argument("--skip-build", action="store_true")
    create.add_argument(
        "--allow-dirty",
        action="store_true",
        help="permit development packaging and record source_dirty=true in provenance",
    )
    check = subcommands.add_parser("verify")
    check.add_argument("--archive", required=True)
    check.add_argument("--provenance", required=True)
    check.add_argument("--checksum", required=True)
    check.add_argument(
        "--allow-dirty",
        action="store_true",
        help="accept development provenance that records source_dirty=true",
    )
    return root


def main() -> int:
    args = parser().parse_args()
    if args.command == "create":
        package(args)
    else:
        verify(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
