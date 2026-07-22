# Near Production Workflow Testing Guide

## Purpose

This is the operator procedure for proving Near production-ready on macOS and Linux. Unit,
component, snapshot, and PTY tests are prerequisites, not substitutes for this guide. A release is
not qualified until every required entry in `specs/operator-workflows.toml` has passing evidence at
the exact candidate revision and `python3 tools/qualify.py production --no-resume` passes from a
clean checkout.

The guide proves Far Manager workflow parity plus documented Near improvements. It never uses a
physical removable drive for destructive testing. Mount and root tests use disposable disk images,
loopback filesystems, temporary directories, or provider fixtures only.

## Roles and Evidence

- The operator executes the workflow in the named terminal and records what was visible and what
  happened to the filesystem, provider, task history, operation journal, and terminal state.
- A reviewer verifies the evidence hashes, candidate revision, expected denial/decision choices,
  and final state before approving a parity or requirement status change.
- Evidence artifacts are stored under `.near/qualification/operator/<platform>/`. Screenshots,
  terminal transcripts, semantic snapshots, journal exports, fixture listings, and JSON diagnostics
  are acceptable. Every workflow entry requires at least one artifact and non-empty notes.
- Secrets, home-directory contents, usernames, remote hostnames, and real device identifiers must
  be redacted before evidence is attached to a PR.

Initialize the platform checklist:

```sh
mkdir -p .near/qualification/operator/macos
python3 tools/workflow_evidence.py init \
  --platform macos \
  --operator "$USER" \
  --output .near/qualification/operator/macos/evidence.json
```

Use `--platform linux` on Linux. Record a completed case with:

```sh
python3 tools/workflow_evidence.py record \
  --evidence .near/qualification/operator/macos/evidence.json \
  --scenario OP-INPUT-ALT-LOOKUP \
  --terminal Terminal.app \
  --status passed \
  --notes "Alt lookup cycled two matches; Escape restored the original cursor." \
  --artifact .near/qualification/operator/macos/alt-lookup-terminal-app.txt
```

The recorder copies every supplied artifact into a content-addressed `artifacts/` directory beside
the evidence document. Later fixture or qualification runs cannot mutate already-recorded proof;
do not edit stored artifacts or their hashes.

Validate continuously:

```sh
python3 tools/workflow_evidence.py validate \
  --evidence .near/qualification/operator/macos/evidence.json
```

Prepare the revision-bound operator session pack before opening Near:

```sh
python3 tools/operator_session.py prepare --platform macos
python3 tools/operator_session.py verify \
  --session .near/qualification/operator/macos/session/operator-session.json
python3 tools/operator_session.py status \
  --evidence .near/qualification/operator/macos/evidence.json
```

`status` groups remaining work by scenario, reports the next terminal/scenario pair, and points to
the generated revision-bound `operator-checklist.md`. Add `--all` only when the full terminal list
is useful. The checklist is generated from `specs/operator-workflows.toml` and
`specs/interaction-conformance.toml`; every scenario contains concrete steps, required artifacts,
and a ready-to-edit `workflow_evidence.py record` command. Do not improvise a smaller walkthrough
or treat the raw pending count as the procedure.

Use `--platform linux` and the corresponding Linux paths on Linux. The session pack records the
candidate revision, dirty state, host/toolchain versions, installed terminal versions, environment,
expected evidence entries, fixture hashes, exact filename bytes, and explicit filesystem
capabilities. It creates lookup, deletion, shell, viewer, and editor corpora under
`.near/qualification/operator/<platform>/session/fixtures/`.

`prepare` replaces only that generated `session/` directory. Run it before a new candidate or to
reset mutable fixtures; never run it after capturing an artifact that still points into the mutable
session tree. Copy completed evidence through `workflow_evidence.py record` first so it becomes
content-addressed and immutable.

The session manifest may report that a filesystem aliases NFC/NFD names or rejects non-UTF-8 name
bytes. Such results are explicit platform capabilities, not silent skips. Exercise the missing case
through a provider fixture or a second platform that supports it and cite both the capability result
and provider evidence. A production claim still requires the acceptance behavior, even when the
native filesystem cannot create that resource class.

Windows CI also runs `operator_session.py prepare --platform windows` as a capability-only pack.
It contains no operator evidence entries and does not expand the first production declaration to
Windows; it records native terminal, path, symlink, and filename capabilities so adapter regressions
remain visible instead of turning the macOS/Linux operator gate into an unsupported Windows failure.

## Candidate Preparation

Routine development and wave qualification run locally. GitHub Actions workflows for CI,
qualification, and performance are manual-only so hosted minutes are reserved for release work.
Use `python3 tools/qualify.py wave --no-resume` as the authoritative pre-merge command. Trigger the
manual qualification workflow only for an explicit release candidate; tag-based release and API
compatibility workflows remain automatic.

1. Use a clean clone of the signed candidate revision. Do not qualify a dirty tree.
2. Install the repository Rust toolchain, Python 3.12+, tmux, and the terminals required for the
   current platform.
3. Build with `cargo build --workspace --all-targets --locked`.
4. Run `python3 tools/qualify.py wave --no-resume`; resolve every critical/high failure first.
   The orchestrator retries only recognized registry/network transport failures; test, Clippy,
   artifact, parity, and workflow failures are never retried into a pass.
5. Generate and verify the operator session pack. Save `operator-session.json` as the first
   environment artifact and inspect every unavailable terminal or filesystem capability.
6. Create isolated configuration and data roots:

   ```sh
   export NEAR_QUALIFY_ROOT="$(mktemp -d)"
   export NEAR_CONFIG_ROOT="$NEAR_QUALIFY_ROOT/config"
   export NEAR_DATA_ROOT="$NEAR_QUALIFY_ROOT/data"
   mkdir -p "$NEAR_CONFIG_ROOT" "$NEAR_DATA_ROOT"
   ```

7. Launch Near with `cargo run -p near-fm -- --config-root "$NEAR_CONFIG_ROOT" --data-root "$NEAR_DATA_ROOT"`.
8. Capture Near diagnostics beside the session manifest. The session manifest already captures
   `git rev-parse HEAD`, toolchain versions, terminal versions, `$TERM`, and tmux availability;
   capture the live terminal diagnostics again when they differ inside tmux or a launched terminal.

## Terminal Matrix

Run every scenario marked `terminal_matrix = true` in the manifest.

### macOS

- Terminal.app without tmux.
- iTerm2 without tmux.
- Ghostty without tmux.
- Terminal.app inside a current tmux release.
- iTerm2 inside a current tmux release.

### Linux

- GNOME Terminal.
- Konsole.
- tmux in either GNOME Terminal or Konsole; record the host terminal.

Use an isolated tmux server whose configuration contains `set -sg escape-time 10`, then record the
effective value with `tmux show-options -sv escape-time`. The default 500-millisecond ambiguity
window can merge Escape-based back navigation with a following key and is not a qualified profile.

Ghostty-on-Linux and SSH-to-Linux are scheduled extended coverage. Failures are release blockers
when they reproduce in a mandatory terminal or reveal a backend-independent defect.

Development qualification runs locally; hosted GitHub workflows remain manual and unused until
release qualification. Hosted runners would provide regression evidence only and do not expose
GNOME Terminal, Konsole, Terminal.app, iTerm2, or Ghostty GUI input stacks, so direct terminal
observations remain mandatory regardless.

Before each terminal's Near walkthrough, capture its normalized transport behavior:

```sh
cargo run -p near-input-probe -- \
  ".near/qualification/operator/macos/input-terminal-app.json" \
  "Terminal.app"
```

Exercise keypad keys, modified arrows, modifier holds/releases, focus loss/regain, paste, and
resize, then press `Ctrl+Q`. The probe uses `near-terminal`'s public normalized event types and
writes keyboard mode, degradations, terminal environment, tmux state, elapsed timing, and every
event as JSON. Use the matching Linux evidence path and terminal label on Linux. The probe is
diagnostic evidence, not a substitute for verifying the corresponding behavior inside Near.

In file panels, verify the Far navigation aliases before the terminal-specific scenarios: `Left`
moves to the first collection entry and `Right` moves to the last; `Home` and `End` remain
equivalent aliases. `PageUp` and `PageDown` move by one visible panel page. Every navigation command
must keep the focused entry visible, including after jumping to the final item. Viewer, editor, menu,
dialog, and embedded-terminal contexts retain their own navigation behavior and must not inherit
panel navigation.

Also verify panel selection motion: `Shift+Up` and `Shift+Down` toggle the current entry's selected
state and move one row in the requested direction, while `Insert` toggles and advances one row.
Selected entries must show both the selection marker and selected semantic role. The navigation-only
parent entry must never become selected.

The authoritative executable inventory is `specs/interaction-conformance.toml`. Each case links its
keys to the panel context, expected semantic commands, model and render assertions, a Rust workflow
test, parity records, requirements, and an operator scenario. `tools/validate_interaction_conformance.py`
rejects missing bindings, stale test names, invalid traceability, or cases that assert model state
without rendered behavior. `--require-complete` additionally rejects any case not operator-verified
and any unfinished discovery record; production qualification uses that stricter mode.

Cursor focus and selection are independent state. A required non-contiguous workflow selects one
file, moves across one or more files with plain arrows, then selects another file without changing
the earlier selection. Tests must separately assert cursor position, the complete selected-resource
set, visible selection marks, and the semantic roles for focused-selected, focused-unselected, and
selected-non-focused entries.

The panel inventory also includes Far-compatible horizontal scrolling and resizing. Use
`Alt+Left/Right`, `Ctrl+Alt+Left/Right`, and `Alt+Home/End` with long Unicode names. Use
`Ctrl+Shift+Left/Right` to move the panel boundary by ten columns, `Ctrl+Shift+Up/Down` to change
panel height by five rows, and `Ctrl+Shift+Home` to reset the layout. Mouse targeting must follow
the resized boundary. `Ctrl+R` must retain surviving focus, selection, and horizontal position.

Repeat Home, End, PageUp, and PageDown in menus, settings, tasks, histories, help, inspector,
viewer, and editor contexts. These surfaces must use their own navigation commands; panel aliases
must never leak into an overlay or document surface.

The manifest also records unresolved discovery programs. Panel navigation, selection, operation
targeting, other interactive surfaces, and terminal key protocols remain explicit pending work until
their required outputs are captured and their cases are promoted from `implemented` to `verified`.

Operator verification does not require editing a case after observation. `operator_session.py
prepare` generates `operator-checklist.md` from the same interaction manifest, and production
interaction completeness reads the exact-revision `evidence.json` directly. This avoids invalidating
terminal evidence by changing the candidate revision merely to promote a source status field.

## Windows Contract

Windows is not part of the first production declaration, but every candidate must remain native-CI
test and Clippy clean. A macOS/Linux cross-check can additionally run:

`cargo check --workspace --all-targets --target x86_64-pc-windows-gnu --locked`

`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings`

Only the native `windows-latest` CI job counts as execution evidence for Windows tests and adapter
contracts; cross-compilation is compile evidence, not runtime evidence.

## WF-TRASH-002 — Deletion Fault Closure

On macOS, first run `python3 tools/test_macos_mount_safety.py`. It moves two disposable same-name
files through the native Trash helper, proves the execution journal records both actual
platform-selected destinations and restores each exact source, then creates a disposable HFS+
image under `/Volumes` to prove classification and pre-plan rejection for Trash, permanent delete,
and wipe. It verifies a sentinel survives, detaches the image in a `finally` block, and writes
`.near/qualification/macos-mount-safety.json`.
On Linux, run `sudo python3 tools/test_linux_mount_safety.py`; it performs the same assertions on
a disposable 16 MiB tmpfs mount and always unmounts it during cleanup.

Before any manual walkthrough, run `python3 tools/run_workflow_prechecks.py`. This executes the
deterministic regression set associated with every operator workflow except the external
`NearTuiProof` repository. Its JSON explicitly records `operator_observation: false`; it is a
mandatory precheck and can never replace the terminal/operator evidence below.

For the practical tmux contract, run `python3 tools/test_tmux_terminal_workflows.py`. It opens a
real `near-fm` binary in a fresh tmux PTY and verifies Home/End/PageDown viewport movement,
non-contiguous Shift/plain/Insert selection, selection-preserving edge jumps, legacy keybar
honesty, Alt lookup cycle/text/paste/Unicode/edit/accept/cancel/no-match and bound-chord behavior,
persistent shell working directory, interactive REPL continuity across user-screen hiding,
embedded clean-profile shell execution, restoration to the
workspace, and restoration to the host shell after F10. Its artifact declares
`operator_observation: false`; GNOME Terminal, Konsole, Terminal.app, iTerm2, and Ghostty still
require direct observation because host key encoding is part of scope.

Start from `session/fixtures/operations/`, which contains ordinary files, a recursive tree,
symlinks, a broken link, read-only content, collisions, and the platform's exact-name capability
result. Add a disappearing source, disposable mount root, filesystem root representation, provider
root, virtual collection, and removable-device fixture; those resources are intentionally created
at execution time because they cannot be represented safely as static files.

1. Invoke normal delete/Trash on each ordinary entry and inspect the preview.
2. Confirm the preview title is “Move to Trash”, states reversibility, and exposes only execute and
   cancel. It must not expose replace, skip, rename, overwrite, or generic conflict policy.
3. Invoke hidden conflict command IDs through a macro/test driver. They must be rejected and must
   not alter the decision.
4. Confirm duplicate Trash names preserve both byte-distinct resources without replace prompts.
5. Invoke Files → Restore last Trash, review the recorded source and original destination, restore
   each ordinary resource, and compare exact filename bytes, contents, type, and recorded source
   metadata. Create an original-path collision and verify no replacement occurs without an explicit
   restore conflict decision.
6. Attempt Trash, permanent delete, and wipe on filesystem and mount roots. No operation preview,
   plan ID, task, or journal “planned” entry may be created.
7. Confirm mount/removable resources offer only capability-gated eject, unmount, disconnect, or an
   explicit unsupported reason.
8. Cancel during recursive/cross-device work and verify completed, pending, failed, and retryable
   resources remain exact in task history and the operation journal.
9. Save before/after fixture listings, the preview semantic snapshot, task history, journal export,
   and provider refresh state.

## OP-INPUT-ALT-LOOKUP — Incremental Filename Lookup

Run in every mandatory terminal from `session/fixtures/lookup/`, which contains `cargo.toml`,
`cat.txt`, `café.txt`, and a no-match case. The decomposed Unicode equivalent is co-located when the
filesystem permits it; otherwise use `session/fixtures/lookup-unicode-nfd/` through a provider
fixture and cite the session capability result.

1. Press Alt plus the first character. The lookup must start only when the effective keymap has no
   binding for that chord.
2. Repeat the Alt chord and verify cycling without changing the query.
3. Type unmodified characters and paste text to extend the query.
4. Verify Backspace, Up/Down cycling, Enter acceptance, and Escape restoration.
5. Verify a bound Alt chord executes its command and never starts lookup.
6. In enhanced keyboard mode, press and hold Alt, then type the character; verify the held state is
   consumed. Release Alt and verify state clears.
7. In legacy mode, verify Escape-prefixed Alt works, standalone hold is not claimed, and diagnostics
   identify the legacy limitation.
8. Switch focus away mid-lookup and verify modifier state clears without moving the accepted cursor.

## OP-INPUT-KEYBAR — Function-Key Layers

1. Capture the base keybar and compare every visible function key to the effective keymap.
2. In enhanced mode, hold Shift, Ctrl, Alt, and supported combinations. The exact alternate layer
   must appear on press and disappear on release or focus loss.
3. Click a keybar item and verify it dispatches the command shown in that layer.
4. In legacy mode, confirm the base layer remains visible and no unsupported hold layer is invented.
5. Verify diagnostics explain enhanced versus legacy behavior.

## OP-INPUT-RESTORATION — Terminal Lifecycle

1. Exercise focus loss/gain, bracketed paste, rapid resize, mouse capture, and color-depth fallback.
2. Suspend to an external viewer/editor and verify raw mode, alternate screen, cursor, paste, mouse,
   and keyboard enhancement are restored before launch and re-enabled after exit.
3. Trigger a controlled panic in a test build and verify the host terminal is usable afterward.
4. Run vim or another alternate-screen application inside the embedded PTY, exit it, then run ssh
   client diagnostics and return to the shell.
5. Record the terminal capability diagnostics and restoration transition log.

## OP-SETTINGS-001 — Typed Settings

1. Open Settings and search normal and advanced descriptors across system, panel, tree, interface,
   dialog, menu, command-line, completion, confirmation, shell, viewer, and editor categories.
2. Verify every entry displays effective value, type, platform availability, source layer/file, and
   live/new-surface/restart scope.
3. Change a live setting and verify immediate application plus one atomic persistence write.
4. Submit an invalid value and verify validation identifies the descriptor and preserves the last
   valid runtime and on-disk state.
5. Inject an apply failure after an earlier ordered setting applied; verify reverse rollback and
   restoration of the previous persisted snapshot.
6. Edit a configuration document externally. Verify valid reload, invalid reload rollback, and
   cross-document apply ordering.
7. Reset a setting to its declared default. Restart Near and verify persistent and restart-scoped
   values are correct.

## OP-SHELL-001 — Native Shell Profiles

1. With `mode = "platform-default"`, verify macOS resolves the account login shell and starts it as
   a login shell; verify Linux resolves the account shell and starts it interactively.
2. Confirm normal zsh/bash startup files run in platform-default mode using a disposable marker.
3. Confirm clean mode omits the marker and is explicitly labeled as clean/no-rc behavior.
4. Verify custom program, arguments, startup command, working directory, inherited/minimal
   environment, OSC 7 directory updates, interrupt, paste, resize, and nested applications.
5. Exercise close, keep-open, and warn exit policies with a running child process and a completed
   shell. No process may be silently abandoned.

## OP-VIEWER-001 — Viewer Policy and Corpus

Use `session/fixtures/viewer-editor/` for empty, Unicode, UTF-16LE/BE, Latin-1, invalid UTF-8,
binary, huge-file, huge-line, mixed-EOL, tabs, read-only, externally changed, and provider-backed
resource cases. `external-change.txt` is intentionally mutable; regenerate the pack before a repeat.

1. Verify configured wrap, hex, encoding, and remember-state defaults on a new viewer.
2. Verify internal, external-default, and association-selection policies route exactly as declared.
3. Verify session state and per-resource offset/bookmarks remain separate from global defaults.
4. Verify streaming remains bounded for huge resources and cancellation leaves the UI responsive.
5. Reopen resources after restart and verify only declared per-resource state is restored.

## OP-EDITOR-001 — Editor Policy and Corpus

Use the same corpus plus writable/read-only and externally replaced files.

1. Verify persistent blocks, tab width, expand-tabs, and open policy load from `editor.toml`.
2. Verify internal, external-default, and association-selection policies route exactly as declared.
3. Verify encoding/BOM/EOL save choices, lossy confirmation, undo/redo, stream/column blocks,
   read-only denial, and safe save.
4. Trigger external modification and verify compare, reload, and keep-local choices.
5. Verify global defaults, document session state, and provider-scoped cursor state remain distinct
   across reopen and restart.

## OP-FAR-PARITY-001 — Parity Plus Improvements

Walk every item in `project/far-parity.toml`, not only the seven historical gaps. For each item:

1. Execute every acceptance statement through the operator-visible command path.
2. Record the Near command ID, keys, visible state, final model/provider/filesystem state, and any
   deliberate platform-native improvement over Far.
3. Treat a missing command, hidden impossible decision, stale refresh, lost identity, or unverified
   durability claim as a failure.
4. Update a parity item to `verified` only after macOS and Linux evidence exists and the checked-in
   evidence document cites the artifacts.

Near improvements that must remain true include provider-neutral identities, exact structured argv,
safe preview/authorization floors, asynchronous responsiveness, semantic accessibility roles,
recoverable operation journals, and explicit platform capability diagnostics.

## OP-TUI-PROOF-001 — Public Consumer

1. Clone the private `NearTuiProof` repository separately from Near.
2. Pin Near public crates to the exact candidate Git revision. Path dependencies are forbidden.
3. Verify `cargo tree` contains no path to the local Near checkout and application code imports no
   Ratatui, Crossterm, Near application internals, file-manager, or dual-panel assumptions.
4. Build and test custom provider, commands, menus, help, dialogs, tasks, themes, keymaps,
   snapshots, and terminal execution on macOS and Linux.
5. Save `Cargo.lock`, `cargo tree`, test output, snapshots, and the proof revision as artifacts.

## Final Production Gate

1. Validate local operator evidence.
2. Merge macOS and Linux evidence in the release qualification job and verify both refer to the same
   signed Near revision.
3. Run production qualification from a clean checkout.
4. Build release binaries, archives, SBOMs, checksums, and provenance; smoke-test each binary.
5. Confirm all parity items are `verified`, all mandatory workflows have evidence, Windows build,
   tests, Clippy, and adapter contracts are green, and no critical/high gate failed.
6. Publish only when `qualification.json` revision and artifact hashes match the packaged source and
   generated release files.

Any contradiction between field behavior and existing verified evidence reopens the requirement and
parity item. Every escaped fault must add both a minimal reproduction and a class-level regression.
