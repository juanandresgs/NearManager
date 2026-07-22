# Production Autonomy Development Plan

## Trigger

A normal delete attempt against a mounted volume root produced a generic operation preview that described cross-device copy-then-delete and offered Skip, Replace, and Rename decisions even though the operator intended to move an ordinary item to Trash. The operation engine was behaving according to its generic policies, but the product workflow was unsafe and semantically wrong.

This fault demonstrates that component correctness and broad test counts are not sufficient evidence of production readiness. Near needs intent-specific product contracts, platform-boundary classification, operator-visible workflow assertions, and one autonomous qualification run that proves the complete TUI platform and application suite.

## Inferences

### Product intent must constrain generic engines

The reusable operation engine should retain generic planning, journaling, cancellation, and outcome machinery. Each product operation must additionally define a presentation contract:

- operator intent and plain-language title;
- eligible resource classes;
- protected or unsupported resource classes;
- decisions the operator can meaningfully make;
- automatic policies that must not become UI choices;
- recovery and reversibility language;
- success refresh, history, and audit behavior.

Trash is not a generic move dialog. It should preserve existing Trash entries through automatic platform-correct unique naming, explain restore behavior, and never offer replacement of an existing trashed item. Permanent deletion, wipe, archive extraction, remote transfer, attributes, and device removal need their own contracts as well.

### Resource identity needs platform-boundary classification

File-like presentation does not imply file-like mutability. Before planning, Near must classify filesystem roots, mounted volume roots, provider roots, virtual collections, removable devices, special files, symlinks, and ordinary entries. Destructive commands must fail closed when classification is unavailable. Mounted resources should route to an explicit eject or unmount capability when supported.

Classification must use provider identity and native platform metadata, not labels, path shape, or panel position. Display escaping and URI encoding must never change the resource used for execution.

### Tests must assert the operator contract

The current operation suite strongly verifies immutable plans and execution outcomes. It must also verify what the operator sees and can do:

- modal title, body, warnings, decisions, and key hints;
- selected and default action;
- absence of irrelevant or dangerous actions;
- exact source and destination identity;
- resulting provider, filesystem, Trash metadata, journal, history, and panel refresh state;
- behavior under cancellation, stale generations, permissions, collisions, cross-device boundaries, and partial failure.

Every escaped production fault should add both a minimal reproduction and a class-level regression matrix entry.

## Priority Work

### Blocking Tranche D0 — Close the observed deletion fault completely

This tranche is the first work package of the autonomous program and must run as one concentrated flow. The immediate patch rejects local filesystem roots and removes generic conflict controls from Trash previews, but the fault is not considered closed until the complete operator journey and its surrounding failure class are proven.

Reproduce from the original state:

- browse `/Volumes` on macOS;
- focus a mounted volume root such as an external drive;
- invoke the normal delete/Trash command;
- verify that no operation plan or generic operation preview is created;
- verify that the user receives an exact protected-resource explanation and an eject/unmount route only when the device service advertises it.

Implement and verify in this order:

1. Add a durable resource classification result for ordinary entries, filesystem roots, mount roots, provider roots, virtual roots, removable devices, and unsupported special resources.
2. Make Trash, permanent delete, and wipe eligibility consume that classification before plan creation.
3. Route removable devices to the capability-gated disconnect/eject service rather than any filesystem mutation.
4. Replace the temporary operation-kind UI branching with an intent-specific presentation contract that declares title, explanation, recovery, decisions, default action, and denial reason.
5. Implement platform-correct Trash collision naming so existing Trash contents are never replaced and no rename decision is requested.
6. Verify normal files, directories, recursive trees, symlinks, broken links, read-only entries, exact names, cross-device sources, and disappearing sources.
7. Verify filesystem roots, mounted roots, provider roots, virtual collections, and removable devices fail closed before a mutation plan exists.
8. Assert the rendered modal, available commands, default action, filesystem result, Trash metadata, journal, task history, and panel refresh.
9. Add the exact screenshot regression plus a class-level matrix regression to the autonomous qualification suite.
10. Re-run project evidence and Far parity review; do not mark `REQ-OPS-003` verified from the containment patch alone.

Hard exit gate:

- `WF-TRASH-002` passes against a real macOS mounted-volume fixture and portable adapter fixtures;
- no protected resource reaches `OperationPlanner` for Trash, delete, or wipe;
- ordinary Trash collisions preserve both items and restoration metadata;
- no hidden key binding can select a decision omitted by the intent contract;
- the qualification manifest records the deletion matrix as a mandatory release-blocking suite.

Until this gate passes, the autonomous run may build infrastructure and tests but must not declare the operations layer production-grade.

### P0 — Safety and semantic containment

1. Reject filesystem roots, mounted roots, provider roots, and other protected resources before recording Trash, delete, or wipe plans.
2. Replace generic Trash previews with a concise Move to Trash contract and automatic collision-safe naming.
3. Separate device removal from deletion and expose eject or unmount only through advertised capabilities.
4. Audit every mutating command for irrelevant generic conflict controls or misleading recovery language.
5. Add release-blocking workflows for ordinary Trash, collisions, protected roots, stale context, and irreversible actions.

Exit: `WF-TRASH-002` passes on real macOS fixtures and adapter tests encode Linux and Windows expectations.

### P1 — Operation presentation contracts

1. Add an operation-intent presentation descriptor owned above `near-ops` and consumable by any Near application.
2. Make preview surfaces render declared decisions rather than infer controls from generic conflict policy.
3. Add typed eligibility and denial reasons to provider capabilities.
4. Define contracts for copy, move, rename, links, Trash, permanent delete, wipe, archive operations, remote transfers, attributes, and elevated retry.
5. Ensure automation and macros consume the same eligibility and safety contract as interactive UI.

Exit: no application performs raw key or operation-kind branching to invent safety UX, and the descriptor is proven by `near-fm` plus a non-filesystem reference application.

### P1 — Boundary and fault matrix

Build reusable fixtures for:

- normal files, directories, empty and recursive trees;
- symlinks, broken links, hard links, special files, and read-only resources;
- spaces, backslashes, control bytes, Unicode normalization variants, and non-UTF-8 names;
- filesystem roots, mounted volume roots, removable devices, provider roots, and virtual collections;
- existing Trash names, metadata write failure, full destination, permission denial, disappearance, and concurrent replacement;
- same-device rename, cross-device copy-then-delete, cancellation, stale generation, and partial completion.

The matrix must assert model state, semantic render roles, visible text, available commands, execution results, audit records, and refreshed application state.

Exit: `REQ-TEST-002` has checked-in evidence on macOS and portable adapter cases run in CI.

### P1 — Autonomous production qualification

Create one orchestration command that starts from a clean checkout and runs:

1. toolchain and dependency verification;
2. formatting, linting, workspace tests, docs, schema, API, security, and release validators;
3. semantic model and render workflows at supported terminal sizes and color depths;
4. real filesystem and platform safety suites;
5. terminal protocol and PTY compatibility fixtures;
6. Near FM, Near View, Near Proc, and non-filesystem reference application journeys;
7. representative Far workflow journeys and honest parity reconciliation;
8. release builds, smoke tests, artifact hashing, and evidence manifest generation.

The manifest should record revision, dirty state, Rust toolchain, dependency lock digest, OS and architecture, terminal capabilities, platform adapter availability, tests and workflows executed, performance budgets, known degradations, artifact digests, and final gate status.

Exit: `WF-QUALIFY-001` runs without developer repair and blocks release on critical or high safety failures.

### P2 — Prove the TUI library

Use the fault-derived contracts to strengthen the abstraction proof:

- scenes must support semantic match, warning, decision, disabled, and destructive roles without application-specific widgets;
- surfaces must declare commands and available decisions without raw-key handling;
- application code supplies intent and resources while shared runtime handles focus, overlays, input, accessibility, snapshots, and task state;
- the same workflow runner must drive file, viewer, process, and non-filesystem applications;
- public APIs must avoid local-path, dual-panel, Ratatui, and Crossterm assumptions unless explicitly adapter-specific.

Exit: the autonomous qualification manifest demonstrates the same public contracts across all reference applications.

## Governance Follow-up

- Keep `REQ-OPS-003`, `REQ-TEST-002`, and `REQ-REL-002` pending until their complete acceptance criteria have evidence.
- Treat a bug that exposes an impossible operator choice as a safety defect, not only a visual defect.
- Reassess verified requirements when field faults contradict the claimed operator workflow, even if their internal engine clauses remain tested.
- Require each parity upgrade to cite an operator-level workflow, not only a primitive or unit test.
- Preserve exact fault screenshots and reproduction steps in evidence when they reveal a new systemic class.

## Recommended Execution Order

1. Execute Blocking Tranche D0 from reproduction through its hard exit gate.
2. Inventory all remaining mutating workflows and define presentation contracts.
3. Build the reusable boundary and fault fixture library in `near-testkit`.
4. Implement the autonomous qualification orchestrator and manifest, with D0 mandatory.
5. Run a fresh Far parity and requirement evidence audit.
6. Complete terminal/input fidelity and the typed settings platform.
7. Complete shell, viewer, and editor policy on top of those shared services.
8. Prove the public TUI contracts across every reference application.
9. Only then run unattended production hardening against all remaining accepted requirements.
