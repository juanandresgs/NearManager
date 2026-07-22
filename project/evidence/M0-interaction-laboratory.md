# M0 Interaction Laboratory Evidence

Date: 2026-06-23

## Purpose

This record captures the first executable implementation slice for milestone M0. It is implementation evidence, not a declaration that the milestone or its requirements are verified.

## Implemented Slice

- `near-core` defines stable semantic identifiers, resources, locations, capability sets, command descriptors, invocations, safety classes, availability, and focused/peer action context.
- `near-terminal` normalizes Crossterm events and owns fail-safe raw-mode, alternate-screen, bracketed-paste, and cursor lifecycle restoration.
- `near-ui` loads contextual keymaps and semantic themes from the normative TOML specifications while keeping Ratatui out of its public API.
- `near-ui` exposes a backend-independent scene and surface protocol plus reusable collection, tree, viewer, inspector, terminal-grid, task, menu, dialog, and help surfaces.
- `FarWorkspace` demonstrates two generic collection views, focused/peer interaction, centralized semantic command dispatch, generated function-key hints, deterministic headless rendering, and a gallery that exercises the complete M0 surface catalog.
- `near-demo` is a non-filesystem application whose object-safe `ProcessProvider` lists `proc://local` resources into the generic `CollectionSurface` and responds to the same semantic collection commands without importing Ratatui or Crossterm.
- `near-ui` centrally validates command registration, argument schemas, contextual availability, safety metadata, and precise unavailable reasons before dispatch.
- `near-testkit` supplies manual time, scripted headless Far workflows, semantic golden frames, fake provider schedules, cancellation, and stale-generation rejection.
- `near-fm` loads `specs/keymap.toml` and `specs/theme.toml` and runs the interaction laboratory on macOS.

## Automated Evidence

The following commands passed on 2026-06-23:

```text
cargo fmt --all -- --check
CARGO_HOME=/tmp/near-cargo cargo test --workspace
CARGO_HOME=/tmp/near-cargo cargo clippy --workspace --all-targets -- -D warnings
python3 tools/validate_project.py
```

Observed results:

- Thirty-three `near-ui` unit/model-render tests, two `near-testkit` workflow/provider tests, and two `near-demo` process-provider contract tests passed.
- Workspace, crate, and documentation tests passed with no failures.
- Clippy passed for all workspace targets with warnings denied.
- Project-definition validation passed with 17 capabilities, 40 requirements, 6 milestones, 14 workflows, and 21/21 functional requirements linked to workflows.

The automated tests cover deterministic context precedence, inheritance, removal, aliases, typed parameterized bindings, trie-based sequences, fake-time timeout fallback, conflict origins, registry completeness for every shipped binding, argument validation, contextual unavailable reasons, reload-driven help and palette updates, valid continuation hints, explicit semantic theme fallback validation, terminal color-depth detection and degradation, low-color interaction-state distinction, role-preserving snapshots at three viewport sizes and three themes, public scene rendering, every reusable surface category, semantic surface effects, text and paste routing, single/focused/peer shell contexts and swaps, Far-style dual-panel rendering, scripted semantic golden frames, deterministic provider cancellation and stale-generation rejection, the surface gallery, the new-folder dialog workflow, and palette action/projection consistency.

## Interactive Evidence

`CARGO_HOME=/tmp/near-cargo cargo run -p near-fm` was launched in a macOS PTY. The application selected terminal color depth from the environment without emitting capability queries, rendered the complete readable 80×24 workspace using the detected low-color/monochrome treatment, displayed and automatically cleared a pending `g → g` sequence at the configured timeout, accepted the F10 escape sequence, exited with status 0, showed the cursor, disabled bracketed paste, and left the alternate screen.

The latest smoke pass also filtered the public menu by typing `term`, activated the terminal gallery surface, routed an unmatched `x` key into its semantic input effect, closed the surface, and exited through F10 with terminal restoration intact. An earlier smoke pass exposed and removed a redundant `Terminal::clear()` call that emitted a terminal cursor-position query; startup no longer depends on a cursor-position response.

## Requirement Status

Every M0 exit requirement is verified: `REQ-CORE-001`, `REQ-CMD-001`, `REQ-KEY-001`, `REQ-KEY-002`, `REQ-THEME-001`, `REQ-UI-001`, `REQ-UI-002`, and `REQ-TEST-001`. Evidence includes deterministic model, render, composition, provider, command, and scripted workflow tests plus live macOS PTY and process-provider smoke runs.

## Remaining Platform Gaps

- Add terminal lifecycle tests for initialization failures, panics, signals, and restoration ordering as part of the M1 terminal-runtime work.
- Implement the M1 local filesystem provider, metadata fidelity, responsive hydration, safe operation planning/execution, shared viewer, and external-tool suspension.

## Next Gate

M0 is complete. M1 now adds the local filesystem provider, safe operation planning/execution, production viewer streaming, external-tool suspension, and deeper macOS integration.
