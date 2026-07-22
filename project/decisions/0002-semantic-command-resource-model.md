# ADR-0002: Center the Platform on Commands, Resources, and Surfaces

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-CORE-001, REQ-CMD-001, REQ-RES-001
- Supersedes: none
- Superseded by: none

## Context

Far's panels and hotkeys are valuable, but directly cloning them would produce a file manager framework rather than a universal TUI application platform.

## Decision Drivers

- The same interaction language must support files, processes, Git, archives, logs, and future domains.
- Keys, menus, macros, tests, and plugins need one invocation model.
- Presentation must not own domain behavior.

## Considered Options

1. Widget callbacks and application-specific models.
2. File-operation-centric framework.
3. Semantic commands operating on provider-neutral resources through surfaces.

## Decision

Use option 3. Commands are stable invocable behavior, resources are provider-neutral domain objects, and surfaces present state while delegating command dispatch to the runtime.

## Consequences

### Positive

- Non-filesystem applications use the same runtime.
- Commands become discoverable, scriptable, testable, and bindable.
- Providers and surfaces can evolve independently.

### Negative

- More up-front modeling than direct callbacks.
- Command context and capability design require discipline.

## Verification

`near-proc` and a third-party example must use the same contracts without filesystem assumptions or raw key matching.

