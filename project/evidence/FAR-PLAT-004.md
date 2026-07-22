# FAR-PLAT-004 — Elevation and privileged operation retry

## Implementation

- `near-ui` detects itemized permission failures and offers `near.operation.retry-elevated` while retaining the original plan identifier, authorization, and conflict decision.
- `near-local-fs` provides native macOS, Linux, and Windows elevation launchers plus a private request/response broker whose privileged paths are confined to validated temporary siblings.
- The request contains the exact serialized immutable plan and is checked against a separately supplied SHA-256 digest before privileged execution.
- `near-fm` exposes only a hidden helper mode for the broker and writes elevated activity to a dedicated append-only journal.
- `near-ops` records an explicit `Elevated` event and preserves the normal started, decision, item, and finished audit sequence.
- Archive and SFTP plans explicitly reject local elevation and delegate only unknown local plans through the operation-service chain.

## Verification

- `near_local_fs::tests::elevation_broker_receives_the_exact_recorded_plan`
- `near_ui::workspace::tests::permission_failure_offers_exact_plan_elevation_retry`
- `near_ui::workspace::tests::internal_viewer_replaces_the_workspace_full_screen`
- Full workspace tests and release validators.
