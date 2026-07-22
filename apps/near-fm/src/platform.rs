use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const PROFILE_SCHEMA: u16 = 1;
const STATE_FILES: &[&str] = &[
    "command-history.toml",
    "folder-navigation.toml",
    "editor-positions.toml",
    "viewer-state.toml",
    "resource-history.toml",
    "operations.log",
    "plugin-grants.toml",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRoots {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl ProfileRoots {
    pub fn resolve(config: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        Self {
            config: absolute_root(
                config
                    .or_else(user_config_root)
                    .unwrap_or_else(|| application_data_root().join("config")),
            ),
            data: absolute_root(data.unwrap_or_else(application_data_root)),
        }
    }

    pub fn portable(root: PathBuf) -> Self {
        let root = absolute_root(root);
        Self {
            config: root.join("config"),
            data: root.join("state"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileTransfer {
    Export(PathBuf),
    Import(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfileManifest {
    schema: u16,
    application: String,
    config_files: Vec<String>,
    state_files: Vec<String>,
}

pub fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn user_config_root() -> Option<PathBuf> {
    std::env::var_os("NEAR_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(platform_user_config_root)
}

pub fn system_config_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/Near")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/etc/near")
    }
    #[cfg(windows)]
    {
        std::env::var_os("PROGRAMDATA")
            .map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from)
            .join("Near")
    }
}

pub fn application_data_root() -> PathBuf {
    if let Some(root) = std::env::var_os("NEAR_DATA_HOME") {
        return PathBuf::from(root);
    }
    #[cfg(target_os = "macos")]
    {
        home_directory()
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Application Support/near")
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_DATA_HOME").map_or_else(
            || {
                home_directory()
                    .unwrap_or_else(std::env::temp_dir)
                    .join(".local/share/near")
            },
            |root| PathBuf::from(root).join("near"),
        )
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("Near")
    }
}

pub fn transfer_profile(
    transfer: &ProfileTransfer,
    roots: &ProfileRoots,
    config_documents: &[&str],
) -> Result<String, String> {
    match transfer {
        ProfileTransfer::Export(destination) => {
            export_profile(destination, roots, config_documents)
        }
        ProfileTransfer::Import(source) => import_profile(source, roots, config_documents),
    }
}

fn export_profile(
    destination: &Path,
    roots: &ProfileRoots,
    config_documents: &[&str],
) -> Result<String, String> {
    let destination = absolute_root(destination.to_path_buf());
    if destination.exists() {
        return Err(format!(
            "profile export destination already exists: {}",
            destination.display()
        ));
    }
    let staging = destination.with_extension(format!("near-export-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(staging.join("config")).map_err(|error| error.to_string())?;
    fs::create_dir_all(staging.join("state")).map_err(|error| error.to_string())?;
    let config_files =
        copy_existing_named(&roots.config, &staging.join("config"), config_documents)?;
    let state_files = copy_existing_named(&roots.data, &staging.join("state"), STATE_FILES)?;
    let manifest = ProfileManifest {
        schema: PROFILE_SCHEMA,
        application: "near-fm".to_owned(),
        config_files,
        state_files,
    };
    fs::write(
        staging.join("near-profile.toml"),
        toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&staging, &destination).map_err(|error| {
        let _ = fs::remove_dir_all(&staging);
        error.to_string()
    })?;
    Ok(format!("Exported profile to {}", destination.display()))
}

fn import_profile(
    source: &Path,
    roots: &ProfileRoots,
    config_documents: &[&str],
) -> Result<String, String> {
    let source = absolute_root(source.to_path_buf());
    let manifest_text = fs::read_to_string(source.join("near-profile.toml"))
        .map_err(|error| format!("cannot read profile manifest: {error}"))?;
    let manifest: ProfileManifest = toml::from_str(&manifest_text)
        .map_err(|error| format!("invalid profile manifest: {error}"))?;
    if manifest.schema != PROFILE_SCHEMA || manifest.application != "near-fm" {
        return Err(format!(
            "unsupported profile {} schema {}",
            manifest.application, manifest.schema
        ));
    }
    validate_manifest_files(&manifest.config_files, config_documents, "configuration")?;
    validate_manifest_files(&manifest.state_files, STATE_FILES, "state")?;
    fs::create_dir_all(&roots.config).map_err(|error| error.to_string())?;
    fs::create_dir_all(&roots.data).map_err(|error| error.to_string())?;
    for name in &manifest.config_files {
        atomic_copy(&source.join("config").join(name), &roots.config.join(name))?;
    }
    for name in &manifest.state_files {
        atomic_copy(&source.join("state").join(name), &roots.data.join(name))?;
    }
    Ok(format!("Imported profile from {}", source.display()))
}

fn copy_existing_named(
    source: &Path,
    destination: &Path,
    names: &[&str],
) -> Result<Vec<String>, String> {
    let mut copied = Vec::new();
    for name in names {
        let source_file = source.join(name);
        if source_file.is_file() {
            fs::copy(&source_file, destination.join(name)).map_err(|error| error.to_string())?;
            copied.push((*name).to_owned());
        }
    }
    Ok(copied)
}

fn validate_manifest_files(files: &[String], allowed: &[&str], kind: &str) -> Result<(), String> {
    for file in files {
        if !allowed.contains(&file.as_str()) {
            return Err(format!("profile contains unsupported {kind} file {file}"));
        }
    }
    Ok(())
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("profile file is missing: {}", source.display()));
    }
    let temporary = destination.with_extension(format!("near-import-{}", std::process::id()));
    let backup = destination.with_extension(format!("near-backup-{}", std::process::id()));
    fs::copy(source, &temporary).map_err(|error| error.to_string())?;
    if destination.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        if backup.exists() {
            let _ = fs::rename(&backup, destination);
        }
        error.to_string()
    })?;
    if backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn absolute_root(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(target_os = "macos")]
fn platform_user_config_root() -> Option<PathBuf> {
    home_directory().map(|home| home.join("Library/Application Support/near"))
}

#[cfg(target_os = "linux")]
fn platform_user_config_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_directory().map(|home| home.join(".config")))
        .map(|root| root.join("near"))
}

#[cfg(windows)]
fn platform_user_config_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .map(|root| root.join("Near"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_roots_are_absolute_when_available() {
        assert!(system_config_root().is_absolute());
        assert!(application_data_root().is_absolute());
        assert!(user_config_root().is_none_or(|path| path.is_absolute()));
    }

    #[test]
    fn portable_roots_and_profile_bundle_round_trip_configuration_and_history() {
        let base =
            std::env::temp_dir().join(format!("near-profile-roundtrip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let source = ProfileRoots::portable(base.join("source"));
        let imported = ProfileRoots::portable(base.join("imported"));
        fs::create_dir_all(&source.config).unwrap();
        fs::create_dir_all(&source.data).unwrap();
        fs::write(
            source.config.join("theme.toml"),
            "schema = 1\nname = \"portable\"\n",
        )
        .unwrap();
        fs::write(
            source.data.join("command-history.toml"),
            "schema = 1\nentries = []\n",
        )
        .unwrap();
        let bundle = base.join("bundle");
        transfer_profile(
            &ProfileTransfer::Export(bundle.clone()),
            &source,
            &["theme.toml"],
        )
        .unwrap();
        let manifest = fs::read_to_string(bundle.join("near-profile.toml")).unwrap();
        assert!(manifest.contains("schema = 1"));
        assert!(manifest.contains("theme.toml"));
        assert!(manifest.contains("command-history.toml"));

        transfer_profile(&ProfileTransfer::Import(bundle), &imported, &["theme.toml"]).unwrap();
        assert_eq!(
            fs::read_to_string(imported.config.join("theme.toml")).unwrap(),
            "schema = 1\nname = \"portable\"\n"
        );
        assert_eq!(
            fs::read_to_string(imported.data.join("command-history.toml")).unwrap(),
            "schema = 1\nentries = []\n"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn profile_import_rejects_unlisted_files_before_writing() {
        let base = std::env::temp_dir().join(format!("near-profile-reject-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("bundle/config")).unwrap();
        fs::create_dir_all(base.join("bundle/state")).unwrap();
        fs::write(
            base.join("bundle/near-profile.toml"),
            "schema = 1\napplication = \"near-fm\"\nconfig_files = [\"../escape\"]\nstate_files = []\n",
        )
        .unwrap();
        let roots = ProfileRoots::portable(base.join("target"));
        let error = transfer_profile(
            &ProfileTransfer::Import(base.join("bundle")),
            &roots,
            &["theme.toml"],
        )
        .unwrap_err();
        assert!(error.contains("unsupported configuration file"));
        assert!(!roots.config.exists());
        fs::remove_dir_all(base).unwrap();
    }
}
