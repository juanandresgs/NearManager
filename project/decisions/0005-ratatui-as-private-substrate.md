# ADR-0005: Keep Ratatui and Crossterm as Private Substrates

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-UI-001, REQ-API-001
- Supersedes: none
- Superseded by: none

## Context

Ratatui provides excellent buffers and layout, while Crossterm provides broad terminal I/O. Neither defines Near's command, focus, theme, task, or provider semantics.

## Decision

Use Ratatui and Crossterm internally behind Near terminal, scene, and surface APIs. Public application and plugin contracts do not expose their types.

## Consequences

### Positive

- Near benefits from mature terminal crates without inheriting them as permanent public APIs.
- Test backend and rendering implementation can evolve independently.

### Negative

- Near must maintain adapters and its own semantic render types.
- Some third-party Ratatui widgets require adaptation rather than direct use.

## Verification

Public example applications must compile without direct Ratatui or Crossterm dependencies.

