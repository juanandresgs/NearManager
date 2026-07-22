# M3 Typed Handler Evidence

Date: 2026-07-05

## Implemented Slice

- `near-handlers` provides schema-versioned ordered rules, shared resource predicates, typed argv atoms, explicit shell templates, and complete rule diagnostics.
- View, edit, and execute actions expose every matching rule in configured order while retaining the first match as the default.
- `ExternalInvocation` records whether execution is structured argv or explicit shell, and explained resolution carries the stable handler ID.
- `LocalExternalToolResolver` adapts exact macOS paths and metadata into the generic handler registry.
- `near-fm` loads user or shipped handler documents and displays the handler ID and invocation mode before suspend-and-run.
- `near.handler.diagnostics` exposes selected and rejected rules through the normal command system and command palette.
- F9 File Associations lists ordered alternatives, mode labels, and resolves one selected handler by stable ID.
- Enter on a non-container resource resolves `ExternalAction::Open`; it no longer opens the internal viewer. F3 and F4 remain independent View and Edit paths.
- Shipped macOS, Linux, and Windows handler documents select Launch Services, `xdg-open`, and the Windows shell association respectively for Open.

## Automated Evidence

- Hostile paths containing whitespace, newlines, shell metacharacters, quotes, and non-UTF-8 bytes remain one exact argv item.
- Explicit shell tests prove substitutions are quoted, unknown placeholders fail closed, and shell mode is marked in both the invocation and explanation.
- Predicate/action tests prove rule selection and rejected-rule reasons are deterministic.
- The version-1 TOML fixture round trips and validates duplicate IDs and predicate compatibility.
- Workspace tests prove handler diagnostics render the selected rule and explicit shell mode is visible before external handoff.
- Association tests prove execute alternatives preserve configured order, named selection invokes the requested handler, and structured argv versus explicit shell mode remains visible.
- Workspace tests prove Enter queues the Open handler, F3 still renders the internal viewer, and a missing Open resolver is denied instead of silently falling back.
- Local-adapter tests prove all three platform documents keep Open separate from text View/Edit handlers.
- The tmux/PTTY binary workflow uses a safe Open fixture and proves Enter performs suspend-and-run rather than rendering viewer content.
- Existing Vim and Neovim PTY tests continue proving terminal restoration through the configured handler path.

## Requirement Status

`REQ-HANDLER-001` is verified by typed exact-byte arguments, explicit visible shell opt-in, predicate-driven Open/View/Edit separation, versioned platform configuration, and inspectable diagnostics. `FAR-AUTO-002` remains partial until representative native applications are observed on the macOS and Linux operator matrix.
