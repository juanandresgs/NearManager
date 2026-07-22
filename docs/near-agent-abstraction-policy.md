# Agent Abstraction Policy

Near FM drives development of the Near TUI platform. Every discovered behavior must either improve a reusable platform layer or be explicitly retained as application policy.

## Evidence Chain

A durable behavior record in `specs/abstraction-ownership.toml` connects:

1. The invariant and owning layer.
2. The application-specific policy that consumes it.
3. Model-level evidence.
4. Semantic render evidence.
5. Application workflow evidence.
6. Public API and non-Near-FM consumer evidence when required.

`AGENTS.md` defines the required agent workflow. `tools/validate_abstraction_policy.py` checks that the ownership inventory, evidence paths, public-consumer declarations, exceptions, agent instructions, and qualification integration remain valid.

## Completion States

- `implemented`: code and automated evidence exist, but external or operator proof may remain.
- `verified`: the full declared evidence chain, including required public-consumer proof, has passed for the candidate revision.
- `partial`: some declared evidence is incomplete.
- `deprecated`: the behavior is retained only for migration or removal.

An application workflow passing does not promote a reusable behavior to `verified` by itself.

## Exceptions

An application-layer implementation of reusable mechanics requires a dated exception. Exceptions state the affected behavior, reason, owner, review date, and expiry date. Expired exceptions fail validation. Active exceptions are technical debt, not proof of architectural completion.
