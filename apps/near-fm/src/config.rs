use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use near_config::{
    ConfigEngine, ConfigLayer, ConfigLayerKind, ConfigMigration, ConfigOrigin, EffectiveConfig,
    SettingProvenance,
};
use near_ui::SettingsDocumentStore;

static SETTINGS_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub const DOCUMENTS: [&str; 16] = [
    "keymap.toml",
    "theme.toml",
    "confirmations.toml",
    "handlers.toml",
    "macros.toml",
    "panel-modes.toml",
    "editor.toml",
    "history.toml",
    "interface.toml",
    "highlighting.toml",
    "user-menu.toml",
    "descriptions.toml",
    "filters.toml",
    "connections.toml",
    "shell.toml",
    "viewer.toml",
];

#[derive(Default)]
pub struct ConfigArguments {
    overrides: BTreeMap<String, PathBuf>,
    pub config_root: Option<PathBuf>,
    pub data_root: Option<PathBuf>,
    pub transfer: Option<crate::platform::ProfileTransfer>,
    pub trust_workspace: bool,
    pub help: bool,
}

impl ConfigArguments {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            let Some(argument) = argument.to_str() else {
                return Err("configuration arguments must be valid Unicode".to_owned());
            };
            match argument {
                "--help" | "-h" => parsed.help = true,
                "--trust-workspace" => parsed.trust_workspace = true,
                "--config-root" | "--data-root" | "--portable" | "--export-profile"
                | "--import-profile" => {
                    let path = PathBuf::from(
                        args.next()
                            .ok_or_else(|| format!("{argument} requires a path"))?,
                    );
                    match argument {
                        "--config-root" => parsed.config_root = Some(path),
                        "--data-root" => parsed.data_root = Some(path),
                        "--portable" => {
                            let roots = crate::platform::ProfileRoots::portable(path);
                            parsed.config_root = Some(roots.config);
                            parsed.data_root = Some(roots.data);
                        }
                        "--export-profile" => {
                            set_transfer(
                                &mut parsed.transfer,
                                crate::platform::ProfileTransfer::Export(path),
                            )?;
                        }
                        "--import-profile" => {
                            set_transfer(
                                &mut parsed.transfer,
                                crate::platform::ProfileTransfer::Import(path),
                            )?;
                        }
                        _ => unreachable!(),
                    }
                }
                "--keymap" | "--theme" | "--confirmations" | "--handlers" | "--macros"
                | "--panel-modes" | "--editor" | "--history" | "--highlighting" | "--user-menu"
                | "--descriptions" | "--filters" | "--connections" | "--interface" | "--shell"
                | "--viewer" => {
                    let path = args
                        .next()
                        .ok_or_else(|| format!("{argument} requires a path"))?;
                    parsed.overrides.insert(
                        format!("{}.toml", argument.trim_start_matches("--")),
                        PathBuf::from(path),
                    );
                }
                _ => return Err(format!("unknown near-fm argument: {argument}")),
            }
        }
        if std::env::var_os("NEAR_TRUST_WORKSPACE").is_some_and(|value| value == "1") {
            parsed.trust_workspace = true;
        }
        for (document, variable) in [
            ("keymap.toml", "NEAR_KEYMAP"),
            ("theme.toml", "NEAR_THEME"),
            ("confirmations.toml", "NEAR_CONFIRMATIONS"),
            ("handlers.toml", "NEAR_HANDLERS"),
            ("macros.toml", "NEAR_MACROS"),
            ("panel-modes.toml", "NEAR_PANEL_MODES"),
            ("editor.toml", "NEAR_EDITOR"),
            ("history.toml", "NEAR_HISTORY"),
            ("interface.toml", "NEAR_INTERFACE"),
            ("highlighting.toml", "NEAR_HIGHLIGHTING"),
            ("user-menu.toml", "NEAR_USER_MENU"),
            ("descriptions.toml", "NEAR_DESCRIPTIONS"),
            ("filters.toml", "NEAR_FILTERS"),
            ("connections.toml", "NEAR_CONNECTIONS"),
            ("shell.toml", "NEAR_SHELL_PROFILE"),
            ("viewer.toml", "NEAR_VIEWER"),
        ] {
            if !parsed.overrides.contains_key(document)
                && let Some(path) = std::env::var_os(variable)
            {
                parsed.overrides.insert(document.to_owned(), path.into());
            }
        }
        Ok(parsed)
    }

    pub fn usage() -> &'static str {
        "usage: near-fm [--portable DIR | --config-root DIR --data-root DIR] [--export-profile DIR | --import-profile DIR] [--keymap FILE] [--theme FILE] [--confirmations FILE] [--handlers FILE] [--macros FILE] [--panel-modes FILE] [--editor FILE] [--viewer FILE] [--history FILE] [--interface FILE] [--highlighting FILE] [--user-menu FILE] [--descriptions FILE] [--filters FILE] [--connections FILE] [--shell FILE] [--trust-workspace]"
    }

    fn override_path(&self, document: &str) -> Option<&PathBuf> {
        self.overrides.get(document)
    }
}

pub struct ResolvedDocument {
    pub text: String,
    pub diagnostics: String,
    pub origins: BTreeMap<String, ConfigOrigin>,
}

impl ResolvedDocument {
    pub fn setting_provenance(&self, field: &str) -> Option<SettingProvenance> {
        self.origins.get(field).map(|origin| SettingProvenance {
            layer: origin.layer,
            source: origin.source.clone(),
        })
    }
}

pub fn resolve_document(
    document: &str,
    built_in: &str,
    arguments: &ConfigArguments,
) -> Result<ResolvedDocument, Box<dyn std::error::Error>> {
    if !DOCUMENTS.contains(&document) {
        return Err(format!("unknown configuration document: {document}").into());
    }
    let mut layers = vec![ConfigLayer::new(
        ConfigLayerKind::BuiltIn,
        format!("<built-in>/{document}"),
        built_in,
    )];
    let platform = crate::platform::system_config_root().join(document);
    push_file_layer(&mut layers, ConfigLayerKind::Platform, 0, &platform, false)?;
    if let Some(root) = config_root(arguments) {
        let plugins = root.join("plugins");
        if let Ok(entries) = std::fs::read_dir(plugins) {
            let mut defaults = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path().join(document))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            defaults.sort();
            for (priority, path) in defaults.iter().enumerate() {
                push_file_layer(
                    &mut layers,
                    ConfigLayerKind::Plugin,
                    u32::try_from(priority).unwrap_or(u32::MAX),
                    path,
                    false,
                )?;
            }
        }
        push_file_layer(
            &mut layers,
            ConfigLayerKind::User,
            0,
            &root.join(document),
            false,
        )?;
    }
    let workspace = std::env::current_dir()?.join(".near").join(document);
    push_file_layer(
        &mut layers,
        ConfigLayerKind::Workspace,
        0,
        &workspace,
        arguments.trust_workspace,
    )?;
    if let Some(path) = arguments.override_path(document) {
        push_file_layer(&mut layers, ConfigLayerKind::Cli, 0, path, true)?;
    }
    let effective = config_engine(document).resolve(layers)?;
    Ok(ResolvedDocument {
        text: toml::to_string_pretty(effective.value())?,
        diagnostics: diagnostics(document, &effective),
        origins: effective.origins().clone(),
    })
}

fn config_engine(document: &str) -> ConfigEngine {
    if document == "macros.toml" {
        ConfigEngine::new(2).with_migration(ConfigMigration::new(1, 2, |mut value| {
            let Some(table) = value.as_table_mut() else {
                return Err("macro configuration root must be a table".to_owned());
            };
            table.insert("schema".to_owned(), toml::Value::Integer(2));
            Ok(value)
        }))
    } else {
        ConfigEngine::default()
    }
}

fn push_file_layer(
    layers: &mut Vec<ConfigLayer>,
    kind: ConfigLayerKind,
    priority: u32,
    path: &std::path::Path,
    trusted: bool,
) -> Result<(), std::io::Error> {
    if !path.is_file() {
        return Ok(());
    }
    let layer = ConfigLayer::new(
        kind,
        path.display().to_string(),
        std::fs::read_to_string(path)?,
    )
    .with_priority(priority);
    layers.push(if trusted { layer.trusted() } else { layer });
    Ok(())
}

fn config_root(arguments: &ConfigArguments) -> Option<PathBuf> {
    arguments
        .config_root
        .clone()
        .or_else(crate::platform::user_config_root)
}

pub fn writable_document_path(document: &str, arguments: &ConfigArguments) -> Option<PathBuf> {
    arguments
        .override_path(document)
        .cloned()
        .or_else(|| config_root(arguments).map(|root| root.join(document)))
}

pub struct AtomicSettingsDocumentStore {
    paths: BTreeMap<String, (PathBuf, ConfigLayerKind)>,
}

impl AtomicSettingsDocumentStore {
    pub fn new(arguments: &ConfigArguments) -> Option<Self> {
        let paths = [
            "keymap.toml",
            "confirmations.toml",
            "panel-modes.toml",
            "viewer.toml",
            "editor.toml",
            "history.toml",
            "interface.toml",
            "shell.toml",
        ]
        .into_iter()
        .filter_map(|document| {
            writable_document_path(document, arguments).map(|path| {
                let layer = if arguments.override_path(document).is_some() {
                    ConfigLayerKind::Cli
                } else {
                    ConfigLayerKind::User
                };
                (document.to_owned(), (path, layer))
            })
        })
        .collect::<BTreeMap<_, _>>();
        (!paths.is_empty()).then_some(Self { paths })
    }
}

impl SettingsDocumentStore for AtomicSettingsDocumentStore {
    fn load(&self, document: &str) -> Result<Option<String>, String> {
        let (path, _) = self
            .paths
            .get(document)
            .ok_or_else(|| format!("{document} has no readable configuration layer"))?;
        match std::fs::read_to_string(path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn persist(&self, document: &str, contents: &str) -> Result<(), String> {
        let (path, _) = self
            .paths
            .get(document)
            .ok_or_else(|| format!("{document} has no writable configuration layer"))?;
        atomic_write(path, contents.as_bytes()).map_err(|error| error.to_string())
    }

    fn provenance(&self, document: &str) -> Option<SettingProvenance> {
        self.paths
            .get(document)
            .map(|(path, layer)| SettingProvenance {
                layer: *layer,
                source: path.display().to_string(),
            })
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let sequence = SETTINGS_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.near-tmp-{}-{sequence}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("settings"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        OpenOptions::new().read(true).open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn set_transfer(
    current: &mut Option<crate::platform::ProfileTransfer>,
    transfer: crate::platform::ProfileTransfer,
) -> Result<(), String> {
    if current.is_some() {
        Err("only one profile import or export action may be requested".to_owned())
    } else {
        *current = Some(transfer);
        Ok(())
    }
}

fn diagnostics(document: &str, effective: &EffectiveConfig) -> String {
    let mut lines = vec![format!("{document} effective values:")];
    lines.extend(effective.origins().iter().map(|(field, origin)| {
        format!(
            "{field} <- {}:{}:{} ({:?}){}",
            origin.source,
            origin.line,
            origin.column,
            origin.layer,
            origin
                .migrated_from
                .map_or_else(String::new, |version| format!(
                    " migrated from schema {version}"
                ))
        )
    }));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_configuration_overrides_are_explicit() {
        let parsed = ConfigArguments::parse([
            OsString::from("--theme"),
            OsString::from("custom.toml"),
            OsString::from("--trust-workspace"),
        ])
        .unwrap();
        assert_eq!(
            parsed.override_path("theme.toml"),
            Some(&PathBuf::from("custom.toml"))
        );
        assert!(parsed.trust_workspace);
    }

    #[test]
    fn portable_mode_redirects_both_roots_and_profile_actions_are_exclusive() {
        let parsed = ConfigArguments::parse([
            OsString::from("--portable"),
            OsString::from("portable-profile"),
            OsString::from("--export-profile"),
            OsString::from("bundle"),
        ])
        .unwrap();
        assert!(
            parsed
                .config_root
                .unwrap()
                .ends_with("portable-profile/config")
        );
        assert!(
            parsed
                .data_root
                .unwrap()
                .ends_with("portable-profile/state")
        );
        assert!(matches!(
            parsed.transfer,
            Some(crate::platform::ProfileTransfer::Export(_))
        ));

        let error = ConfigArguments::parse([
            OsString::from("--export-profile"),
            OsString::from("one"),
            OsString::from("--import-profile"),
            OsString::from("two"),
        ])
        .err()
        .unwrap();
        assert!(error.contains("only one profile"));
    }

    #[test]
    fn all_shipped_document_names_are_stable() {
        assert_eq!(
            DOCUMENTS,
            [
                "keymap.toml",
                "theme.toml",
                "confirmations.toml",
                "handlers.toml",
                "macros.toml",
                "panel-modes.toml",
                "editor.toml",
                "history.toml",
                "interface.toml",
                "highlighting.toml",
                "user-menu.toml",
                "descriptions.toml",
                "filters.toml",
                "connections.toml",
                "shell.toml",
                "viewer.toml",
            ]
        );
    }

    #[test]
    fn macro_configuration_migrates_schema_one_to_binding_schema_two() {
        let effective = config_engine("macros.toml")
            .resolve([ConfigLayer::new(
                ConfigLayerKind::BuiltIn,
                "macros-v1.toml",
                "schema = 1\nmacros = []\n",
            )])
            .unwrap();
        assert_eq!(
            effective
                .value()
                .get("schema")
                .and_then(toml::Value::as_integer),
            Some(2)
        );
    }

    #[test]
    fn cli_document_layer_wins_and_reports_its_exact_origin() {
        let path =
            std::env::temp_dir().join(format!("near-config-cli-{}-theme.toml", std::process::id()));
        std::fs::write(&path, "schema = 1\nname = \"cli-theme\"\n").unwrap();
        let arguments = ConfigArguments {
            overrides: BTreeMap::from([("theme.toml".to_owned(), path.clone())]),
            config_root: None,
            data_root: None,
            transfer: None,
            trust_workspace: false,
            help: false,
        };
        let resolved = resolve_document(
            "theme.toml",
            "schema = 1\nname = \"built-in\"\n",
            &arguments,
        )
        .unwrap();
        assert!(resolved.text.contains("name = \"cli-theme\""));
        assert!(resolved.diagnostics.contains(&path.display().to_string()));
        assert!(resolved.diagnostics.contains("(Cli)"));
        let provenance = resolved.setting_provenance("name").unwrap();
        assert_eq!(provenance.layer, ConfigLayerKind::Cli);
        assert_eq!(provenance.source, path.display().to_string());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn atomic_settings_store_replaces_complete_documents_without_residue() {
        let root = std::env::temp_dir().join(format!(
            "near-settings-store-{}-{}",
            std::process::id(),
            SETTINGS_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let arguments = ConfigArguments {
            config_root: Some(root.clone()),
            ..ConfigArguments::default()
        };
        let store = AtomicSettingsDocumentStore::new(&arguments).unwrap();
        store
            .persist(
                "keymap.toml",
                "schema = 1\n[settings]\nsequence_timeout_ms = 250\n",
            )
            .unwrap();
        store
            .persist("viewer.toml", "schema = 1\nwrap = false\n")
            .unwrap();
        store
            .persist("viewer.toml", "schema = 1\nwrap = true\n")
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("viewer.toml")).unwrap(),
            "schema = 1\nwrap = true\n"
        );
        assert!(
            std::fs::read_to_string(root.join("keymap.toml"))
                .unwrap()
                .contains("sequence_timeout_ms = 250")
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_settings_store_rejects_unmapped_documents() {
        let store = AtomicSettingsDocumentStore {
            paths: BTreeMap::new(),
        };
        assert!(store.persist("viewer.toml", "schema = 1\n").is_err());
    }
}
