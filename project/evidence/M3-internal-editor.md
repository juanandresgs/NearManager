# M3 Internal Editor Evidence

`REQ-EDIT-001` is verified by the provider write contract, local filesystem replacement implementation, reusable full-screen editor surface, semantic command/keymap integration, and end-to-end workspace tests.

## Contract Evidence

- `ResourceProvider::write` defaults to an explicit unsupported result, so read-only and virtual providers fail closed.
- `LocalFileProvider::write` rejects opened-version mismatches, stages exact replacement bytes, checks cancellation before commit, and reports provider-scoped errors.
- `EditorSurface` never converts a `Location` into a native path and opens only when `resource.write` is advertised.

## Interaction Evidence

- `F4` opens `surface.editor` by default; `Alt+F4` exposes external handlers as a separate semantic command.
- Each open resource retains an independent document, undo stack, selection, dirty state, and cursor while panels or other editor screens are active.
- `F12` lists panels and every editor screen; `Ctrl+Tab` and `Ctrl+Shift+Tab` cycle them without discarding state.
- `EditorPositionStore` persists provider/location scoped row, column, and viewport positions through the application data root.
- Application quit fails closed while any retained editor has unsaved content and identifies each blocking document.
- Editing covers navigation, insertion, newline, indentation, deletion, terminal paste, undo, redo, literal search, stream selection, select-all, and editor-local cut/copy/paste.
- Search and replace use explicit prompt stages so normal Enter and Tab keys confirm fields without mutating the document.
- Regex mode supports numbered and named capture expansion, replace-all, invalid-pattern diagnostics, and optional case/style preservation.
- Find All renders a navigable result list and activates the exact source row and column.
- Shift movement creates stream blocks; Alt+Shift movement creates rectangular blocks whose copy, cut, and padded paste preserve columns across unequal line lengths.
- `editor.toml` provides a layered, versioned `persistent_blocks` default, while `near.editor.toggle-persistent-blocks` changes active-session behavior immediately.
- `Ctrl+S` persists through the provider. Dirty state remains until success, and the first close on dirty content arms an explicit discard warning.
- `F2` saves the current format; `Shift+F2` and `Ctrl+Shift+S` open Save As with provider location, UTF-8/UTF-16/Latin-1, BOM, EOL, create/replace, and lossy-conversion fields.
- External version conflicts expose reload, line comparison, and explicit keep-local overwrite commands.
- The editor uses `SurfacePresentation::FullScreen`, semantic scene roles, registered commands, and binding-derived function hints.

## Automated Verification

- `editor::tests::edits_undoes_redoes_searches_and_saves_through_the_provider`
- `editor::tests::stream_selection_supports_copy_cut_and_paste`
- `editor::tests::shift_selection_and_persistent_blocks_follow_far_movement_semantics`
- `editor::tests::column_selection_copies_cuts_and_pastes_rectangles`
- `editor::tests::editor_settings_are_versioned_and_configure_persistent_blocks`
- `editor::tests::find_and_replace_prompts_confirm_through_normal_editor_keys`
- `editor::tests::regex_groups_style_preservation_and_invalid_patterns_are_safe`
- `editor::tests::find_all_results_are_navigable_and_activate_source_positions`
- `editor::tests::dirty_editor_requires_a_second_close_before_discarding`
- `editor::tests::save_as_writes_selected_encoding_bom_and_line_endings`
- `editor::tests::lossy_save_requires_explicit_confirmation`
- `editor::tests::external_change_supports_compare_reload_and_keep_local`
- `local_provider::tests::provider_write_replaces_file_contents_and_honors_cancellation`
- `local_provider::tests::provider_write_rejects_an_external_version_change`
- `workspace::tests::filesystem_provider_drives_navigation_parent_and_view_workflows`
- `workspace::tests::editor_screens_retain_documents_and_restore_closed_positions`
- `workspace::tests::dirty_editor_sessions_block_application_quit`
- `workspace::tests::editor_save_as_and_external_change_choices_run_end_to_end`
- `workspace::tests::editor_save_as_refuses_lossy_output_until_confirmed`
- `local_provider::tests::editor_position_store_round_trips_provider_locations`
- `workspace::tests::every_shipped_binding_resolves_to_a_registered_command`
