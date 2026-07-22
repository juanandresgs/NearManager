# M1 Operation Safety Evidence

Date: 2026-06-23

## Implemented Slice

- `near-ops` provides immutable plans, stable operation identifiers, policy snapshots, authorization, conflict resolution, cancellation, exact summaries, retry, and append-only journals.
- `near-local-fs` plans and executes copy, move, rename, hard-link, symbolic-link, Trash, permanent delete, create-directory, touch, and Unix-mode mutations.
- `near-ui` provides a backend-independent operation preview surface and executes recorded plan identifiers rather than direct filesystem calls.
- Shift+F6 opens single or selected-resource multi-rename. Template expansion is deterministic, duplicate and unsafe targets fail before planning, and previews list exact source-to-target mappings, conflicts, and backup recovery before execution.
- Alt+F6 explicitly selects hard, symbolic, or junction-equivalent links. Provider planning rejects hard-link directories and junction files before execution, while a real symbolic-link workflow refreshes the panel with `ResourceKind::Symlink` and its resolved target metadata.
- Ctrl+A plans portable read-only changes, Unix mode and ownership, modified/accessed timestamps, and optional recursion. Recursive descendants appear as exact preview items; real execution verifies metadata fields and a two-item test preserves one completed and one failed outcome after a source disappears.
- `near-ui` provides a typed confirmation policy with mandatory destructive floors and two-step high-impact authorization.
- `near-fm` wires local operations to a persistent journal under `~/Library/Application Support/near/operations.log`.
- Real filesystem tests replace an existing rename target through an explicit conflict decision, then rename two selected resources with indexed templates and verify refreshed panel and filesystem state.
- `near-fm` loads confirmation policy from `NEAR_CONFIRMATIONS`, macOS application support, or the shipped safe default.

## Automated Evidence

- The local mutation matrix proves that every required mutation family executes from a recorded plan.
- Plan snapshots include resolved targets, policy groups, safety class, context generation, and explicit copy-then-delete cross-device behavior.
- Execution rejects stale context generations, unconfirmed plans, and recursive permanent deletion without high-impact confirmation.
- Conflict tests prove decisions can apply once or to all remaining conflicts.
- Cancellation tests preserve exact completed and pending counts and persist a finished journal record.
- Fault injection preserves exact failed resources and produces retry plans containing only retryable outcomes.
- Local replacement preserves a recovery backup; Trash moves into the configured Trash directory; permanent local deletion is separately tested.
- The Far F5 workflow proves plan preview, confirmation, execution by plan identifier, and provider-panel refresh.
- The expert-policy Trash workflow proves configurable preview bypass still uses recorded plans, background execution, and provider refresh.
- Policy and preview tests prove destructive safeguards cannot be disabled and high-impact execution requires a separate second action.

## Requirement Status

- `REQ-OPS-001` is verified: all acceptance mutations use immutable plans, copy-then-delete is visible in previews, and execution consumes a recorded `OperationId`.
- `REQ-OPS-002` is verified: cancellation, conflict scopes, exact outcomes, journaling, inspection, retry, background execution, task state, and input responsiveness are tested.
- `REQ-SEC-001` is verified: Trash preference, stale-context rejection, configurable lower-impact confirmation, mandatory destructive floors, and explicit high-impact authorization are tested.

## Remaining M1 Work

- Add richer per-item progress events while retaining the current indeterminate running state and exact completion summary.
- Add crash-recovery replay and richer journal serialization once the task runtime is established.
