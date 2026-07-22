# Contributing to Near

Near is pre-release software. Bug reports, focused fixes, portability results, and API feedback are welcome.

## Development setup

Install Rust 1.88 or newer, clone the repository, and run:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 tools/validate_project.py
python3 tools/validate_abstraction_policy.py
```

See `AGENTS.md` before changing user-visible behavior. Every durable behavior must have an ownership classification in `specs/abstraction-ownership.toml`, and reusable behavior needs owner-layer, visible-render, application, and public-consumer evidence as applicable.

Keep pull requests narrow, update `CHANGELOG.md` for user-visible changes, and include the exact tests run. Do not commit secrets, private transcripts, qualification state under `.near/`, or third-party media without an explicit redistribution license.

By contributing, you agree that your contribution is licensed under either the Apache License 2.0 or the MIT License, at the recipient's option.
