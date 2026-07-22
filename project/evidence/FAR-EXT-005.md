# FAR-EXT-005 — Remote and network providers

## Implementation

- `near-sftp` provides real libssh2 SFTP sessions, paged resource listings, metadata, bounded streams, writes, profile roots, and provider capabilities.
- Profiles reject unknown credential fields. Authentication is exclusively delegated to the platform SSH agent and connection establishment requires a matching OpenSSH `known_hosts` entry.
- `SftpOperationService` plans local↔SFTP and SFTP↔SFTP copy or move requests through the shared immutable preview, conflict, authorization, cancellation, execution-summary, and journal contracts.
- Files transfer in bounded chunks; recursive directories remain provider-addressed; move removes a source only after successful copy.
- Generic provider disconnect and reconnect commands retain panel location and rows. Retry refreshes the same panel generation.

## Verification

- `near_sftp::tests::navigation_reads_and_reconnect_preserve_provider_locations`
- `near_sftp::tests::immutable_plans_copy_and_move_between_local_and_sftp_resources`
- `near_sftp::tests::catalog_rejects_plaintext_credentials_duplicate_ids_and_unsafe_roots`
- `near_sftp::tests::profile_root_cannot_be_escaped_by_a_forged_location`
- `near_ui::workspace::tests::provider_disconnect_and_retry_preserve_panel_location_and_refresh_state`
