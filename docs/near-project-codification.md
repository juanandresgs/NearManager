# Codifying Near for Full Implementation

## Recommendation

Near should use a **layered docs-as-code system**, not a single master specification and not a heavyweight external requirements database.

The layers are:

1. **Product intent and research** — explanatory documents and Far evidence under `docs/` and `assets/`.
2. **Capability breadth model** — the complete problem space in `project/capabilities.toml`.
3. **Normative requirements** — atomic commitments in `project/requirements.toml`.
4. **Architecture model** — arc42 concerns and C4 views in `project/architecture/`.
5. **Decision history** — immutable ADRs in `project/decisions/`.
6. **Executable behavior** — acceptance workflows in `project/workflows.toml`.
7. **Contracts** — versioned TOML, JSON Schema, Rust API, CLI, and WIT definitions.
8. **Delivery allocation** — milestone exit gates in `project/milestones.toml`.
9. **Assurance** — threat model, risks, tests, benchmarks, audits, and release evidence.
10. **Automated traceability** — `tools/validate_project.py` as a required CI gate.

Each layer answers a different question. Combining them into one document would make it hard to review, validate, and evolve; separating them without cross-links would allow drift. Stable IDs and automated references provide the connection.

## Research Findings

### Requirements need atomic records

ISO/IEC/IEEE 29148 treats requirements engineering as a lifecycle of elicitation, specification, validation, and traceability. For Near, the practical adaptation is one independently reviewable requirement per record, with:

- Stable ID.
- Type and normative priority.
- One atomic statement.
- Rationale.
- Capability ownership.
- Target milestone.
- Measurable acceptance criteria.
- Verification method.
- Planned or actual evidence.
- Architecture references.

Uppercase BCP 14 keywords make strength explicit. `MUST` is a delivery commitment, `SHOULD` requires a documented reason to omit, and `MAY` is optional scope. Priority must not be inferred from prose tone.

### Architecture needs multiple views

arc42 is well suited to Near because it explicitly covers goals, constraints, context, strategy, building blocks, runtime, deployment, crosscutting concepts, decisions, quality, risks, and glossary. C4 complements it with consistent zoom levels and diagrams.

Near should maintain only diagrams that answer active questions:

- System context: users, terminals, operating systems, tools, plugins, and release channels.
- Container/crate view: application shell, runtime, renderer, services, extension host, and state.
- Runtime views: command dispatch, async tasks, operation planning, PTY lifecycle, and plugin invocation.
- Deployment views: macOS first, then Linux and Windows variants.
- Trust-boundary/data-flow views: terminal, OS, tools, config, plugins, and distribution.

Code-level diagrams should be generated or written only for genuinely complex subsystems. Manually maintaining class-by-class diagrams would add cost without preserving the intended abstraction.

### Decisions and requirements are different records

A requirement says what must be true. An ADR says why a durable design choice was selected over alternatives. Requirements can survive architecture changes; ADRs preserve the history and consequences of those changes.

Create an ADR when a decision is:

- Expensive to reverse.
- Crosscutting across crates or applications.
- Security or data-safety sensitive.
- A public compatibility commitment.
- A choice among credible alternatives.
- A change to a previously accepted decision.

Do not create one ADR per requirement. A single architectural decision often satisfies several requirements, while straightforward requirements may need only an architecture reference.

### Quality must be scenario-based

Broad adjectives such as fast, reliable, safe, portable, or extensible are not implementable requirements. arc42's quality-scenario guidance recommends context plus measurable acceptance criteria.

Near quality requirements therefore specify observable thresholds or conditions, for example:

- Warm local navigation key-to-render p95 below 16 milliseconds on a documented reference setup.
- No eager stat of every directory item before first paint.
- Exact completed and incomplete resource lists after cancellation.
- Terminal restoration after panic and external-tool handoff.
- Monochrome presentation retains critical focus and safety distinctions.
- Plugin traps do not terminate the host.

Thresholds can change through reviewed requirement updates, but they cannot remain undefined.

### Interface contracts need their own versioning

Project releases, Rust crates, configuration schemas, CLI behavior, and WIT packages are different compatibility surfaces and must not share an implicit versioning policy.

- Rust APIs follow Cargo SemVer guidance and Rust API Guidelines.
- Persisted TOML/JSON records carry schema versions and migrations.
- CLI changes document exit-code, stdout/stderr, and automation compatibility.
- WIT package declarations use full SemVer; unstable items remain gated until stable.
- Command, context, role, provider, and capability IDs are serialized names and therefore compatibility surfaces.

### Security must remain a live architecture concern

NIST SSDF recommends documenting security requirements, risks, design decisions, and evidence. OWASP recommends continuously asking what is being built, what can go wrong, what will be done, and whether the response is adequate.

Near's threat model therefore tracks:

- Assets.
- Trust boundaries.
- Threats and categories.
- Linked requirements.
- Mitigations.
- Status and review triggers.

It is reviewed when execution modes, providers, plugin capabilities, trust boundaries, or release mechanisms change—not only before a security release.

## Why TOML Records

TOML matches the Rust ecosystem, is readable in code review, supports comments, and can be parsed by Python's standard library for CI validation. JSON Schema remains useful as the contract vocabulary and for JSON-based generated forms, but Near's hand-authored project registries benefit from TOML ergonomics.

The validator intentionally checks semantic relationships that generic schema validators cannot:

- Requirement IDs exist and are unique.
- Priority matches normative wording.
- Capabilities have requirement coverage.
- Milestone exit requirements belong to that milestone.
- Workflow requirement links are valid.
- Architecture references exist.
- ADR requirement links resolve.
- Threats and risks link to known requirements, assets, and boundaries.
- Verified requirements point to actual evidence.

If the records later outgrow TOML, they can be generated from a dedicated requirements service because the conceptual schema and IDs are already explicit.

## Traceability Graph

```text
research evidence
      ↓ informs
capability ──covered by──> requirement ──allocated to──> milestone
                              │   │
                   realized by│   └──verified by──> workflow / test / benchmark
                              ↓
                     architecture view
                              │
                       constrained by
                              ↓
                             ADR
                              │
                    mitigates / is exposed to
                              ↓
                         threat and risk
```

Traceability is bidirectional:

- Starting from a capability, reviewers can find all commitments and gaps.
- Starting from a requirement, developers can find its design, milestone, workflow, and evidence.
- Starting from a test, maintainers can identify which requirement it proves.
- Starting from an ADR, reviewers can see the requirements and risks that drove it.

## Breadth Control

The capability taxonomy is the authoritative breadth checklist. It prevents the project from declaring success after building only a visually convincing file manager.

Current breadth domains include:

- Foundation and event model.
- Commands, keymaps, help, histories, and macros.
- Semantic rendering and themes.
- Workspaces and reusable surfaces.
- Resource/provider model.
- macOS filesystem semantics.
- Safe operation planning and execution.
- Viewing and editor integration.
- Search and predicates.
- Shell, external tools, and PTY.
- Configuration and migrations.
- Extensions and capability grants.
- Application suite and third-party facade.
- Cross-platform adaptation.
- Quality, security, delivery, and governance.

Adding a major feature requires deciding whether it fits an existing capability or justifies a new breadth domain. Deleting a capability is an explicit scope decision, not an omission.

## Implementation Workflow

### Before implementation

1. Identify or add the requirement.
2. Confirm acceptance criteria are observable.
3. Link the requirement to a capability and milestone.
4. Add an ADR when the design choice meets the ADR threshold.
5. Add or update workflows for user-visible behavior.
6. Update threat and risk records where trust or data safety changes.

### During implementation

1. Keep domain behavior behind command and provider contracts.
2. Add model, contract, integration, render, and workflow tests as appropriate.
3. Record evidence paths in the requirement.
4. Change status from `accepted` to `implemented` only when code exists.

### Before merge

1. Run project-definition validation.
2. Run focused implementation tests.
3. Confirm public API and schema compatibility.
4. Review architecture and threat impact.
5. Ensure documentation describes effective behavior rather than intended future behavior.

### Before milestone completion

1. Every exit requirement is `verified`.
2. Evidence exists and passes in CI.
3. Golden workflows pass on the milestone's required platforms and terminals.
4. Open risks are accepted with owners or block release.
5. Migration and compatibility notes exist.
6. User-facing and application-author documentation is current.

## Definition of Full Implementation

The intended abstraction is fully implemented only when:

1. Every capability is either delivered or explicitly removed through an accepted scope decision.
2. Every accepted `must` requirement is verified by existing evidence.
3. `near-fm`, `near-view`, and a non-filesystem application share the public runtime.
4. A third-party example uses the platform without importing internal backend crates.
5. Keymaps, themes, commands, help, tasks, configuration, and diagnostics behave consistently across the suite.
6. macOS workflows are complete and Linux/Windows commitments match their declared requirements.
7. Safe operations, external tools, plugins, and releases pass their threat-model controls.
8. Public Rust, CLI, schema, and WIT contracts have compatibility policies and migration evidence.
9. All milestone exit gates and workflow suites pass.
10. The project-definition validator reports no errors and verified records have real evidence.

This is intentionally stricter than “the file manager works.” The goal is the reusable abstraction and cohesive application ecosystem.

## Adopted Repository Structure

```text
project/
├── README.md
├── capabilities.toml
├── requirements.toml
├── milestones.toml
├── workflows.toml
├── risks.toml
├── architecture/
│   └── README.md
├── decisions/
│   ├── 0000-template.md
│   └── 0001-....md
├── security/
│   └── threat-model.toml
├── schemas/
│   └── project-records.schema.json
└── templates/
    └── requirement.toml

tools/
└── validate_project.py
```

This structure is the recommended long-term source of truth. Generated websites, tables, roadmaps, and dashboards should consume it rather than introduce independent copies.

## Sources

- RFC 8174, BCP 14 requirement keywords: <https://www.rfc-editor.org/rfc/rfc8174>
- ISO/IEC/IEEE 29148 requirements engineering: <https://www.iso.org/standard/72089.html>
- arc42 architecture template: <https://arc42.org/overview>
- arc42 quality scenarios: <https://quality.arc42.org/articles/specify-quality-requirements>
- C4 model: <https://c4model.com/>
- Architecture decision records: <https://github.com/joelparkerhenderson/architecture-decision-record>
- JSON Schema 2020-12: <https://json-schema.org/draft/2020-12>
- Rust API Guidelines: <https://rust-lang.github.io/api-guidelines/>
- Cargo SemVer compatibility: <https://doc.rust-lang.org/cargo/reference/semver.html>
- WIT format and version gates: <https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md>
- NIST Secure Software Development Framework: <https://csrc.nist.gov/pubs/sp/800/218/final>
- OWASP threat modeling: <https://owasp.org/www-community/Threat_Modeling>

