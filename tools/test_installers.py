#!/usr/bin/env python3
"""Regression coverage for parameter-free Near Manager installers."""

from __future__ import annotations

import hashlib
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import threading
import unittest


ROOT = Path(__file__).resolve().parents[1]
SHELL_INSTALLER = ROOT / "install.sh"
POWERSHELL_INSTALLER = ROOT / "install.ps1"
BINARIES = ("near-fm", "near-view", "near-proc", "near-demo")


class QuietRequestHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass


def run_shell(environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["sh", str(SHELL_INSTALLER)],
        cwd=ROOT,
        env={**os.environ, **environment},
        text=True,
        capture_output=True,
        check=False,
    )


class InstallerTests(unittest.TestCase):
    def test_shell_target_detection_requires_no_arguments(self) -> None:
        cases = {
            ("Darwin", "arm64"): "near-macos-aarch64.tar.gz",
            ("Darwin", "x86_64"): "near-macos-x86_64.tar.gz",
            ("Linux", "x86_64"): "near-linux-x86_64.tar.gz",
        }
        for (operating_system, architecture), archive in cases.items():
            with self.subTest(operating_system=operating_system, architecture=architecture):
                result = run_shell(
                    {
                        "NEAR_INSTALL_OS": operating_system,
                        "NEAR_INSTALL_ARCH": architecture,
                        "NEAR_INSTALL_DRY_RUN": "1",
                    }
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), archive)

    def test_shell_rejects_an_unpublished_target(self) -> None:
        result = run_shell(
            {
                "NEAR_INSTALL_OS": "Linux",
                "NEAR_INSTALL_ARCH": "aarch64",
                "NEAR_INSTALL_DRY_RUN": "1",
            }
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("currently support x86_64", result.stderr)

    def test_shell_downloads_verifies_and_installs_the_complete_archive(self) -> None:
        with tempfile.TemporaryDirectory(prefix="near-installer-test-") as directory:
            temporary = Path(directory)
            release = temporary / "release"
            payload = temporary / "payload"
            for path in (release, payload):
                path.mkdir()

            for binary in BINARIES:
                contents = "#!/bin/sh\nprintf '%s\\n' 'near-fm 0.2.0'\n"
                path = payload / binary
                path.write_text(contents)
                path.chmod(0o755)

            archive = release / "near-linux-x86_64.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                for binary in BINARIES:
                    bundle.add(payload / binary, arcname=binary)
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (release / f"{archive.name}.sha256").write_bytes(
                f"{digest}  {archive.name}\n".encode()
            )

            handler = lambda *args, **kwargs: QuietRequestHandler(  # noqa: E731
                *args, directory=str(release), **kwargs
            )
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                cases = {
                    "zsh": (".zprofile", ".zshrc"),
                    "bash": (".bash_profile", ".bashrc"),
                }
                for shell, profiles in cases.items():
                    with self.subTest(shell=shell):
                        home = temporary / shell
                        home.mkdir()
                        if shell == "bash":
                            (home / ".bash_profile").write_text("# existing login profile\n")
                        destination = home / ".local" / "bin"
                        result = run_shell(
                            {
                                "HOME": str(home),
                                "ZDOTDIR": str(home),
                                "SHELL": f"/bin/{shell}",
                                "PATH": os.environ["PATH"],
                                "NEAR_INSTALL_OS": "Linux",
                                "NEAR_INSTALL_ARCH": "x86_64",
                                "NEAR_INSTALL_BASE_URL": (
                                    f"http://127.0.0.1:{server.server_port}"
                                ),
                                "NEAR_INSTALL_ALLOW_INSECURE": "1",
                            }
                        )
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn("Near Manager is installed", result.stdout)
                        for binary in BINARIES:
                            installed = destination / binary
                            self.assertTrue(installed.is_file())
                            self.assertTrue(os.access(installed, os.X_OK))
                        for profile in profiles:
                            self.assertIn(
                                'export PATH="$HOME/.local/bin:$PATH"',
                                (home / profile).read_text(),
                            )
            finally:
                server.shutdown()
                thread.join()
                server.server_close()

    def test_powershell_installer_has_no_parameters_and_verifies_hashes(self) -> None:
        source = POWERSHELL_INSTALLER.read_text()
        self.assertNotIn("param(", source.lower())
        self.assertIn("RuntimeInformation]::OSArchitecture", source)
        self.assertIn("Get-FileHash", source)
        self.assertIn("SetEnvironmentVariable", source)

        powershell = shutil.which("pwsh") or shutil.which("powershell")
        if os.name == "nt" and powershell:
            result = subprocess.run(
                [powershell, "-NoProfile", "-File", str(POWERSHELL_INSTALLER)],
                env={
                    **os.environ,
                    "NEAR_INSTALL_OS": "windows",
                    "NEAR_INSTALL_ARCH": "x64",
                    "NEAR_INSTALL_DRY_RUN": "1",
                },
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "near-windows-x86_64.zip")


if __name__ == "__main__":
    unittest.main()
