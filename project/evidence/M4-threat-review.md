# M4 Extension Threat Review

Date: 2026-06-23
Owner: project maintainers
Next review: 2026-09-23

The Component Model and isolated-process execution modes cross `TRUST-PLUGIN`. They introduce component bytes, executables, manifests, declared capabilities, user grants, workspace trust, guest-controlled strings and JSON, package paths, and resource-limit settings as attacker-controlled inputs.

Mitigations implemented in this review are: no WASI linkage; exact allowed import/export names; explicit declared-and-granted host calls; separate workspace trust; relative artifacts that must canonicalize inside their package; bounded reads and protocol documents; fresh stores and child processes; memory/object ceilings; fuel; epoch and process deadlines; trap/crash isolation; cleared process environments; deny-default macOS Seatbelt rules; versioned WIT and JSON protocols; immutable WIT compatibility baselines; and structured diagnostics. Hostile process fixtures prove external filesystem, environment, network, and child-execution denial.

Residual risks are Wasmtime vulnerabilities, denial of service during component compilation, maliciously large package collections, package-private data exposure to process extensions, and Apple's deprecated `sandbox-exec` implementation disappearing or changing behavior. Owners must review Wasmtime advisories continuously, add package-size and compilation concurrency limits before stable distribution, replace or revalidate the process launcher before declaring it stable, and revisit this record by the review date.
