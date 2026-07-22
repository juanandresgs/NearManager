#![allow(clippy::needless_borrows_for_generic_args, clippy::too_many_lines)]

use std::sync::Arc;

#[cfg(feature = "plugins")]
use std::fmt::Write as _;

mod config;
mod platform;
#[cfg(feature = "plugins")]
mod plugins;

use near_archive::{ArchiveOperationService, ZipArchiveProvider};
use near_core::{Location, RemovableDeviceService};
use near_handlers::UserMenuCatalog;
use near_local_fs::{
    DescribedLocalFileProvider, DescriptionSettings, LocalClipboard, LocalCommandHistoryStore,
    LocalCommandLineArgumentResolver, LocalCommandLineExecutor, LocalEditorPositionStore,
    LocalExternalToolResolver, LocalFileProvider, LocalFolderNavigationStore,
    LocalOperationService, LocalResourceHistoryStore, LocalStateDocumentStore,
    LocalViewerStateStore, PlatformElevationBroker, PlatformRemovableDeviceService,
    execute_elevated_request,
};
#[cfg(target_os = "macos")]
use near_local_fs::{execute_native_restore_helper, execute_native_trash_helper};
use near_macros::{MacroDocument, TomlMacroStore};
use near_ops::OperationJournal;
use near_pty::ShellProfile;
use near_reference_providers::RemovableDeviceProvider;
use near_sftp::{SftpConnectionDocument, SftpOperationService, SftpProvider};
use near_ui::{
    CollectionSurface, ConfirmationPolicy, EditorSettings, FarWorkspace, FilterCatalog,
    HighlightingCatalog, HistorySettings, InterfaceSettings, Keymap, PanelModeCatalog,
    SemanticTheme, ViewerSettings, run_workspace,
};
#[cfg(target_os = "macos")]
use std::{io::Write as _, os::unix::ffi::OsStrExt as _};

const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
const THEME: &str = include_str!("../../../specs/theme.toml");
const THEME_TERMINAL_NATIVE: &str = include_str!("../../../specs/theme-terminal-native.toml");
const THEME_HIGH_CONTRAST: &str = include_str!("../../../specs/theme-high-contrast.toml");
const CONFIRMATIONS: &str = include_str!("../../../specs/confirmations.toml");
#[cfg(target_os = "macos")]
const HANDLERS: &str = include_str!("../../../specs/handlers.toml");
#[cfg(target_os = "linux")]
const HANDLERS: &str = include_str!("../../../specs/handlers-linux.toml");
#[cfg(windows)]
const HANDLERS: &str = include_str!("../../../specs/handlers-windows.toml");
const MACROS: &str = include_str!("../../../specs/macros.toml");
const PANEL_MODES: &str = include_str!("../../../specs/panel-modes.toml");
const EDITOR: &str = include_str!("../../../specs/editor.toml");
const HISTORY: &str = include_str!("../../../specs/history.toml");
const INTERFACE: &str = include_str!("../../../specs/interface.toml");
const HIGHLIGHTING: &str = include_str!("../../../specs/highlighting.toml");
const USER_MENU: &str = include_str!("../../../specs/user-menu.toml");
const DESCRIPTIONS: &str = include_str!("../../../specs/descriptions.toml");
const FILTERS: &str = include_str!("../../../specs/filters.toml");
const CONNECTIONS: &str = include_str!("../../../specs/connections.toml");
const SHELL: &str = include_str!("../../../specs/shell.toml");
const VIEWER: &str = include_str!("../../../specs/viewer.toml");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        let raw_os_arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if raw_os_arguments
            .first()
            .is_some_and(|argument| argument == "--near-native-trash-helper")
        {
            let source = raw_os_arguments
                .get(1)
                .map(std::path::PathBuf::from)
                .ok_or("missing native Trash source path")?;
            let target = execute_native_trash_helper(&source)?;
            std::io::stdout().write_all(target.as_os_str().as_bytes())?;
            return Ok(());
        }
        if raw_os_arguments
            .first()
            .is_some_and(|argument| argument == "--near-native-restore-helper")
        {
            let source = raw_os_arguments
                .get(1)
                .map(std::path::PathBuf::from)
                .ok_or("missing native Restore source path")?;
            let target = raw_os_arguments
                .get(2)
                .map(std::path::PathBuf::from)
                .ok_or("missing native Restore target path")?;
            let restored = execute_native_restore_helper(&source, &target)?;
            std::io::stdout().write_all(restored.as_os_str().as_bytes())?;
            return Ok(());
        }
    }
    let raw_arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "--version")
    {
        println!("near-fm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "--elevated-operation")
    {
        let request = raw_arguments
            .get(1)
            .ok_or("missing elevated request path")?;
        let digest = raw_arguments
            .get(2)
            .ok_or("missing elevated request digest")?;
        execute_elevated_request(request, digest)?;
        return Ok(());
    }
    let arguments = config::ConfigArguments::parse(std::env::args_os().skip(1))?;
    if arguments.help {
        println!("{}", config::ConfigArguments::usage());
        return Ok(());
    }
    let roots =
        platform::ProfileRoots::resolve(arguments.config_root.clone(), arguments.data_root.clone());
    if let Some(transfer) = &arguments.transfer {
        println!(
            "{}",
            platform::transfer_profile(transfer, &roots, &config::DOCUMENTS)?
        );
        return Ok(());
    }
    let keymap_document = config::resolve_document("keymap.toml", KEYMAP, &arguments)?;
    let theme_document = config::resolve_document("theme.toml", THEME, &arguments)?;
    let confirmation_document =
        config::resolve_document("confirmations.toml", CONFIRMATIONS, &arguments)?;
    let handler_document = config::resolve_document("handlers.toml", HANDLERS, &arguments)?;
    let macro_document = config::resolve_document("macros.toml", MACROS, &arguments)?;
    let panel_mode_document =
        config::resolve_document("panel-modes.toml", PANEL_MODES, &arguments)?;
    let editor_document = config::resolve_document("editor.toml", EDITOR, &arguments)?;
    let history_document = config::resolve_document("history.toml", HISTORY, &arguments)?;
    let interface_document = config::resolve_document("interface.toml", INTERFACE, &arguments)?;
    let highlighting_document =
        config::resolve_document("highlighting.toml", HIGHLIGHTING, &arguments)?;
    let user_menu_document = config::resolve_document("user-menu.toml", USER_MENU, &arguments)?;
    let description_document =
        config::resolve_document("descriptions.toml", DESCRIPTIONS, &arguments)?;
    let filter_document = config::resolve_document("filters.toml", FILTERS, &arguments)?;
    let connection_document =
        config::resolve_document("connections.toml", CONNECTIONS, &arguments)?;
    let shell_document = config::resolve_document("shell.toml", SHELL, &arguments)?;
    let viewer_document = config::resolve_document("viewer.toml", VIEWER, &arguments)?;
    let setting_provenance = [
        (
            "keymap.sequence_timeout_ms",
            &keymap_document,
            "settings.sequence_timeout_ms",
        ),
        (
            "keymap.show_pending_sequence",
            &keymap_document,
            "settings.show_pending_sequence",
        ),
        (
            "keymap.prefer_physical_keys",
            &keymap_document,
            "settings.prefer_physical_keys",
        ),
        (
            "confirmations.reversible",
            &confirmation_document,
            "confirmations.reversible",
        ),
        (
            "confirmations.confirmable",
            &confirmation_document,
            "confirmations.confirmable",
        ),
        ("panel-modes.left", &panel_mode_document, "defaults.left"),
        ("panel-modes.right", &panel_mode_document, "defaults.right"),
        ("viewer.wrap", &viewer_document, "wrap"),
        ("viewer.hex", &viewer_document, "hex"),
        ("viewer.detect_binary", &viewer_document, "detect_binary"),
        ("viewer.encoding", &viewer_document, "encoding"),
        ("viewer.open_policy", &viewer_document, "open_policy"),
        (
            "viewer.remember_per_resource",
            &viewer_document,
            "remember_per_resource",
        ),
        (
            "viewer.remember_position",
            &viewer_document,
            "remember_position",
        ),
        (
            "viewer.remember_bookmarks",
            &viewer_document,
            "remember_bookmarks",
        ),
        (
            "viewer.remember_encoding",
            &viewer_document,
            "remember_encoding",
        ),
        (
            "viewer.remember_view_mode",
            &viewer_document,
            "remember_view_mode",
        ),
        (
            "editor.persistent_blocks",
            &editor_document,
            "persistent_blocks",
        ),
        ("editor.expand_tabs", &editor_document, "expand_tabs"),
        ("editor.tab_size", &editor_document, "tab_size"),
        ("editor.open_policy", &editor_document, "open_policy"),
        (
            "history.command_max_unlocked",
            &history_document,
            "command_max_unlocked",
        ),
        (
            "history.folder_max_unlocked",
            &history_document,
            "folder_max_unlocked",
        ),
        (
            "history.resource_max_unlocked",
            &history_document,
            "resource_max_unlocked",
        ),
        (
            "interface.show_status_line",
            &interface_document,
            "show_status_line",
        ),
        ("interface.show_keybar", &interface_document, "show_keybar"),
        (
            "interface.tree_indent_width",
            &interface_document,
            "tree_indent_width",
        ),
        (
            "interface.menu_wrap_navigation",
            &interface_document,
            "menu_wrap_navigation",
        ),
        (
            "interface.dialog_wrap_focus",
            &interface_document,
            "dialog_wrap_focus",
        ),
        (
            "interface.command_line_completion",
            &interface_document,
            "command_line_completion",
        ),
        (
            "interface.startup_panel",
            &interface_document,
            "startup_panel",
        ),
        ("shell.program", &shell_document, "program"),
        ("shell.mode", &shell_document, "mode"),
        ("shell.startup_command", &shell_document, "startup_command"),
        ("shell.arguments", &shell_document, "arguments"),
        ("shell.close_policy", &shell_document, "close_policy"),
        (
            "shell.inherit_environment",
            &shell_document,
            "inherit_environment",
        ),
    ]
    .into_iter()
    .filter_map(|(id, document, field)| {
        document
            .setting_provenance(field)
            .map(|provenance| (id.to_owned(), provenance))
    })
    .collect::<Vec<_>>();
    let macros: MacroDocument = toml::from_str(&macro_document.text)?;
    macros.validate()?;
    let panel_modes = PanelModeCatalog::from_toml(&panel_mode_document.text)?;
    let theme = SemanticTheme::from_toml(&theme_document.text)?;
    let theme_presets = [
        SemanticTheme::from_toml(THEME_TERMINAL_NATIVE)?,
        SemanticTheme::from_toml(THEME_HIGH_CONTRAST)?,
    ];
    #[cfg(feature = "plugins")]
    let discovery = plugins::discover(&roots.data);
    #[cfg(feature = "plugins")]
    let plugin_diagnostics = discovery.diagnostics.as_str();
    #[cfg(not(feature = "plugins"))]
    let plugin_diagnostics = "Plugin capability was not linked into this build";
    let configuration_diagnostics = [
        keymap_document.diagnostics.as_str(),
        theme_document.diagnostics.as_str(),
        confirmation_document.diagnostics.as_str(),
        handler_document.diagnostics.as_str(),
        macro_document.diagnostics.as_str(),
        panel_mode_document.diagnostics.as_str(),
        editor_document.diagnostics.as_str(),
        history_document.diagnostics.as_str(),
        interface_document.diagnostics.as_str(),
        highlighting_document.diagnostics.as_str(),
        plugin_diagnostics,
        user_menu_document.diagnostics.as_str(),
        description_document.diagnostics.as_str(),
        filter_document.diagnostics.as_str(),
        connection_document.diagnostics.as_str(),
    ]
    .join("\n\n");
    #[cfg(feature = "plugins")]
    let mut configuration_diagnostics = configuration_diagnostics;
    let description_settings = DescriptionSettings::from_toml(&description_document.text)?;
    let provider = Arc::new(DescribedLocalFileProvider::new(
        description_settings.clone(),
    ));
    let sftp_provider = Arc::new(SftpProvider::new(SftpConnectionDocument::from_toml(
        &connection_document.text,
    )?)?);
    let device_service: Arc<dyn RemovableDeviceService> = Arc::new(PlatformRemovableDeviceService);
    let device_provider = Arc::new(RemovableDeviceProvider::new(Arc::clone(&device_service)));
    let current = std::env::current_dir()?;
    let home = platform::home_directory().unwrap_or_else(|| current.clone());
    let workspace = FarWorkspace::new(
        empty_collection(
            LocalFileProvider::location(&current),
            "near-fm.left",
            "Current",
        ),
        empty_collection(LocalFileProvider::location(&home), "near-fm.right", "Home"),
    )
    .with_provider(provider)
    .with_provider(Arc::new(ZipArchiveProvider))
    .with_provider(Arc::clone(&sftp_provider) as Arc<dyn near_core::ResourceProvider>)
    .with_provider(device_provider)
    .with_removable_device_service(device_service)
    .with_confirmation_policy(ConfirmationPolicy::from_toml(&confirmation_document.text)?)
    .with_external_tool_resolver(LocalExternalToolResolver::from_toml(
        &handler_document.text,
    )?)
    .with_user_menus(UserMenuCatalog::from_toml(&user_menu_document.text)?)
    .with_command_line_executor(LocalCommandLineExecutor)
    .with_command_line_argument_resolver(LocalCommandLineArgumentResolver)
    .with_theme_presets(theme.clone(), theme_presets);
    let mut workspace = with_local_state(workspace, &roots.data)
        .with_history_settings(HistorySettings::from_toml(&history_document.text)?)
        .with_interface_settings(InterfaceSettings::from_toml(&interface_document.text)?)
        .with_highlighting(HighlightingCatalog::from_toml(&highlighting_document.text)?)
        .with_filters(FilterCatalog::from_toml(&filter_document.text)?)
        .with_editor_settings(EditorSettings::from_toml(&editor_document.text)?)
        .with_viewer_settings(ViewerSettings::from_toml(&viewer_document.text)?)
        .with_setting_provenance(setting_provenance)
        .with_panel_modes(panel_modes)
        .with_initial_listings()
        .with_macros(macros.macros)
        .with_embedded_pty(std::env::var_os("NEAR_EMBEDDED_PTY").is_none_or(|value| value != "0"))
        .with_shell_profile(ShellProfile::from_toml(&shell_document.text)?)
        .with_operation_service(SftpOperationService::new(
            ArchiveOperationService::new(
                LocalOperationService::platform_default(OperationJournal::append_file(
                    operation_journal_path(&roots.data),
                ))
                .with_elevation_broker(PlatformElevationBroker::new(
                    elevated_operation_journal_path(&roots.data).to_string_lossy(),
                ))
                .with_description_settings(description_settings),
                OperationJournal::append_file(archive_journal_path(&roots.data)),
            ),
            sftp_provider,
            OperationJournal::append_file(sftp_journal_path(&roots.data)),
        ));
    if let Some(path) = config::writable_document_path("macros.toml", &arguments) {
        workspace = workspace.with_macro_store(TomlMacroStore::new(path));
    }
    if let Some(store) = config::AtomicSettingsDocumentStore::new(&arguments) {
        workspace = workspace.with_settings_document_store(store);
    }
    #[cfg(feature = "plugins")]
    {
        for provider in discovery.providers {
            if let Err(error) = workspace.register_provider(provider) {
                let _ = write!(
                    configuration_diagnostics,
                    "\nPlugin provider registration failed: {error}"
                );
            }
        }
        for extension in discovery.extensions {
            if let Err(error) = workspace.register_extension(extension) {
                let _ = write!(
                    configuration_diagnostics,
                    "\nPlugin command registration failed: {error}"
                );
            }
        }
    }
    let keymap = Keymap::from_toml(&keymap_document.text)?;
    workspace = workspace
        .with_keymap_document(keymap_document.text, *keymap.settings())
        .with_configuration_diagnostics(configuration_diagnostics);
    workspace.initialize_shell_dock();
    run_workspace(workspace, &theme, keymap)?;
    Ok(())
}

fn with_local_state(workspace: FarWorkspace, data_root: &std::path::Path) -> FarWorkspace {
    workspace
        .with_command_history_store(LocalCommandHistoryStore::new(command_history_path(
            data_root,
        )))
        .with_folder_navigation_store(LocalFolderNavigationStore::new(folder_navigation_path(
            data_root,
        )))
        .with_editor_position_store(LocalEditorPositionStore::new(editor_position_path(
            data_root,
        )))
        .with_viewer_state_store(LocalViewerStateStore::new(viewer_state_path(data_root)))
        .with_clipboard(LocalClipboard)
        .with_resource_history_store(LocalResourceHistoryStore::new(resource_history_path(
            data_root,
        )))
        .with_state_document_store(LocalStateDocumentStore::new(data_root))
}

fn operation_journal_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("operations.log")
}

fn archive_journal_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let _ = std::fs::create_dir_all(data_root);
    data_root.join("archive-operations.log")
}

fn elevated_operation_journal_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let _ = std::fs::create_dir_all(data_root);
    data_root.join("elevated-operations.log")
}

fn sftp_journal_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let _ = std::fs::create_dir_all(data_root);
    data_root.join("sftp-operations.log")
}

fn command_history_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("command-history.toml")
}

fn folder_navigation_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("folder-navigation.toml")
}

fn editor_position_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("editor-positions.toml")
}

fn viewer_state_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("viewer-state.toml")
}

fn resource_history_path(data_root: &std::path::Path) -> std::path::PathBuf {
    let directory = data_root;
    let _ = std::fs::create_dir_all(&directory);
    directory.join("resource-history.toml")
}

fn empty_collection(location: Location, id: &str, title: &str) -> CollectionSurface {
    CollectionSurface::new(id, "workspace.panel", title, location, Vec::new())
}
