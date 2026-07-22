use std::{
    ffi::OsString,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    sync::Arc,
};

use near_app::{
    ApplicationBuilder, CancellationToken, Keymap, Location, OpenRequest, ProviderError,
    ResourceProvider, ResourceRef, SemanticTheme, ViewerSurface, block_on,
};
use near_local_fs::LocalFileProvider;
use near_reference_providers::{PluginCatalogProvider, PluginItem, ProcessProvider};

const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
const THEME: &str = include_str!("../../../specs/theme.toml");
const WINDOW: usize = 64 * 1024;

enum ViewerInput {
    Stdin(Vec<u8>),
    Resource {
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        title: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argument = std::env::args_os().nth(1);
    if argument.as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!("usage: near-view [FILE|-|PROVIDER_URI]");
        return Ok(());
    }
    if argument.as_deref() == Some(std::ffi::OsStr::new("--version")) {
        println!("near-view {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let input = resolve_input(argument)?;
    if !io::stdout().is_terminal() {
        write_plain(input, &mut io::stdout().lock())?;
        return Ok(());
    }
    let viewer = match input {
        ViewerInput::Stdin(bytes) => ViewerSurface::bytes("near-view.viewer", "stdin", bytes),
        ViewerInput::Resource {
            provider,
            resource,
            title,
        } => ViewerSurface::stream(
            "near-view.viewer",
            title,
            provider,
            resource,
            CancellationToken::default(),
        )?,
    };
    ApplicationBuilder::new("near-view", "Near View", viewer)
        .theme(SemanticTheme::from_toml(THEME)?)
        .keymap(Keymap::from_toml(KEYMAP)?)
        .build()?
        .run()?;
    Ok(())
}

fn resolve_input(argument: Option<OsString>) -> Result<ViewerInput, Box<dyn std::error::Error>> {
    let Some(argument) = argument else {
        if io::stdin().is_terminal() {
            return Err("no input; use near-view FILE, '-', or PROVIDER_URI".into());
        }
        return read_stdin();
    };
    if argument == "-" {
        return read_stdin();
    }
    if let Some(text) = argument.to_str()
        && text.contains("://")
    {
        return resolve_uri(Location::new(text));
    }
    let path = PathBuf::from(argument);
    let provider: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);
    let resource = LocalFileProvider::resource_for_path(&path);
    let metadata = block_on(provider.stat(&resource))?;
    Ok(ViewerInput::Resource {
        provider,
        resource,
        title: metadata.name,
    })
}

fn read_stdin() -> Result<ViewerInput, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    io::stdin().lock().read_to_end(&mut bytes)?;
    Ok(ViewerInput::Stdin(bytes))
}

fn resolve_uri(location: Location) -> Result<ViewerInput, Box<dyn std::error::Error>> {
    let scheme = location
        .as_str()
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .ok_or_else(|| ProviderError::Failed("provider URI has no scheme".to_owned()))?;
    let provider: Arc<dyn ResourceProvider> = match scheme {
        "file" => Arc::new(LocalFileProvider),
        "proc" => Arc::new(ProcessProvider::local()),
        "plugin" => Arc::new(default_plugin_catalog()),
        _ => {
            return Err(ProviderError::Unsupported(format!(
                "near-view has no provider for scheme {scheme}"
            ))
            .into());
        }
    };
    let resource = ResourceRef {
        provider: provider.id(),
        location,
    };
    let metadata = block_on(provider.stat(&resource))?;
    Ok(ViewerInput::Resource {
        provider,
        resource,
        title: metadata.name,
    })
}

fn default_plugin_catalog() -> PluginCatalogProvider {
    PluginCatalogProvider::new(vec![
        PluginItem {
            id: "near.archive".to_owned(),
            name: "Archive Provider".to_owned(),
            version: "0.1.0".to_owned(),
            description: "Browsable archive resources".to_owned(),
        },
        PluginItem {
            id: "near.git".to_owned(),
            name: "Git Provider".to_owned(),
            version: "0.1.0".to_owned(),
            description: "Repository status resources".to_owned(),
        },
    ])
}

fn write_plain(
    input: ViewerInput,
    output: &mut dyn Write,
) -> Result<(), Box<dyn std::error::Error>> {
    match input {
        ViewerInput::Stdin(bytes) => output.write_all(&bytes)?,
        ViewerInput::Resource {
            provider, resource, ..
        } => {
            let cancellation = CancellationToken::default();
            let mut offset = 0_u64;
            loop {
                let stream = block_on(provider.open(
                    &resource,
                    OpenRequest {
                        offset,
                        length: WINDOW,
                        cancellation: cancellation.clone(),
                    },
                ))?;
                output.write_all(&stream.bytes)?;
                if stream.complete || stream.bytes.is_empty() {
                    break;
                }
                offset = stream
                    .offset
                    .saturating_add(u64::try_from(stream.bytes.len()).unwrap_or(u64::MAX));
            }
        }
    }
    Ok(())
}
