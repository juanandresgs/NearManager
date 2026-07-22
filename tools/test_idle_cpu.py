#!/usr/bin/env python3
"""Verify interactive Near binaries remain CPU-idle without terminal input."""

from __future__ import annotations

import argparse
import ctypes
import os
import pty
import resource
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def process_cpu_seconds(pid: int) -> float:
    if sys.platform == "darwin":
        buffer = ctypes.create_string_buffer(256)
        library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        if library.proc_pid_rusage(pid, 2, ctypes.byref(buffer)) != 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))
        user_nanoseconds, system_nanoseconds = struct.unpack_from("QQ", buffer, 16)
        return (user_nanoseconds + system_nanoseconds) / 1_000_000_000
    if sys.platform.startswith("linux"):
        fields = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8").split()
        ticks = int(fields[13]) + int(fields[14])
        return ticks / os.sysconf("SC_CLK_TCK")
    raise RuntimeError(f"process CPU sampling is unsupported on {sys.platform}")


def run_idle(
    command: list[str], cwd: Path, seconds: float, warmup_seconds: float
) -> tuple[float, float, float]:
    master, slave = pty.openpty()
    import fcntl

    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 0, 0))
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
    )
    os.close(slave)
    stop_drain = threading.Event()

    def drain_output() -> None:
        import select

        while not stop_drain.is_set():
            readable, _, _ = select.select([master], [], [], 0.05)
            if not readable:
                continue
            try:
                if not os.read(master, 65_536):
                    return
            except OSError:
                return

    drain = threading.Thread(target=drain_output, daemon=True)
    drain.start()
    try:
        time.sleep(warmup_seconds)
        cpu_before = process_cpu_seconds(process.pid)
        started = time.monotonic()
        time.sleep(seconds)
        cpu_after = process_cpu_seconds(process.pid)
        termination_started = time.monotonic()
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        termination_elapsed = time.monotonic() - termination_started
        elapsed = time.monotonic() - started
        cpu = cpu_after - cpu_before
        return cpu, elapsed, termination_elapsed
    finally:
        stop_drain.set()
        drain.join(timeout=1)
        os.close(master)
        if process.poll() is None:
            process.kill()
            process.wait()


def run_hangup(command: list[str], cwd: Path) -> tuple[float, float]:
    master, slave = pty.openpty()
    import fcntl

    def establish_controlling_terminal() -> None:
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=establish_controlling_terminal,
    )
    os.close(slave)
    time.sleep(0.25)
    os.close(master)
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired as error:
        process.kill()
        process.wait(timeout=5)
        raise RuntimeError(f"{Path(command[0]).name} survived terminal hangup") from error
    elapsed = time.monotonic() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return cpu, elapsed


def run_detached_hangup(command: list[str], cwd: Path) -> tuple[float, float]:
    master, slave = pty.openpty()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
    )
    os.close(slave)
    time.sleep(0.25)
    os.close(master)
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired as error:
        process.kill()
        process.wait(timeout=5)
        raise RuntimeError(
            f"{Path(command[0]).name} survived detached terminal closure"
        ) from error
    elapsed = time.monotonic() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return cpu, elapsed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seconds", type=float, default=2.0)
    parser.add_argument("--warmup-seconds", type=float, default=1.0)
    parser.add_argument("--maximum-ratio", type=float, default=0.15)
    args = parser.parse_args()
    binaries = [
        ROOT / "target/debug/near-fm",
        ROOT / "target/debug/near-view",
        ROOT / "target/debug/near-input-probe",
    ]
    subprocess.run(
        [
            "cargo",
            "build",
            "-p",
            "near-fm",
            "-p",
            "near-view",
            "-p",
            "near-input-probe",
            "--locked",
        ],
        cwd=ROOT,
        check=True,
    )
    fixture = Path(tempfile.mkdtemp(prefix="near-idle-cpu-"))
    try:
        document = fixture / "document.txt"
        document.write_text("idle viewer\n", encoding="utf-8")
        commands = {
            "near-fm": [str(binaries[0]), "--portable", str(fixture / "profile")],
            "near-view": [str(binaries[1]), str(document)],
            "near-input-probe": [
                str(binaries[2]),
                str(fixture / "input-probe.json"),
                "idle-cpu-fixture",
            ],
        }
        failures: list[str] = []
        for name, command in commands.items():
            cpu, elapsed, termination_elapsed = run_idle(
                command, fixture, args.seconds, args.warmup_seconds
            )
            ratio = cpu / elapsed
            print(
                f"{name}: cpu={cpu:.4f}s elapsed={elapsed:.4f}s "
                f"ratio={ratio:.3%} terminate={termination_elapsed:.4f}s"
            )
            if ratio > args.maximum_ratio:
                failures.append(
                    f"{name} used {ratio:.1%} of one core while idle; "
                    f"limit is {args.maximum_ratio:.1%}"
                )
            if termination_elapsed > 1.0:
                failures.append(
                    f"{name} took {termination_elapsed:.2f}s to exit after SIGTERM"
                )
            hangup_cpu, hangup_elapsed = run_hangup(command, fixture)
            print(
                f"{name} hangup: cpu={hangup_cpu:.4f}s "
                f"elapsed={hangup_elapsed:.4f}s"
            )
            if hangup_elapsed > 2.0:
                failures.append(
                    f"{name} took {hangup_elapsed:.2f}s to exit after terminal hangup"
                )
            detached_cpu, detached_elapsed = run_detached_hangup(command, fixture)
            print(
                f"{name} detached hangup: cpu={detached_cpu:.4f}s "
                f"elapsed={detached_elapsed:.4f}s"
            )
            if detached_elapsed > 2.0:
                failures.append(
                    f"{name} took {detached_elapsed:.2f}s to exit after detached terminal closure"
                )
        if failures:
            for failure in failures:
                print(f"FAIL: {failure}", file=sys.stderr)
            return 1
        print("idle CPU: PASS")
        return 0
    finally:
        shutil.rmtree(fixture, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
