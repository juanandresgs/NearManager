# Security policy

## Reporting a vulnerability

Please report vulnerabilities through GitHub's private vulnerability-reporting flow for this repository. Do not open a public issue for an unpatched vulnerability.

Include the affected revision, platform, reproduction steps, expected impact, and any suggested mitigation. Reports are acknowledged as capacity permits; no response-time or disclosure deadline is guaranteed while Near remains pre-release.

## Supported versions

Only the latest tagged release and the current `main` branch receive security fixes. Older pre-release builds are unsupported.

Near can invoke external tools, connect to remote systems, load workspace configuration, and run extensions. Review prompts, trust boundaries, and capability grants before using those features with untrusted files or repositories.
