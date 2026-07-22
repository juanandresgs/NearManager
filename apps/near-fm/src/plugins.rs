use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

use near_core::{CommandExtension, ResourceProvider, ResourceRef};
use near_local_fs::LocalFileProvider;
use near_plugins::{
    CapabilityGrantStore, PluginManifest, PluginOrigin, PluginProviderAdapter,
    PluginResourceReader, ProcessPlugin, ProcessPluginHost, ProcessPluginManifest, WasmPluginHost,
};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantDocument {
    #[serde(default)]
    grant: Vec<GrantEntry>,
    #[serde(default)]
    trusted_workspace: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantEntry {
    plugin: String,
    capability: String,
}

struct LocalPluginReader;

impl PluginResourceReader for LocalPluginReader {
    fn read(&self, resource: &ResourceRef, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        let path =
            LocalFileProvider::path(&resource.location).map_err(|error| error.to_string())?;
        let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = vec![0; length.min(1024 * 1024) as usize];
        let read = file.read(&mut bytes).map_err(|error| error.to_string())?;
        bytes.truncate(read);
        Ok(bytes)
    }
}

pub struct PluginDiscovery {
    pub extensions: Vec<Arc<dyn CommandExtension>>,
    pub providers: Vec<Arc<dyn ResourceProvider>>,
    pub diagnostics: String,
}

pub fn discover(data_root: &Path) -> PluginDiscovery {
    let grants_path = std::env::var_os("NEAR_PLUGIN_GRANTS")
        .map_or_else(|| default_grants_path(data_root), PathBuf::from);
    let (grants, mut diagnostics) = load_grants(&grants_path);
    let process_host = ProcessPluginHost::new(grants.clone());
    let mut wasm_host = LazyWasmHost::new(grants);
    let mut packages = Vec::new();
    if let Some(root) = std::env::var_os("NEAR_PLUGIN_DIR").map(PathBuf::from) {
        packages.extend(package_directories(&root, PluginOrigin::Installed));
    } else {
        packages.extend(package_directories(
            &default_installed_root(data_root),
            PluginOrigin::Installed,
        ));
        if let Ok(current) = std::env::current_dir() {
            packages.extend(package_directories(
                &current.join(".near/plugins"),
                PluginOrigin::Workspace,
            ));
        }
    }
    packages.sort_by(|left, right| left.0.cmp(&right.0));

    let mut extensions: Vec<Arc<dyn CommandExtension>> = Vec::new();
    let mut providers: Vec<Arc<dyn ResourceProvider>> = Vec::new();
    for (directory, origin) in packages {
        match load_package(&mut wasm_host, &process_host, &directory, origin) {
            Ok(LoadedPackage::Wasm(extension)) => {
                let extension = Arc::new(extension);
                diagnostics.push(format!(
                    "Loaded plugin {} {} from {}",
                    extension.manifest().id,
                    extension.manifest().version,
                    directory.display()
                ));
                if !extension.manifest().providers.is_empty() {
                    match PluginProviderAdapter::new(Arc::clone(&extension)) {
                        Ok(provider) => providers.push(Arc::new(provider)),
                        Err(error) => diagnostics.push(format!(
                            "Rejected provider exports from {}: {error}",
                            extension.manifest().id
                        )),
                    }
                }
                extensions.push(extension);
            }
            Ok(LoadedPackage::Process(extension)) => {
                diagnostics.push(format!(
                    "Loaded process plugin {} {} from {}",
                    extension.manifest().id,
                    extension.manifest().version,
                    directory.display()
                ));
                extensions.push(Arc::new(extension));
            }
            Err(error) => diagnostics.push(format!(
                "Rejected plugin package {}: {error}",
                directory.display()
            )),
        }
    }
    if diagnostics.is_empty() {
        diagnostics.push("No plugin packages discovered".to_owned());
    }
    PluginDiscovery {
        extensions,
        providers,
        diagnostics: diagnostics.join("\n"),
    }
}

enum LoadedPackage {
    Wasm(near_plugins::LoadedPlugin),
    Process(ProcessPlugin),
}

struct LazyWasmHost {
    grants: CapabilityGrantStore,
    host: Option<WasmPluginHost>,
}

impl LazyWasmHost {
    fn new(grants: CapabilityGrantStore) -> Self {
        Self { grants, host: None }
    }

    fn host(&mut self) -> Result<&WasmPluginHost, String> {
        if self.host.is_none() {
            self.host = Some(
                WasmPluginHost::new(self.grants.clone())
                    .map_err(|error| error.to_string())?
                    .with_resource_reader(Arc::new(LocalPluginReader)),
            );
        }
        self.host
            .as_ref()
            .ok_or_else(|| "Wasm plugin host did not initialize".to_owned())
    }

    #[cfg(all(test, target_os = "macos"))]
    fn initialized(&self) -> bool {
        self.host.is_some()
    }
}

fn load_package(
    wasm_host: &mut LazyWasmHost,
    process_host: &ProcessPluginHost,
    directory: &Path,
    origin: PluginOrigin,
) -> Result<LoadedPackage, String> {
    let manifest_text = fs::read_to_string(directory.join("plugin.toml"))
        .map_err(|error| format!("manifest read failed: {error}"))?;
    let runtime = toml::from_str::<toml::Value>(&manifest_text)
        .map_err(|error| error.to_string())?
        .get("runtime")
        .and_then(toml::Value::as_str)
        .unwrap_or("wasm")
        .to_owned();
    match runtime.as_str() {
        "wasm" => {
            let manifest =
                PluginManifest::from_toml(&manifest_text).map_err(|error| error.to_string())?;
            let component = directory.join(&manifest.component);
            wasm_host
                .host()?
                .load(manifest, origin, &component)
                .map(LoadedPackage::Wasm)
                .map_err(|error| error.to_string())
        }
        "process" => {
            let manifest = ProcessPluginManifest::from_toml(&manifest_text)
                .map_err(|error| error.to_string())?;
            process_host
                .load(manifest, origin, directory)
                .map(LoadedPackage::Process)
                .map_err(|error| error.to_string())
        }
        other => Err(format!("unknown plugin runtime {other}")),
    }
}

fn package_directories(root: &Path, origin: PluginOrigin) -> Vec<(PathBuf, PluginOrigin)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("plugin.toml").is_file())
        .map(|path| (path, origin))
        .collect()
}

fn load_grants(path: &Path) -> (CapabilityGrantStore, Vec<String>) {
    let mut grants = CapabilityGrantStore::default();
    if !path.is_file() {
        return (grants, Vec::new());
    }
    let document = match fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| toml::from_str::<GrantDocument>(&text).map_err(|error| error.to_string()))
    {
        Ok(document) => document,
        Err(error) => {
            return (
                grants,
                vec![format!(
                    "Plugin grants {} are invalid: {error}",
                    path.display()
                )],
            );
        }
    };
    for entry in document.grant {
        grants.grant(entry.plugin, entry.capability);
    }
    for plugin in document.trusted_workspace {
        grants.trust_workspace_plugin(plugin);
    }
    (grants, vec![format!("Plugin grants: {}", path.display())])
}

fn default_installed_root(data_root: &Path) -> PathBuf {
    data_root.join("plugins")
}

fn default_grants_path(data_root: &Path) -> PathBuf {
    data_root.join("plugin-grants.toml")
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn grant_document_is_explicit_and_revocable_at_the_source() {
        let document: GrantDocument = toml::from_str(
            r#"
trusted_workspace = ["workspace.tool"]

[[grant]]
plugin = "archive.tool"
capability = "near.resource.read@1"
"#,
        )
        .unwrap();
        assert_eq!(document.trusted_workspace, ["workspace.tool"]);
        assert_eq!(document.grant.len(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_runtime_packages_use_the_process_host() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let package = std::env::temp_dir().join(format!(
            "near-fm-process-discovery-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("plugin"), "fixture").unwrap();
        fs::write(
            package.join("plugin.toml"),
            r#"
schema = 1
id = "test.process"
name = "Process Test"
version = "1.0.0"
protocol = "^0.1"
runtime = "process"
executable = "plugin"

[[commands]]
id = "test.process.invoke"
title = "Invoke"
description = "Invoke process test"
safety = "read-only"
"#,
        )
        .unwrap();
        let mut wasm_host = LazyWasmHost::new(CapabilityGrantStore::default());
        let process_host = ProcessPluginHost::new(CapabilityGrantStore::default());
        let loaded = load_package(
            &mut wasm_host,
            &process_host,
            &package,
            PluginOrigin::Installed,
        )
        .unwrap();
        assert!(matches!(loaded, LoadedPackage::Process(_)));
        assert!(!wasm_host.initialized());
        fs::remove_dir_all(package).unwrap();
    }
}
