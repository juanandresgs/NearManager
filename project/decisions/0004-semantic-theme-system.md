# ADR-0004: Theme Semantic Roles Instead of Widgets

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-THEME-001, REQ-ACCESS-001
- Supersedes: none
- Superseded by: none

## Context

Coordinate- or widget-specific colors cannot create a cohesive suite and do not degrade safely across terminal palettes.

## Decision

Render with namespaced semantic roles and documented fallback chains. Themes define styles, glyph sets, and density tokens. Applications add roles only for meaningful domain distinctions and provide core fallbacks.

## Consequences

### Positive

- One theme applies across unrelated applications.
- Far's selected/cursor state matrix can be preserved semantically.
- Accessibility and low-color fallback are testable.

### Negative

- Role naming becomes a compatibility surface.
- Theme validation and fallback diagnostics are required.

## Verification

Multi-theme and monochrome snapshots must preserve focus, selection, warnings, and validation state.

