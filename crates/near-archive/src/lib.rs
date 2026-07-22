//! Provider-backed ZIP browsing and conflict-aware archive operations.

#![allow(clippy::needless_pass_by_value, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs::{self, File},
    io::{Read, Seek, Write as IoWrite},
    path::{Component, Path, PathBuf},
};

use near_core::{
    CancellationToken, CapabilitySet, ListPage, ListRequest, ListingGeneration, Location,
    MetadataValue, OpenRequest, OperationId, ProviderError, ProviderFuture, ProviderId,
    ResourceEntry, ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef, ResourceStream,
    SafetyClass,
};
#[cfg(test)]
use near_local_fs::local_resource;
use near_local_fs::{local_location, local_path};
use near_ops::{
    ConflictAction, ConflictDecision, ConflictResolver, CrossDeviceBehavior,
    ExecutionAuthorization, ExecutionSummary, MetadataPolicy, OperationBackend, OperationEngine,
    OperationIntent, OperationJournal, OperationKind, OperationPlan, OperationPlanner,
    OperationService, PlanPolicies, PlanRequest, PlannedItem, RecoveryPolicy, SymlinkPolicy,
    VerificationPolicy,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const ARCHIVE_PROVIDER_ID: &str = "near.archive";
const ARCHIVE_SCHEMES: &[&str] = &["archive"];
const MAX_OPEN_LENGTH: usize = 4 * 1024 * 1024;
const MAX_ENTRY_SIZE: u64 = 256 * 1024 * 1024;
const MAX_EXTRACTED_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArchiveAddress {
    source: Location,
    inner: String,
}

impl ArchiveAddress {
    fn root(source: &Location) -> Location {
        Location::new(format!(
            "archive://zip/{}!/",
            hex_encode(source.as_str().as_bytes())
        ))
    }

    fn location(&self) -> Location {
        let root = Self::root(&self.source);
        Location::new(format!("{}{}", root.as_str(), self.inner))
    }

    fn parse(location: &Location) -> Result<Self, ProviderError> {
        let value = location
            .as_str()
            .strip_prefix("archive://zip/")
            .ok_or_else(|| ProviderError::Failed("unsupported archive location".to_owned()))?;
        let (encoded_source, inner) = value
            .split_once("!/")
            .ok_or_else(|| ProviderError::Failed("invalid archive location".to_owned()))?;
        let source = String::from_utf8(hex_decode(encoded_source)?).map_err(|error| {
            ProviderError::Failed(format!("archive source is not UTF-8: {error}"))
        })?;
        Ok(Self {
            source: Location::new(source),
            inner: normalize_inner(inner)?,
        })
    }

    fn child(&self, name: &str) -> Self {
        Self {
            source: self.source.clone(),
            inner: if self.inner.is_empty() {
                name.to_owned()
            } else {
                format!("{}/{}", self.inner, name)
            },
        }
    }

    fn parent(&self) -> Option<Self> {
        if self.inner.is_empty() {
            return None;
        }
        let parent = self
            .inner
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_owned();
        Some(Self {
            source: self.source.clone(),
            inner: parent,
        })
    }
}

#[derive(Clone, Debug)]
struct ArchiveNode {
    path: String,
    directory: bool,
    size: u64,
    compressed_size: u64,
}

#[derive(Clone, Debug)]
struct ArchiveIndex {
    nodes: BTreeMap<String, ArchiveNode>,
}

impl ArchiveIndex {
    fn open(source: &Location) -> Result<Self, ProviderError> {
        let path = local_path(source)?;
        let file = File::open(&path).map_err(|error| {
            ProviderError::Failed(format!("cannot open archive {}: {error}", path.display()))
        })?;
        let mut archive = ZipArchive::new(file).map_err(zip_error)?;
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            return Err(ProviderError::Failed(format!(
                "archive contains more than {MAX_ARCHIVE_ENTRIES} entries"
            )));
        }
        let mut nodes = BTreeMap::new();
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(zip_error)?;
            let Some(path) = safe_zip_path(&file) else {
                continue;
            };
            let directory = file.is_dir();
            let size = file.size();
            let compressed_size = file.compressed_size();
            if !directory && size > MAX_ENTRY_SIZE {
                return Err(ProviderError::Failed(format!(
                    "archive entry {path} exceeds the {MAX_ENTRY_SIZE} byte safety limit"
                )));
            }
            insert_parent_nodes(&mut nodes, &path);
            nodes.insert(
                path.clone(),
                ArchiveNode {
                    path,
                    directory,
                    size,
                    compressed_size,
                },
            );
        }
        Ok(Self { nodes })
    }

    fn node(&self, inner: &str) -> Option<ArchiveNode> {
        if inner.is_empty() {
            return Some(ArchiveNode {
                path: String::new(),
                directory: true,
                size: 0,
                compressed_size: 0,
            });
        }
        self.nodes.get(inner).cloned()
    }

    fn children(&self, parent: &str) -> Vec<ArchiveNode> {
        let prefix = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        self.nodes
            .values()
            .filter(|node| {
                node.path
                    .strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZipArchiveProvider;

impl ZipArchiveProvider {
    pub fn root_for(source: &Location) -> Location {
        ArchiveAddress::root(source)
    }

    pub fn format_capabilities() -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        capabilities.insert("archive.create");
        capabilities.insert("archive.update");
        capabilities
    }

    fn resource(address: &ArchiveAddress) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from(ARCHIVE_PROVIDER_ID),
            location: address.location(),
        }
    }

    fn entry(address: &ArchiveAddress, node: &ArchiveNode) -> ResourceEntry {
        let name = node
            .path
            .rsplit_once('/')
            .map_or(node.path.as_str(), |(_, name)| name)
            .to_owned();
        ResourceEntry {
            resource: Self::resource(address),
            metadata: ResourceMetadata {
                name,
                kind: if node.directory {
                    ResourceKind::Directory
                } else {
                    ResourceKind::File
                },
                size: (!node.directory).then_some(node.size),
                stable_id: Some(format!("zip:{}:{}", address.source.as_str(), address.inner)),
                extensions: BTreeMap::from([
                    (
                        "near.archive.source".to_owned(),
                        MetadataValue::String(address.source.as_str().to_owned()),
                    ),
                    (
                        "near.archive.entry".to_owned(),
                        MetadataValue::String(address.inner.clone()),
                    ),
                    (
                        "near.archive.compressed-size".to_owned(),
                        MetadataValue::Integer(
                            i64::try_from(node.compressed_size).unwrap_or(i64::MAX),
                        ),
                    ),
                ]),
                ..ResourceMetadata::default()
            },
            details: if node.directory {
                "archive directory".to_owned()
            } else {
                format!("{} B → {} B", node.size, node.compressed_size)
            },
        }
    }
}

impl ResourceProvider for ZipArchiveProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from(ARCHIVE_PROVIDER_ID)
    }

    fn schemes(&self) -> &[&str] {
        ARCHIVE_SCHEMES
    }

    fn list<'a>(
        &'a self,
        location: &'a Location,
        request: ListRequest,
    ) -> ProviderFuture<'a, ListPage> {
        Box::pin(async move {
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let address = ArchiveAddress::parse(location)?;
            let index = ArchiveIndex::open(&address.source)?;
            let node = index
                .node(&address.inner)
                .ok_or_else(|| ProviderError::NotFound(Self::resource(&address)))?;
            if !node.directory {
                return Err(ProviderError::Unsupported(format!(
                    "{} is not an archive directory",
                    address.inner
                )));
            }
            let entries = index
                .children(&address.inner)
                .into_iter()
                .map(|node| {
                    let child = address.child(
                        node.path
                            .rsplit_once('/')
                            .map_or(node.path.as_str(), |(_, name)| name),
                    );
                    Self::entry(&child, &node)
                })
                .collect::<Vec<_>>();
            let start = request
                .continuation
                .as_deref()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0)
                .min(entries.len());
            let end = start.saturating_add(request.page_size).min(entries.len());
            Ok(ListPage {
                generation: request.generation,
                entries: entries[start..end].to_vec(),
                continuation: (end < entries.len()).then(|| end.to_string()),
                complete: end == entries.len(),
            })
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            let address = ArchiveAddress::parse(&resource.location)?;
            let index = ArchiveIndex::open(&address.source)?;
            let node = index
                .node(&address.inner)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))?;
            Ok(Self::entry(&address, &node).metadata)
        })
    }

    fn open<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream> {
        Box::pin(async move {
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let address = ArchiveAddress::parse(&resource.location)?;
            let path = local_path(&address.source)?;
            let file =
                File::open(&path).map_err(|error| ProviderError::Failed(error.to_string()))?;
            let mut archive = ZipArchive::new(file).map_err(zip_error)?;
            let mut entry = archive.by_name(&address.inner).map_err(zip_error)?;
            if entry.is_dir() {
                return Err(ProviderError::Unsupported(
                    "cannot read an archive directory".to_owned(),
                ));
            }
            if entry.size() > MAX_ENTRY_SIZE {
                return Err(ProviderError::Failed(
                    "archive entry exceeds safety limit".to_owned(),
                ));
            }
            let offset = request.offset.min(entry.size());
            std::io::copy(&mut entry.by_ref().take(offset), &mut std::io::sink())
                .map_err(|error| ProviderError::Failed(error.to_string()))?;
            let mut bytes = Vec::new();
            entry
                .take(request.length.min(MAX_OPEN_LENGTH) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| ProviderError::Failed(error.to_string()))?;
            let total_size = entry_size(&address)?;
            Ok(ResourceStream {
                bytes,
                offset: request.offset,
                total_size: Some(total_size),
                complete: request.offset.saturating_add(request.length as u64) >= total_size,
            })
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        if let Ok(address) = ArchiveAddress::parse(&resource.location)
            && let Ok(index) = ArchiveIndex::open(&address.source)
            && let Some(node) = index.node(&address.inner)
        {
            capabilities.insert("resource.inspect");
            capabilities.insert("resource.copy");
            if node.directory {
                capabilities.insert("resource.list");
                capabilities.insert("archive.update");
            } else {
                capabilities.insert("resource.read");
            }
        }
        capabilities
    }

    fn parent(&self, location: &Location) -> Option<Location> {
        let address = ArchiveAddress::parse(location).ok()?;
        address.parent().map_or_else(
            || {
                local_path(&address.source)
                    .ok()?
                    .parent()
                    .map(local_location)
            },
            |parent| Some(parent.location()),
        )
    }

    fn mount(&self, resource: &ResourceRef) -> Result<Option<Location>, ProviderError> {
        if resource.provider != ProviderId::from("near.local-fs") {
            return Ok(None);
        }
        let path = local_path(&resource.location)?;
        if !path.is_file()
            || !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            return Ok(None);
        }
        ArchiveIndex::open(&resource.location)?;
        Ok(Some(Self::root_for(&resource.location)))
    }

    fn create_container(
        &self,
        parent: &Location,
        name: &str,
    ) -> Result<Option<Location>, ProviderError> {
        if !parent.as_str().starts_with("file:") {
            return Ok(None);
        }
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(ProviderError::Failed(format!(
                "invalid archive name: {name}"
            )));
        }
        if !Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            return Ok(None);
        }
        let parent = local_path(parent)?;
        Ok(Some(Self::root_for(&local_location(&parent.join(name)))))
    }

    fn container_capabilities(&self, location: &Location) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        if location.as_str().starts_with("file:") {
            capabilities.insert("archive.create");
        }
        if location.as_str().starts_with("archive://zip/") {
            capabilities.insert("archive.update");
        }
        capabilities
    }

    fn locations(&self) -> Vec<near_core::ProviderLocation> {
        Vec::new()
    }
}

pub struct ArchiveOperationService {
    fallback: Box<dyn OperationService>,
    planner: OperationPlanner,
    engine: OperationEngine<ArchiveOperationBackend>,
    archive_plans: BTreeSet<OperationId>,
}

impl ArchiveOperationService {
    pub fn new(fallback: impl OperationService + 'static, journal: OperationJournal) -> Self {
        Self {
            fallback: Box::new(fallback),
            planner: OperationPlanner::with_prefix("near.archive.operation"),
            engine: OperationEngine::new(ArchiveOperationBackend, journal),
            archive_plans: BTreeSet::new(),
        }
    }

    fn archive_copy_plan(
        &mut self,
        sources: &[ResourceRef],
        destination: &Location,
        generation: ListingGeneration,
    ) -> Result<Option<OperationPlan>, String> {
        let extracting = sources
            .iter()
            .all(|source| source.provider == ProviderId::from(ARCHIVE_PROVIDER_ID));
        let adding = destination.as_str().starts_with("archive://zip/")
            && sources
                .iter()
                .all(|source| source.provider == ProviderId::from("near.local-fs"));
        if !extracting && !adding {
            return Ok(None);
        }
        let items = if extracting {
            plan_extraction(sources, destination)?
        } else {
            plan_addition(sources, destination)?
        };
        self.planner
            .plan(PlanRequest {
                kind: OperationKind::Copy,
                items,
                destination: Some(destination.clone()),
                policies: PlanPolicies {
                    conflict: near_ops::ConflictPolicy::Ask,
                    metadata: MetadataPolicy::ContentsOnly,
                    verification: VerificationPolicy::SizeAndTime,
                    recovery: RecoveryPolicy::JournalOnly,
                    cross_device: CrossDeviceBehavior::NotApplicable,
                    symlink: SymlinkPolicy::Reject,
                },
                safety: SafetyClass::Confirmable,
                context_generation: generation,
                high_impact: false,
            })
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

impl OperationService for ArchiveOperationService {
    fn plan(
        &mut self,
        intent: OperationIntent,
        generation: ListingGeneration,
    ) -> Result<OperationPlan, String> {
        if let OperationIntent::CopyTo {
            sources,
            destination,
        } = &intent
            && let Some(plan) = self.archive_copy_plan(sources, destination, generation)?
        {
            let id = self
                .engine
                .record(plan.clone())
                .map_err(|error| error.to_string())?;
            self.archive_plans.insert(id);
            return Ok(plan);
        }
        if matches!(&intent, OperationIntent::MoveTo { sources, destination }
            if sources.iter().any(|source| source.provider == ProviderId::from(ARCHIVE_PROVIDER_ID))
                || destination.as_str().starts_with("archive://zip/"))
        {
            return Err(
                "moving into or out of archives is unsupported; copy and delete explicitly"
                    .to_owned(),
            );
        }
        self.fallback.plan(intent, generation)
    }

    fn execute(
        &mut self,
        plan: &OperationId,
        authorization: ExecutionAuthorization,
        cancellation: &CancellationToken,
        conflict: ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        if !self.archive_plans.contains(plan) {
            return self
                .fallback
                .execute(plan, authorization, cancellation, conflict);
        }
        let mut resolver = FixedConflictResolver(conflict);
        self.engine
            .execute(plan, authorization, cancellation, &mut resolver)
            .map_err(|error| error.to_string())
    }

    fn execute_elevated(
        &mut self,
        plan: &OperationId,
        authorization: ExecutionAuthorization,
        conflict: ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        if self.archive_plans.contains(plan) {
            return Err("archive operations cannot be retried with local elevation".to_owned());
        }
        self.fallback
            .execute_elevated(plan, authorization, conflict)
    }
}

struct FixedConflictResolver(ConflictDecision);

impl ConflictResolver for FixedConflictResolver {
    fn decide(&mut self, _plan: &OperationPlan, _item: &PlannedItem) -> ConflictDecision {
        self.0
    }
}

struct ArchiveOperationBackend;

impl OperationBackend for ArchiveOperationBackend {
    fn target_exists(&self, target: &Location) -> bool {
        if target.as_str().starts_with("archive://zip/") {
            return ArchiveAddress::parse(target)
                .ok()
                .and_then(|address| {
                    ArchiveIndex::open(&address.source)
                        .ok()
                        .map(|index| (address, index))
                })
                .is_some_and(|(address, index)| index.node(&address.inner).is_some());
        }
        local_path(target).is_ok_and(|path| path.exists())
    }

    fn execute(
        &mut self,
        _plan: &OperationPlan,
        item: &PlannedItem,
        action: Option<ConflictAction>,
        cancellation: &CancellationToken,
    ) -> Result<near_ops::ExecutionEffect, String> {
        match item.parameters.get("near.archive.mode").map(String::as_str) {
            Some("extract") => extract_item(item, action, cancellation),
            Some("add") => add_item(item, action, cancellation),
            _ => Err("archive plan item has no execution mode".to_owned()),
        }?;
        Ok(near_ops::ExecutionEffect::default())
    }
}

fn plan_extraction(
    sources: &[ResourceRef],
    destination: &Location,
) -> Result<Vec<PlannedItem>, String> {
    let destination_path = local_path(destination).map_err(|error| error.to_string())?;
    sources
        .iter()
        .map(|source| {
            let address =
                ArchiveAddress::parse(&source.location).map_err(|error| error.to_string())?;
            let index = ArchiveIndex::open(&address.source).map_err(|error| error.to_string())?;
            let node = index
                .node(&address.inner)
                .ok_or_else(|| format!("archive entry not found: {}", address.inner))?;
            let name = Path::new(&address.inner)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "archive entry has no portable file name".to_owned())?;
            let target = local_location(&destination_path.join(name));
            Ok(PlannedItem {
                source: Some(source.clone()),
                conflict_expected: local_path(&target).is_ok_and(|path| path.exists()),
                target,
                recursive: node.directory,
                parameters: BTreeMap::from([(
                    "near.archive.mode".to_owned(),
                    "extract".to_owned(),
                )]),
            })
        })
        .collect()
}

fn plan_addition(
    sources: &[ResourceRef],
    destination: &Location,
) -> Result<Vec<PlannedItem>, String> {
    let root = ArchiveAddress::parse(destination).map_err(|error| error.to_string())?;
    let index = ArchiveIndex::open(&root.source).ok();
    sources
        .iter()
        .map(|source| {
            let path = local_path(&source.location).map_err(|error| error.to_string())?;
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "local source has no portable file name".to_owned())?;
            let target = root.child(name).location();
            Ok(PlannedItem {
                source: Some(source.clone()),
                conflict_expected: index
                    .as_ref()
                    .is_some_and(|index| index.node(name).is_some()),
                target,
                recursive: path.is_dir(),
                parameters: BTreeMap::from([("near.archive.mode".to_owned(), "add".to_owned())]),
            })
        })
        .collect()
}

fn extract_item(
    item: &PlannedItem,
    action: Option<ConflictAction>,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let source = item
        .source
        .as_ref()
        .ok_or("archive extraction has no source")?;
    let address = ArchiveAddress::parse(&source.location).map_err(|error| error.to_string())?;
    let mut target = local_path(&item.target).map_err(|error| error.to_string())?;
    resolve_local_conflict(&mut target, action)?;
    let archive_path = local_path(&address.source).map_err(|error| error.to_string())?;
    let file = File::open(&archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let prefix = if item.recursive {
        format!("{}/", address.inner.trim_end_matches('/'))
    } else {
        address.inner.clone()
    };
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        if cancellation.is_cancelled() {
            return Err("archive extraction cancelled".to_owned());
        }
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(path) = safe_zip_path(&entry) else {
            continue;
        };
        let relative = if item.recursive {
            let Some(relative) = path.strip_prefix(&prefix) else {
                continue;
            };
            if relative.is_empty() {
                continue;
            }
            relative
        } else if path == prefix {
            Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("archive entry has no file name")?
        } else {
            continue;
        };
        extracted = extracted.saturating_add(entry.size());
        if extracted > MAX_EXTRACTED_SIZE {
            return Err(format!(
                "archive extraction exceeds {MAX_EXTRACTED_SIZE} bytes"
            ));
        }
        let output = if item.recursive {
            target.join(relative)
        } else {
            target.clone()
        };
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            continue;
        }
        if is_zip_symlink(&entry) {
            return Err(format!("archive symlink entries are rejected: {path}"));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = output.with_extension(format!("near-extract-{}", std::process::id()));
        let mut destination = File::create(&temporary).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut destination).map_err(|error| error.to_string())?;
        destination.flush().map_err(|error| error.to_string())?;
        commit_file(&temporary, &output)?;
    }
    Ok(())
}

fn add_item(
    item: &PlannedItem,
    action: Option<ConflictAction>,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let source = item
        .source
        .as_ref()
        .ok_or("archive addition has no source")?;
    let source_path = local_path(&source.location).map_err(|error| error.to_string())?;
    let address = ArchiveAddress::parse(&item.target).map_err(|error| error.to_string())?;
    let archive_path = local_path(&address.source).map_err(|error| error.to_string())?;
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    rebuild_zip(
        &archive_path,
        &source_path,
        &address.inner,
        action,
        cancellation,
    )
}

fn rebuild_zip(
    archive_path: &Path,
    source: &Path,
    target_name: &str,
    action: Option<ConflictAction>,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let temporary = archive_path.with_extension(format!("near-archive-{}", std::process::id()));
    let output = File::create(&temporary).map_err(|error| error.to_string())?;
    let mut writer = ZipWriter::new(output);
    let mut replacement = target_name.to_owned();
    if action == Some(ConflictAction::Rename) {
        replacement = unique_archive_name(archive_path, target_name)?;
    }
    if archive_path.exists() {
        let input = File::open(archive_path).map_err(|error| error.to_string())?;
        let mut existing = ZipArchive::new(input).map_err(|error| error.to_string())?;
        for index in 0..existing.len() {
            if cancellation.is_cancelled() {
                return Err("archive update cancelled".to_owned());
            }
            let mut entry = existing
                .by_index(index)
                .map_err(|error| error.to_string())?;
            let Some(path) = safe_zip_path(&entry) else {
                continue;
            };
            if (path == target_name || path.starts_with(&format!("{target_name}/")))
                && action == Some(ConflictAction::Replace)
            {
                continue;
            }
            let options = SimpleFileOptions::default().compression_method(entry.compression());
            if entry.is_dir() {
                writer
                    .add_directory(path, options)
                    .map_err(|error| error.to_string())?;
            } else {
                writer
                    .start_file(path, options)
                    .map_err(|error| error.to_string())?;
                std::io::copy(&mut entry, &mut writer).map_err(|error| error.to_string())?;
            }
        }
    }
    write_source_to_zip(&mut writer, source, &replacement, cancellation)?;
    writer.finish().map_err(|error| error.to_string())?;
    commit_file(&temporary, archive_path)
}

fn write_source_to_zip<W: IoWrite + Seek>(
    writer: &mut ZipWriter<W>,
    source: &Path,
    target: &str,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("archive update cancelled".to_owned());
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing to archive symlink {}", source.display()));
    }
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    if metadata.is_dir() {
        writer
            .add_directory(format!("{}/", target.trim_end_matches('/')), options)
            .map_err(|error| error.to_string())?;
        for child in fs::read_dir(source).map_err(|error| error.to_string())? {
            let child = child.map_err(|error| error.to_string())?;
            let name = child
                .file_name()
                .to_str()
                .ok_or_else(|| "archive names must be UTF-8".to_owned())?
                .to_owned();
            write_source_to_zip(
                writer,
                &child.path(),
                &format!("{}/{name}", target.trim_end_matches('/')),
                cancellation,
            )?;
        }
    } else {
        writer
            .start_file(target, options)
            .map_err(|error| error.to_string())?;
        let mut input = File::open(source).map_err(|error| error.to_string())?;
        std::io::copy(&mut input, writer).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn resolve_local_conflict(
    target: &mut PathBuf,
    action: Option<ConflictAction>,
) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    match action {
        Some(ConflictAction::Replace) => {
            if target.is_dir() {
                fs::remove_dir_all(&*target).map_err(|error| error.to_string())?;
            }
            Ok(())
        }
        Some(ConflictAction::Rename) => {
            *target = unique_local_path(target);
            Ok(())
        }
        Some(ConflictAction::Skip | ConflictAction::Cancel) | None => Ok(()),
    }
}

#[cfg(unix)]
fn commit_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(temporary, destination).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn commit_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    if !destination.exists() {
        return fs::rename(temporary, destination).map_err(|error| error.to_string());
    }
    let backup = destination.with_extension(format!("near-backup-{}", std::process::id()));
    fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error.to_string());
    }
    fs::remove_file(backup).map_err(|error| error.to_string())
}

fn unique_local_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("item");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let name = extension.map_or_else(
            || format!("{stem} ({index})"),
            |extension| format!("{stem} ({index}).{extension}"),
        );
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_archive_name(path: &Path, target: &str) -> Result<String, String> {
    let existing = if path.exists() {
        ArchiveIndex::open(&local_location(path))
            .map_err(|error| error.to_string())?
            .nodes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let source = Path::new(target);
    let parent = source
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty());
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("item");
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let name = extension.map_or_else(
            || format!("{stem} ({index})"),
            |extension| format!("{stem} ({index}).{extension}"),
        );
        let candidate = parent
            .as_ref()
            .map_or(name.clone(), |parent| format!("{parent}/{name}"));
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!()
}

fn entry_size(address: &ArchiveAddress) -> Result<u64, ProviderError> {
    ArchiveIndex::open(&address.source)?
        .node(&address.inner)
        .map(|node| node.size)
        .ok_or_else(|| ProviderError::NotFound(ZipArchiveProvider::resource(address)))
}

fn insert_parent_nodes(nodes: &mut BTreeMap<String, ArchiveNode>, path: &str) {
    let mut parent = Path::new(path).parent();
    while let Some(path) = parent {
        let value = path.to_string_lossy().replace('\\', "/");
        if value.is_empty() {
            break;
        }
        nodes.entry(value.clone()).or_insert(ArchiveNode {
            path: value,
            directory: true,
            size: 0,
            compressed_size: 0,
        });
        parent = path.parent();
    }
}

fn safe_zip_path<R: Read>(file: &zip::read::ZipFile<'_, R>) -> Option<String> {
    let path = file.enclosed_name()?;
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalize_inner(normalized.trim_end_matches('/')).ok()
}

fn is_zip_symlink<R: Read>(file: &zip::read::ZipFile<'_, R>) -> bool {
    file.unix_mode()
        .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

fn normalize_inner(value: &str) -> Result<String, ProviderError> {
    let path = Path::new(value);
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| ProviderError::Failed("archive path is not UTF-8".to_owned()))?,
            ),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(ProviderError::Failed("unsafe archive path".to_owned()));
            }
        }
    }
    Ok(parts.join("/"))
}

fn zip_error(error: zip::result::ZipError) -> ProviderError {
    ProviderError::Failed(error.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut encoded, byte| {
            let _ = write!(encoded, "{byte:02x}");
            encoded
        },
    )
}

fn hex_decode(value: &str) -> Result<Vec<u8>, ProviderError> {
    if !value.len().is_multiple_of(2) {
        return Err(ProviderError::Failed(
            "invalid archive source encoding".to_owned(),
        ));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| ProviderError::Failed("invalid archive source encoding".to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    use near_core::{
        CancellationToken, ListRequest, ListingGeneration, OpenRequest, ProviderFuture,
    };
    use near_local_fs::LocalOperationService;
    use near_ops::{ConflictAction, DecisionScope, ExecutionAuthorization, OperationJournal};

    use super::*;

    fn block_on<T>(mut future: ProviderFuture<'_, T>) -> Result<T, ProviderError> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(result) => return result,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn fixture(root: &Path) -> PathBuf {
        let path = root.join("fixture.zip");
        let file = File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer.add_directory("folder/", options).unwrap();
        writer.start_file("folder/nested.txt", options).unwrap();
        writer.write_all(b"nested archive text").unwrap();
        writer.start_file("root.txt", options).unwrap();
        writer.write_all(b"root archive text").unwrap();
        writer.start_file("../escape.txt", options).unwrap();
        writer.write_all(b"must never extract").unwrap();
        writer.finish().unwrap();
        path
    }

    fn list(provider: ZipArchiveProvider, location: &Location) -> Vec<ResourceEntry> {
        block_on(provider.list(
            location,
            ListRequest {
                generation: ListingGeneration(1),
                continuation: None,
                page_size: 100,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap()
        .entries
    }

    #[test]
    fn zip_mount_browses_reads_and_navigates_without_exposing_unsafe_paths() {
        let root =
            std::env::temp_dir().join(format!("near-archive-provider-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = fixture(&root);
        let source = local_resource(&path);
        let provider = ZipArchiveProvider;
        let mounted = provider.mount(&source).unwrap().unwrap();
        let entries = list(provider, &mounted);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.metadata.name.as_str())
                .collect::<Vec<_>>(),
            ["folder", "root.txt"]
        );
        assert!(
            !entries
                .iter()
                .any(|entry| entry.metadata.name == "escape.txt")
        );
        let folder = entries
            .iter()
            .find(|entry| entry.metadata.name == "folder")
            .unwrap();
        let nested = list(provider, &folder.resource.location);
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].metadata.name, "nested.txt");
        let stream = block_on(provider.open(
            &nested[0].resource,
            OpenRequest {
                offset: 7,
                length: 7,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(stream.bytes, b"archive".to_vec());
        assert_eq!(provider.parent(&folder.resource.location), Some(mounted));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_operations_extract_add_replace_and_rename_through_normal_plans() {
        let root = std::env::temp_dir().join(format!("near-archive-ops-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source_dir = root.join("source");
        let output_dir = root.join("output");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();
        let archive_path = fixture(&root);
        let provider = ZipArchiveProvider;
        let archive_root = provider
            .mount(&local_resource(&archive_path))
            .unwrap()
            .unwrap();
        let root_entry = list(provider, &archive_root)
            .into_iter()
            .find(|entry| entry.metadata.name == "root.txt")
            .unwrap();
        fs::write(output_dir.join("root.txt"), b"existing").unwrap();

        let fallback = LocalOperationService::new(root.join("trash"), OperationJournal::memory());
        let mut service = ArchiveOperationService::new(fallback, OperationJournal::memory());
        let extract = service
            .plan(
                OperationIntent::CopyTo {
                    sources: vec![root_entry.resource],
                    destination: local_location(&output_dir),
                },
                ListingGeneration(9),
            )
            .unwrap();
        assert_eq!(extract.conflict_count(), 1);
        let summary = service
            .execute(
                extract.id(),
                ExecutionAuthorization {
                    context_generation: ListingGeneration(9),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                ConflictDecision {
                    action: ConflictAction::Rename,
                    scope: DecisionScope::Remaining,
                },
            )
            .unwrap();
        assert_eq!(summary.completed(), 1);
        assert_eq!(
            fs::read(output_dir.join("root (1).txt")).unwrap(),
            b"root archive text"
        );
        assert!(!root.join("escape.txt").exists());

        let addition = source_dir.join("added.txt");
        fs::write(&addition, b"first version").unwrap();
        let add = service
            .plan(
                OperationIntent::CopyTo {
                    sources: vec![local_resource(&addition)],
                    destination: archive_root.clone(),
                },
                ListingGeneration(10),
            )
            .unwrap();
        service
            .execute(
                add.id(),
                ExecutionAuthorization {
                    context_generation: ListingGeneration(10),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                ConflictDecision {
                    action: ConflictAction::Replace,
                    scope: DecisionScope::Remaining,
                },
            )
            .unwrap();
        fs::write(&addition, b"second version").unwrap();
        let replace = service
            .plan(
                OperationIntent::CopyTo {
                    sources: vec![local_resource(&addition)],
                    destination: archive_root.clone(),
                },
                ListingGeneration(11),
            )
            .unwrap();
        assert_eq!(replace.conflict_count(), 1);
        service
            .execute(
                replace.id(),
                ExecutionAuthorization {
                    context_generation: ListingGeneration(11),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                ConflictDecision {
                    action: ConflictAction::Replace,
                    scope: DecisionScope::Remaining,
                },
            )
            .unwrap();
        let added = list(provider, &archive_root)
            .into_iter()
            .find(|entry| entry.metadata.name == "added.txt")
            .unwrap();
        let bytes = block_on(provider.open(
            &added.resource,
            OpenRequest {
                offset: 0,
                length: 100,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(bytes.bytes, b"second version");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_creation_is_format_and_capability_driven() {
        let root = std::env::temp_dir().join(format!("near-archive-create-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let provider = ZipArchiveProvider;
        let destination = provider
            .create_container(&local_location(&root), "bundle.zip")
            .unwrap()
            .unwrap();
        assert!(destination.as_str().starts_with("archive://zip/"));
        assert!(
            ZipArchiveProvider::format_capabilities()
                .contains(&near_core::CapabilityId::from("archive.create"))
        );
        assert!(
            provider
                .create_container(&local_location(&root), "bundle.tar")
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
