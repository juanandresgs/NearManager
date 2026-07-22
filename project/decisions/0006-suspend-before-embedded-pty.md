# ADR-0006: Ship Suspend-and-Run Before Embedded PTY

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-EXT-001, REQ-PTY-001
- Supersedes: none
- Superseded by: none

## Context

External editors and shells are essential, but embedding a correct terminal emulator requires PTY lifecycle, parsing, resize, mouse, clipboard, OSC, alternate screen, and nested-TUI support.

## Decision

Phase one restores the original terminal and runs interactive children directly. Embedded PTY support is later, optional, replaceable, and never removes the reliable suspend path.

## Consequences

### Positive

- Vim, Neovim, SSH, and specialist tools work early.
- The file workspace is not delayed by terminal-emulator complexity.

### Negative

- Early external tools temporarily leave the Near screen.
- Embedded shell integration arrives later than core file workflows.

## Verification

External-tool restoration tests are a Phase 1 gate; embedded PTY compatibility is a separate Phase 3 gate.

