# M1 Confirmation Policy Evidence

Date: 2026-06-23

## Implemented Slice

- `ConfirmationPolicy` parses a schema-versioned, unknown-field-denying TOML contract.
- Reversible and confirmable previews are configurable; destructive, privileged, high-impact, and unknown future safety classes fail closed.
- `near-fm` loads an explicit `NEAR_CONFIRMATIONS` path, then the macOS application-support policy, then the shipped safe default.
- Preview bypass does not bypass planning: the workspace executes the previously recorded immutable plan identifier through the same generation checks, authorization, task runtime, journal, and exact outcome path.
- High-impact previews require two explicit execute actions and send a separate `high_impact_confirmed` authorization bit only after arming.

## Automated Evidence

- Policy unit tests prove an expert profile can skip reversible and confirmable previews while destructive and high-impact plans remain modal.
- Invalid policy tests prove mandatory destructive safeguards cannot be disabled.
- The real local Trash workflow proves a reversible operation can execute without a preview under explicit expert policy and still refresh provider panels after completion.
- The operation-preview test proves the first high-impact Enter only arms the surface and the second emits explicit authorization.
- Existing operation-engine tests prove default Trash preference, stale-context rejection, ordinary confirmation enforcement, and recursive permanent-delete high-impact enforcement.

## Requirement Status

- `REQ-SEC-001` is verified. Configurability is bounded by mandatory irreversible-operation floors, and every acceptance clause has direct automated evidence.
