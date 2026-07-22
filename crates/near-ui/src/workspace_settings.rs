use std::{collections::BTreeMap, sync::Arc};

use near_config::{ConfigLayerKind, SettingProvenance};

use crate::{
    ConfirmationPolicy, EditorSettings, HistorySettings, InterfaceSettings, Keymap, KeymapSettings,
    PanelModeCatalog, SettingsDocumentStore, ViewerSettings,
};

pub(super) struct WorkspaceSettings {
    pub(super) keymap: KeymapSettings,
    pub(super) keymap_source: Option<String>,
    pub(super) pending_keymap_source: Option<String>,
    pub(super) provenance: BTreeMap<String, SettingProvenance>,
    document_provenance: BTreeMap<String, SettingProvenance>,
    pub(super) interface: InterfaceSettings,
    pub(super) confirmations: ConfirmationPolicy,
    pub(super) panel_modes: PanelModeCatalog,
    pub(super) viewer: ViewerSettings,
    pub(super) editor: EditorSettings,
    pub(super) history: HistorySettings,
    pub(super) store: Option<Arc<dyn SettingsDocumentStore>>,
    #[cfg(feature = "embedded-pty")]
    pub(super) shell: near_pty::ShellProfile,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            keymap: KeymapSettings::default(),
            keymap_source: None,
            pending_keymap_source: None,
            provenance: BTreeMap::new(),
            document_provenance: BTreeMap::new(),
            interface: InterfaceSettings::default(),
            confirmations: ConfirmationPolicy::default(),
            panel_modes: PanelModeCatalog::built_in(),
            viewer: ViewerSettings::default(),
            editor: EditorSettings::default(),
            history: HistorySettings::default(),
            store: None,
            #[cfg(feature = "embedded-pty")]
            shell: near_pty::ShellProfile::native_default(),
        }
    }
}

impl WorkspaceSettings {
    pub(super) fn persist(&self, document: &str) -> Result<bool, String> {
        let Some(store) = &self.store else {
            return Ok(false);
        };
        let contents = match document {
            "keymap.toml" => Ok(self
                .keymap_source
                .clone()
                .ok_or_else(|| "keymap source is unavailable".to_owned())?),
            "confirmations.toml" => {
                return self
                    .confirmations
                    .to_toml()
                    .map_err(|error| error.to_string())
                    .and_then(|contents| store.persist(document, &contents).map(|()| true));
            }
            "panel-modes.toml" => {
                return store
                    .persist(document, &self.panel_modes.to_toml())
                    .map(|()| true);
            }
            "viewer.toml" => toml::to_string_pretty(&self.viewer),
            "editor.toml" => toml::to_string_pretty(&self.editor),
            "history.toml" => toml::to_string_pretty(&self.history),
            "interface.toml" => toml::to_string_pretty(&self.interface),
            #[cfg(feature = "embedded-pty")]
            "shell.toml" => toml::to_string_pretty(&self.shell),
            _ => return Err(format!("unknown settings document {document}")),
        }
        .map_err(|error| error.to_string())?;
        store.persist(document, &contents)?;
        Ok(true)
    }

    pub(super) fn reload(&mut self) -> Result<bool, String> {
        let Some(store) = &self.store else {
            return Ok(false);
        };
        let viewer = store
            .load("viewer.toml")?
            .map_or(Ok(self.viewer), |source| ViewerSettings::from_toml(&source))?;
        let keymap = store.load("keymap.toml")?.map_or_else(
            || Ok((self.keymap_source.clone(), self.keymap)),
            |source| {
                Keymap::from_toml(&source)
                    .map(|keymap| (Some(source), *keymap.settings()))
                    .map_err(|error| error.to_string())
            },
        )?;
        let confirmations = store
            .load("confirmations.toml")?
            .map_or(Ok(self.confirmations.clone()), |source| {
                ConfirmationPolicy::from_toml(&source).map_err(|error| error.to_string())
            })?;
        let panel_modes = store
            .load("panel-modes.toml")?
            .map_or(Ok(self.panel_modes.clone()), |source| {
                PanelModeCatalog::from_toml(&source).map_err(|error| error.to_string())
            })?;
        let editor = store
            .load("editor.toml")?
            .map_or(Ok(self.editor), |source| EditorSettings::from_toml(&source))?;
        let history = store
            .load("history.toml")?
            .map_or(Ok(self.history), |source| {
                HistorySettings::from_toml(&source)
            })?;
        let interface = store
            .load("interface.toml")?
            .map_or(Ok(self.interface), |source| {
                InterfaceSettings::from_toml(&source).map_err(|error| error.to_string())
            })?;
        #[cfg(feature = "embedded-pty")]
        let shell = store.load("shell.toml")?.map_or_else(
            || Ok(self.shell.clone()),
            |source| near_pty::ShellProfile::from_toml(&source),
        )?;
        self.confirmations = confirmations;
        self.keymap_source = keymap.0;
        self.keymap = keymap.1;
        self.pending_keymap_source.clone_from(&self.keymap_source);
        self.panel_modes = panel_modes;
        self.viewer = viewer;
        self.editor = editor;
        self.history = history;
        self.interface = interface;
        #[cfg(feature = "embedded-pty")]
        {
            self.shell = shell;
        }
        for document in [
            "keymap.toml",
            "confirmations.toml",
            "panel-modes.toml",
            "viewer.toml",
            "editor.toml",
            "history.toml",
            "interface.toml",
        ] {
            if store.load(document)?.is_some()
                && let Some(origin) = store.provenance(document)
            {
                self.document_provenance.insert(document.to_owned(), origin);
            }
        }
        #[cfg(feature = "embedded-pty")]
        if store.load("shell.toml")?.is_some()
            && let Some(origin) = store.provenance("shell.toml")
        {
            self.document_provenance
                .insert("shell.toml".to_owned(), origin);
        }
        Ok(true)
    }

    pub(super) fn provenance_for(&self, id: &str, document: &str) -> SettingProvenance {
        self.document_provenance
            .get(document)
            .or_else(|| self.provenance.get(id))
            .cloned()
            .unwrap_or(SettingProvenance {
                layer: ConfigLayerKind::BuiltIn,
                source: format!("<built-in>/{document}"),
            })
    }

    pub(super) fn record_persisted_origin(&mut self, document: &str) {
        if let Some(origin) = self
            .store
            .as_ref()
            .and_then(|store| store.provenance(document))
        {
            self.document_provenance.insert(document.to_owned(), origin);
        }
    }
}
