# Near Agent Development Contract

These instructions apply to the entire repository.

## Product And Platform Rule

Near FM is the primary proving application for the Near TUI platform. It is not the default owner of reusable interaction mechanics.

For every user-visible feature, defect, parity change, or workflow correction, classify the behavior before implementation as exactly one of:

- `core-contract`
- `terminal-adapter`
- `runtime`
- `reusable-surface`
- `application-composition`
- `application-policy`
- `platform-adapter`

Record or update that classification in `specs/abstraction-ownership.toml` when the work introduces or changes a durable behavior or invariant.

## Mandatory Abstraction Harvest

When work in an application reveals a general invariant, implement the invariant at the lowest reusable layer that can own it without application assumptions.

Examples of reusable invariants include:

- Cursor, selection, viewport, paging, scrolling, focus, and mouse-to-item mapping.
- Semantic render roles and visible state distinctions.
- Commands, contexts, tasks, dialogs, menus, help, keymaps, terminal lifecycle, and restoration.
- Provider-neutral resource and operation contracts.

Application code may own bindings, composition, workflow policy, Far compatibility, and domain-specific decisions. It must not become the sole implementation site for reusable mechanics merely because the defect was first observed in Near FM.

If behavior remains application-local, document why it is application policy in the ownership record.

## Required Evidence

Reusable behavior is not complete until all applicable evidence exists:

1. A model-level regression at the owning layer.
2. A semantic render or visible-workflow regression.
3. An application integration regression.
4. Public API exposure through `near-app` or another published crate when consumers need it.
5. A non-Near-FM consumer or `NearTuiProof` exercise for significant public behavior.
6. Platform capability or degradation declarations when terminal or OS behavior differs.

A `FarWorkspace` test alone cannot prove a reusable abstraction complete.

## Change Audit

Before editing:

1. State the observed behavior and root invariant.
2. Identify the owning layer and application policy separately.
3. Identify the reusable and application regressions that will prove the result.
4. Check whether public consumer proof must change.

Before claiming completion:

1. Run `python3 tools/validate_abstraction_policy.py`.
2. Run `python3 tools/check_public_api.py` when published crates or application-facing contracts changed.
3. Run the focused owner-layer, render, and application workflow tests.
4. Run the applicable qualification profile.
5. Report any evidence that remains operator-only, external, degraded, or pending.

## Prohibited Completion Claims

Do not describe work as platform-complete, parity-verified, production-ready, or publicly reusable when:

- The behavior exists only in an application coordinator such as `FarWorkspace`.
- The ownership record is missing or stale.
- Only internal model state was asserted without visible semantic output.
- The public consumer proof is absent for a significant new public abstraction.
- Required discovery or operator evidence remains pending.

## Architecture Exceptions

Exceptions must be explicit records in `specs/abstraction-ownership.toml` with a reason, owner, review date, and expiry date. Never silently preserve an application-layer implementation because extraction is inconvenient.
