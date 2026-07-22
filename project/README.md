# Near Project Definition System

This directory is the normative implementation contract for Near. The longer documents under `docs/` explain intent and design; this directory turns that intent into stable, reviewable, and mechanically checked project records.

## Authority Order

When records disagree, resolve them in this order:

1. Accepted requirements in `requirements.toml`.
2. Accepted architecture decision records under `decisions/`.
3. Versioned interface and configuration contracts under `../specs/`.
4. Architecture views under `architecture/`.
5. Roadmap and milestone allocation in `milestones.toml`.
6. Explanatory documents under `../docs/`.

Contradictions are defects. Do not silently choose whichever document is convenient.

## Normative Language

The words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are interpreted as described by BCP 14 when written in uppercase. Requirements use exactly one leading strength corresponding to their `priority` field.

## Files

- `capabilities.toml` — breadth model defining everything the platform must eventually support.
- `requirements.toml` — atomic functional, quality, constraint, security, and governance requirements.
- `milestones.toml` — ordered delivery gates and requirement allocation.
- `workflows.toml` — executable-style acceptance scenarios derived from Far and Near use cases.
- `architecture/README.md` — arc42-oriented architecture record and C4 views.
- `decisions/` — immutable architecture decision records.
- `schemas/` — machine-readable shapes for project records.
- `../tools/validate_project.py` — structural, reference, coverage, and traceability validation.

## Record Lifecycle

### Requirements

`draft` → `accepted` → `implemented` → `verified`

- `draft`: under discussion; implementation must not depend on it as a commitment.
- `accepted`: normative scope approved for implementation.
- `implemented`: code exists but required evidence is incomplete.
- `verified`: acceptance criteria are backed by the listed evidence.
- `deprecated`: retained for history and linked to its replacement.

Changing the meaning of an accepted requirement requires an ADR or a superseding requirement. IDs are never reused.

### Decisions

`proposed` → `accepted` → `superseded` or `deprecated`

An ADR records context, considered options, decision, consequences, and requirement links. Accepted ADR text is not rewritten to reflect a later choice; a new ADR supersedes it.

## Traceability Rule

Every accepted requirement must link to:

- At least one capability.
- Exactly one target milestone or `continuous`.
- At least one architecture or decision reference.
- One or more measurable acceptance criteria.
- A verification method and planned evidence.

Every capability must have requirements, every milestone gate must reference requirements, and every workflow must verify requirements. The validator enforces these relationships.

## Change Workflow

1. Add or modify a requirement record.
2. Add an ADR if the change is architecturally significant, costly to reverse, security-sensitive, or changes a published contract.
3. Update affected workflows and milestone gates.
4. Update versioned schemas or WIT/Rust contracts when applicable.
5. Run `python3 tools/validate_project.py`.
6. Require review from the owners of requirements, architecture, and the affected implementation area.

Branch protection must require the `test`, `Portability (ubuntu-latest)`, and `Portability (windows-latest)` checks. `CODEOWNERS` routes normative project records to project maintainers and release/security workflows to their respective owners. The pull-request template makes same-change traceability explicit.

## Why This Structure

The system combines complementary practices rather than forcing one framework to do everything:

- ISO/IEC/IEEE 29148-style atomic and traceable requirements.
- BCP 14 normative language.
- arc42 sections for architecture communication and quality scenarios.
- C4 abstractions for system, container, component, runtime, and deployment views.
- ADRs for costly or durable decisions.
- JSON Schema-inspired versioned record contracts.
- Rust/Cargo semantic-versioning rules for published crates.
- WIT package versions and feature gates for future component interfaces.
- NIST SSDF and continuous threat modeling for security requirements and evidence.

## Research Sources

- BCP 14 / RFC 8174: <https://www.rfc-editor.org/rfc/rfc8174>
- ISO/IEC/IEEE 29148: <https://www.iso.org/standard/72089.html>
- arc42 template: <https://arc42.org/overview>
- arc42 quality scenarios: <https://quality.arc42.org/articles/specify-quality-requirements>
- C4 model: <https://c4model.com/>
- Architecture decision records: <https://github.com/joelparkerhenderson/architecture-decision-record>
- JSON Schema 2020-12: <https://json-schema.org/draft/2020-12>
- Rust API Guidelines: <https://rust-lang.github.io/api-guidelines/>
- Cargo SemVer compatibility: <https://doc.rust-lang.org/cargo/reference/semver.html>
- WIT format and version gates: <https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md>
- NIST SSDF: <https://csrc.nist.gov/pubs/sp/800/218/final>
- OWASP threat modeling: <https://owasp.org/www-community/Threat_Modeling>
