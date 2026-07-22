#!/usr/bin/env python3
"""Exercise Near terminal workflows through a real tmux PTY."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        **kwargs,
    )
    if completed.returncode != 0:
        rendered = " ".join(command[:3])
        raise RuntimeError(
            f"command failed ({completed.returncode}): {rendered}\n{completed.stdout}"
        )
    return completed


def capture(session: str) -> str:
    return run(["tmux", "capture-pane", "-p", "-t", session, "-S", "-200"]).stdout


def capture_escaped(session: str) -> str:
    return run(
        ["tmux", "capture-pane", "-p", "-e", "-t", session, "-S", "-200"]
    ).stdout


def wait_for(session: str, needle: str, timeout: float = 8.0) -> str:
    deadline = time.monotonic() + timeout
    latest = ""
    while time.monotonic() < deadline:
        latest = capture(session)
        if needle in latest:
            return latest
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for {needle!r}\n{latest}")


def wait_for_occurrences(
    session: str, needle: str, count: int, timeout: float = 8.0
) -> str:
    deadline = time.monotonic() + timeout
    latest = ""
    while time.monotonic() < deadline:
        latest = capture(session)
        if latest.count(needle) >= count:
            return latest
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for {count} occurrences of {needle!r}\n{latest}")


def wait_without(session: str, needle: str, timeout: float = 8.0) -> str:
    deadline = time.monotonic() + timeout
    latest = ""
    while time.monotonic() < deadline:
        latest = capture(session)
        if needle not in latest:
            return latest
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for {needle!r} to disappear\n{latest}")


def wait_for_change(session: str, previous: str, timeout: float = 8.0) -> str:
    deadline = time.monotonic() + timeout
    latest = previous
    while time.monotonic() < deadline:
        latest = capture(session)
        if latest != previous:
            return latest
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for rendered output to change\n{latest}")


def wait_for_any(session: str, needles: tuple[str, ...], timeout: float = 8.0) -> tuple[str, str]:
    deadline = time.monotonic() + timeout
    latest = ""
    while time.monotonic() < deadline:
        latest = capture(session)
        for needle in needles:
            if needle in latest:
                return needle, latest
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for one of {needles!r}\n{latest}")


def send(session: str, *keys: str) -> None:
    run(["tmux", "send-keys", "-t", session, *keys])


def send_literal(session: str, text: str) -> None:
    run(["tmux", "send-keys", "-l", "-t", session, text])


def paste(session: str, text: str) -> None:
    buffer_name = "near-qualification-paste"
    run(["tmux", "set-buffer", "-b", buffer_name, text])
    run(["tmux", "paste-buffer", "-p", "-d", "-b", buffer_name, "-t", session])


def return_to_workspace(session: str) -> str:
    for _ in range(8):
        send(session, "Escape")
        time.sleep(0.06)
        current = capture(session)
        if "9Menu 10Quit" in current and "Find:" not in current:
            return current
    return wait_for(session, "9Menu 10Quit")


def open_typed_settings(session: str) -> None:
    return_to_workspace(session)
    send(session, "F9")
    category, _ = wait_for_any(
        session,
        ("Left Menu", "Files Menu", "Commands Menu", "Options Menu", "Right Menu"),
    )
    transitions = {
        "Left Menu": ("Right", "Files Menu"),
        "Files Menu": ("Right", "Commands Menu"),
        "Commands Menu": ("Right", "Options Menu"),
        "Right Menu": ("Left", "Options Menu"),
    }
    while category != "Options Menu":
        key, expected = transitions[category]
        send(session, key)
        wait_for(session, expected)
        category = expected
    send(session, "y")


def open_typed_settings_via_palette(session: str) -> None:
    send(session, "C-p")
    wait_for(session, "Command Palette")
    send_literal(session, "near.settings.show")
    wait_for(session, "Settings")
    send(session, "Enter")
    wait_for(session, "Typed Settings")


def open_main_menu_category(session: str, target: str) -> None:
    send(session, "F9")
    category, _ = wait_for_any(
        session,
        ("Left Menu", "Files Menu", "Commands Menu", "Options Menu", "Right Menu"),
    )
    transitions = {
        "Left Menu": ("Right", "Files Menu"),
        "Files Menu": ("Right", "Commands Menu"),
        "Commands Menu": ("Right", "Options Menu"),
        "Options Menu": ("Right", "Right Menu"),
        "Right Menu": ("Tab", "Left Menu"),
    }
    while category != target:
        key, expected = transitions[category]
        send(session, key)
        wait_for(session, expected)
        category = expected


def open_terminal_tabs_from_commands_menu(session: str) -> None:
    open_main_menu_category(session, "Commands Menu")
    send(session, "t")
    wait_for(session, "Terminal Tabs")


def open_tasks(session: str) -> str:
    send(session, "C-p")
    wait_for(session, "Command Palette")
    send_literal(session, "near.demo.tasks")
    wait_for(session, "Task surface")
    send(session, "Enter")
    return wait_for(session, "Tasks")


def run_palette_command(session: str, command: str, title: str) -> None:
    send(session, "C-p")
    wait_for(session, "Command Palette")
    send_literal(session, command)
    time.sleep(0.2)
    wait_for(session, title)
    send(session, "Enter")
    wait_without(session, "Command Palette")


def close_terminal_via_palette(session: str) -> None:
    run_palette_command(session, "near.terminal.close", "Close user screen")


def open_temporary_panel_from_right(session: str, slot: int) -> None:
    send(session, "F9", "Left", "Left", "End", "Up", "Up", "Enter", "Home")
    if slot:
        send(session, *("Down" for _ in range(slot)))
    send(session, "Enter")


def open_named_resource(session: str, name: str, command: str) -> None:
    send(session, "Home", f"M-{name[0]}")
    send_literal(session, name[1:])
    wait_for(session, "1 of 1")
    send(session, "Enter", command)


def digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def text_documents(root: Path) -> dict[str, dict[str, str]]:
    return {
        str(path.relative_to(root)): {
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "text": path.read_text(encoding="utf-8"),
        }
        for path in sorted(root.rglob("*.toml"))
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/near-fm")
    parser.add_argument("--output", default=".near/qualification/tmux-terminal-workflows.json")
    args = parser.parse_args()
    if shutil.which("tmux") is None:
        parser.error("tmux is required")
    binary = (ROOT / args.binary).resolve()
    if args.binary == "target/debug/near-fm":
        run(["cargo", "build", "-p", "near-fm", "--locked"], cwd=ROOT)
    elif not binary.is_file():
        parser.error(f"binary does not exist: {binary}")
    fixture = Path(tempfile.mkdtemp(prefix="near-tmux-workflow-"))
    tmux_root = Path(tempfile.mkdtemp(prefix="ntmux-", dir="/tmp"))
    tmux_root.chmod(0o700)
    os.environ["TMUX_TMPDIR"] = str(tmux_root)
    tmux_config = tmux_root / "tmux.conf"
    tmux_config.write_text(
        "set -s escape-time 10\nset -g remain-on-exit on\n", encoding="utf-8"
    )
    profile = fixture / "profile"
    (profile / "config").mkdir(parents=True)
    (profile / "state").mkdir(parents=True)
    shell_program = shutil.which("zsh") or shutil.which("bash") or "/bin/sh"
    (profile / "config" / "shell.toml").write_text(
        "\n".join(
            [
                "schema = 1",
                f'program = "{shell_program}"',
                'mode = "clean"',
                'close_policy = "warn"',
                "inherit_environment = true",
                "arguments = []",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (profile / "config" / "handlers.toml").write_text(
        """schema = 1

[[handlers]]
id = "near.test.default-open"
actions = ["open", "view", "edit"]

[handlers.predicate]
schema_version = 1
hidden = "include"
ignore = "none"

[handlers.invocation]
mode = "argv"
program = "/usr/bin/true"
arguments = [{ value = "native-path" }]
current_directory = { value = "native-parent" }
""",
        encoding="utf-8",
    )
    history_lines = ["schema_version = 1", ""]
    for index in range(80):
        history_lines.extend(
            [
                "[[entries]]",
                f'command = "history-entry-{index:03}"',
                "locked = false",
                "use_count = 1",
                "",
            ]
        )
    (profile / "state" / "command-history.toml").write_text(
        "\n".join(history_lines), encoding="utf-8"
    )
    (fixture / "folder-sample").mkdir()
    for name, content in {
        "cargo.toml": "cargo\n",
        "cat.txt": "cat\n",
        "café.txt": "unicode\n",
        "tab-raw.txt": "",
        "tab-spaces.txt": "",
        "zz-last-界界界界界界界界界界界界界界界界界界界界.txt": "wide unicode\n",
    }.items():
        (fixture / name).write_text(content, encoding="utf-8")
    for name in ("nested-a.txt", "nested-b.txt"):
        (fixture / "folder-sample" / name).write_text(name + "\n", encoding="utf-8")
    (fixture / "viewer-empty.txt").write_bytes(b"")
    (fixture / "viewer-unicode.txt").write_text("Near — café — 東京 — 😀\n", encoding="utf-8")
    (fixture / "viewer-utf16le.txt").write_bytes(
        b"\xff\xfe" + "Near UTF-16LE\nSecond UTF-16LE line\n".encode("utf-16le")
    )
    (fixture / "viewer-utf16be.txt").write_bytes(
        b"\xfe\xff" + "Near UTF-16BE\nSecond UTF-16BE line\n".encode("utf-16be")
    )
    (fixture / "viewer-latin1.txt").write_bytes("Near café £\n".encode("latin-1"))
    (fixture / "viewer-invalid-utf8.bin").write_bytes(b"valid-prefix\xff\xfeinvalid\n")
    (fixture / "viewer-binary.bin").write_bytes(bytes(range(256)) + b"\x00Near\x00")
    (fixture / "viewer-huge-line.txt").write_bytes(b"x" * (1024 * 1024) + b"\n")
    (fixture / "viewer-huge-file.txt").write_bytes(
        (b"0123456789abcdef" * 4096 + b"\n") * 32
    )
    (fixture / "viewer-mixed-eol.txt").write_bytes(b"lf\ncrlf\r\ncr\rfinal")
    (fixture / "viewer-tabs.txt").write_bytes(b"one\ttwo\tthree\n\tindented\n")
    (fixture / "viewer-read-only.txt").write_bytes(b"read only\n")
    (fixture / "viewer-read-only.txt").chmod(0o444)
    (fixture / "editor-workflow.txt").write_text("alpha\nbeta\ngamma\n", encoding="utf-8")
    (fixture / "editor-external-change.txt").write_text(
        "baseline external content\n", encoding="utf-8"
    )
    temporary_source_names = (
        "café.txt",
        "cat.txt",
        "folder-sample/nested-a.txt",
        "folder-sample/nested-b.txt",
    )
    temporary_source_before = {
        name: hashlib.sha256((fixture / name).read_bytes()).hexdigest()
        for name in temporary_source_names
    }
    for index in range(64):
        (fixture / f"item-{index:03}.txt").write_text(f"item {index}\n", encoding="utf-8")
    arbitrary_list = fixture / "zzz-lines.temp"
    arbitrary_list.write_text(
        "printf 'temporary arbitrary line'\nsftp://example.invalid/path\n",
        encoding="utf-8",
    )
    menu_list = fixture / "zzz-menu.temp"
    menu_list.write_text(
        "|&Fixture|" + str(fixture) + "\n|-|\n|&Command|printf menu-action\n",
        encoding="utf-8",
    )
    exported_list = fixture / "exported.temp"
    copy_destination = fixture / "operation-destination"
    stale_source = fixture / "stale-source.txt"
    stale_source.write_text("stale source\n", encoding="utf-8")
    stale_list = fixture / "stale.temp"
    stale_list.write_text(str(stale_source) + "\n", encoding="utf-8")
    settings_documents_before = text_documents(profile / "config")
    session = f"near-qualification-{os.getpid()}"
    captures: dict[str, str] = {}
    try:
        command = (
            f"cd {shlex_quote(str(fixture))} && "
            f"{shlex_quote(str(binary))} --portable {shlex_quote(str(profile))}; "
            "printf '\\nNEAR_HOST_RESTORED\\n'; exec /bin/sh"
        )
        run(
            [
                "tmux",
                "-f",
                str(tmux_config),
                "new-session",
                "-d",
                "-s",
                session,
                "-x",
                "160",
                "-y",
                "40",
                command,
            ]
        )
        tmux_escape_time = run(["tmux", "show-options", "-sv", "escape-time"]).stdout.strip()
        if tmux_escape_time != "10":
            raise RuntimeError(f"tmux escape-time is {tmux_escape_time!r}, expected '10'")
        captures["initial"] = wait_for(session, "1Help 2User menu 3View")
        if "9Menu 10Quit" not in captures["initial"]:
            raise RuntimeError("legacy base keybar was not rendered honestly")
        if "folder-sample" not in captures["initial"] or "Folder" not in captures["initial"]:
            raise RuntimeError("Far-style directory name and size presentation was not rendered")
        if "/ folder-sample" in captures["initial"]:
            raise RuntimeError("directory name retained the non-Far slash marker")
        send(session, "Home", *("Down" for _ in range(6)), "F8")
        captures["folder_trash_preview"] = wait_for(session, "Move to Trash")
        if "folder-sample" not in captures["folder_trash_preview"]:
            raise RuntimeError("F8 folder preview omitted its exact source")
        send(session, "Escape")
        wait_without(session, "Move to Trash")
        send_literal(session, "\x1b")
        time.sleep(0.005)
        send_literal(session, "[20~")
        captures["fragmented_legacy_f9"] = wait_for(session, "Left Menu")
        if "[20~" in captures["fragmented_legacy_f9"]:
            raise RuntimeError("fragmented legacy F9 leaked into the command line")
        for capture_name, expected_title in [
            ("menu_files", "Files Menu"),
            ("menu_commands", "Commands Menu"),
            ("menu_options", "Options Menu"),
            ("menu_right", "Right Menu"),
        ]:
            send(session, "Right")
            captures[capture_name] = wait_for(session, expected_title)
        send(session, "Tab")
        captures["menu_tab_to_left"] = wait_for(session, "Left Menu")
        send(session, "S-Tab")
        captures["menu_shift_tab_to_right"] = wait_for(session, "Right Menu")
        send(session, "Tab")
        captures["menu_focus_restored"] = wait_for(session, "Left Menu")
        send(session, "Escape")
        wait_without(session, "Left Menu")
        send(session, "C-p")
        captures["command_palette_open"] = wait_for(session, "Command Palette")
        send_literal(session, "near.settings.show")
        captures["command_palette_filtered"] = wait_for(session, "Settings")
        send(session, "Escape")
        wait_without(session, "Command Palette")

        send(session, "M-F8")
        captures["command_history_home"] = wait_for(session, "history-entry-079")
        send(session, "End")
        captures["command_history_end"] = wait_for(session, "history-entry-000")
        send(session, "PageUp")
        captures["command_history_page_up"] = wait_without(session, "history-entry-000")
        send(session, "PageDown")
        captures["command_history_page_down"] = wait_for(session, "history-entry-000")
        send(session, "Home")
        captures["command_history_return_home"] = wait_for(session, "history-entry-079")
        send(session, "Escape")
        wait_without(session, "Command History")

        send(session, "F1")
        captures["help_home"] = wait_for(session, "Context Help")
        send(session, "End")
        captures["help_end"] = wait_for_change(session, captures["help_home"])
        if "Context Help" not in captures["help_end"]:
            raise RuntimeError("End closed Context Help instead of navigating it")
        send(session, "Home")
        captures["help_return_home"] = wait_for_change(session, captures["help_end"])
        send(session, "PageDown")
        captures["help_page_down"] = wait_for_change(
            session, captures["help_return_home"]
        )
        send(session, "PageUp")
        captures["help_page_up"] = wait_for_change(
            session, captures["help_page_down"]
        )
        send(session, "Escape")
        wait_without(session, "Context Help")

        captures["tasks_open"] = open_tasks(session)
        send(session, "End", "PageUp", "Home", "PageDown")
        captures["tasks_navigation"] = wait_for(session, "Tasks")
        send(session, "Escape")
        wait_without(session, "Tasks")

        send(session, "Home")
        captures["navigation_home"] = wait_for(session, "cargo.toml")
        send(session, "Down", "Down")
        send(session, "Enter")
        captures["enter_default_association"] = wait_for(
            session, "External tool exited with status 0"
        )
        if "offset 0:0" in captures["enter_default_association"]:
            raise RuntimeError("Enter opened the internal viewer instead of the Open association")
        send(session, "End")
        captures["navigation_end"] = wait_for(
            session, "zz-last-界界界界界界界界界界界界界界界界界界界界.txt"
        )
        send(session, "Home", "PageDown")
        time.sleep(0.3)
        captures["navigation_page_down"] = capture(session)
        if captures["navigation_page_down"] == captures["navigation_home"]:
            raise RuntimeError("PageDown did not change the rendered collection viewport")
        send(session, "Home", "Down", "S-Down", "Down", "IC")
        time.sleep(0.3)
        captures["non_contiguous_selection"] = capture(session)
        if captures["non_contiguous_selection"].count("√") < 2:
            raise RuntimeError("Shift movement plus plain navigation did not preserve two selections")
        send(session, "End")
        captures["selection_survives_end"] = wait_for(
            session, "zz-last-界界界界界界界界界界界界界界界界界界界界.txt"
        )
        send(session, "Home")
        captures["selection_survives_home"] = wait_for(session, "cargo.toml")

        send(session, "Tab")
        open_temporary_panel_from_right(session, 2)
        captures["temporary_panel_empty"] = wait_for(
            session, "Temporary panel 2: 0 reference(s)"
        )
        if "temporary://slots/2" not in captures["temporary_panel_empty"]:
            raise RuntimeError("temporary panel slot 2 was not visible on the right side")
        send(session, "Tab", "F5")
        captures["temporary_panel_added"] = wait_for(
            session, "Added 2 reference(s) to temporary panel 2"
        )
        if not (fixture / "café.txt").is_file() or not (fixture / "cat.txt").is_file():
            raise RuntimeError("copy-as-reference modified a source resource")
        captures["temporary_panel_copy_task_history"] = open_tasks(session)
        if "No tasks" not in captures["temporary_panel_copy_task_history"]:
            raise RuntimeError("copy-as-reference unexpectedly created an operation task")
        send(session, "Escape")
        wait_without(session, "Tasks")
        send(session, "Tab")
        time.sleep(0.2)
        captures["temporary_panel_references"] = capture(session)
        if (
            "café.txt" not in captures["temporary_panel_references"]
            or "cat.txt" not in captures["temporary_panel_references"]
        ):
            raise RuntimeError("temporary panel did not render both source references")
        send(session, "F3")
        captures["temporary_panel_source_view"] = wait_for(session, "offset 0:0")
        if "unicode" not in captures["temporary_panel_source_view"]:
            raise RuntimeError("temporary-panel View did not read the original source resource")
        send(session, "Escape")
        wait_for(session, "temporary://slots/2")
        send(session, "F7")
        captures["temporary_panel_removed"] = wait_for(
            session, "Removed 1 reference(s) from temporary panel 2"
        )
        if not (fixture / "café.txt").is_file() or not (fixture / "cat.txt").is_file():
            raise RuntimeError("F7 removed or modified a source resource")
        send(session, "C-PageUp")
        captures["temporary_panel_revealed"] = wait_without(
            session, "temporary://slots/2"
        )
        if fixture.name not in captures["temporary_panel_revealed"]:
            raise RuntimeError("source reveal did not navigate to the fixture directory")
        open_temporary_panel_from_right(session, 7)
        wait_for(session, "Temporary panel 7: 0 reference(s)")
        send(session, "Tab", "F5")
        wait_for(session, "Added 2 reference(s) to temporary panel 7")
        open_named_resource(session, "folder-sample", "Enter")
        wait_for(session, "nested-a.txt")
        send(session, "Down", "F5", "Down", "F5")
        captures["temporary_panel_cross_directory_references"] = wait_for(
            session, "Added 1 reference(s) to temporary panel 7"
        )
        send(session, "BSpace", "Tab")
        time.sleep(0.3)
        captures["temporary_panel_non_contiguous_before_remove"] = capture(session)
        if any(
            name not in captures["temporary_panel_non_contiguous_before_remove"]
            for name in ("café.txt", "cat.txt", "nested-a.txt", "nested-b.txt")
        ):
            raise RuntimeError("temporary panel did not retain four cross-directory references")
        send(session, "Space", "Down", "Down", "Space", "F7")
        captures["temporary_panel_non_contiguous_removed"] = wait_for(
            session, "Removed 2 reference(s) from temporary panel 7"
        )
        if any(not (fixture / name).is_file() for name in temporary_source_names):
            raise RuntimeError("non-contiguous F7 removal changed a source resource")
        captures["temporary_panel_multi_source_task_history"] = open_tasks(session)
        if "No tasks" not in captures["temporary_panel_multi_source_task_history"]:
            raise RuntimeError("cross-directory copy-as-reference created an operation task")
        send(session, "Escape")
        wait_without(session, "Tasks")
        open_temporary_panel_from_right(session, 3)
        captures["temporary_panel_slot_isolation"] = wait_for(
            session, "Temporary panel 3: 0 reference(s)"
        )
        open_temporary_panel_from_right(session, 2)
        captures["temporary_panel_slot_restored"] = wait_for(
            session, "Temporary panel 2: 1 reference(s)"
        )
        send(session, "M-S-F2")
        captures["temporary_panel_export_dialog"] = wait_for(
            session, "Export UTF-8 provider-qualified resource list"
        )
        send_literal(session, str(exported_list))
        send(session, "Enter")
        captures["temporary_panel_exported"] = wait_for(
            session, "Temporary panel 2: exported 1 reference(s)"
        )
        exported_text = exported_list.read_text(encoding="utf-8-sig")
        if not exported_list.read_bytes().startswith(b"\xef\xbb\xbf"):
            raise RuntimeError("temporary-panel export omitted the UTF-8 BOM")
        if "near.local-fs:" not in exported_text or "cat.txt" not in exported_text:
            raise RuntimeError(
                "temporary-panel export did not preserve provider-qualified identity"
            )
        open_temporary_panel_from_right(session, 4)
        send_literal(session, f'tmp:+4+replace"{exported_list}"')
        send(session, "Enter")
        captures["temporary_panel_imported_export"] = wait_for(
            session, "Temporary panel 4: imported 1 reference(s), rejected 0 line(s)"
        )
        if "cat.txt" not in captures["temporary_panel_imported_export"]:
            raise RuntimeError("temporary-panel export/import lost the source resource")
        send(session, "F4")
        captures["temporary_panel_source_edit"] = wait_for(session, "Ln 1, Col 1")
        send_literal(session, "edited-")
        send(session, "F2")
        captures["temporary_panel_source_saved"] = wait_for(session, "Saved •")
        if not (fixture / "cat.txt").read_text(encoding="utf-8").startswith("edited-"):
            raise RuntimeError("temporary-panel Edit did not save through the source provider")
        send(session, "Escape")
        wait_for(session, "temporary://slots/4")
        copy_destination.mkdir()
        send(session, "Tab", "C-r")
        wait_for(session, f"resources from {fixture.resolve().as_uri()}")
        send(session, "M-o")
        wait_for(session, "Find: o_")
        send_literal(session, "peration-")
        wait_for(session, "1 of 1")
        send(session, "Enter", "Enter")
        wait_for(session, f"resources from {copy_destination.resolve().as_uri()}")
        send(session, "Tab", "F5")
        captures["temporary_panel_source_copy_preview"] = wait_for(
            session, "Operation Preview"
        )
        send(session, "Enter")
        for _ in range(80):
            if (copy_destination / "cat.txt").is_file():
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("temporary-panel copy did not reach the peer destination")
        captures["temporary_panel_source_copy_completed"] = capture(session)
        captures["temporary_panel_operation_task_history"] = open_tasks(session)
        if "Completed File operation" not in captures["temporary_panel_operation_task_history"]:
            raise RuntimeError("temporary-panel source copy was absent from operation history")
        send(session, "Escape")
        wait_without(session, "Tasks")
        send(session, "Tab", "BSpace")
        wait_for(session, f"resources from {fixture.resolve().as_uri()}")
        send(session, "Tab")
        wait_for(session, "temporary://slots/4")
        open_temporary_panel_from_right(session, 2)
        send_literal(session, "tmp:+2 +safe")
        send(session, "Enter")
        captures["temporary_panel_safe"] = wait_for(
            session, "Temporary panel 2 opened in safe mode"
        )
        send(session, "F5")
        captures["temporary_panel_safe_copy_denial"] = wait_for(
            session, "safe mode; resource mutation is disabled"
        )
        send(session, "F8")
        captures["temporary_panel_safe_operation_denial"] = wait_for(
            session, "safe mode; resource mutation is disabled"
        )
        send(session, "F7")
        captures["temporary_panel_safe_denial"] = wait_for(
            session, "safe mode; removing references is disabled"
        )
        send(session, "C-PageUp")
        captures["temporary_panel_safe_navigation"] = wait_without(
            session, "temporary://slots/2"
        )
        open_temporary_panel_from_right(session, 2)
        wait_for(session, "Temporary panel 2: 1 reference(s), 0 stale, safe mode")
        send_literal(session, "tmp:+2 -safe")
        send(session, "Enter")
        wait_for(session, "Temporary panel 2 opened")
        send_literal(
            session,
            f'tmp:+5 +any +replace "{arbitrary_list}"',
        )
        send(session, "Enter")
        captures["temporary_panel_any"] = wait_for(
            session, "Temporary panel 5: imported 2 reference(s), rejected 0 line(s)"
        )
        if "printf 'temporary arbitrary line'" not in captures["temporary_panel_any"]:
            raise RuntimeError("tmp:+any did not render arbitrary list lines")
        send(session, "Enter")
        captures["temporary_panel_any_command_line"] = wait_for(
            session, "Copied Temporary Panel text to the command line"
        )
        if "printf 'temporary arbitrary line'" not in captures["temporary_panel_any_command_line"]:
            raise RuntimeError("Enter did not copy arbitrary Temporary Panel text")
        send(session, "C-c")
        time.sleep(0.1)
        send_literal(session, "tmp:+8+replace<pwd")
        send(session, "Enter")
        captures["temporary_panel_command_output"] = wait_for(
            session, "Temporary panel 8: command exited Some(0), added 1, rejected 0"
        )
        if fixture.name not in captures["temporary_panel_command_output"]:
            raise RuntimeError("tmp:<command output did not populate the Temporary Panel")
        captures["temporary_panel_command_task_history"] = open_tasks(session)
        if "Temporary-panel command" not in captures["temporary_panel_command_task_history"]:
            raise RuntimeError(
                "temporary-panel command task was absent from task history\n"
                + captures["temporary_panel_command_task_history"]
            )
        send(session, "Escape")
        wait_without(session, "Tasks")
        send_literal(session, f'tmp:+menu"{menu_list}"')
        send(session, "Enter")
        captures["temporary_panel_list_menu"] = wait_for(session, "[F]ixture")
        if "[C]ommand" not in captures["temporary_panel_list_menu"]:
            raise RuntimeError("tmp:+menu did not render labeled list actions")
        send(session, "End", "Enter")
        captures["temporary_panel_list_menu_action"] = wait_for(
            session, "Copied Temporary Panel menu action to the command line"
        )
        if "printf menu-action" not in captures["temporary_panel_list_menu_action"]:
            raise RuntimeError("Temporary Panel menu action did not reach the command line")
        send(session, "C-c")
        time.sleep(0.1)
        send_literal(session, "tmp:+9+full")
        send(session, "Enter")
        captures["temporary_panel_full"] = wait_for(
            session, "Temporary panel 9 opened in full-screen mode"
        )
        if "Current [Unsorted" in captures["temporary_panel_full"]:
            raise RuntimeError("tmp:+full left the peer panel visible")
        send_literal(session, "tmp:+9-full")
        send(session, "Enter")
        captures["temporary_panel_full_restored"] = wait_for(
            session, "Current [Unsorted"
        )
        send_literal(session, f'tmp:+6+replace"{stale_list}"')
        send(session, "Enter")
        captures["temporary_panel_stale_source_loaded"] = wait_for(
            session, "Temporary panel 6: imported 1 reference(s), rejected 0 line(s)"
        )
        stale_source.unlink()
        if stale_source.exists():
            raise RuntimeError("stale-reference fixture source still exists after unlink")
        send(session, "C-r")
        captures["temporary_panel_refresh_key"] = wait_for(
            session, f"resources from {fixture.resolve().as_uri()}"
        )
        send(session, "C-p")
        wait_for(session, "Command Palette")
        send_literal(session, "near.temp-panel.refresh")
        wait_for(session, "Refresh temporary-panel references")
        send(session, "Enter")
        captures["temporary_panel_stale_reference"] = wait_for(
            session, "Temporary panel 6: refreshed, 1 stale reference(s)"
        )
        if "stale-source.txt" not in captures["temporary_panel_stale_reference"]:
            raise RuntimeError("missing source was not retained as a visible stale reference")
        send_literal(session, "tmp:+2")
        send(session, "Enter")
        wait_for(session, "Temporary panel 2 opened")
        send(session, "Tab")

        send_literal(session, "printf PRESERVED_DOCK_COMMAND")
        send(session, "M-c")
        captures["lookup_over_shell_draft"] = wait_for(session, "Find: c_")
        send(session, "Escape")
        captures["lookup_restored_shell_draft"] = wait_for(
            session, "printf PRESERVED_DOCK_COMMAND"
        )
        send(session, "C-u")
        send_literal(session, "echo WRONG_DOCK_COMMAND")
        send(session, "C-u")
        send_literal(session, "printf NATIVE_DOCK_EDIT_OK")
        send(session, "Enter")
        captures["native_shell_dock_editing"] = wait_for(session, "NATIVE_DOCK_EDIT_OK")
        if "WRONG_DOCK_COMMAND\r\n" in captures["native_shell_dock_editing"]:
            raise RuntimeError("native shell Ctrl+U did not replace the docked command")
        send_literal(
            session,
            "printf '\\033[1;4;31;44mNEAR_STYLE\\033[0m ✓界\\n'",
        )
        send(session, "Enter")
        captures["native_shell_styled_unicode"] = wait_for_occurrences(
            session, "NEAR_STYLE", 2
        )
        escaped_terminal = capture_escaped(session)
        styled_at = escaped_terminal.rfind("NEAR_STYLE")
        if styled_at < 0 or "\x1b[" not in escaped_terminal[max(0, styled_at - 120):styled_at]:
            raise RuntimeError("embedded terminal flattened ANSI cell styling")
        if "✓界" not in captures["native_shell_styled_unicode"]:
            raise RuntimeError("embedded terminal damaged Unicode or wide glyph output")

        send(session, "C-M-n")
        captures["terminal_second_tab"] = wait_for(session, "[2:Shell 2]")
        if "1:Shell 1" not in captures["terminal_second_tab"]:
            raise RuntimeError("terminal tab strip did not retain the first shell")
        send_literal(session, "printf AGENT_TWO_TAB")
        send(session, "Enter")
        captures["terminal_second_tab_output"] = wait_for_occurrences(
            session, "AGENT_TWO_TAB", 2
        )
        send_literal(session, "sleep 30")
        send(session, "Enter")
        wait_for(session, "sleep 30")
        send(session, "F12")
        captures["terminal_numbered_screens"] = wait_for(session, "sleep • running")
        for label in ("[1] Panels", "[2] Terminal: Shell 1", "[3] Terminal: Shell 2"):
            if label not in captures["terminal_numbered_screens"]:
                raise RuntimeError(f"screen switcher omitted numbered entry {label!r}")
        send(session, "2")
        wait_for(session, "NATIVE_DOCK_EDIT_OK")
        send(session, "F12", "3")
        wait_for(session, "AGENT_TWO_TAB")
        send(session, "C-c")
        send_literal(session, "printf SHELL_TWO_RESUMED")
        send(session, "Enter")
        wait_for_occurrences(session, "SHELL_TWO_RESUMED", 2)
        open_terminal_tabs_from_commands_menu(session)
        send(session, "r")
        wait_without(session, "Terminal Tabs")
        captures["terminal_right_pane"] = wait_for(session, "AGENT_TWO_TAB")
        file_workspace_marker = next(
            (
                marker
                for marker in ("Temporary Panel", "cargo.toml", "folder-sample")
                if marker in captures["terminal_right_pane"]
            ),
            None,
        )
        if file_workspace_marker is None:
            raise RuntimeError(
                "right-pane terminal placement did not retain visible file-panel content\n"
                + captures["terminal_right_pane"]
            )
        if "C-A-N New" not in captures["terminal_right_pane"] or "C-A-P Peer" not in captures["terminal_right_pane"]:
            raise RuntimeError("terminal pane did not render its binding-derived keybar")
        send(session, "C-M-PPage")
        captures["terminal_previous_tab"] = wait_for(session, "[1:Shell 1]")
        if "NATIVE_DOCK_EDIT_OK" not in captures["terminal_previous_tab"]:
            raise RuntimeError("switching terminal tabs did not restore first-shell output")
        send(session, "C-M-p")
        send(session, "Tab")
        send_literal(session, "printf TERMINAL_FOCUS_RETURNED")
        send(session, "Enter")
        captures["terminal_peer_focus_return"] = wait_for_occurrences(
            session, "TERMINAL_FOCUS_RETURNED", 2
        )
        send(session, "C-o")
        captures["terminal_zoomed"] = wait_without(session, file_workspace_marker)
        if "TERMINAL_FOCUS_RETURNED" not in captures["terminal_zoomed"]:
            raise RuntimeError("terminal zoom lost the active shell projection")
        if "C-A-N New" not in captures["terminal_zoomed"] or "C-A-P Peer" in captures["terminal_zoomed"]:
            raise RuntimeError("full-screen terminal keybar did not hide unavailable peer focus")
        send(session, "C-o")
        captures["terminal_zoom_restored"] = wait_for(session, file_workspace_marker)
        if "TERMINAL_FOCUS_RETURNED" not in captures["terminal_zoom_restored"]:
            raise RuntimeError("terminal zoom did not restore the exact peer pane")
        run_palette_command(session, "near.terminal.hide", "Hide terminal pane")
        captures["terminal_workspace_hidden"] = wait_for(session, "1Help 2User menu 3View")
        if "[1:Shell 1]" in captures["terminal_workspace_hidden"]:
            raise RuntimeError("hiding the terminal workspace left its tab strip visible")

        send(session, "Tab", "Home")
        send(session, "F3")
        captures["viewer_parent_denial"] = wait_for(session, "parent entry is navigation-only")
        send(session, "Down", "Down", "F3")
        captures["viewer_open"] = wait_for(session, "offset 0:0")
        if "cargo" not in captures["viewer_open"]:
            raise RuntimeError("F3 viewer did not render the current file contents")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        send(session, "F4")
        captures["editor_open"] = wait_for(session, "Ln 1, Col 1")
        if "cargo" not in captures["editor_open"]:
            raise RuntimeError("F4 editor did not render the current file contents")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")

        send(session, "Home")
        send(session, "M-c")
        captures["lookup_start"] = wait_for(session, "Find: c_   1 of 3")
        if "1Help" in captures["lookup_start"] or "file:///" in captures["lookup_start"]:
            raise RuntimeError("filename lookup retained the busy keybar or URI panel footer")
        if "↑↓ next" not in captures["lookup_start"] or "Enter keep" not in captures["lookup_start"]:
            raise RuntimeError("filename lookup did not expose compact contextual hints")
        send(session, "M-c")
        captures["lookup_cycle"] = wait_for(session, "Find: c_   2 of 3")
        send(session, "Down")
        captures["lookup_cycle_down"] = wait_for(session, "Find: c_   3 of 3")
        send(session, "Up")
        captures["lookup_cycle_up"] = wait_for(session, "Find: c_   2 of 3")
        send(session, "M-a")
        captures["lookup_extended"] = wait_for(session, "Find: ca_   1 of 3")
        paste(session, "rg")
        captures["lookup_pasted"] = wait_for(session, "Find: carg_   1 of 1")
        send(session, "BSpace")
        captures["lookup_backspace"] = wait_for(session, "Find: car_   1 of 1")
        send(session, "Enter")
        captures["lookup_accepted"] = wait_without(session, "Find:")
        send(session, "Home", "M-c")
        paste(session, "afé")
        captures["lookup_unicode_paste"] = wait_for(session, "Find: café_   1 of 1")
        send(session, "Escape")
        captures["lookup_unicode_cancelled"] = wait_without(session, "Find:")
        send(session, "M-x")
        send_literal(session, "qq")
        captures["lookup_no_match"] = wait_for(
            session, "Find: xqq_   contains no matches"
        )
        send(session, "Down")
        captures["lookup_no_match_cancelled"] = wait_without(session, "Find:")
        send(session, "Home", "M-s")
        send_literal(session, "ample")
        captures["lookup_contains_fallback"] = wait_for(
            session, "Find: sample_   contains 1 of 1"
        )
        if "folder-sample" not in captures["lookup_contains_fallback"]:
            raise RuntimeError("contains fallback did not focus the internal filename match")
        send(session, "Enter")
        wait_without(session, "Find:")
        send(session, "M-Left")
        time.sleep(0.2)
        captures["lookup_bound_alt"] = capture(session)
        if "Find:" in captures["lookup_bound_alt"]:
            raise RuntimeError(
                "a bound Alt chord incorrectly started filename lookup\n"
                + captures["lookup_bound_alt"]
            )
        open_typed_settings(session)
        captures["typed_settings"] = wait_for(session, "Typed Settings")
        if "Persistent blocks" not in captures["typed_settings"] or "Command history limit" not in captures["typed_settings"]:
            raise RuntimeError("typed settings did not expose editor and history policies")
        for title in ["Show status line", "Tree indentation", "Wrap menu navigation", "Wrap dialog focus", "Fallback command-line completion"]:
            if title not in captures["typed_settings"]:
                raise RuntimeError(f"typed settings did not expose {title}")
        if "Prefer physical keys" in captures["typed_settings"]:
            raise RuntimeError("advanced settings were visible before explicit disclosure")
        if "F6 show advanced" not in captures["typed_settings"]:
            raise RuntimeError("typed settings did not advertise advanced disclosure")
        send(session, "F6")
        captures["advanced_settings_shown"] = wait_for(session, "Prefer physical keys")
        if "F6 hide advanced" not in captures["advanced_settings_shown"]:
            raise RuntimeError("advanced settings disclosure did not update the control hint")
        send(session, "F6")
        wait_without(session, "Prefer physical keys")
        send_literal(session, "physical keys")
        captures["advanced_setting_metadata"] = wait_for(
            session, "type=Boolean platform=All scope=Live"
        )
        if (
            "advanced" not in captures["advanced_setting_metadata"]
            or "type=Boolean platform=All scope=Live"
            not in captures["advanced_setting_metadata"]
        ):
            raise RuntimeError("advanced unavailable key setting did not explain its degradation")
        send(session, "Escape")
        wait_without(session, "Typed Settings")
        open_typed_settings(session)
        send(session, "F4")
        captures["typed_setting_editor"] = wait_for(session, "Edit shell.arguments")
        if "Value" not in captures["typed_setting_editor"]:
            raise RuntimeError("F4 did not open the generic typed setting editor")
        open_typed_settings(session)
        send_literal(session, "wrap text")
        captures["typed_setting_filtered"] = wait_for(session, "Find: wrap text")
        send(session, "Enter")
        captures["typed_setting_persisted"] = wait_for(session, "Applied viewer.wrap=")
        viewer_path = profile / "config" / "viewer.toml"
        return_to_workspace(session)
        open_named_resource(session, "viewer-unicode", "F3")
        captures["typed_setting_new_viewer"] = wait_for(session, "wrap:true")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_typed_settings(session)
        send_literal(session, "wrap text")
        wait_for(session, "Find: wrap text")
        viewer_settings = viewer_path.read_text(encoding="utf-8")
        viewer_path.write_text(viewer_settings.replace("wrap = true", "wrap = false"), encoding="utf-8")
        send(session, "F5")
        captures["typed_setting_reloaded"] = wait_for(session, "Reloaded externally edited settings")
        viewer_path.write_text(viewer_path.read_text(encoding="utf-8").replace("schema = 1", "schema = 99"), encoding="utf-8")
        send(session, "F5")
        captures["typed_setting_reload_rollback"] = wait_for(session, "last-valid values retained")
        viewer_path.write_text(viewer_path.read_text(encoding="utf-8").replace("schema = 99", "schema = 1"), encoding="utf-8")
        panel_modes_path = profile / "config" / "panel-modes.toml"
        panel_modes_path.write_text(
            'schema = 1\n\n[defaults]\nleft = "compact"\nright = "medium"\n', encoding="utf-8"
        )
        send(session, "F5")
        wait_for(session, "Reloaded externally edited settings")
        return_to_workspace(session)
        captures["panel_mode_reloaded"] = wait_for(session, "[Compact]")
        open_typed_settings(session)
        send_literal(session, "wrap text")
        send(session, "DC")
        captures["typed_setting_reset"] = wait_for(session, "Applied viewer.wrap=false")
        return_to_workspace(session)
        open_named_resource(session, "viewer-empty", "F3")
        captures["typed_setting_reset_viewer"] = wait_for(session, "wrap:false")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_typed_settings(session)
        send_literal(session, "show status line")
        send(session, "Enter")
        wait_for(session, "Applied interface.show_status_line=false")
        return_to_workspace(session)
        captures["status_line_hidden"] = capture(session)
        if " | selected:" in captures["status_line_hidden"]:
            raise RuntimeError("status line remained visible after live setting change")
        open_typed_settings(session)
        send_literal(session, "show status line")
        send(session, "Enter")
        wait_for(session, "Applied interface.show_status_line=true")
        return_to_workspace(session)
        captures["status_line_restored"] = wait_for(session, " | selected:")
        open_typed_settings(session)
        send_literal(session, "show function keybar")
        send(session, "Enter")
        wait_for(session, "Applied interface.show_keybar=false")
        send(session, "Escape")
        captures["keybar_hidden"] = wait_without(session, "Typed Settings")
        if "1Help" in captures["keybar_hidden"]:
            raise RuntimeError("function keybar remained visible after live setting change")
        open_typed_settings_via_palette(session)
        send_literal(session, "show function keybar")
        send(session, "Enter")
        wait_for(session, "Applied interface.show_keybar=true")
        return_to_workspace(session)
        captures["keybar_restored"] = wait_for(session, "1Help")
        open_typed_settings(session)
        send_literal(session, "wrap menu navigation")
        send(session, "Enter")
        wait_for(session, "Applied interface.menu_wrap_navigation=true")
        return_to_workspace(session)
        send(session, "F9", "Home", "Up", "Enter")
        captures["menu_wrap_applied"] = wait_for(session, "Panel View Modes")
        return_to_workspace(session)
        open_typed_settings(session)
        send_literal(session, "wrap menu navigation")
        send(session, "Enter")
        wait_for(session, "Applied interface.menu_wrap_navigation=false")
        open_typed_settings(session)
        send_literal(session, "wrap dialog focus")
        send(session, "Enter")
        wait_for(session, "Applied interface.dialog_wrap_focus=false")
        return_to_workspace(session)
        send(session, "Tab", "C-p")
        wait_for(session, "Command Palette")
        send_literal(session, "near.temp-panel.import")
        wait_for(session, "Import temporary-panel list")
        send(session, "Home", "Enter")
        wait_for(session, "Import UTF-8 resource list")
        send(session, "Tab", "Tab", "Tab")
        send_literal(session, "X")
        captures["dialog_focus_clamped"] = wait_for(session, "appendX")
        send(session, "Escape")
        return_to_workspace(session)
        open_typed_settings(session)
        send_literal(session, "wrap dialog focus")
        send(session, "Enter")
        wait_for(session, "Applied interface.dialog_wrap_focus=true")
        return_to_workspace(session)
        send(session, "C-p")
        wait_for(session, "Command Palette")
        send_literal(session, "near.temp-panel.import")
        wait_for(session, "Import temporary-panel list")
        send(session, "Home", "Enter")
        wait_for(session, "Import UTF-8 resource list")
        send(session, "Tab", "Tab", "Tab")
        send_literal(session, "/tmp/wrapped")
        captures["dialog_focus_wrapped"] = wait_for(session, "/tmp/wrapped")
        send(session, "Escape")
        return_to_workspace(session)
        send(session, "Tab")
        open_typed_settings(session)
        send_literal(session, "tree indentation")
        send(session, "F4")
        wait_for(session, "Edit interface.tree_indent_width")
        send(session, "Tab", "BSpace")
        send_literal(session, "4")
        send(session, "Enter")
        captures["tree_indent_persisted"] = wait_for(
            session, "Applied interface.tree_indent_width=4"
        )
        return_to_workspace(session)
        open_main_menu_category(session, "Left Menu")
        send(session, "t")
        captures["tree_indent_rendered"] = wait_for(session, "left panel type: tree")
        send(session, "Escape")
        wait_without(session, "Left Menu")
        open_main_menu_category(session, "Left Menu")
        send(session, "t")
        wait_for(session, "left panel type: file")
        open_typed_settings(session)
        send_literal(session, "tab size")
        send(session, "F4")
        wait_for(session, "Edit editor.tab_size")
        send(session, "Tab", "BSpace")
        send_literal(session, "3")
        send(session, "Enter")
        captures["editor_tab_size_persisted"] = wait_for(
            session, "Applied editor.tab_size=3"
        )
        open_typed_settings(session)
        send_literal(session, "expand tabs")
        send(session, "Enter")
        captures["editor_expand_tabs_persisted"] = wait_for(
            session, "Applied editor.expand_tabs=true"
        )
        return_to_workspace(session)
        send(session, "Home", "M-t")
        send_literal(session, "ab-spaces")
        send(session, "Enter", "F4", "Tab")
        captures["editor_expand_tabs_rendered"] = wait_for(session, "tab:3:spaces")
        send_literal(session, "x")
        send(session, "F2")
        wait_for(session, "Saved •")
        expanded_tab_bytes = (fixture / "tab-spaces.txt").read_bytes()
        if expanded_tab_bytes != b"   x":
            raise RuntimeError(
                f"expanded editor tab policy saved {expanded_tab_bytes!r}, expected b'   x'"
            )
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_typed_settings(session)
        send_literal(session, "expand tabs")
        send(session, "Enter")
        wait_for(session, "Applied editor.expand_tabs=false")
        return_to_workspace(session)
        send(session, "Home", "M-t")
        send_literal(session, "ab-raw")
        send(session, "Enter", "F4", "Tab")
        captures["editor_raw_tab_rendered"] = wait_for(session, "tab:3:literal")
        send_literal(session, "x")
        send(session, "F2")
        wait_for(session, "Saved •")
        raw_tab_bytes = (fixture / "tab-raw.txt").read_bytes()
        if raw_tab_bytes != b"\tx":
            raise RuntimeError(
                f"raw editor tab policy saved {raw_tab_bytes!r}, expected b'\\tx'"
            )
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-utf16le", "F3")
        captures["viewer_utf16le"] = wait_for(session, "encoding:utf-16le")
        if "Near UTF-16LE" not in captures["viewer_utf16le"]:
            raise RuntimeError("automatic UTF-16LE viewer decoding did not render text")
        send(session, "Down")
        wait_for(session, "offset 30:0")
        send(session, "M-1", "F2")
        captures["viewer_utf16le_state"] = wait_for(session, "wrap:true")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-utf16le", "F3")
        captures["viewer_utf16le_reopened"] = wait_for(session, "offset 30:0")
        if "wrap:true" not in captures["viewer_utf16le_reopened"]:
            raise RuntimeError("viewer per-resource mode did not restore on reopen")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-utf16be", "F3")
        captures["viewer_utf16be"] = wait_for(session, "encoding:utf-16be")
        if "Near UTF-16BE" not in captures["viewer_utf16be"]:
            raise RuntimeError("automatic UTF-16BE viewer decoding did not render text")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-latin1", "F3")
        wait_for(session, "encoding:utf-8")
        send(session, "F8")
        captures["viewer_latin1"] = wait_for(session, "encoding:latin-1")
        if "Near café £" not in captures["viewer_latin1"]:
            raise RuntimeError("Latin-1 viewer mode did not render exact text")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-invalid-utf8", "F3")
        captures["viewer_invalid_utf8"] = wait_for(session, "valid-prefix")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-binary", "F3")
        captures["viewer_binary_hex"] = wait_for(session, "hex:true")
        if "00 01 02 03" not in captures["viewer_binary_hex"]:
            raise RuntimeError("binary detection did not render the hexadecimal stream")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "viewer-huge-file", "F3")
        wait_for(session, "2097184")
        send(session, "PageDown")
        captures["viewer_huge_file"] = wait_for(session, "offset ")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        for name, expected in [
            ("viewer-empty", " / 0"),
            ("viewer-unicode", "東京"),
            ("viewer-mixed-eol", "final"),
            ("viewer-tabs", "indented"),
            ("viewer-read-only", "read only"),
            ("viewer-huge-line", "1048577"),
        ]:
            open_named_resource(session, name, "F3")
            captures[name.replace("-", "_")] = wait_for(session, expected)
            send(session, "Escape")
            wait_for(session, "1Help 2User menu 3View")
        send(session, "Home", "M-v")
        send_literal(session, "iewer-huge-file")
        wait_for(session, "1 of 1")
        send(session, "Enter", "C-q", "Down")
        captures["viewer_quick_view_cancelled"] = wait_for(
            session, "viewer-huge-line.txt"
        )
        if "viewer-huge-file.txt" in captures["viewer_quick_view_cancelled"].splitlines()[0]:
            raise RuntimeError("stale huge-file quick view replaced the current resource")
        send(session, "C-q")
        wait_for(session, "1Help 2User menu 3View")
        open_typed_settings(session)
        send_literal(session, "viewer.open_policy")
        send(session, "F4")
        wait_for(session, "Edit viewer.open_policy")
        send(session, "Tab", *("BSpace" for _ in "internal"))
        send_literal(session, "external")
        send(session, "Enter")
        wait_for(session, "Applied viewer.open_policy=external")
        return_to_workspace(session)
        open_named_resource(session, "viewer-unicode", "F3")
        captures["viewer_external_policy"] = wait_for(
            session, "External tool exited with status 0"
        )
        open_typed_settings(session)
        send_literal(session, "viewer.open_policy")
        send(session, "F4")
        wait_for(session, "Edit viewer.open_policy")
        send(session, "Tab", *("BSpace" for _ in "external"))
        send_literal(session, "association")
        send(session, "Enter")
        wait_for(session, "Applied viewer.open_policy=association")
        return_to_workspace(session)
        open_named_resource(session, "viewer-unicode", "F3")
        captures["viewer_association_policy"] = wait_for(session, "File Associations")
        if "View — near.test.default-open" not in captures["viewer_association_policy"]:
            raise RuntimeError("viewer association policy omitted its resolved View handler")
        send(session, "Home", "Enter")
        wait_for(session, "External tool exited with status 0")
        open_typed_settings(session)
        send_literal(session, "viewer.open_policy")
        send(session, "F4")
        wait_for(session, "Edit viewer.open_policy")
        send(session, "Tab", *("BSpace" for _ in "association"))
        send_literal(session, "internal")
        send(session, "Enter")
        wait_for(session, "Applied viewer.open_policy=internal")
        return_to_workspace(session)
        open_named_resource(session, "viewer-read-only", "F4")
        captures["editor_read_only_denial"] = wait_for(session, "is read-only")
        for name, expected in [
            ("viewer-empty", "UTF-8"),
            ("viewer-unicode", "UTF-8"),
            ("viewer-utf16le", "UTF-16LE BOM"),
            ("viewer-utf16be", "UTF-16BE BOM"),
            ("viewer-latin1", "Latin-1"),
            ("viewer-invalid-utf8", "Latin-1"),
            ("viewer-binary", "Latin-1"),
            ("viewer-mixed-eol", "LF"),
            ("viewer-tabs", "UTF-8"),
            ("viewer-huge-line", "UTF-8"),
            ("viewer-huge-file", "UTF-8"),
        ]:
            open_named_resource(session, name, "F4")
            captures[f"editor_{name.replace('-', '_')}"] = wait_for(session, expected)
            send(session, "Escape")
            wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "editor-workflow", "F4")
        wait_for(session, "Ln 1, Col 1")
        send_literal(session, "X")
        send(session, "C-z")
        captures["editor_undo"] = wait_for(session, "Undo")
        if "X▌alpha" in captures["editor_undo"]:
            raise RuntimeError("editor undo left the inserted byte visible")
        send(session, "C-y")
        captures["editor_redo"] = wait_for(session, "Redo")
        if "X" not in captures["editor_redo"] or "alpha" not in captures["editor_redo"]:
            raise RuntimeError("editor redo did not restore the inserted byte")
        send(session, "S-Down")
        captures["editor_stream_block"] = wait_for(session, "stream:")
        send(session, "C-p")
        wait_for(session, "Command Palette")
        send_literal(session, "near.editor.toggle-persistent-blocks")
        wait_for(session, "Toggle persistent editor blocks")
        send(session, "Enter")
        captures["editor_persistent_block"] = wait_for(
            session, "Persistent blocks: true"
        )
        send(session, "C-u", "M-S-Down", "M-S-Right")
        captures["editor_column_block"] = wait_for(session, "column:")
        send(session, "C-u", "F2")
        wait_for(session, "Saved • UTF-8")
        send(session, "Home")
        wait_for(session, "Ln 3, Col 1")
        send(session, "Right", "Right")
        captures["editor_position"] = wait_for(session, "Ln 3, Col 3")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "editor-workflow", "F4")
        captures["editor_position_reopened"] = wait_for(session, "Ln 3, Col 3")
        send(session, "S-F2")
        wait_for(session, "Editor Save As")
        send(session, "Tab", "Tab", *("BSpace" for _ in "UTF-8"))
        send_literal(session, "UTF-16BE")
        send(session, "Tab", "BSpace", "BSpace")
        send_literal(session, "yes")
        send(session, "Tab", "BSpace", "BSpace")
        send_literal(session, "CRLF")
        send(session, "Tab", "BSpace", "BSpace")
        send_literal(session, "yes")
        send(session, "Tab", "Enter")
        captures["editor_save_as"] = wait_for(session, "Saved • UTF-16BE BOM • CRLF")
        editor_saved_bytes = (fixture / "editor-workflow.txt").read_bytes()
        if not editor_saved_bytes.startswith(b"\xfe\xff") or b"\x00\r\x00\n" not in editor_saved_bytes:
            raise RuntimeError("editor Save As did not write UTF-16BE BOM plus CRLF bytes")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        latin1_before = (fixture / "viewer-latin1.txt").read_bytes()
        open_named_resource(session, "viewer-latin1", "F4")
        send(session, "End")
        send_literal(session, "😀")
        send(session, "F2")
        captures["editor_lossy_warning"] = wait_for(session, "Lossy Save Warning")
        if (fixture / "viewer-latin1.txt").read_bytes() != latin1_before:
            raise RuntimeError("lossy editor save mutated the resource before confirmation")
        send(session, "Enter")
        captures["editor_lossy_confirmed"] = wait_for(session, "Saved • Latin-1")
        if b"?" not in (fixture / "viewer-latin1.txt").read_bytes():
            raise RuntimeError("confirmed Latin-1 save omitted replacement bytes")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_named_resource(session, "editor-external-change", "F4")
        wait_for(session, "Ln 1, Col 1")
        send_literal(session, "local-")
        external_change = fixture / "editor-external-change.txt"
        original_stat = external_change.stat()
        external_change.write_text(
            "external version\n", encoding="utf-8"
        )
        os.utime(
            external_change,
            ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns + 2_000_000_000),
        )
        send(session, "F2")
        captures["editor_external_change"] = wait_for(session, "Resource Changed Externally")
        send(session, "Down", "Enter")
        captures["editor_external_compare"] = wait_for(session, "External ↔ Local Comparison")
        if "local-" not in captures["editor_external_compare"] or "external version" not in captures["editor_external_compare"]:
            raise RuntimeError("external comparison omitted local or provider content")
        send(session, "Escape")
        wait_for(session, "editor-external-change.txt")
        send(session, "F2")
        wait_for(session, "Resource Changed Externally")
        send(session, "Home", "Enter")
        captures["editor_external_reload"] = wait_for(session, "Reloaded external version")
        send(session, "Home")
        send_literal(session, "kept-")
        (fixture / "editor-external-change.txt").write_text(
            "second external version\n", encoding="utf-8"
        )
        send(session, "F2")
        wait_for(session, "Resource Changed Externally")
        send(session, "End", "Enter")
        captures["editor_external_keep_local"] = wait_for(session, "Saved • UTF-8")
        if not (fixture / "editor-external-change.txt").read_text(encoding="utf-8").startswith("kept-"):
            raise RuntimeError("keep-local editor choice did not overwrite the external version")
        send(session, "Escape")
        wait_for(session, "1Help 2User menu 3View")
        open_typed_settings(session)
        send_literal(session, "editor.open_policy")
        send(session, "F4")
        wait_for(session, "Edit editor.open_policy")
        send(session, "Tab", *("BSpace" for _ in "internal"))
        send_literal(session, "external")
        send(session, "Enter")
        wait_for(session, "Applied editor.open_policy=external")
        return_to_workspace(session)
        open_named_resource(session, "viewer-unicode", "F4")
        captures["editor_external_policy"] = wait_for(
            session, "External tool exited with status 0"
        )
        open_typed_settings(session)
        send_literal(session, "editor.open_policy")
        send(session, "F4")
        wait_for(session, "Edit editor.open_policy")
        send(session, "Tab", *("BSpace" for _ in "external"))
        send_literal(session, "association")
        send(session, "Enter")
        wait_for(session, "Applied editor.open_policy=association")
        return_to_workspace(session)
        open_named_resource(session, "viewer-unicode", "F4")
        captures["editor_association_policy"] = wait_for(session, "File Associations")
        if "Edit — near.test.default-open" not in captures["editor_association_policy"]:
            raise RuntimeError("editor association policy omitted its resolved Edit handler")
        send(session, "Home", "Enter")
        wait_for(session, "External tool exited with status 0")
        open_typed_settings(session)
        send_literal(session, "editor.open_policy")
        send(session, "F4")
        wait_for(session, "Edit editor.open_policy")
        send(session, "Tab", *("BSpace" for _ in "association"))
        send_literal(session, "internal")
        send(session, "Enter")
        wait_for(session, "Applied editor.open_policy=internal")
        return_to_workspace(session)
        keymap_path = profile / "config" / "keymap.toml"
        keymap_path.write_text(
            (ROOT / "specs" / "keymap.toml")
            .read_text(encoding="utf-8")
            .replace('on = "F1"', 'on = ["Ctrl+G", "Ctrl+G"]', 1),
            encoding="utf-8",
        )
        open_typed_settings(session)
        send(session, "F5")
        wait_for(session, "Reloaded externally edited settings")
        return_to_workspace(session)
        send(session, "C-g")
        captures["keymap_binding_reloaded"] = wait_for(session, "Ctrl+g → Ctrl+g")
        send(session, "C-g")
        captures["keymap_binding_executed"] = wait_for(session, "Context Help")
        send(session, "Escape")
        wait_without(session, "Context Help")
        open_typed_settings(session)
        send_literal(session, "pending key")
        captures["keymap_setting"] = wait_for(session, "Show pending key sequence")
        send(session, "Enter")
        captures["keymap_setting_persisted"] = wait_for(
            session, "Applied keymap.show_pending_sequence="
        )
        return_to_workspace(session)
        send(session, "C-g")
        time.sleep(0.2)
        captures["pending_sequence_hidden"] = capture(session)
        if "Ctrl+g → Ctrl+g" in captures["pending_sequence_hidden"]:
            raise RuntimeError("disabled pending-sequence display remained visible")
        send(session, "C-g")
        wait_for(session, "Context Help")
        send(session, "Escape")
        wait_without(session, "Context Help")
        open_typed_settings(session)
        send_literal(session, "command-line completion")
        captures["completion_setting"] = wait_for(session, "Fallback command-line completion")
        send(session, "Enter")
        captures["completion_setting_persisted"] = wait_for(
            session, "Applied interface.command_line_completion="
        )
        return_to_workspace(session)
        open_typed_settings(session)
        send_literal(session, "reversible")
        captures["confirmation_setting"] = wait_for(session, "Preview reversible operations")
        send(session, "Enter")
        captures["confirmation_persisted"] = wait_for(
            session, "Applied confirmations.reversible="
        )
        return_to_workspace(session)
        open_typed_settings(session)
        send_literal(session, "startup panel")
        captures["restart_setting"] = wait_for(session, "scope=Restart")
        send(session, "F4")
        captures["restart_setting_dialog"] = wait_for(
            session, "Edit interface.startup_panel"
        )
        send(session, "Tab")
        for _ in "left":
            send(session, "BSpace")
        wait_for(session, "Value                                                               ")
        send_literal(session, "right")
        captures["restart_setting_edited"] = wait_for(session, "Value          right")
        send(session, "Enter")
        captures["restart_setting_persisted"] = wait_for(
            session, "Applied interface.startup_panel=right and persisted"
        )
        interface_path = profile / "config" / "interface.toml"
        interface_settings = interface_path.read_text(encoding="utf-8")
        if 'startup_panel = "right"' not in interface_settings:
            raise RuntimeError(
                "restart-scoped startup panel did not persist as right:\n"
                + interface_settings
                + "\nrendered dialog before accept:\n"
                + captures["restart_setting_edited"]
            )
        return_to_workspace(session)
        send(session, "F9")
        captures["restart_setting_current_process"] = wait_for(session, "Left Menu")
        send(session, "Escape")
        wait_without(session, "Left Menu")
        send_literal(session, "cd folder-sample")
        send(session, "Enter")
        captures["shell_open"] = wait_for(
            session, "Shell 1 [running/Normal shell=clean close=warn]"
        )
        send(session, "C-o")
        wait_for(session, "Restored previous terminal layout")
        send_literal(session, "pwd")
        send(session, "Enter")
        captures["shell_persistent_directory"] = wait_for(
            session, f"{fixture}/folder-sample"
        )
        send(session, "C-o")
        wait_for(session, "Restored previous terminal layout")
        send_literal(session, "python3 -q")
        send(session, "Enter")
        time.sleep(0.3)
        captures["shell_repl_started"] = capture(session)
        send_literal(session, 'print("NEAR_REPL_RESULT_" + str(6 * 7))')
        send(session, "Enter")
        captures["shell_repl_before_hide"] = wait_for(
            session, "NEAR_REPL_RESULT_42"
        )
        send(session, "C-o")
        captures["shell_repl_workspace"] = wait_for(
            session, "Restored previous terminal layout"
        )
        send(session, "C-o")
        wait_for(session, "NEAR_REPL_RESULT_42")
        send_literal(session, 'print("NEAR_REPL_RESULT_" + str(7 * 7))')
        send(session, "Enter")
        captures["shell_repl_after_restore"] = wait_for(
            session, "NEAR_REPL_RESULT_49"
        )
        send_literal(session, "exit()")
        send(session, "Enter")
        wait_for(session, "folder-sample")
        send_literal(session, "printf NEAR_SHELL_OK")
        send(session, "Enter")
        captures["shell_command"] = wait_for(session, "NEAR_SHELL_OK")
        close_terminal_via_palette(session)
        captures["shell_warn_close"] = wait_for(session, "Close Running Shell")
        if "shell=clean close=warn" not in captures["shell_warn_close"]:
            raise RuntimeError("warn close policy abandoned the active shell screen")
        send(session, "Enter")
        captures["shell_warn_terminated"] = wait_for(
            session, "Shell 2 [running/Normal shell=clean close=warn]"
        )
        send_literal(session, "exit")
        send(session, "Enter")
        captures["shell_warn_exited"] = wait_for(session, "Shell Exited — Output Retained")
        if "[exited/Normal shell=clean close=warn]" not in captures["shell_warn_exited"]:
            raise RuntimeError("warn policy did not retain completed shell output")
        send(session, "Enter")
        captures["workspace_restored"] = wait_for(session, "Closed user screen")
        send(session, "F10")
        captures["host_restored"] = wait_for(session, "NEAR_HOST_RESTORED")
        if "schema = 1" not in viewer_path.read_text(encoding="utf-8"):
            raise RuntimeError("viewer settings fixture was not restored for restart verification")
        confirmation_settings = (profile / "config" / "confirmations.toml").read_text(encoding="utf-8")
        if 'reversible = "execute"' not in confirmation_settings or 'destructive = "preview"' not in confirmation_settings:
            raise RuntimeError("confirmation setting did not persist with mandatory safeguards")
        shell_path = profile / "config" / "shell.toml"
        shell_path.write_text(
            shell_path.read_text(encoding="utf-8").replace(
                'close_policy = "warn"', 'close_policy = "keep-open"'
            ),
            encoding="utf-8",
        )
        send_literal(
            session,
            f"{shlex_quote(str(binary))} --portable {shlex_quote(str(profile))}; "
            "printf '\\nNEAR_EMERGENCY_RESTORED\\n'",
        )
        send(session, "Enter")
        captures["panel_mode_restart"] = wait_for(session, "[Compact]")
        send(session, "Tab")
        open_named_resource(session, "viewer-utf16le", "F3")
        captures["viewer_state_restart"] = wait_for(session, "offset 30:0")
        if "wrap:true" not in captures["viewer_state_restart"] or "encoding:utf-16le" not in captures["viewer_state_restart"]:
            raise RuntimeError("viewer per-resource state did not survive process restart")
        send(session, "Escape")
        wait_for(session, "2User menu 3View")
        open_named_resource(session, "editor-workflow", "F4")
        captures["editor_position_restart"] = wait_for(session, "Ln 3, Col 3")
        send(session, "Escape")
        wait_for(session, "2User menu 3View")
        send(session, "Tab")
        send(session, "C-o")
        captures["shell_keep_open"] = wait_for(session, "close=keep-open")
        send_literal(session, "printf NEAR_KEEP_OPEN_BEFORE")
        send(session, "Enter")
        wait_for(session, "NEAR_KEEP_OPEN_BEFORE")
        close_terminal_via_palette(session)
        captures["shell_keep_open_hidden"] = wait_for(
            session, "Shell kept running in a hidden terminal tab"
        )
        send(session, "C-o")
        captures["shell_keep_open_resumed"] = wait_for(
            session, "NEAR_KEEP_OPEN_BEFORE"
        )
        send_literal(session, "printf NEAR_KEEP_OPEN_AFTER")
        send(session, "Enter")
        wait_for(session, "NEAR_KEEP_OPEN_AFTER")
        send(session, "C-o")
        wait_for(session, "Restored previous terminal layout")
        send(session, "F9")
        captures["restart_setting_applied"] = wait_for(session, "Right Menu")
        send(session, "Escape")
        wait_without(session, "Right Menu")
        open_typed_settings(session)
        send_literal(session, "pending key")
        captures["keymap_setting_restart"] = wait_for(session, "Show pending key sequence false")
        open_typed_settings(session)
        send_literal(session, "command-line completion")
        captures["completion_setting_restart"] = wait_for(session, "Fallback command-line completion false")
        open_typed_settings(session)
        send_literal(session, "reversible")
        captures["settings_restart"] = wait_for(session, "Preview reversible operations false")
        send(session, "C-M-q")
        captures["emergency_host_restored"] = wait_for(
            session, "NEAR_EMERGENCY_RESTORED"
        )
        shell_path.write_text(
            shell_path.read_text(encoding="utf-8").replace(
                'close_policy = "keep-open"', 'close_policy = "close"'
            ),
            encoding="utf-8",
        )
        handlers_path = profile / "config" / "handlers.toml"
        handlers_path.write_text(
            handlers_path.read_text(encoding="utf-8").replace(
                'actions = ["open", "view", "edit"]', 'actions = ["view", "edit"]'
            ),
            encoding="utf-8",
        )
        send_literal(
            session,
            f"{shlex_quote(str(binary))} --portable {shlex_quote(str(profile))}; "
            "printf '\\nNEAR_NO_HANDLER_RESTORED\\n'",
        )
        send(session, "Enter")
        wait_for(session, "9Menu 10Quit")
        send(session, "C-o")
        wait_for(session, "close=close")
        send_literal(session, "exit")
        send(session, "Enter")
        captures["shell_close_on_exit"] = wait_for(
            session, "Shell exited and the user screen closed"
        )
        send(session, "C-o")
        wait_for(session, "close=close")
        close_terminal_via_palette(session)
        captures["shell_close_running"] = wait_for(session, "Closed user screen")
        send(session, "Home", "Down", "Enter")
        captures["enter_without_open_handler"] = wait_for(
            session, "no handler matched the requested action and resource"
        )
        if "offset 0:0" in captures["enter_without_open_handler"]:
            raise RuntimeError("Enter silently fell back to the internal viewer")
        send(session, "C-M-q")
        captures["no_handler_host_restored"] = wait_for(
            session, "NEAR_NO_HANDLER_RESTORED"
        )

        evidence = {
            "schema_version": 1,
            "kind": "tmux-pty-operator-precheck",
            "operator_observation": False,
            "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "revision": run(["git", "rev-parse", "HEAD"], cwd=ROOT).stdout.strip(),
            "platform": "macos" if os.uname().sysname == "Darwin" else "linux",
            "tmux_version": run(["tmux", "-V"]).stdout.strip(),
            "tmux_escape_time_millis": int(tmux_escape_time),
            "shell_program": shell_program,
            "shell_mode": "clean",
            "temporary_panel_artifacts": {
                "exported_list_sha256": hashlib.sha256(exported_list.read_bytes()).hexdigest(),
                "exported_list_text": exported_list.read_text(encoding="utf-8-sig"),
                "source_before_sha256": temporary_source_before,
                "source_after_sha256": {
                    name: hashlib.sha256((fixture / name).read_bytes()).hexdigest()
                    for name in temporary_source_names
                },
                "stale_source_exists_after_refresh": stale_source.exists(),
                "copied_resource_sha256": hashlib.sha256(
                    (copy_destination / "cat.txt").read_bytes()
                ).hexdigest(),
            },
            "settings_artifacts": {
                "documents_before": settings_documents_before,
                "documents_after": text_documents(profile / "config"),
            },
            "editor_policy_artifacts": {
                "resources": {
                    name: {
                        "bytes_hex": (fixture / name).read_bytes().hex(),
                        "sha256": hashlib.sha256((fixture / name).read_bytes()).hexdigest(),
                    }
                    for name in (
                        "tab-spaces.txt",
                        "tab-raw.txt",
                        "editor-workflow.txt",
                        "viewer-latin1.txt",
                        "editor-external-change.txt",
                    )
                },
                "position_state_text": (profile / "state" / "editor-positions.toml").read_text(
                    encoding="utf-8"
                ),
                "position_state_sha256": hashlib.sha256(
                    (profile / "state" / "editor-positions.toml").read_bytes()
                ).hexdigest(),
            },
            "viewer_policy_artifacts": {
                "state_text": (profile / "state" / "viewer-state.toml").read_text(encoding="utf-8"),
                "state_sha256": hashlib.sha256(
                    (profile / "state" / "viewer-state.toml").read_bytes()
                ).hexdigest(),
                "corpus_sha256": {
                    path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                    for path in sorted(fixture.glob("viewer-*"))
                },
            },
            "assertions": [
                "legacy base keybar rendered",
                "fragmented legacy F9 remained one atomic function-key event",
                "Left and Right traversed all five Far top-level menu categories",
                "Tab and Shift+Tab switched directly between panel-specific menus",
                "command history Home, End, PageUp, and PageDown kept the focused entry visible",
                "Context Help handled Home, End, PageUp, and PageDown without closing",
                "the task surface accepted edge and page navigation and returned with Escape",
                "directory remained visibly distinct from regular files without relying on color",
                "F8 on an ordinary folder opened a source-specific Trash preview",
                "Enter selected the semantic Open handler instead of the internal viewer",
                "Enter without an Open handler reported an explicit denial and did not fall back to the viewer",
                "Home rendered the first collection page",
                "End rendered the final collection item",
                "PageDown changed the visible collection page",
                "Shift selection, plain navigation, and Insert preserved two non-contiguous selections",
                "edge navigation preserved selection while changing the viewport",
                "temporary-panel slots stayed isolated and retained references",
                "F5 copied selected resources as references without modifying source files",
                "copy-as-reference left operation task history empty",
                "F3 viewed a temporary-panel row through its original provider resource",
                "F7 removed only a temporary-panel reference",
                "F5 added references from separate source directories without operation tasks",
                "F7 removed a non-contiguous reference set without changing source resources",
                "Ctrl+PageUp revealed the remaining reference in its source directory",
                "tmp:+safe blocked reference removal without changing the slot",
                "Alt+Shift+F2 exported provider-qualified UTF-8 identities and replace import restored them",
                "F4 edited and saved a referenced resource through its original provider",
                "F5 copied a referenced resource through its original provider and retained operation history",
                "tmp:+safe blocked source mutation as well as reference removal",
                "tmp:+safe denied copy trash and removal while source reveal remained available",
                "tmp:+any imported arbitrary lines and Enter copied text to the command line",
                "tmp:<command captured shell output asynchronously into a provider-validated slot",
                "temporary-panel command completion remained visible in task history",
                "tmp:+menu rendered labeled actions and copied a non-resource action to the command line",
                "tmp:+full used the complete panel viewport and -full restored dual panels",
                "refresh retained a missing source as an explicitly stale reference",
                "native shell Ctrl+U editing replaced the docked command before execution",
                "Alt lookup preempted the shell dock without mutating its nonempty draft",
                "embedded terminal preserved ANSI styling and wide Unicode glyph output",
                "two independent terminal tabs retained their own shell output",
                "F12 showed foreground process state and numbered direct screen selection",
                "a terminal tab occupied the right peer pane without hiding the workspace",
                "terminal focus crossed to the file peer and returned without losing input",
                "Ctrl+O zoomed and restored the exact terminal pane",
                "F3 on the parent entry reported an explicit navigation-only denial",
                "F3 opened the current file in the internal viewer",
                "F4 opened the current writable file in the internal editor",
                "Alt lookup started at 1/3",
                "repeated Alt chord cycled to 2/3",
                "Up and Down cycled lookup matches",
                "held-Alt letters accumulated one visible filename lookup query",
                "plain text extended the lookup query",
                "bracketed paste extended the lookup query with Unicode",
                "Backspace edited and Enter accepted the lookup",
                "no-match Down closed lookup without stale status text",
                "lookup fell back from empty prefix results to an explicit internal match",
                "a bound Alt chord executed without starting lookup",
                "Options opened the searchable typed settings catalog",
                "advanced settings stayed hidden by default, toggled with F6, and remained searchable",
                "F4 opened the generic typed setting value editor",
                "a typed setting persisted to the portable profile document",
                "new-surface viewer settings applied only to a newly opened viewer",
                "F5 atomically reloaded an externally edited settings document",
                "invalid external settings retained the last-valid runtime state",
                "reset restored the viewer default and the next viewer rendered it",
                "status-line and keybar visibility settings applied live and restored live",
                "menu boundary wrapping followed the live interaction setting",
                "dialog focus clamped or wrapped according to the live interaction setting",
                "tree indentation persisted and changed the rendered tree panel",
                "editor tab size and expansion policy changed rendered insertion and exact saved bytes",
                "editor empty Unicode UTF-16 Latin-1 invalid-UTF-8 binary mixed-EOL tab huge-line and huge-file corpus opened",
                "editor undo and redo changed visible content through semantic status",
                "editor stream and column blocks rendered and persistent blocks remained available through the command palette",
                "editor provider-scoped position restored after close and after process restart",
                "editor Save As emitted exact UTF-16BE BOM and CRLF bytes",
                "editor lossy Latin-1 save required confirmation before replacing unsupported text",
                "editor external-change compare reload and keep-local choices preserved their declared data",
                "editor internal external and association policies routed through distinct visible workflows",
                "viewer auto-detected UTF-16LE and UTF-16BE while retaining encoded line offsets",
                "viewer Latin-1 mode and lossy invalid-UTF-8 mode rendered their declared text",
                "binary detection opened control-heavy content in the bounded hexadecimal viewer",
                "empty Unicode mixed-EOL tab read-only huge-line and huge-file viewer corpus remained responsive",
                "rapid quick-view replacement cancelled stale large-resource work and rendered the current item",
                "viewer position bookmark encoding and wrap mode restored for the same provider resource",
                "viewer internal external and association policies routed through distinct visible workflows",
                "panel mode defaults applied live and survived a second process",
                "external keymap bindings reloaded and executed without restart",
                "pending-sequence display policy applied live and persisted across restart",
                "fallback-only Near completion policy applied live and persisted across restart",
                "editable confirmation policy persisted without weakening mandatory safeguards",
                "restart-scoped startup panel persisted without changing the current process and applied after restart",
                "a second Near process loaded the persisted settings document",
                "embedded clean-profile shell opened",
                "warn policy presented explicit running-shell decisions and retained completed output",
                "keep-open policy hid and resumed the same running shell",
                "close policy closed completed and running shell screens without abandonment",
                "panel submissions reused one shell working directory",
                "interactive Python REPL state survived user-screen hide and restore",
                "embedded shell command output rendered",
                "workspace restored from user screen",
                "host PTY restored after F10 quit",
                "Ctrl+Alt+Q bypassed the active surface and restored the host PTY",
                "Ctrl+Alt+Q restored the host PTY after an Open-handler denial",
            ],
            "captures": {name: {"sha256": digest(text), "text": text} for name, text in captures.items()},
            "status": "passed",
        }
        output = ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        try:
            displayed_output = output.relative_to(ROOT)
        except ValueError:
            displayed_output = output
        print(f"tmux terminal workflows: PASS ({displayed_output})")
        return 0
    finally:
        subprocess.run(["tmux", "kill-session", "-t", session], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        shutil.rmtree(fixture, ignore_errors=True)
        shutil.rmtree(tmux_root, ignore_errors=True)


def shlex_quote(value: str) -> str:
    import shlex
    return shlex.quote(value)


if __name__ == "__main__":
    raise SystemExit(main())
