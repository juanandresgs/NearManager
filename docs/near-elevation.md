# Privileged Operation Retry

Near handles local permission failures as a retry of an immutable operation plan, not as a new file-operation request.

## User workflow

When a local operation returns `permission denied`, `operation not permitted`, or `access denied`, the workspace retains the plan identifier, authorization generation, confirmation state, and conflict decision. The status line offers `near.operation.retry-elevated`. Invoking it starts a background task and opens the platform-native authorization path:

- macOS: the system administrator-privileges dialog through `osascript`.
- Linux: the configured polkit agent through `pkexec`.
- Windows: User Account Control through PowerShell `Start-Process -Verb RunAs`.

Remote SFTP and archive mutation plans do not cross into the local elevation helper; their provider-specific authorization remains separate.

## Broker protocol

The current process serializes the exact recorded `OperationPlan`, `ExecutionAuthorization`, and `ConflictDecision` into a mode-0600 temporary request. A SHA-256 digest is passed separately to the helper and validated before deserialization.

The elevated `near-fm --elevated-operation REQUEST DIGEST` mode accepts only a regular mode-0600 request with Near's reserved name inside the platform temporary directory. Response and audit paths are derived as private siblings, never accepted from caller data. The helper records an explicit `Elevated` event, executes the exact plan through `LocalOperationBackend`, and writes the itemized `ExecutionSummary`. The unprivileged process appends the temporary audit to the configured journal and removes all broker files.

No shell command, source list, destination, or policy is reconstructed from UI state after failure. Cancellation before launch remains available through the task model; once the native authorization helper owns execution, the final audited summary is authoritative.

## Audit and failure behavior

Normal and elevated operations use separate append-only journals. Elevated records include planning, elevation, start, conflict decisions, item outcomes, and the final summary. Digest mismatch, authorization cancellation, missing platform helpers, malformed requests, and response failures surface as task errors without falling back to an unaudited mutation.
