# ADR-0007: Use Tiered Configuration, Process, and Wasm Extensions

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-PLUGIN-001, REQ-PLUGIN-002, REQ-PLUGIN-003
- Supersedes: none
- Superseded by: none

## Context

Near needs simple customization, multi-language extensibility, failure isolation, explicit authority, and long-lived interface compatibility.

## Decision

Use four tiers: data-only configuration, isolated process extensions, capability-controlled WebAssembly components with versioned WIT, and compile-time native Rust crates for trusted first-party code. Native dynamic libraries are not the public ABI.

## Consequences

### Positive

- Simple customizations avoid plugin complexity.
- Process and Wasm failures do not corrupt the host.
- WIT provides typed cross-language contracts and semantic versions.

### Negative

- Multiple hosting mechanisms require consistent command/provider adapters.
- Wasmtime increases binary size and startup/resource considerations.
- Plugin APIs must remain smaller than internal Rust APIs.

## Verification

M4 verification includes process crash and Wasm trap isolation, canonical-ABI denied/granted capability tests, and immutable WIT and process-protocol compatibility checks.
