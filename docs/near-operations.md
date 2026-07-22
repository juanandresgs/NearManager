# Near Operation Planning and Execution

Near models filesystem mutation as a provider-independent plan followed by an explicitly authorized execution. The UI never asks a filesystem backend to mutate a path directly.

## Architecture

- `near-ops` owns immutable operation plans, policies, plan storage, authorization, conflict decisions, exact outcome summaries, cancellation, retry, and append-only journaling.
- `near-local-fs` translates exact-byte local resources into plans and executes approved plans with macOS-aware Trash and recovery behavior.
- `near-archive` translates cross-provider copy intents into extraction or create/update plans while preserving the same preview, conflict, cancellation, authorization, and journal contracts.
- `near-sftp` translates local↔SFTP and SFTP↔SFTP copy or move intents into bounded recursive transfer plans with the same preview, conflict, cancellation, authorization, and journal contracts.
- `near-ui` renders operation previews and sends only recorded plan identifiers to the operation service.
- `near-fm` composes the local planner, backend, and persistent journal without exposing local filesystem types to reusable surfaces.

## Plan Contract

Every `OperationPlan` records:

- A stable `OperationId` consumed by execution.
- Operation kind and resolved item targets.
- Source resources and recursive intent.
- Conflict, metadata, verification, recovery, cross-device, and symlink policies.
- Safety classification, context generation, and high-impact classification.
- A preview projection suitable for terminal surfaces and tests.

Plan fields are private after construction. Execution looks up the recorded plan by identifier and rejects stale generations, missing confirmation, and missing high-impact confirmation.

## Supported Local Mutations

The local planner and backend currently support copy, move, rename, hard links, symbolic links, Trash, permanent delete, directory creation, touch, and Unix mode changes. Cross-device moves are represented explicitly as copy-then-delete rather than hidden behind a generic move label.

The Rename command opens provider-planned in-place rename from the command palette or resource menu. `Shift+F6` follows Far Manager and moves only the current item to the peer panel, ignoring any broader selection. A single rename starts with its exact current name; multiple selected resources start with `{stem}_{index}{dotext}`. Templates support `{name}`, `{stem}`, `{ext}`, `{dotext}`, and a configurable `{index}` origin. Empty, reserved, path-containing, and duplicate generated targets are rejected before planning. The immutable preview lists every source-to-target mapping, marks existing-target conflicts, exposes skip/replace/rename decisions, and records backup recovery policy before execution.

Alt+F6 opens explicit hard-link, symbolic-link, or directory-junction creation for one source. The operation service validates the source type before recording a plan: hard links require regular files and junction equivalents require directories. Symbolic links retain provider-resolved target metadata after panel refresh; Windows directory links use the platform directory-symlink path while Unix uses a directory symbolic link.

Ctrl+A opens one attributes, ownership, and timestamps dialog for the current or selected resources. Portable read-only state works across local platforms; Unix exposes octal mode plus owner and group IDs. Modified and accessed times accept Unix milliseconds or `now`. Recursive requests expand every descendant into an individual immutable plan item, never follow symbolic-link directories, and show the exact item count before execution. Unsupported Unix-only fields fail during planning on other platforms, and mixed success/failure summaries retain each resource outcome.

Replacement preserves a recovery backup before mutation. Trash is the default reversible deletion path, while permanent recursive deletion is classified as high impact.

## Conflict and Outcome Model

Conflict decisions support skip, replace, rename, and cancel. A decision can apply to one item or all remaining conflicts. Execution records each item as completed, skipped, failed, or pending and retains exact messages for inspection and retry.

Cancellation is checked between items and during recursive local copies. Unstarted items remain pending in the final summary. The journal records the immutable plan, decisions, and finished summary to memory or an append-only file.

Local permission failures retain the same plan identifier, authorization, and conflict decision for an explicit platform-native retry. The privileged helper digest-checks the serialized plan, records an elevation event in a separate append-only journal, and returns the normal itemized summary. See `docs/near-elevation.md`.

## UI Workflow

In `near-fm`, operation commands always create a recorded plan. `ConfirmationPolicy` decides whether lower-impact plans open `OperationPreviewSurface` or proceed directly through the same authorized execution path. The surface displays policies, safety information, and exact source-to-target mappings, allows conflict-policy selection, and requires a separate second execute action for high-impact plans. Successful execution refreshes provider-backed panels from the current locations.

## Runtime Behavior

Execution runs on the bounded `near-runtime` worker pool. Confirmation returns to the input loop immediately, the Tasks surface shows running and final state, cancellation reaches the operation engine, and completion preserves exact item outcomes. Confirmation is enforced by safety class, generation, and high-impact flags. Runtime policy discovery and mandatory safety floors are documented in `docs/near-confirmation-policy.md`.

## Verification

Tests cover the full mutation matrix, immutable plan-ID execution, stale-context rejection, ordinary and high-impact confirmation, Trash, recovery backups, copy-then-delete previews, source-to-target projections, one-item and remaining-item conflict scopes, cancellation summaries, append-only journals, precise failures, retry construction, the end-to-end F5 preview/execute/refresh workflow, single plus selected-resource rename, current-only Shift+F6 move, typed Alt+F6 links, preflight source validation, refreshed symbolic-link targets, recursive Ctrl+A expansion, permissions, ownership, timestamps, and itemized partial attribute failures.
