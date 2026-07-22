use std::{
    io::IsTerminal,
    time::{Duration, Instant},
};

use near_app::{
    ApplicationBuilder, CapabilitySet, CollectionEntry, CollectionSurface, ContextId, Keymap,
    ListRequest, ListingGeneration, RenderContext, Scene, SceneRect, SemanticTheme, Surface,
    SurfaceEvent, SurfaceId, SurfaceState, TaskOutcome, TaskPool, UpdateContext, UpdateResult,
    block_on,
};
use near_reference_providers::ProcessProvider;

const KEYMAP_BASE: &str = include_str!("../../../specs/keymap.toml");
const THEME: &str = include_str!("../../../specs/theme.toml");
const PROCESS_KEYMAP: &str = r#"

[[context]]
id = "near-proc.processes"
inherits = ["global"]

[[context.bindings]]
on = "Up"
run = { command = "near.collection.move", args = { rows = -1 } }
description = "Previous process"

[[context.bindings]]
on = "Down"
run = { command = "near.collection.move", args = { rows = 1 } }
description = "Next process"

[[context.bindings]]
on = "Enter"
run = "near.process.toggle-details"
description = "Toggle process details"

[[context.bindings]]
on = "F5"
run = "near.process.toggle-details"
description = "Process details"
hint = { group = "function", slot = 5 }
"#;

struct ProcessSurface {
    collection: CollectionSurface,
    expanded: bool,
}

impl ProcessSurface {
    fn new(collection: CollectionSurface) -> Self {
        Self {
            collection,
            expanded: false,
        }
    }
}

impl Surface for ProcessSurface {
    fn id(&self) -> SurfaceId {
        self.collection.id()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("near-proc.processes")]
    }

    fn capabilities(&self) -> CapabilitySet {
        self.collection.capabilities()
    }

    fn state(&self) -> SurfaceState {
        self.collection.state()
    }

    fn update(&mut self, event: &SurfaceEvent, context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return self.collection.update(event, context);
        };
        if invocation.id.as_str() == "near.process.toggle-details" {
            self.expanded = !self.expanded;
            return UpdateResult::handled();
        }
        self.collection.update(event, context)
    }

    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
        let mut scene = self.collection.scene(area, context);
        if self.expanded
            && let Some(resource) = self.collection.state().current
        {
            let height = 5.min(area.height.saturating_sub(2));
            let width = 38.min(area.width.saturating_sub(2));
            let popup = SceneRect::new(
                area.right().saturating_sub(width + 1),
                area.y.saturating_add(1),
                width,
                height,
            );
            scene.fill(popup, "dialog.background");
            scene.border(popup, Some(" Process Details ".to_owned()), "dialog.border");
            scene.text(
                popup.inset(1),
                format!(
                    "provider: {}\nresource: {}",
                    resource.provider,
                    resource.location.as_str()
                ),
                "text",
            );
        }
        scene
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h") => {
            println!("usage: near-proc");
            return Ok(());
        }
        Some("--version") => {
            println!("near-proc {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(argument) => return Err(format!("unknown near-proc argument: {argument}").into()),
        None => {}
    }
    let provider = load_process_provider()?;
    let surface = process_surface(&provider)?;
    let keymap = Keymap::from_toml(&format!("{KEYMAP_BASE}{PROCESS_KEYMAP}"))?;
    let application =
        ApplicationBuilder::new("near-proc", "Near Processes", ProcessSurface::new(surface))
            .theme(SemanticTheme::from_toml(THEME)?)
            .keymap(keymap)
            .build()?;
    if std::io::stdout().is_terminal() {
        application.run()?;
    } else {
        for line in application.snapshot(100, 20).text_lines() {
            println!("{line}");
        }
    }
    Ok(())
}

fn load_process_provider() -> Result<ProcessProvider, Box<dyn std::error::Error>> {
    let pool = TaskPool::new(1, 1);
    let task = pool.spawn(|_| ProcessProvider::local())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(completion) = pool.try_completion() {
            return match completion.outcome {
                TaskOutcome::Completed(provider) => Ok(provider),
                TaskOutcome::Cancelled => Err("process snapshot was cancelled".into()),
                TaskOutcome::Panicked => Err("process snapshot task panicked".into()),
            };
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    task.cancel();
    Err("process snapshot timed out".into())
}

fn process_surface(
    provider: &ProcessProvider,
) -> Result<CollectionSurface, Box<dyn std::error::Error>> {
    let location = ProcessProvider::root();
    let page = block_on(near_app::ResourceProvider::list(
        provider,
        &location,
        ListRequest {
            generation: ListingGeneration(1),
            continuation: None,
            page_size: 256,
            cancellation: near_app::CancellationToken::default(),
        },
    ))?;
    Ok(CollectionSurface::new(
        "near-proc.processes",
        "near-proc.processes",
        "Processes",
        location,
        page.entries
            .into_iter()
            .map(|entry| CollectionEntry {
                resource: entry.resource,
                metadata: entry.metadata,
                details: entry.details,
                selected: false,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_app::{CommandId, CommandInvocation};
    use near_reference_providers::ProcessRecord;

    use super::*;

    fn application() -> near_app::Application {
        let provider = ProcessProvider::new(vec![
            ProcessRecord {
                pid: 10,
                cpu: "1.0".to_owned(),
                command: "alpha".to_owned(),
            },
            ProcessRecord {
                pid: 20,
                cpu: "2.0".to_owned(),
                command: "beta".to_owned(),
            },
        ]);
        ApplicationBuilder::new(
            "near-proc",
            "Near Processes",
            ProcessSurface::new(process_surface(&provider).unwrap()),
        )
        .theme(SemanticTheme::from_toml(THEME).unwrap())
        .keymap(Keymap::from_toml(&format!("{KEYMAP_BASE}{PROCESS_KEYMAP}")).unwrap())
        .build()
        .unwrap()
    }

    #[test]
    fn domain_command_appears_in_help_and_changes_the_surface() {
        let mut application = application();
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.help.context"),
            arguments: BTreeMap::default(),
        });
        let help = application.snapshot(100, 24).text_lines().join("\n");
        assert!(help.contains("near.process.toggle-details"));
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.overlay.cancel"),
            arguments: BTreeMap::default(),
        });
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.process.toggle-details"),
            arguments: BTreeMap::default(),
        });
        let details = application.snapshot(100, 24).text_lines().join("\n");
        assert!(details.contains("Process Details"));
        assert!(details.contains("proc://local/10"));
    }

    #[test]
    fn domain_command_appears_in_the_normal_palette() {
        let mut application = application();
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.command-palette.open"),
            arguments: BTreeMap::default(),
        });
        let snapshot = application.snapshot(100, 24).text_lines().join("\n");
        assert!(snapshot.contains("Toggle process details"), "{snapshot}");
    }
}
