# M3 Persistent History Evidence

Date: 2026-06-23

## Implemented Slice

- Command, folder, viewed-file, and edited-file histories persist through explicit store contracts and atomic local TOML documents.
- Alt+F8 opens command history, Alt+F11 opens the viewed/edited history chooser, and Alt+F12 opens folder history.
- Every history surface supports incremental filtering. Command, folder, view, and edit entries can be locked; clearing removes unlocked entries while preserving locked records.
- Viewed and edited entries retain provider-scoped resource identity, display labels, use counts, and the latest unavailable-provider diagnostic.
- `history.toml` configures independent unlocked-entry limits for command, folder, and resource histories through the standard layered configuration system.

## Verification

- `workspace::tests::persistent_command_history_filters_locks_and_restores_entries`
- `workspace::tests::ten_folder_shortcuts_and_searchable_history_persist_and_navigate`
- `workspace::tests::viewed_and_edited_histories_persist_filter_lock_clear_and_reopen`
- `near_local_fs::tests::resource_history_store_round_trips_retention_locks_and_errors`
- `near_local_fs::tests::folder_navigation_store_round_trips_shortcuts_history_and_errors`
