//! Deterministic clocks, providers, and headless workflow drivers for Near.

#![allow(clippy::missing_errors_doc)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

use near_app::Application;
use near_core::{ListingGeneration, Location, ResourceMetadata, ResourceRef};
use near_terminal::TerminalEvent;
use near_ui::{
    FarWorkspace, Keymap, SemanticSnapshot, SemanticTheme, WorkspaceAction, parse_key_stroke,
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManualClock {
    now: Duration,
}

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct FilesystemFaultFixture {
    root: PathBuf,
}

impl FilesystemFaultFixture {
    pub fn new(label: &str) -> std::io::Result<Self> {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "near-testkit-{}-{}-{sequence}",
            std::process::id(),
            label.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn file(&self, name: impl AsRef<Path>, contents: &[u8]) -> std::io::Result<PathBuf> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    pub fn directory(&self, name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        let path = self.root.join(name);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn disappearing_file(&self, name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        let path = self.file(name, b"disappearing")?;
        fs::remove_file(&path)?;
        Ok(path)
    }

    pub fn collision(
        &self,
        directory: impl AsRef<Path>,
        name: &str,
    ) -> std::io::Result<(PathBuf, PathBuf)> {
        let left = self.file(Path::new("source").join(name), b"source")?;
        let right = self.file(directory.as_ref().join(name), b"existing")?;
        Ok((left, right))
    }

    #[cfg(unix)]
    pub fn exact_name_bytes(&self, bytes: Vec<u8>, contents: &[u8]) -> std::io::Result<PathBuf> {
        self.file(PathBuf::from(OsString::from_vec(bytes)), contents)
    }
}

impl Drop for FilesystemFaultFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl ManualClock {
    pub fn now(self) -> Duration {
        self.now
    }

    pub fn advance(&mut self, duration: Duration) -> Duration {
        self.now = self.now.saturating_add(duration);
        self.now
    }
}

pub use near_core::ListingGeneration as Generation;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderPayload {
    Item {
        resource: ResourceRef,
        metadata: Box<ResourceMetadata>,
    },
    Failure(String),
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEvent {
    pub request: RequestId,
    pub generation: Generation,
    pub payload: ProviderPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderStep {
    pub after: Duration,
    pub payload: ProviderPayload,
}

#[derive(Clone, Debug)]
struct ActiveListing {
    generation: ListingGeneration,
    started: Duration,
    cancelled: bool,
    next_step: usize,
    steps: Vec<ProviderStep>,
}

#[derive(Default)]
pub struct FakeProvider {
    clock: ManualClock,
    scripts: BTreeMap<Location, Vec<ProviderStep>>,
    active: BTreeMap<RequestId, ActiveListing>,
    next_request: u64,
}

impl FakeProvider {
    pub fn script(&mut self, location: Location, steps: Vec<ProviderStep>) {
        self.scripts.insert(location, steps);
    }

    pub fn start(&mut self, location: &Location, generation: ListingGeneration) -> RequestId {
        let request = RequestId(self.next_request);
        self.next_request = self.next_request.saturating_add(1);
        let steps = self.scripts.get(location).cloned().unwrap_or_else(|| {
            vec![ProviderStep {
                after: Duration::ZERO,
                payload: ProviderPayload::Failure("location is not scripted".to_owned()),
            }]
        });
        self.active.insert(
            request,
            ActiveListing {
                generation,
                started: self.clock.now(),
                cancelled: false,
                next_step: 0,
                steps,
            },
        );
        request
    }

    pub fn cancel(&mut self, request: RequestId) -> bool {
        let Some(listing) = self.active.get_mut(&request) else {
            return false;
        };
        listing.cancelled = true;
        true
    }

    pub fn advance(&mut self, duration: Duration) -> Vec<ProviderEvent> {
        let now = self.clock.advance(duration);
        let mut events = Vec::new();
        for (request, listing) in &mut self.active {
            if listing.cancelled {
                continue;
            }
            while let Some(step) = listing.steps.get(listing.next_step)
                && now.saturating_sub(listing.started) >= step.after
            {
                events.push(ProviderEvent {
                    request: *request,
                    generation: listing.generation,
                    payload: step.payload.clone(),
                });
                listing.next_step = listing.next_step.saturating_add(1);
            }
        }
        events
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationGate {
    active: ListingGeneration,
}

impl GenerationGate {
    pub fn new(active: ListingGeneration) -> Self {
        Self { active }
    }

    pub fn replace(&mut self, generation: ListingGeneration) {
        self.active = generation;
    }

    pub fn accepts(self, event: &ProviderEvent) -> bool {
        event.generation == self.active
    }
}

pub struct WorkflowHarness {
    workspace: FarWorkspace,
    keymap: Keymap,
    theme: SemanticTheme,
    clock: ManualClock,
    width: u16,
    height: u16,
}

pub struct ApplicationWorkflowHarness {
    application: Application,
    clock: ManualClock,
    width: u16,
    height: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowStep {
    Key(String),
    Paste(String),
    Advance(Duration),
    Capture(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldenFrame {
    pub name: String,
    pub snapshot: SemanticSnapshot,
}

impl WorkflowHarness {
    pub fn new(
        workspace: FarWorkspace,
        keymap: Keymap,
        theme: SemanticTheme,
        width: u16,
        height: u16,
    ) -> Self {
        Self {
            workspace,
            keymap,
            theme,
            clock: ManualClock::default(),
            width,
            height,
        }
    }

    /// Sends a normalized key at deterministic manual time.
    ///
    /// # Panics
    ///
    /// Panics when `key` is not a valid Near key specification.
    pub fn key(&mut self, key: &str) -> WorkspaceAction {
        let event = TerminalEvent::Key(parse_key_stroke(key).expect("valid workflow key"));
        self.workspace
            .handle_terminal_event_at(&mut self.keymap, event, self.clock.now())
    }

    pub fn paste(&mut self, text: impl Into<String>) -> WorkspaceAction {
        self.workspace.handle_terminal_event_at(
            &mut self.keymap,
            TerminalEvent::Paste(text.into()),
            self.clock.now(),
        )
    }

    pub fn advance(&mut self, duration: Duration) -> WorkspaceAction {
        let now = self.clock.advance(duration);
        self.workspace
            .handle_keymap_timeout_at(&mut self.keymap, now)
    }

    pub fn snapshot(&self) -> SemanticSnapshot {
        self.workspace
            .semantic_snapshot(&self.theme, &self.keymap, self.width, self.height)
    }

    pub fn workspace(&self) -> &FarWorkspace {
        &self.workspace
    }

    pub fn run_script(&mut self, steps: &[WorkflowStep]) -> Vec<GoldenFrame> {
        let mut frames = Vec::new();
        for step in steps {
            match step {
                WorkflowStep::Key(key) => {
                    self.key(key);
                }
                WorkflowStep::Paste(text) => {
                    self.paste(text.clone());
                }
                WorkflowStep::Advance(duration) => {
                    self.advance(*duration);
                }
                WorkflowStep::Capture(name) => frames.push(GoldenFrame {
                    name: name.clone(),
                    snapshot: self.snapshot(),
                }),
            }
        }
        frames
    }
}

impl ApplicationWorkflowHarness {
    pub fn new(application: Application, width: u16, height: u16) -> Self {
        Self {
            application,
            clock: ManualClock::default(),
            width,
            height,
        }
    }

    /// Sends a normalized key at deterministic manual time.
    ///
    /// # Errors
    ///
    /// Returns a keymap error when `key` is not a valid Near key specification.
    pub fn key(&mut self, key: &str) -> Result<(), near_app::KeymapError> {
        self.application.handle_key_at(key, self.clock.now())
    }

    pub fn paste(&mut self, text: impl Into<String>) {
        self.application.paste_at(text, self.clock.now());
    }

    pub fn advance(&mut self, duration: Duration) {
        let now = self.clock.advance(duration);
        self.application.handle_keymap_timeout_at(now);
    }

    pub fn snapshot(&self) -> SemanticSnapshot {
        self.application.snapshot(self.width, self.height)
    }

    pub fn application(&self) -> &Application {
        &self.application
    }

    /// Runs a backend-neutral application workflow and captures named semantic frames.
    ///
    /// # Errors
    ///
    /// Returns a keymap error when a scripted key is invalid.
    pub fn run_script(
        &mut self,
        steps: &[WorkflowStep],
    ) -> Result<Vec<GoldenFrame>, near_app::KeymapError> {
        let mut frames = Vec::new();
        for step in steps {
            match step {
                WorkflowStep::Key(key) => self.key(key)?,
                WorkflowStep::Paste(text) => self.paste(text.clone()),
                WorkflowStep::Advance(duration) => self.advance(*duration),
                WorkflowStep::Capture(name) => frames.push(GoldenFrame {
                    name: name.clone(),
                    snapshot: self.snapshot(),
                }),
            }
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use near_app::{ApplicationBuilder, CollectionEntry, CollectionSurface};
    use near_core::{ProviderId, ResourceKind};

    use super::*;

    #[test]
    fn filesystem_fault_fixture_models_collisions_and_disappearance() {
        let fixture = FilesystemFaultFixture::new("faults").unwrap();
        let (source, existing) = fixture.collision("trash", "same.txt").unwrap();
        let disappeared = fixture.disappearing_file("gone.txt").unwrap();

        assert_eq!(fs::read(source).unwrap(), b"source");
        assert_eq!(fs::read(existing).unwrap(), b"existing");
        assert!(!disappeared.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn filesystem_fault_fixture_preserves_exact_filename_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let fixture = FilesystemFaultFixture::new("bytes").unwrap();
        let path = fixture
            .exact_name_bytes(b"invalid-\xff-name".to_vec(), b"content")
            .unwrap();
        assert_eq!(path.file_name().unwrap().as_bytes(), b"invalid-\xff-name");
    }

    fn item(name: &str) -> ProviderPayload {
        ProviderPayload::Item {
            resource: ResourceRef {
                provider: ProviderId::from("test.fake"),
                location: Location::new(format!("/items/{name}")),
            },
            metadata: Box::new(ResourceMetadata {
                name: name.to_owned(),
                kind: ResourceKind::Virtual,
                size: None,
                modified_unix_ms: None,
                ..ResourceMetadata::default()
            }),
        }
    }

    #[test]
    fn fake_provider_makes_stale_and_cancelled_results_deterministic() {
        let location = Location::new("/items");
        let mut provider = FakeProvider::default();
        provider.script(
            location.clone(),
            vec![
                ProviderStep {
                    after: Duration::from_millis(10),
                    payload: item("first"),
                },
                ProviderStep {
                    after: Duration::from_millis(20),
                    payload: ProviderPayload::Complete,
                },
            ],
        );
        let old = provider.start(&location, Generation(1));
        let current = provider.start(&location, Generation(2));
        let gate = GenerationGate::new(Generation(2));
        let events = provider.advance(Duration::from_millis(10));
        assert_eq!(events.len(), 2);
        assert_eq!(events.iter().filter(|event| gate.accepts(event)).count(), 1);
        assert!(provider.cancel(current));
        let remaining = provider.advance(Duration::from_millis(10));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].request, old);
        assert!(!gate.accepts(&remaining[0]));
    }

    #[test]
    fn scripted_far_workflow_runs_headlessly_with_semantic_frames() {
        const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
        const THEME: &str = include_str!("../../../specs/theme.toml");
        let keymap = KEYMAP.replace(
            "[[context]]\nid = \"workspace.panel\"",
            "[[context.bindings]]\non = [\"Ctrl+G\", \"Ctrl+G\"]\nrun = \"near.collection.first\"\ndescription = \"First item sequence\"\n\n[[context]]\nid = \"workspace.panel\"",
        );
        let mut harness = WorkflowHarness::new(
            FarWorkspace::demo(),
            Keymap::from_toml(&keymap).unwrap(),
            SemanticTheme::from_toml(THEME).unwrap(),
            100,
            30,
        );
        let frames = harness.run_script(&[
            WorkflowStep::Key("F7".to_owned()),
            WorkflowStep::Paste("Artifacts".to_owned()),
            WorkflowStep::Capture("dialog".to_owned()),
            WorkflowStep::Key("Enter".to_owned()),
            WorkflowStep::Capture("created".to_owned()),
            WorkflowStep::Key("Ctrl+G".to_owned()),
            WorkflowStep::Capture("sequence-pending".to_owned()),
            WorkflowStep::Advance(Duration::from_millis(700)),
            WorkflowStep::Capture("sequence-timeout".to_owned()),
        ]);
        assert_eq!(frames.len(), 4);
        let dialog = frames[0].snapshot.text_lines().join("\n");
        assert!(dialog.contains("New folder"));
        assert!(dialog.contains("Artifacts"));
        let created = frames[1].snapshot.text_lines().join("\n");
        assert!(created.contains("No operation service is configured"));
        assert!(
            frames[0]
                .snapshot
                .role_lines()
                .iter()
                .any(|line| line.contains("dialog.border"))
        );
        assert!(
            frames[2]
                .snapshot
                .text_lines()
                .join("\n")
                .contains("keys: Ctrl+g")
        );
        assert!(
            !frames[3]
                .snapshot
                .text_lines()
                .join("\n")
                .contains("keys: Ctrl+g")
        );
    }

    #[test]
    fn public_application_workflow_runs_without_file_manager_internals() {
        const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
        const THEME: &str = include_str!("../../../specs/theme.toml");
        let entries = (0..24)
            .map(|index| {
                let resource = ResourceRef {
                    provider: ProviderId::from("proof.generic"),
                    location: Location::new(format!("proof://items/{index}")),
                };
                CollectionEntry::new(
                    resource,
                    ResourceMetadata {
                        name: format!("item-{index:02}"),
                        ..ResourceMetadata::default()
                    },
                    "generic item",
                )
            })
            .collect();
        let application = ApplicationBuilder::new(
            "proof.generic",
            "Generic workflow",
            CollectionSurface::new(
                "proof.collection",
                "workspace.panel",
                "Items",
                Location::new("proof://items"),
                entries,
            ),
        )
        .theme(SemanticTheme::from_toml(THEME).unwrap())
        .keymap(Keymap::from_toml(KEYMAP).unwrap())
        .build()
        .unwrap();
        let mut harness = ApplicationWorkflowHarness::new(application, 80, 10);
        let frames = harness
            .run_script(&[
                WorkflowStep::Key("End".to_owned()),
                WorkflowStep::Capture("end".to_owned()),
                WorkflowStep::Key("Home".to_owned()),
                WorkflowStep::Capture("home".to_owned()),
            ])
            .unwrap();
        assert!(
            frames[0]
                .snapshot
                .text_lines()
                .join("\n")
                .contains("item-23")
        );
        assert!(
            frames[1]
                .snapshot
                .text_lines()
                .join("\n")
                .contains("item-00")
        );
    }
}
