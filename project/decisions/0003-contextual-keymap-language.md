# ADR-0003: Use a Contextual Command Keymap Language

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-KEY-001, REQ-KEY-002
- Supersedes: none
- Superseded by: none

## Context

Hard-coded keys make help stale, macros fragile, and applications inconsistent. Global key maps cannot express dialogs, panels, viewers, and application-specific modes safely.

## Decision

Use a layered context stack and trie-based resolver. Bindings reference command IDs and typed arguments, support sequences and inheritance, and retain their configuration origin. All hints and help derive from the effective map.

## Consequences

### Positive

- Rebinding is complete and discoverable.
- Far-style function keys and optional Vim-like sequences can coexist as profiles.
- Conflicts are diagnosable.

### Negative

- Sequence timing and text-input precedence require careful design.
- Terminal protocol limitations must be surfaced.

## Verification

`WF-REBIND-001` and deterministic resolver tests must pass for legacy and enhanced key fixtures.

