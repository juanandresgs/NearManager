# M1 Terminal and External Tool Evidence

Date: 2026-06-23

## Implemented Slice

- `near-terminal` now owns a capability-aware terminal lifecycle state machine with reverse-order rollback and best-effort restoration.
- Optional bracketed-paste and cursor features degrade independently without blocking startup.
- Optional keyboard enhancement is probed progressively, reports active legacy/enhanced mode, and restores through explicit push/pop transitions.
- Panic unwinding and failed restoration paths reuse the same idempotent cleanup state.
- Common termination signals are published atomically and handled on the workspace thread before terminal restoration.
- `near-core` provides provider-neutral structured external invocations and resolver contracts.
- `near-local-fs` resolves exact-byte local file resources to one path argument without shell evaluation.
- `near-ui` suspends rendering for external tools, reports child status, reconstructs the terminal backend, and preserves workspace state.
- `near-fm` binds F4 to `$VISUAL`, `$EDITOR`, or the macOS text-editor fallback.

## Automated Evidence

- Terminal unit tests cover normal exit, required initialization failure, optional capability failure, partial restoration failure, panic unwinding, successful handoff, failed handoff, returned child status, and signal observation.
- Keyboard fixtures cover legacy Escape/Alt/function keys, enhanced repeat/release, and disambiguated Tab versus Ctrl-I events.
- The hostile-path resolver test proves whitespace, newlines, shell metacharacters, and invalid UTF-8 remain one exact process argument.
- The workspace test proves edit requests preserve focused resource and selection state.
- macOS PTY integration tests execute the built `near-fm` binary through real Vim 9.1 and Neovim 0.12.1 round trips, then verify repeated workspace content and alternate-screen leave/re-entry sequences.

## Interactive Evidence

- F4 with `/usr/bin/true` visibly left the alternate screen, returned status 0, redrew the same focused `file.txt`, and restored normally through F10.
- F4 round trips through Vim 9.1 and Neovim 0.12.1 each returned status 0 and redrew the unchanged panel and cursor.
- A live SIGTERM sent to the PTY process restored cursor visibility, disabled bracketed paste, left the alternate screen, and returned `Terminated(15)`.

## Requirement Status

- `REQ-TERM-001` is verified by lifecycle unit tests, real PTY editor integration tests, and the live termination-signal restoration check.
- `REQ-TERM-002` is verified by capability-mode diagnostics, enhancement lifecycle tests, deterministic legacy fallback, and normalized legacy/enhanced fixtures.
- `REQ-EXT-001` is verified by structured child invocation, returned exit status, automated Vim/Neovim PTY round trips, and workspace-state preservation tests.
