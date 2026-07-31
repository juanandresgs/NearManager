#!/usr/bin/env python3
"""Validate Near Manager's public showcase asset and reassessment policy."""

from __future__ import annotations

import struct
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GIF = ROOT / "docs/assets/near-manager-preview.gif"
POLICY = ROOT / "docs/showcase-media.md"
README = ROOT / "README.md"
CONTRIBUTING = ROOT / "CONTRIBUTING.md"
PR_TEMPLATE = ROOT / ".github/pull_request_template.md"
OWNERSHIP = ROOT / "specs/abstraction-ownership.toml"
QUALIFICATION = ROOT / "specs/qualification.toml"
OBSOLETE_TAPE = ROOT / "docs/assets/near-manager-preview.tape"


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def skip_sub_blocks(data: bytes, offset: int) -> int:
    while offset < len(data):
        size = data[offset]
        offset += 1
        if size == 0:
            return offset
        offset += size
    raise ValueError("truncated GIF sub-blocks")


def inspect_gif(data: bytes) -> tuple[int, int, int, float]:
    if len(data) < 13 or data[:6] not in {b"GIF87a", b"GIF89a"}:
        raise ValueError("asset is not a GIF")

    width, height = struct.unpack_from("<HH", data, 6)
    packed = data[10]
    offset = 13
    if packed & 0x80:
        offset += 3 * (2 ** ((packed & 0x07) + 1))

    frames = 0
    duration_cs = 0
    while offset < len(data):
        marker = data[offset]
        offset += 1
        if marker == 0x3B:
            break
        if marker == 0x21:
            if offset >= len(data):
                raise ValueError("truncated GIF extension")
            label = data[offset]
            offset += 1
            if label == 0xF9:
                if offset + 6 > len(data) or data[offset] != 4:
                    raise ValueError("invalid GIF graphics control extension")
                duration_cs += struct.unpack_from("<H", data, offset + 2)[0]
                offset += 6
            else:
                offset = skip_sub_blocks(data, offset)
            continue
        if marker == 0x2C:
            if offset + 9 > len(data):
                raise ValueError("truncated GIF image descriptor")
            descriptor_packed = data[offset + 8]
            offset += 9
            if descriptor_packed & 0x80:
                offset += 3 * (2 ** ((descriptor_packed & 0x07) + 1))
            if offset >= len(data):
                raise ValueError("truncated GIF image data")
            offset += 1
            offset = skip_sub_blocks(data, offset)
            frames += 1
            continue
        raise ValueError(f"unexpected GIF block marker 0x{marker:02x}")

    return width, height, frames, duration_cs / 100


def main() -> int:
    errors: list[str] = []

    if not GIF.exists():
        fail(errors, "primary showcase GIF is missing")
        width = height = frames = 0
        duration = 0.0
    else:
        try:
            width, height, frames, duration = inspect_gif(GIF.read_bytes())
        except ValueError as error:
            fail(errors, str(error))
            width = height = frames = 0
            duration = 0.0

        if width < 1000 or height < 600:
            fail(errors, f"primary showcase is too small: {width}x{height}")
        if frames < 3:
            fail(errors, f"primary showcase is not meaningfully animated: {frames} frames")
        if not 8 <= duration <= 20:
            fail(errors, f"primary showcase duration must be 8-20 seconds, got {duration:.2f}")
        if GIF.stat().st_size > 5 * 1024 * 1024:
            fail(errors, "primary showcase exceeds 5 MiB")

    readme = README.read_text(encoding="utf-8")
    if "docs/assets/near-manager-preview.gif" not in readme:
        fail(errors, "README does not embed the primary showcase GIF")
    if "contextual shortcuts" not in readme:
        fail(errors, "README showcase alt text does not describe the current story")

    policy = POLICY.read_text(encoding="utf-8") if POLICY.exists() else ""
    for phrase in ("Keep the primary GIF", "Refresh the primary GIF", "Add a differentiated GIF"):
        if phrase not in policy:
            fail(errors, f"showcase policy is missing decision {phrase!r}")

    contributing = CONTRIBUTING.read_text(encoding="utf-8")
    if "docs/showcase-media.md" not in contributing:
        fail(errors, "CONTRIBUTING.md does not link the showcase policy")

    template = PR_TEMPLATE.read_text(encoding="utf-8")
    for decision in ("keep", "refresh", "differentiated"):
        if decision not in template:
            fail(errors, f"pull request template is missing showcase decision {decision!r}")

    ownership = tomllib.loads(OWNERSHIP.read_text(encoding="utf-8"))
    showcase = next(
        (item for item in ownership.get("behavior", []) if item.get("id") == "ABST-SHOWCASE-001"),
        None,
    )
    if showcase is None or showcase.get("layer") != "application-policy":
        fail(errors, "ABST-SHOWCASE-001 application-policy ownership is missing")

    qualification = tomllib.loads(QUALIFICATION.read_text(encoding="utf-8"))
    gate = next(
        (item for item in qualification.get("gates", []) if item.get("id") == "showcase-media"),
        None,
    )
    if gate is None:
        fail(errors, "qualification manifest is missing the showcase-media gate")
    elif gate.get("command") != ["python3", "tools/validate_showcase_media.py"]:
        fail(errors, "showcase-media qualification gate runs the wrong command")
    elif set(gate.get("profiles", [])) != {"developer", "wave", "production"}:
        fail(errors, "showcase-media gate must run in every qualification profile")

    if OBSOLETE_TAPE.exists():
        fail(errors, "obsolete synthetic preview tape must not be published")

    print(
        f"Showcase media: {width}x{height}, {frames} frames, "
        f"{duration:.2f}s, {GIF.stat().st_size if GIF.exists() else 0} bytes"
    )
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
