# Near Confirmation Policy

Near separates immutable operation planning from the user policy that decides whether a recorded plan must open a preview before execution.

## Configuration

`near-fm` loads the first available policy from:

1. `NEAR_CONFIRMATIONS`, interpreted as an explicit file path.
2. `~/Library/Application Support/near/confirmations.toml` on macOS.
3. The shipped `specs/confirmations.toml` safe default.

The document is versioned and rejects unknown fields:

```toml
schema = 1

[confirmations]
reversible = "preview"
confirmable = "preview"
destructive = "preview"
privileged = "preview"
high_impact = "preview"
```

`reversible` and `confirmable` may be set to `"execute"` for an expert profile. The operation is still planned, recorded, generation-checked, journaled, and executed through the normal task runtime; only the modal preview is skipped.

## Mandatory Floors

`destructive`, `privileged`, and `high_impact` must remain `"preview"`. A configuration that changes any of them to `"execute"` fails closed at startup. Unknown future `SafetyClass` values also require preview by default.

A high-impact plan cannot dispatch on its first execute action. The first Enter arms the preview and changes its prompt; a second Enter emits the explicit high-impact authorization. The operation engine independently rejects stale generations, ordinary missing confirmation, and missing high-impact confirmation.

This arrangement keeps policy universal and safety-class based rather than embedding file-manager command names in the platform contract.
