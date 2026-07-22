//! Reference non-filesystem providers and derived resource collections.

use std::{collections::BTreeMap, process::Command, sync::Arc};

use near_core::{
    CapabilitySet, ListPage, ListRequest, Location, MetadataValue, OpenRequest, ProviderError,
    ProviderFuture, ProviderId, RemovableDevice, RemovableDeviceService, ResourceEntry,
    ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef, ResourceStream,
};

const PROCESS_SCHEMES: &[&str] = &["proc"];
const SEARCH_SCHEMES: &[&str] = &["search"];
const PLUGIN_SCHEMES: &[&str] = &["plugin"];
const DEVICE_SCHEMES: &[&str] = &["device"];

pub struct RemovableDeviceProvider {
    service: Arc<dyn RemovableDeviceService>,
}

impl RemovableDeviceProvider {
    pub fn new(service: Arc<dyn RemovableDeviceService>) -> Self {
        Self { service }
    }

    pub fn root() -> Location {
        Location::new("device://attached")
    }

    fn resource(index: usize) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from("near.removable-devices"),
            location: Location::new(format!("device://attached/{index}")),
        }
    }

    fn entry(index: usize, device: &RemovableDevice) -> ResourceEntry {
        let mut metadata = ResourceMetadata {
            name: device.label.clone(),
            kind: ResourceKind::Virtual,
            stable_id: Some(format!("device:{}", device.id)),
            extensions: BTreeMap::from([
                (
                    "near.device.id".to_owned(),
                    MetadataValue::String(device.id.clone()),
                ),
                (
                    "near.device.system-path".to_owned(),
                    MetadataValue::String(device.system_path.clone()),
                ),
                (
                    "near.device.disconnectable".to_owned(),
                    MetadataValue::Boolean(device.can_disconnect),
                ),
            ]),
            ..ResourceMetadata::default()
        };
        if let Some(mount) = &device.mount {
            metadata.extensions.insert(
                "near.device.mount".to_owned(),
                MetadataValue::String(mount.as_str().to_owned()),
            );
        }
        ResourceEntry {
            resource: Self::resource(index),
            metadata,
            details: device.mount.as_ref().map_or_else(
                || device.system_path.clone(),
                |mount| format!("{} — {}", device.system_path, mount.as_str()),
            ),
        }
    }
}

impl ResourceProvider for RemovableDeviceProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.removable-devices")
    }

    fn schemes(&self) -> &[&str] {
        DEVICE_SCHEMES
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
            if location != &Self::root() {
                return Err(ProviderError::Failed(format!(
                    "unknown device location {}",
                    location.as_str()
                )));
            }
            let devices = self.service.list_devices().map_err(ProviderError::Failed)?;
            Ok(ListPage {
                generation: request.generation,
                entries: devices
                    .iter()
                    .enumerate()
                    .map(|(index, device)| Self::entry(index, device))
                    .collect(),
                continuation: None,
                complete: true,
            })
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            let devices = self.service.list_devices().map_err(ProviderError::Failed)?;
            devices
                .iter()
                .enumerate()
                .find(|(index, _)| Self::resource(*index) == *resource)
                .map(|(index, device)| Self::entry(index, device).metadata)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))
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
            let devices = self.service.list_devices().map_err(ProviderError::Failed)?;
            let device = devices
                .iter()
                .enumerate()
                .find(|(index, _)| Self::resource(*index) == *resource)
                .map(|(_, device)| device)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))?;
            let bytes = format!(
                "device: {}\nsystem path: {}\nmount: {}\ndisconnectable: {}\n",
                device.label,
                device.system_path,
                device
                    .mount
                    .as_ref()
                    .map_or("not mounted", Location::as_str),
                device.can_disconnect
            )
            .into_bytes();
            Ok(slice_stream(&bytes, request.offset, request.length))
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        if self.service.list_devices().is_ok_and(|devices| {
            devices
                .iter()
                .enumerate()
                .any(|(index, device)| Self::resource(index) == *resource && device.can_disconnect)
        }) {
            capabilities.insert("device.disconnect");
        }
        capabilities
    }

    fn parent(&self, location: &Location) -> Option<Location> {
        (location != &Self::root()).then(Self::root)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub cpu: String,
    pub command: String,
}

pub struct ProcessProvider {
    records: Vec<ProcessRecord>,
}

impl ProcessProvider {
    pub fn new(records: Vec<ProcessRecord>) -> Self {
        Self { records }
    }

    pub fn local() -> Self {
        let records = Command::new("ps")
            .args(["-axo", "pid=,pcpu=,comm="])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .map(|output| {
                output
                    .lines()
                    .filter_map(parse_process)
                    .take(200)
                    .collect::<Vec<_>>()
            })
            .filter(|records| !records.is_empty())
            .unwrap_or_else(|| {
                vec![ProcessRecord {
                    pid: std::process::id(),
                    cpu: "0.0".to_owned(),
                    command: "near-demo".to_owned(),
                }]
            });
        Self::new(records)
    }

    pub fn root() -> Location {
        Location::new("proc://local")
    }

    fn resource(record: &ProcessRecord) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from("near.process"),
            location: Location::new(format!("proc://local/{}", record.pid)),
        }
    }

    fn metadata(record: &ProcessRecord) -> ResourceMetadata {
        ResourceMetadata {
            name: record.command.clone(),
            kind: ResourceKind::Virtual,
            stable_id: Some(format!("process:local:{}", record.pid)),
            extensions: BTreeMap::from([
                (
                    "near.process.pid".to_owned(),
                    MetadataValue::Integer(i64::from(record.pid)),
                ),
                (
                    "near.process.cpu-percent".to_owned(),
                    MetadataValue::String(record.cpu.clone()),
                ),
            ]),
            ..ResourceMetadata::default()
        }
    }

    fn record_for(&self, resource: &ResourceRef) -> Option<&ProcessRecord> {
        let pid = resource
            .location
            .as_str()
            .rsplit('/')
            .next()?
            .parse::<u32>()
            .ok()?;
        self.records.iter().find(|record| record.pid == pid)
    }
}

impl ResourceProvider for ProcessProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.process")
    }

    fn schemes(&self) -> &[&str] {
        PROCESS_SCHEMES
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
            if location != &Self::root() {
                return Err(ProviderError::Failed(format!(
                    "unsupported process location: {}",
                    location.as_str()
                )));
            }
            let (offset, end) = page_bounds(self.records.len(), &request)?;
            let entries = self.records[offset..end]
                .iter()
                .map(|record| ResourceEntry {
                    resource: Self::resource(record),
                    metadata: Self::metadata(record),
                    details: format!("PID {:>6}  CPU {:>5}%", record.pid, record.cpu),
                })
                .collect();
            Ok(page(&request, entries, end, self.records.len()))
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            self.record_for(resource)
                .map(Self::metadata)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))
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
            let record = self
                .record_for(resource)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))?;
            let bytes = format!(
                "pid: {}\ncpu: {}\ncommand: {}\n",
                record.pid, record.cpu, record.command
            )
            .into_bytes();
            Ok(slice_stream(&bytes, request.offset, request.length))
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        if self.record_for(resource).is_none() {
            return CapabilitySet::default();
        }
        capabilities(["resource.read", "resource.inspect"])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub source: ResourceRef,
    pub metadata: ResourceMetadata,
    pub details: String,
}

pub struct SearchResultsProvider {
    session: String,
    location: Location,
    results: Vec<SearchResult>,
}

impl SearchResultsProvider {
    pub fn new(session: impl Into<String>, results: Vec<SearchResult>) -> Self {
        let session = session.into();
        let location = Location::new(format!("search://sessions/{}", encode_component(&session)));
        Self {
            session,
            location,
            results,
        }
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    fn entry(&self, result: &SearchResult) -> ResourceEntry {
        let mut metadata = result.metadata.clone();
        metadata.extensions.insert(
            "near.search.session".to_owned(),
            MetadataValue::String(self.session.clone()),
        );
        metadata.extensions.insert(
            "near.search.source-provider".to_owned(),
            MetadataValue::String(result.source.provider.as_str().to_owned()),
        );
        ResourceEntry {
            resource: result.source.clone(),
            metadata,
            details: result.details.clone(),
        }
    }
}

impl ResourceProvider for SearchResultsProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.search-results")
    }

    fn schemes(&self) -> &[&str] {
        SEARCH_SCHEMES
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
            if location != &self.location {
                return Err(ProviderError::Failed(format!(
                    "unknown search session: {}",
                    location.as_str()
                )));
            }
            let (offset, end) = page_bounds(self.results.len(), &request)?;
            let entries = self.results[offset..end]
                .iter()
                .map(|result| self.entry(result))
                .collect();
            Ok(page(&request, entries, end, self.results.len()))
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "search results delegate stat to {}",
                resource.provider
            )))
        })
    }

    fn open<'a>(
        &'a self,
        resource: &'a ResourceRef,
        _request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "search results delegate open to {}",
                resource.provider
            )))
        })
    }

    fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
        CapabilitySet::default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
}

pub struct PluginCatalogProvider {
    items: Vec<PluginItem>,
}

impl PluginCatalogProvider {
    pub fn new(items: Vec<PluginItem>) -> Self {
        Self { items }
    }

    pub fn root() -> Location {
        Location::new("plugin://catalog")
    }

    fn resource(item: &PluginItem) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from("near.plugin-catalog"),
            location: Location::new(format!("plugin://catalog/{}", encode_component(&item.id))),
        }
    }

    fn metadata(item: &PluginItem) -> ResourceMetadata {
        ResourceMetadata {
            name: item.name.clone(),
            kind: ResourceKind::Virtual,
            stable_id: Some(format!("plugin:{}", item.id)),
            extensions: BTreeMap::from([
                (
                    "near.plugin.id".to_owned(),
                    MetadataValue::String(item.id.clone()),
                ),
                (
                    "near.plugin.version".to_owned(),
                    MetadataValue::String(item.version.clone()),
                ),
            ]),
            ..ResourceMetadata::default()
        }
    }

    fn item_for(&self, resource: &ResourceRef) -> Option<&PluginItem> {
        self.items
            .iter()
            .find(|item| Self::resource(item) == *resource)
    }
}

impl ResourceProvider for PluginCatalogProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.plugin-catalog")
    }

    fn schemes(&self) -> &[&str] {
        PLUGIN_SCHEMES
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
            if location != &Self::root() {
                return Err(ProviderError::Failed(format!(
                    "unsupported plugin location: {}",
                    location.as_str()
                )));
            }
            let (offset, end) = page_bounds(self.items.len(), &request)?;
            let entries = self.items[offset..end]
                .iter()
                .map(|item| ResourceEntry {
                    resource: Self::resource(item),
                    metadata: Self::metadata(item),
                    details: format!("{} — {}", item.version, item.description),
                })
                .collect();
            Ok(page(&request, entries, end, self.items.len()))
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            self.item_for(resource)
                .map(Self::metadata)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))
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
            let item = self
                .item_for(resource)
                .ok_or_else(|| ProviderError::NotFound(resource.clone()))?;
            let bytes = format!(
                "id: {}\nname: {}\nversion: {}\ndescription: {}\n",
                item.id, item.name, item.version, item.description
            )
            .into_bytes();
            Ok(slice_stream(&bytes, request.offset, request.length))
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        if self.item_for(resource).is_none() {
            return CapabilitySet::default();
        }
        capabilities(["resource.read", "resource.inspect", "plugin.activate"])
    }
}

fn page_bounds(total: usize, request: &ListRequest) -> Result<(usize, usize), ProviderError> {
    let offset = request
        .continuation
        .as_deref()
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|_| ProviderError::Failed("invalid continuation".to_owned()))?;
    if offset > total {
        return Err(ProviderError::Failed(
            "continuation exceeds collection".to_owned(),
        ));
    }
    let end = offset.saturating_add(request.page_size.max(1)).min(total);
    Ok((offset, end))
}

fn page(request: &ListRequest, entries: Vec<ResourceEntry>, end: usize, total: usize) -> ListPage {
    ListPage {
        generation: request.generation,
        entries,
        continuation: (end < total).then(|| end.to_string()),
        complete: end == total,
    }
}

fn slice_stream(bytes: &[u8], offset: u64, length: usize) -> ResourceStream {
    let total_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let end = start.saturating_add(length).min(bytes.len());
    ResourceStream {
        offset,
        bytes: bytes[start..end].to_vec(),
        total_size: Some(total_size),
        complete: end == bytes.len(),
    }
}

fn capabilities<const N: usize>(ids: [&str; N]) -> CapabilitySet {
    let mut capabilities = CapabilitySet::default();
    for id in ids {
        capabilities.insert(id);
    }
    capabilities
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    encoded
}

fn parse_process(line: &str) -> Option<ProcessRecord> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let cpu = fields.next()?.to_owned();
    let command = fields.collect::<Vec<_>>().join(" ");
    (!command.is_empty()).then_some(ProcessRecord { pid, cpu, command })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        future::Future,
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    use near_core::{
        ActionContext, CancellationToken, CapabilityId, CommandId, CommandInvocation, CommandValue,
        DeviceDisconnectReport, ListRequest, ListingGeneration, Location, OpenRequest,
        ProviderError, ProviderFuture, RemovableDevice, RemovableDeviceService, ResourceProvider,
    };
    use near_local_fs::LocalFileProvider;
    use near_ui::{CollectionEntry, CollectionSurface, Surface, SurfaceEvent, UpdateContext};

    use super::{
        PluginCatalogProvider, PluginItem, ProcessProvider, ProcessRecord, RemovableDeviceProvider,
        SearchResult, SearchResultsProvider,
    };

    struct TestDeviceService {
        devices: Vec<RemovableDevice>,
    }

    impl RemovableDeviceService for TestDeviceService {
        fn list_devices(&self) -> Result<Vec<RemovableDevice>, String> {
            Ok(self.devices.clone())
        }

        fn disconnect(&self, _id: &str) -> Result<DeviceDisconnectReport, String> {
            unreachable!("provider listing does not disconnect devices")
        }
    }

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

    fn list(
        provider: &dyn ResourceProvider,
        location: &near_core::Location,
    ) -> Vec<near_core::ResourceEntry> {
        block_on(provider.list(
            location,
            ListRequest {
                generation: ListingGeneration(7),
                continuation: None,
                page_size: 100,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap()
        .entries
    }

    fn surface(
        id: &str,
        location: near_core::Location,
        entries: Vec<near_core::ResourceEntry>,
    ) -> CollectionSurface {
        CollectionSurface::new(
            id,
            "reference.collection",
            id,
            location,
            entries
                .into_iter()
                .map(|entry| CollectionEntry {
                    resource: entry.resource,
                    metadata: entry.metadata,
                    details: entry.details,
                    selected: false,
                })
                .collect(),
        )
    }

    #[test]
    fn removable_devices_preserve_platform_identity_and_gate_disconnect() {
        let provider = RemovableDeviceProvider::new(Arc::new(TestDeviceService {
            devices: vec![
                RemovableDevice {
                    id: "disk4".to_owned(),
                    label: "Backup".to_owned(),
                    mount: Some(Location::new("file:///Volumes/Backup")),
                    system_path: "/dev/disk4".to_owned(),
                    can_disconnect: true,
                },
                RemovableDevice {
                    id: "disk5".to_owned(),
                    label: "Busy".to_owned(),
                    mount: None,
                    system_path: "/dev/disk5".to_owned(),
                    can_disconnect: false,
                },
            ],
        }));

        let entries = list(&provider, &RemovableDeviceProvider::root());
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].metadata.extensions.get("near.device.id"),
            Some(&near_core::MetadataValue::String("disk4".to_owned()))
        );
        assert_eq!(
            entries[0].metadata.extensions.get("near.device.mount"),
            Some(&near_core::MetadataValue::String(
                "file:///Volumes/Backup".to_owned()
            ))
        );
        assert!(
            provider
                .capabilities(&entries[0].resource)
                .contains(&CapabilityId::from("device.disconnect"))
        );
        assert!(
            !provider
                .capabilities(&entries[1].resource)
                .contains(&CapabilityId::from("device.disconnect"))
        );
        let opened = block_on(provider.open(
            &entries[0].resource,
            OpenRequest {
                offset: 0,
                length: usize::MAX,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert!(
            String::from_utf8(opened.bytes)
                .unwrap()
                .contains("/dev/disk4")
        );
    }

    #[test]
    fn local_search_process_and_plugin_items_share_resource_identity_contracts() {
        let root =
            std::env::temp_dir().join(format!("near-reference-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("before.txt");
        let renamed = root.join("after.txt");
        std::fs::write(&original, b"identity").unwrap();
        let local = LocalFileProvider;
        let original_ref = near_core::ResourceRef {
            provider: local.id(),
            location: LocalFileProvider::location(&original),
        };
        let original_metadata = block_on(local.stat(&original_ref)).unwrap();
        std::fs::rename(&original, &renamed).unwrap();
        let renamed_ref = near_core::ResourceRef {
            provider: local.id(),
            location: LocalFileProvider::location(&renamed),
        };
        let renamed_metadata = block_on(local.stat(&renamed_ref)).unwrap();
        assert_ne!(original_ref.location, renamed_ref.location);
        assert_eq!(original_metadata.stable_id, renamed_metadata.stable_id);
        assert_eq!(
            original_metadata.identity_for(&original_ref),
            renamed_metadata.identity_for(&renamed_ref)
        );

        let search = SearchResultsProvider::new(
            "renamed files",
            vec![SearchResult {
                source: renamed_ref.clone(),
                metadata: renamed_metadata.clone(),
                details: "matched file name".to_owned(),
            }],
        );
        let search_entry = list(&search, search.location()).remove(0);
        assert_eq!(search_entry.resource, renamed_ref);
        assert_eq!(search_entry.metadata.stable_id, renamed_metadata.stable_id);
        assert_eq!(
            search_entry.metadata.identity_for(&search_entry.resource),
            renamed_metadata.identity_for(&search_entry.resource)
        );

        let process_a = ProcessProvider::new(vec![ProcessRecord {
            pid: 42,
            cpu: "1.0".to_owned(),
            command: "old-display-name".to_owned(),
        }]);
        let process_b = ProcessProvider::new(vec![ProcessRecord {
            pid: 42,
            cpu: "2.0".to_owned(),
            command: "new-display-name".to_owned(),
        }]);
        let old_process_entry = list(&process_a, &ProcessProvider::root()).remove(0);
        let renamed_process_entry = list(&process_b, &ProcessProvider::root()).remove(0);
        assert_eq!(old_process_entry.resource, renamed_process_entry.resource);
        assert_eq!(
            old_process_entry.metadata.stable_id,
            renamed_process_entry.metadata.stable_id
        );
        assert_ne!(
            old_process_entry.metadata.name,
            renamed_process_entry.metadata.name
        );

        let plugin_a = PluginCatalogProvider::new(vec![PluginItem {
            id: "com.example.tool".to_owned(),
            name: "Old Plugin Name".to_owned(),
            version: "1.0.0".to_owned(),
            description: "first display".to_owned(),
        }]);
        let plugin_b = PluginCatalogProvider::new(vec![PluginItem {
            id: "com.example.tool".to_owned(),
            name: "New Plugin Name".to_owned(),
            version: "1.1.0".to_owned(),
            description: "renamed display".to_owned(),
        }]);
        let old_plugin_entry = list(&plugin_a, &PluginCatalogProvider::root()).remove(0);
        let renamed_plugin_entry = list(&plugin_b, &PluginCatalogProvider::root()).remove(0);
        assert_eq!(old_plugin_entry.resource, renamed_plugin_entry.resource);
        assert_eq!(
            old_plugin_entry.metadata.stable_id,
            renamed_plugin_entry.metadata.stable_id
        );
        assert_ne!(
            old_plugin_entry.metadata.name,
            renamed_plugin_entry.metadata.name
        );

        let resources: Vec<near_core::ResourceRef> = vec![
            search_entry.resource,
            old_process_entry.resource,
            old_plugin_entry.resource,
        ];
        assert_eq!(resources.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_operations_are_absent_from_reference_capability_sets() {
        let process = ProcessProvider::new(vec![ProcessRecord {
            pid: 7,
            cpu: "0.0".to_owned(),
            command: "worker".to_owned(),
        }]);
        let process_resource = list(&process, &ProcessProvider::root()).remove(0).resource;
        let process_capabilities = process.capabilities(&process_resource);
        assert!(process_capabilities.contains(&CapabilityId::from("resource.read")));
        assert!(!process_capabilities.contains(&CapabilityId::from("resource.write")));
        assert!(!process_capabilities.contains(&CapabilityId::from("resource.trash")));

        let plugin = PluginCatalogProvider::new(vec![PluginItem {
            id: "com.example.readonly".to_owned(),
            name: "Readonly".to_owned(),
            version: "1.0.0".to_owned(),
            description: "catalog item".to_owned(),
        }]);
        let plugin_resource = list(&plugin, &PluginCatalogProvider::root())
            .remove(0)
            .resource;
        let plugin_capabilities = plugin.capabilities(&plugin_resource);
        assert!(plugin_capabilities.contains(&CapabilityId::from("plugin.activate")));
        assert!(!plugin_capabilities.contains(&CapabilityId::from("resource.write")));
        assert!(!plugin_capabilities.contains(&CapabilityId::from("resource.delete")));

        let search = SearchResultsProvider::new("empty", Vec::new());
        assert_eq!(search.capabilities(&process_resource).iter().count(), 0);
    }

    #[test]
    fn generic_collection_commands_work_across_reference_provider_types() {
        let process = ProcessProvider::new(vec![
            ProcessRecord {
                pid: 1,
                cpu: "0.1".to_owned(),
                command: "one".to_owned(),
            },
            ProcessRecord {
                pid: 2,
                cpu: "0.2".to_owned(),
                command: "two".to_owned(),
            },
        ]);
        let plugin = PluginCatalogProvider::new(vec![
            PluginItem {
                id: "one".to_owned(),
                name: "One".to_owned(),
                version: "1".to_owned(),
                description: "first".to_owned(),
            },
            PluginItem {
                id: "two".to_owned(),
                name: "Two".to_owned(),
                version: "1".to_owned(),
                description: "second".to_owned(),
            },
        ]);
        let process_entries = list(&process, &ProcessProvider::root());
        let search = SearchResultsProvider::new(
            "process matches",
            process_entries
                .iter()
                .map(|entry| SearchResult {
                    source: entry.resource.clone(),
                    metadata: entry.metadata.clone(),
                    details: entry.details.clone(),
                })
                .collect(),
        );
        let mut surfaces = [
            surface("processes", ProcessProvider::root(), process_entries),
            surface(
                "plugins",
                PluginCatalogProvider::root(),
                list(&plugin, &PluginCatalogProvider::root()),
            ),
            surface(
                "search",
                search.location().clone(),
                list(&search, search.location()),
            ),
        ];
        let action = ActionContext::default();
        let event = SurfaceEvent::Command(CommandInvocation {
            id: CommandId::from("near.collection.move"),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        });
        for collection in &mut surfaces {
            let before = collection.state().current.unwrap();
            collection.update(&event, &mut UpdateContext { action: &action });
            let after = collection.state().current.unwrap();
            assert_ne!(before, after);
        }
    }

    #[test]
    fn bounded_reads_and_cancellation_apply_to_virtual_resources() {
        let plugin = PluginCatalogProvider::new(vec![PluginItem {
            id: "com.example.viewer".to_owned(),
            name: "Viewer".to_owned(),
            version: "2.0".to_owned(),
            description: "virtual resource".to_owned(),
        }]);
        let resource = list(&plugin, &PluginCatalogProvider::root())
            .remove(0)
            .resource;
        let stream = block_on(plugin.open(
            &resource,
            OpenRequest {
                offset: 4,
                length: 8,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(stream.offset, 4);
        assert_eq!(stream.bytes.len(), 8);

        let cancellation = CancellationToken::default();
        cancellation.cancel();
        assert_eq!(
            block_on(plugin.open(
                &resource,
                OpenRequest {
                    offset: 0,
                    length: 8,
                    cancellation,
                },
            )),
            Err(ProviderError::Cancelled)
        );
    }
}
