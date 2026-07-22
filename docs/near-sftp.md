# Near SFTP Provider

Near exposes SSH File Transfer Protocol resources as normal provider-backed panel collections. The implementation uses the `ssh2` SFTP subsystem for paged navigation, metadata, bounded reads and writes, and copy or move plans between local and SFTP resources or between SFTP profiles.

## Security Model

Near never accepts or stores passwords, private keys, key passphrases, or bearer tokens in `connections.toml`. Unknown profile fields are rejected, so adding a `password` field fails closed.

Authentication is delegated to the platform OpenSSH agent. Private key and passphrase custody therefore remains with the operating-system credential path configured by the user, such as macOS OpenSSH/Keychain integration, the Windows OpenSSH Authentication Agent service, or the user's Linux SSH agent. Near receives only the result of the agent authentication attempt.

Every profile must name an OpenSSH `known_hosts` file. Connection establishment fails when the host is absent, its key mismatches, or the file cannot be read. Near never implements trust-on-first-use and never writes host keys automatically.

Profile roots are security boundaries. Provider locations cannot navigate above the configured root, including forged `sftp://` locations containing parent components.

## Configuration

`connections.toml` is layered through the normal configuration engine and can be overridden with `--connections FILE` or `NEAR_CONNECTIONS`. Workspace configuration remains subject to the existing workspace-trust policy.

```toml
[[connection]]
id = "production"
label = "Production server"
host = "files.example.com"
port = 22
username = "deploy"
root = "/srv/application"
known_hosts = "/Users/alex/.ssh/known_hosts"
```

The profile ID forms provider locations such as `sftp://production/srv/application`. The location chooser lists configured profiles without opening a connection. The first list, stat, view, edit, or transfer request establishes the session lazily.

## Navigation and Recovery

SFTP panels use the same parent navigation, selection, view, edit, search, history, and stale-generation rules as every other collection provider.

`near.provider.disconnect` closes the active session without changing the panel location, cursor, or rows. `near.provider.retry` authenticates again and refreshes that retained location. A retryable connection failure also causes one automatic reconnect before the provider returns a visible error.

## Transfers

Local-to-SFTP, SFTP-to-local, and SFTP-to-SFTP copy and move requests enter the shared immutable operation-plan, preview, conflict, confirmation, cancellation, and journal workflow. File contents move in bounded 256 KiB chunks. Directories recurse through provider listings, and moves delete the source only after its copy succeeds. SFTP operations are recorded separately in `sftp-operations.log`.

The transfer backend rejects resources from unrelated providers rather than converting virtual locations into native paths. Symbolic-link transfer is rejected by plan policy until an explicit portable link policy is selected.

## Upstream Contract

The provider relies on the documented `ssh2` session, SSH-agent, known-host, and SFTP APIs:

- <https://docs.rs/ssh2/0.9.5/ssh2/struct.Session.html>
- <https://docs.rs/ssh2/0.9.5/ssh2/struct.KnownHosts.html>
- <https://docs.rs/ssh2/0.9.5/ssh2/struct.Sftp.html>
