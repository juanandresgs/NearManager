# ADR-0001: Use Schema-Checked Docs-as-Code Governance

- Status: accepted
- Date: 2026-06-23
- Owners: project maintainers
- Requirements: REQ-GOV-001
- Supersedes: none
- Superseded by: none

## Context

The initial research and blueprint are comprehensive but prose alone cannot prove coverage, detect broken references, or connect scope to tests and release gates.

## Decision Drivers

- Requirements must remain close to implementation.
- Traceability must be automatable without adopting a heavyweight proprietary requirements tool.
- Architecture decisions need durable history.

## Considered Options

1. Continue with narrative Markdown only.
2. Adopt an external requirements-management service.
3. Keep narrative documentation and add versioned TOML records, ADRs, schemas, and repository validation.

## Decision

Use option 3. Markdown communicates architecture and rationale. TOML registries define atomic requirements, capabilities, milestones, and workflows. CI validates IDs, references, normative language, and coverage.

## Consequences

### Positive

- Reviewable with ordinary Git workflows.
- Requirements can generate reports and gates later.
- The repository remains usable offline and tool-neutral.

### Negative

- Maintainers must update multiple linked records for architectural changes.
- The custom validator becomes maintained project infrastructure.

### Follow-up

- Add code ownership and a required validation status check when the repository is published.
- Generate human-readable traceability reports once implementation paths exist.

## Verification

`python3 tools/validate_project.py` must pass and report no uncovered capabilities or broken milestone/workflow links.

