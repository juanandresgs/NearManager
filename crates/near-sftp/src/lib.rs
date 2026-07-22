//! SSH-agent-authenticated SFTP resources with strict host verification.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Component, Path},
    sync::{Arc, Mutex},
    time::Duration,
};

use near_core::{
    CancellationToken, CapabilitySet, ListPage, ListRequest, Location, OpenRequest, OperationId,
    PermissionSummary, ProviderError, ProviderFuture, ProviderId, ProviderLocation, ResourceEntry,
    ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef, ResourceStream, ResourceVersion,
    WriteRequest,
};
use near_local_fs::{local_location, local_path, local_resource};
use near_ops::{
    ConflictAction, ConflictDecision, ConflictResolver, CrossDeviceBehavior,
    ExecutionAuthorization, ExecutionSummary, MetadataPolicy, OperationBackend, OperationEngine,
    OperationIntent, OperationJournal, OperationKind, OperationPlan, OperationPlanner,
    OperationService, PlanPolicies, PlanRequest, PlannedItem, RecoveryPolicy, SymlinkPolicy,
    VerificationPolicy,
};
use serde::{Deserialize, Serialize};
use ssh2::{CheckResult, FileStat, KnownHostFileKind, OpenFlags, OpenType, Session, Sftp};
use thiserror::Error;

const PROVIDER_ID: &str = "near.sftp";
const SCHEMES: &[&str] = &["sftp"];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SftpConnectionProfile {
    pub id: String,
    pub label: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default = "default_root")]
    pub root: String,
    pub known_hosts: String,
}

const fn default_port() -> u16 {
    22
}

fn default_root() -> String {
    "/".to_owned()
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SftpConnectionDocument {
    #[serde(default = "connection_schema")]
    pub schema: u16,
    #[serde(default)]
    pub connection: Vec<SftpConnectionProfile>,
}

const fn connection_schema() -> u16 {
    1
}

impl SftpConnectionDocument {
    /// Parses and validates a versioned user connection catalog.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed TOML, duplicate IDs, unsafe roots, or missing host identity
    /// configuration.
    pub fn from_toml(source: &str) -> Result<Self, SftpError> {
        let document: Self =
            toml::from_str(source).map_err(|error| SftpError::configuration(error.to_string()))?;
        if document.schema != connection_schema() {
            return Err(SftpError::configuration(format!(
                "unsupported SFTP connection schema {}",
                document.schema
            )));
        }
        let mut ids = BTreeSet::new();
        for profile in &document.connection {
            profile.validate()?;
            if !ids.insert(profile.id.as_str()) {
                return Err(SftpError::configuration(format!(
                    "duplicate SFTP connection ID {}",
                    profile.id
                )));
            }
        }
        Ok(document)
    }
}

impl SftpConnectionProfile {
    fn validate(&self) -> Result<(), SftpError> {
        if self.id.trim().is_empty()
            || self.label.trim().is_empty()
            || self.host.trim().is_empty()
            || self.username.trim().is_empty()
        {
            return Err(SftpError::configuration(
                "SFTP id, label, host, and username are required".to_owned(),
            ));
        }
        if self.port == 0 {
            return Err(SftpError::configuration(
                "SFTP port must be greater than zero".to_owned(),
            ));
        }
        if self.known_hosts.trim().is_empty() {
            return Err(SftpError::configuration(
                "SFTP known_hosts path is required".to_owned(),
            ));
        }
        normalize_remote_path(&self.root)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpFailureKind {
    Configuration,
    HostVerification,
    Authentication,
    Connection,
    Resource,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct SftpError {
    pub kind: SftpFailureKind,
    pub message: String,
}

impl SftpError {
    fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: SftpFailureKind::Configuration,
            message: message.into(),
        }
    }

    fn connection(message: impl Into<String>) -> Self {
        Self {
            kind: SftpFailureKind::Connection,
            message: message.into(),
        }
    }

    fn resource(message: impl Into<String>) -> Self {
        Self {
            kind: SftpFailureKind::Resource,
            message: message.into(),
        }
    }

    fn retryable(&self) -> bool {
        self.kind == SftpFailureKind::Connection
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SftpMetadata {
    pub kind: ResourceKind,
    pub size: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    pub readonly: bool,
    pub executable: bool,
}

#[allow(clippy::missing_errors_doc)]
pub trait SftpTransport: Send + Sync {
    fn list(&self, path: &str) -> Result<Vec<(String, SftpMetadata)>, SftpError>;
    fn stat(&self, path: &str) -> Result<SftpMetadata, SftpError>;
    fn read(&self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>, SftpError>;
    fn write_chunk(
        &self,
        path: &str,
        offset: u64,
        bytes: &[u8],
        truncate: bool,
    ) -> Result<(), SftpError>;
    fn create_directory(&self, path: &str) -> Result<(), SftpError>;
    fn rename(&self, source: &str, destination: &str) -> Result<(), SftpError>;
    fn remove(&self, path: &str, directory: bool) -> Result<(), SftpError>;
    fn disconnect(&self);
}

#[allow(clippy::missing_errors_doc)]
pub trait SftpTransportFactory: Send + Sync {
    fn connect(&self, profile: &SftpConnectionProfile)
    -> Result<Arc<dyn SftpTransport>, SftpError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SshAgentTransportFactory;

impl SftpTransportFactory for SshAgentTransportFactory {
    fn connect(
        &self,
        profile: &SftpConnectionProfile,
    ) -> Result<Arc<dyn SftpTransport>, SftpError> {
        profile.validate()?;
        let address = (profile.host.as_str(), profile.port)
            .to_socket_addrs()
            .map_err(|error| SftpError::connection(error.to_string()))?
            .next()
            .ok_or_else(|| SftpError::connection("SFTP host resolved to no addresses"))?;
        let tcp = TcpStream::connect_timeout(&address, Duration::from_secs(15))
            .map_err(|error| SftpError::connection(error.to_string()))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| SftpError::connection(error.to_string()))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| SftpError::connection(error.to_string()))?;
        let mut session =
            Session::new().map_err(|error| SftpError::connection(error.to_string()))?;
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|error| SftpError::connection(error.to_string()))?;
        verify_host(profile, &session)?;
        session
            .userauth_agent(&profile.username)
            .map_err(|error| SftpError {
                kind: SftpFailureKind::Authentication,
                message: format!(
                    "SSH agent authentication failed for {}@{}: {error}",
                    profile.username, profile.host
                ),
            })?;
        if !session.authenticated() {
            return Err(SftpError {
                kind: SftpFailureKind::Authentication,
                message: "SSH agent did not authenticate the SFTP session".to_owned(),
            });
        }
        let sftp = session
            .sftp()
            .map_err(|error| SftpError::connection(error.to_string()))?;
        Ok(Arc::new(Ssh2Transport { session, sftp }))
    }
}

fn verify_host(profile: &SftpConnectionProfile, session: &Session) -> Result<(), SftpError> {
    let (key, _) = session.host_key().ok_or_else(|| SftpError {
        kind: SftpFailureKind::HostVerification,
        message: "SSH server did not provide a host key".to_owned(),
    })?;
    let mut known_hosts = session.known_hosts().map_err(|error| SftpError {
        kind: SftpFailureKind::HostVerification,
        message: error.to_string(),
    })?;
    known_hosts
        .read_file(Path::new(&profile.known_hosts), KnownHostFileKind::OpenSSH)
        .map_err(|error| SftpError {
            kind: SftpFailureKind::HostVerification,
            message: format!("cannot read {}: {error}", profile.known_hosts),
        })?;
    match known_hosts.check_port(&profile.host, profile.port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => Err(SftpError {
            kind: SftpFailureKind::HostVerification,
            message: format!(
                "{}:{} is absent from {}",
                profile.host, profile.port, profile.known_hosts
            ),
        }),
        CheckResult::Mismatch => Err(SftpError {
            kind: SftpFailureKind::HostVerification,
            message: format!("host key mismatch for {}:{}", profile.host, profile.port),
        }),
        CheckResult::Failure => Err(SftpError {
            kind: SftpFailureKind::HostVerification,
            message: format!(
                "host key verification failed for {}:{}",
                profile.host, profile.port
            ),
        }),
    }
}

struct Ssh2Transport {
    session: Session,
    sftp: Sftp,
}

impl SftpTransport for Ssh2Transport {
    fn list(&self, path: &str) -> Result<Vec<(String, SftpMetadata)>, SftpError> {
        self.sftp
            .readdir(Path::new(path))
            .map_err(map_ssh_resource)
            .map(|entries| {
                entries
                    .into_iter()
                    .filter_map(|(path, stat)| {
                        let name = path.file_name()?.to_string_lossy().into_owned();
                        Some((name, metadata_from_stat(&stat)))
                    })
                    .collect()
            })
    }

    fn stat(&self, path: &str) -> Result<SftpMetadata, SftpError> {
        self.sftp
            .lstat(Path::new(path))
            .map(|stat| metadata_from_stat(&stat))
            .map_err(map_ssh_resource)
    }

    fn read(&self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>, SftpError> {
        let mut file = self.sftp.open(Path::new(path)).map_err(map_ssh_resource)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| SftpError::resource(error.to_string()))?;
        let mut bytes = vec![0; length];
        let read = file
            .read(&mut bytes)
            .map_err(|error| SftpError::resource(error.to_string()))?;
        bytes.truncate(read);
        Ok(bytes)
    }

    fn write_chunk(
        &self,
        path: &str,
        offset: u64,
        bytes: &[u8],
        truncate: bool,
    ) -> Result<(), SftpError> {
        let mut flags = OpenFlags::WRITE | OpenFlags::CREATE;
        if truncate {
            flags |= OpenFlags::TRUNCATE;
        }
        let mut file = self
            .sftp
            .open_mode(Path::new(path), flags, 0o644, OpenType::File)
            .map_err(map_ssh_resource)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| SftpError::resource(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| SftpError::resource(error.to_string()))
    }

    fn create_directory(&self, path: &str) -> Result<(), SftpError> {
        self.sftp
            .mkdir(Path::new(path), 0o755)
            .map_err(map_ssh_resource)
    }

    fn rename(&self, source: &str, destination: &str) -> Result<(), SftpError> {
        self.sftp
            .rename(Path::new(source), Path::new(destination), None)
            .map_err(map_ssh_resource)
    }

    fn remove(&self, path: &str, directory: bool) -> Result<(), SftpError> {
        if directory {
            self.sftp.rmdir(Path::new(path)).map_err(map_ssh_resource)
        } else {
            self.sftp.unlink(Path::new(path)).map_err(map_ssh_resource)
        }
    }

    fn disconnect(&self) {
        let _ = self.session.disconnect(None, "Near SFTP disconnect", None);
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_ssh_resource(error: ssh2::Error) -> SftpError {
    SftpError::resource(error.to_string())
}

fn metadata_from_stat(stat: &FileStat) -> SftpMetadata {
    let permissions = stat.perm.unwrap_or_default();
    let kind = match permissions & 0o170_000 {
        0o040_000 => ResourceKind::Directory,
        0o120_000 => ResourceKind::Symlink,
        0o100_000 => ResourceKind::File,
        _ => ResourceKind::Other,
    };
    SftpMetadata {
        kind,
        size: stat.size,
        modified_unix_ms: stat.mtime.and_then(|seconds| {
            i64::try_from(seconds)
                .ok()
                .and_then(|seconds| seconds.checked_mul(1_000))
        }),
        readonly: permissions & 0o200 == 0,
        executable: permissions & 0o111 != 0,
    }
}

pub struct SftpProvider {
    profiles: BTreeMap<String, SftpConnectionProfile>,
    factory: Arc<dyn SftpTransportFactory>,
    sessions: Mutex<BTreeMap<String, Arc<dyn SftpTransport>>>,
}

impl SftpProvider {
    /// Creates a provider that authenticates through the platform SSH agent.
    ///
    /// # Errors
    ///
    /// Returns profile validation errors.
    pub fn new(document: SftpConnectionDocument) -> Result<Self, SftpError> {
        Self::with_factory(document, Arc::new(SshAgentTransportFactory))
    }

    /// Creates a provider with an injected transport factory.
    ///
    /// # Errors
    ///
    /// Returns profile validation errors.
    pub fn with_factory(
        document: SftpConnectionDocument,
        factory: Arc<dyn SftpTransportFactory>,
    ) -> Result<Self, SftpError> {
        let mut profiles = BTreeMap::new();
        for profile in document.connection {
            profile.validate()?;
            if profiles.insert(profile.id.clone(), profile).is_some() {
                return Err(SftpError::configuration("duplicate SFTP connection ID"));
            }
        }
        Ok(Self {
            profiles,
            factory,
            sessions: Mutex::new(BTreeMap::new()),
        })
    }

    /// Closes a cached profile session.
    ///
    /// # Panics
    ///
    /// Panics only if another thread poisoned the internal session lock.
    pub fn disconnect(&self, profile: &str) -> bool {
        let transport = self.sessions.lock().unwrap().remove(profile);
        if let Some(transport) = transport {
            transport.disconnect();
            true
        } else {
            false
        }
    }

    /// Reconnects a profile without changing provider locations.
    ///
    /// # Errors
    ///
    /// Returns host-verification, authentication, or connection errors.
    pub fn retry(&self, profile: &str) -> Result<(), SftpError> {
        self.disconnect(profile);
        self.transport(profile).map(|_| ())
    }

    fn transport(&self, profile: &str) -> Result<Arc<dyn SftpTransport>, SftpError> {
        if let Some(transport) = self.sessions.lock().unwrap().get(profile).cloned() {
            return Ok(transport);
        }
        let profile_config = self
            .profiles
            .get(profile)
            .ok_or_else(|| SftpError::configuration(format!("unknown SFTP profile {profile}")))?;
        let transport = self.factory.connect(profile_config)?;
        self.sessions
            .lock()
            .unwrap()
            .insert(profile.to_owned(), Arc::clone(&transport));
        Ok(transport)
    }

    fn with_retry<T>(
        &self,
        profile: &str,
        operation: impl Fn(&dyn SftpTransport) -> Result<T, SftpError>,
    ) -> Result<T, SftpError> {
        let transport = self.transport(profile)?;
        match operation(transport.as_ref()) {
            Ok(value) => Ok(value),
            Err(error) if error.retryable() => {
                self.disconnect(profile);
                let transport = self.transport(profile)?;
                operation(transport.as_ref())
            }
            Err(error) => Err(error),
        }
    }

    fn address(&self, location: &Location) -> Result<SftpAddress, ProviderError> {
        let address = SftpAddress::parse(location).map_err(provider_error)?;
        let profile = self.profiles.get(&address.profile).ok_or_else(|| {
            ProviderError::Unsupported(format!("unknown SFTP profile {}", address.profile))
        })?;
        let root = normalize_remote_path(&profile.root).map_err(provider_error)?;
        if !remote_path_contains(&root, &address.path) {
            return Err(ProviderError::Unsupported(format!(
                "SFTP location {} escapes profile root {}",
                address.path, root
            )));
        }
        Ok(address)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SftpAddress {
    profile: String,
    path: String,
}

impl SftpAddress {
    fn parse(location: &Location) -> Result<Self, SftpError> {
        let value = location
            .as_str()
            .strip_prefix("sftp://")
            .ok_or_else(|| SftpError::configuration("not an SFTP location"))?;
        let (profile, path) = value.split_once('/').unwrap_or((value, ""));
        if profile.is_empty() {
            return Err(SftpError::configuration("SFTP profile ID is missing"));
        }
        Ok(Self {
            profile: profile.to_owned(),
            path: normalize_remote_path(&format!("/{path}"))?,
        })
    }

    fn location(&self) -> Location {
        Location::new(format!(
            "sftp://{}{}",
            self.profile,
            if self.path == "/" {
                "/"
            } else {
                self.path.as_str()
            }
        ))
    }

    fn child(&self, name: &str) -> Result<Self, SftpError> {
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            return Err(SftpError::configuration("unsafe SFTP child name"));
        }
        Ok(Self {
            profile: self.profile.clone(),
            path: normalize_remote_path(&format!("{}/{}", self.path.trim_end_matches('/'), name))?,
        })
    }
}

fn normalize_remote_path(path: &str) -> Result<String, SftpError> {
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::RootDir => parts.clear(),
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(SftpError::configuration(
                        "SFTP path cannot escape its profile root",
                    ));
                }
            }
            Component::Prefix(_) => {
                return Err(SftpError::configuration(
                    "SFTP path cannot use a drive prefix",
                ));
            }
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

fn provider_error(error: SftpError) -> ProviderError {
    match error.kind {
        SftpFailureKind::Configuration => ProviderError::Unsupported(error.message),
        SftpFailureKind::HostVerification
        | SftpFailureKind::Authentication
        | SftpFailureKind::Connection
        | SftpFailureKind::Resource => ProviderError::Failed(error.message),
    }
}

impl ResourceProvider for SftpProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from(PROVIDER_ID)
    }

    fn schemes(&self) -> &[&str] {
        SCHEMES
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
            let address = self.address(location)?;
            let mut entries = self
                .with_retry(&address.profile, |transport| transport.list(&address.path))
                .map_err(provider_error)?;
            entries.sort_by_key(|left| left.0.to_lowercase());
            let offset = request
                .continuation
                .as_deref()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or_default();
            let end = offset
                .saturating_add(request.page_size.max(1))
                .min(entries.len());
            let page = entries[offset..end]
                .iter()
                .map(|(name, metadata)| {
                    let child = address.child(name).map_err(provider_error)?;
                    Ok(ResourceEntry {
                        resource: ResourceRef {
                            provider: self.id(),
                            location: child.location(),
                        },
                        metadata: resource_metadata(name, metadata),
                        details: format!("{}@{}", address.profile, child.path),
                    })
                })
                .collect::<Result<Vec<_>, ProviderError>>()?;
            Ok(ListPage {
                generation: request.generation,
                entries: page,
                continuation: (end < entries.len()).then(|| end.to_string()),
                complete: end == entries.len(),
            })
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            let address = self.address(&resource.location)?;
            let metadata = self
                .with_retry(&address.profile, |transport| transport.stat(&address.path))
                .map_err(provider_error)?;
            let name = address.path.rsplit('/').next().unwrap_or("/");
            Ok(resource_metadata(name, &metadata))
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
            let address = self.address(&resource.location)?;
            let metadata = self
                .with_retry(&address.profile, |transport| transport.stat(&address.path))
                .map_err(provider_error)?;
            let bytes = self
                .with_retry(&address.profile, |transport| {
                    transport.read(&address.path, request.offset, request.length)
                })
                .map_err(provider_error)?;
            let complete = metadata.size.is_none_or(|size| {
                request
                    .offset
                    .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                    >= size
            });
            Ok(ResourceStream {
                offset: request.offset,
                bytes,
                total_size: metadata.size,
                complete,
            })
        })
    }

    fn write<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: WriteRequest,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let address = self.address(&resource.location)?;
            if let Some(expected) = request.expected {
                let metadata = self
                    .with_retry(&address.profile, |transport| transport.stat(&address.path))
                    .map_err(provider_error)?;
                let current = ResourceVersion {
                    size: metadata.size,
                    modified_unix_ms: metadata.modified_unix_ms,
                };
                if current != expected {
                    return Err(ProviderError::Conflict(format!(
                        "remote resource changed: {}",
                        resource.location.as_str()
                    )));
                }
            }
            self.with_retry(&address.profile, |transport| {
                transport.write_chunk(&address.path, 0, &request.bytes, true)
            })
            .map_err(provider_error)
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        let Ok(address) = self.address(&resource.location) else {
            return CapabilitySet::default();
        };
        let Ok(metadata) =
            self.with_retry(&address.profile, |transport| transport.stat(&address.path))
        else {
            return CapabilitySet::default();
        };
        let mut capabilities = CapabilitySet::default();
        capabilities.insert("resource.inspect");
        capabilities.insert("resource.copy");
        capabilities.insert("resource.rename");
        capabilities.insert("resource.delete");
        match metadata.kind {
            ResourceKind::Directory => {
                capabilities.insert("resource.list");
                capabilities.insert("resource.create-directory");
            }
            ResourceKind::File => {
                capabilities.insert("resource.read");
                if !metadata.readonly {
                    capabilities.insert("resource.write");
                }
            }
            _ => {}
        }
        capabilities
    }

    fn locations(&self) -> Vec<ProviderLocation> {
        self.profiles
            .values()
            .map(|profile| ProviderLocation {
                location: SftpAddress {
                    profile: profile.id.clone(),
                    path: normalize_remote_path(&profile.root).unwrap_or_else(|_| "/".to_owned()),
                }
                .location(),
                label: profile.label.clone(),
                detail: format!(
                    "{}@{}:{} via platform SSH agent",
                    profile.username, profile.host, profile.port
                ),
            })
            .collect()
    }

    fn parent(&self, location: &Location) -> Option<Location> {
        let address = self.address(location).ok()?;
        let profile_root =
            normalize_remote_path(&self.profiles.get(&address.profile)?.root).ok()?;
        if address.path == profile_root || address.path == "/" {
            return None;
        }
        let parent = Path::new(&address.path).parent()?.to_string_lossy();
        let parent = normalize_remote_path(&parent).ok()?;
        if !parent.starts_with(&profile_root) {
            return None;
        }
        Some(
            SftpAddress {
                profile: address.profile,
                path: parent,
            }
            .location(),
        )
    }

    fn disconnect(&self, location: &Location) -> Result<bool, ProviderError> {
        let address = self.address(location)?;
        Ok(self.disconnect(&address.profile))
    }

    fn reconnect(&self, location: &Location) -> Result<bool, ProviderError> {
        let address = self.address(location)?;
        self.retry(&address.profile).map_err(provider_error)?;
        Ok(true)
    }
}

fn remote_path_contains(root: &str, path: &str) -> bool {
    root == "/"
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn resource_metadata(name: &str, metadata: &SftpMetadata) -> ResourceMetadata {
    ResourceMetadata {
        name: name.to_owned(),
        kind: metadata.kind,
        size: metadata.size,
        modified_unix_ms: metadata.modified_unix_ms,
        permissions: Some(PermissionSummary {
            readonly: metadata.readonly,
            executable: metadata.executable,
            unix_mode: None,
        }),
        ..ResourceMetadata::default()
    }
}

pub struct SftpOperationService {
    fallback: Box<dyn OperationService>,
    provider: Arc<SftpProvider>,
    planner: OperationPlanner,
    engine: OperationEngine<SftpOperationBackend>,
    plans: BTreeSet<OperationId>,
}

impl SftpOperationService {
    pub fn new(
        fallback: impl OperationService + 'static,
        provider: Arc<SftpProvider>,
        journal: OperationJournal,
    ) -> Self {
        Self {
            fallback: Box::new(fallback),
            planner: OperationPlanner::with_prefix("near.sftp.operation"),
            engine: OperationEngine::new(
                SftpOperationBackend {
                    provider: Arc::clone(&provider),
                },
                journal,
            ),
            provider,
            plans: BTreeSet::new(),
        }
    }

    fn transfer_plan(
        &mut self,
        kind: OperationKind,
        sources: &[ResourceRef],
        destination: &Location,
        generation: near_core::ListingGeneration,
    ) -> Result<Option<OperationPlan>, String> {
        let remote = destination.as_str().starts_with("sftp://")
            || sources
                .iter()
                .any(|source| source.provider == ProviderId::from(PROVIDER_ID));
        if !remote {
            return Ok(None);
        }
        if sources.iter().any(|source| {
            source.provider != ProviderId::from(PROVIDER_ID)
                && source.provider != ProviderId::from("near.local-fs")
        }) {
            return Err("SFTP transfers currently accept local or SFTP resources".to_owned());
        }
        let items = sources
            .iter()
            .map(|source| {
                let name = transfer_source_name(source)?;
                let target = transfer_child(destination, &name)?;
                let recursive = transfer_source_is_directory(&self.provider, source)?;
                Ok(PlannedItem {
                    source: Some(source.clone()),
                    conflict_expected: transfer_target_exists(&self.provider, &target),
                    target,
                    recursive,
                    parameters: BTreeMap::from([(
                        "near.sftp.transfer".to_owned(),
                        if kind == OperationKind::Move {
                            "move"
                        } else {
                            "copy"
                        }
                        .to_owned(),
                    )]),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.planner
            .plan(PlanRequest {
                kind,
                items,
                destination: Some(destination.clone()),
                policies: PlanPolicies {
                    conflict: near_ops::ConflictPolicy::Ask,
                    metadata: MetadataPolicy::Portable,
                    verification: VerificationPolicy::SizeAndTime,
                    recovery: RecoveryPolicy::JournalOnly,
                    cross_device: CrossDeviceBehavior::CopyThenDelete,
                    symlink: SymlinkPolicy::Reject,
                },
                safety: near_core::SafetyClass::Confirmable,
                context_generation: generation,
                high_impact: false,
            })
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

impl OperationService for SftpOperationService {
    fn plan(
        &mut self,
        intent: OperationIntent,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, String> {
        let candidate = match &intent {
            OperationIntent::CopyTo {
                sources,
                destination,
            } => self.transfer_plan(OperationKind::Copy, sources, destination, generation)?,
            OperationIntent::MoveTo {
                sources,
                destination,
            } => self.transfer_plan(OperationKind::Move, sources, destination, generation)?,
            _ => None,
        };
        if let Some(plan) = candidate {
            let id = self
                .engine
                .record(plan.clone())
                .map_err(|error| error.to_string())?;
            self.plans.insert(id);
            Ok(plan)
        } else {
            self.fallback.plan(intent, generation)
        }
    }

    fn execute(
        &mut self,
        plan: &OperationId,
        authorization: ExecutionAuthorization,
        cancellation: &CancellationToken,
        conflict: ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        if !self.plans.contains(plan) {
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
        if self.plans.contains(plan) {
            return Err("SFTP operations cannot be retried with local elevation".to_owned());
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

struct SftpOperationBackend {
    provider: Arc<SftpProvider>,
}

impl OperationBackend for SftpOperationBackend {
    fn target_exists(&self, target: &Location) -> bool {
        transfer_target_exists(&self.provider, target)
    }

    fn execute(
        &mut self,
        plan: &OperationPlan,
        item: &PlannedItem,
        action: Option<ConflictAction>,
        cancellation: &CancellationToken,
    ) -> Result<near_ops::ExecutionEffect, String> {
        let source = item
            .source
            .as_ref()
            .ok_or("SFTP transfer source is missing")?;
        let target = resolve_transfer_conflict(&self.provider, &item.target, action)?;
        copy_transfer_resource(&self.provider, source, &target, cancellation)?;
        if plan.kind() == OperationKind::Move {
            remove_transfer_source(&self.provider, source, cancellation)?;
        }
        Ok(near_ops::ExecutionEffect::default())
    }
}

fn transfer_source_name(source: &ResourceRef) -> Result<String, String> {
    if source.provider == ProviderId::from(PROVIDER_ID) {
        let address = SftpAddress::parse(&source.location).map_err(|error| error.to_string())?;
        return address
            .path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "SFTP source root cannot be transferred".to_owned());
    }
    local_path(&source.location)
        .map_err(|error| error.to_string())?
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "local source has no portable UTF-8 name".to_owned())
}

fn transfer_child(destination: &Location, name: &str) -> Result<Location, String> {
    if destination.as_str().starts_with("sftp://") {
        return SftpAddress::parse(destination)
            .and_then(|address| address.child(name))
            .map(|address| address.location())
            .map_err(|error| error.to_string());
    }
    let path = local_path(destination).map_err(|error| error.to_string())?;
    Ok(local_location(&path.join(name)))
}

fn transfer_source_is_directory(
    provider: &SftpProvider,
    source: &ResourceRef,
) -> Result<bool, String> {
    if source.provider == ProviderId::from(PROVIDER_ID) {
        let address = provider
            .address(&source.location)
            .map_err(|error| error.to_string())?;
        return provider
            .with_retry(&address.profile, |transport| transport.stat(&address.path))
            .map(|metadata| metadata.kind == ResourceKind::Directory)
            .map_err(|error| error.to_string());
    }
    Ok(local_path(&source.location)
        .map_err(|error| error.to_string())?
        .is_dir())
}

fn transfer_target_exists(provider: &SftpProvider, target: &Location) -> bool {
    if target.as_str().starts_with("sftp://") {
        return provider.address(target).is_ok_and(|address| {
            provider
                .with_retry(&address.profile, |transport| transport.stat(&address.path))
                .is_ok()
        });
    }
    local_path(target).is_ok_and(|path| path.exists())
}

fn resolve_transfer_conflict(
    provider: &SftpProvider,
    target: &Location,
    action: Option<ConflictAction>,
) -> Result<Location, String> {
    if !transfer_target_exists(provider, target) || action != Some(ConflictAction::Rename) {
        return Ok(target.clone());
    }
    for suffix in 1..=10_000_u32 {
        let candidate = renamed_transfer_target(target, suffix)?;
        if !transfer_target_exists(provider, &candidate) {
            return Ok(candidate);
        }
    }
    Err("cannot find a free transfer target name".to_owned())
}

fn renamed_transfer_target(target: &Location, suffix: u32) -> Result<Location, String> {
    if target.as_str().starts_with("sftp://") {
        let address = SftpAddress::parse(target).map_err(|error| error.to_string())?;
        let name = address.path.rsplit('/').next().unwrap_or("resource");
        let parent = address
            .path
            .rsplit_once('/')
            .map_or("/", |(parent, _)| parent);
        return Ok(SftpAddress {
            profile: address.profile,
            path: normalize_remote_path(&format!("{parent}/{name}.{suffix}"))
                .map_err(|error| error.to_string())?,
        }
        .location());
    }
    let path = local_path(target).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("resource");
    Ok(local_location(
        &path.with_file_name(format!("{name}.{suffix}")),
    ))
}

fn copy_transfer_resource(
    provider: &SftpProvider,
    source: &ResourceRef,
    target: &Location,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("SFTP transfer cancelled".to_owned());
    }
    match (
        source.provider == ProviderId::from(PROVIDER_ID),
        target.as_str().starts_with("sftp://"),
    ) {
        (false, true) => copy_local_to_sftp(provider, source, target, cancellation),
        (true, false) => copy_sftp_to_local(provider, source, target, cancellation),
        (true, true) => copy_sftp_to_sftp(provider, source, target, cancellation),
        (false, false) => Err("SFTP backend received a local-only transfer".to_owned()),
    }
}

const TRANSFER_CHUNK: usize = 256 * 1024;

fn copy_local_to_sftp(
    provider: &SftpProvider,
    source: &ResourceRef,
    target: &Location,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let source_path = local_path(&source.location).map_err(|error| error.to_string())?;
    let target_address = provider
        .address(target)
        .map_err(|error| error.to_string())?;
    if source_path.is_dir() {
        provider
            .with_retry(&target_address.profile, |transport| {
                transport.create_directory(&target_address.path)
            })
            .or_else(|error| {
                transfer_target_exists(provider, target)
                    .then_some(())
                    .ok_or(error)
            })
            .map_err(|error| error.to_string())?;
        for entry in fs::read_dir(&source_path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry
                .file_name()
                .to_str()
                .map(str::to_owned)
                .ok_or("local child has no portable UTF-8 name")?;
            let child_source = local_resource(&entry.path());
            let child_target = target_address
                .child(&name)
                .map_err(|error| error.to_string())?
                .location();
            copy_local_to_sftp(provider, &child_source, &child_target, cancellation)?;
        }
        return Ok(());
    }
    let mut file = fs::File::open(&source_path).map_err(|error| error.to_string())?;
    let mut offset = 0_u64;
    let mut buffer = vec![0; TRANSFER_CHUNK];
    loop {
        if cancellation.is_cancelled() {
            return Err("SFTP transfer cancelled".to_owned());
        }
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            if offset == 0 {
                provider
                    .with_retry(&target_address.profile, |transport| {
                        transport.write_chunk(&target_address.path, 0, &[], true)
                    })
                    .map_err(|error| error.to_string())?;
            }
            return Ok(());
        }
        provider
            .with_retry(&target_address.profile, |transport| {
                transport.write_chunk(&target_address.path, offset, &buffer[..read], offset == 0)
            })
            .map_err(|error| error.to_string())?;
        offset = offset.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
    }
}

fn copy_sftp_to_local(
    provider: &SftpProvider,
    source: &ResourceRef,
    target: &Location,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let source_address = provider
        .address(&source.location)
        .map_err(|error| error.to_string())?;
    let metadata = provider
        .with_retry(&source_address.profile, |transport| {
            transport.stat(&source_address.path)
        })
        .map_err(|error| error.to_string())?;
    let target_path = local_path(target).map_err(|error| error.to_string())?;
    if metadata.kind == ResourceKind::Directory {
        fs::create_dir_all(&target_path).map_err(|error| error.to_string())?;
        let children = provider
            .with_retry(&source_address.profile, |transport| {
                transport.list(&source_address.path)
            })
            .map_err(|error| error.to_string())?;
        for (name, _) in children {
            let child_source = ResourceRef {
                provider: ProviderId::from(PROVIDER_ID),
                location: source_address
                    .child(&name)
                    .map_err(|error| error.to_string())?
                    .location(),
            };
            let child_target = local_location(&target_path.join(name));
            copy_sftp_to_local(provider, &child_source, &child_target, cancellation)?;
        }
        return Ok(());
    }
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut output = fs::File::create(&target_path).map_err(|error| error.to_string())?;
    let mut offset = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err("SFTP transfer cancelled".to_owned());
        }
        let bytes = provider
            .with_retry(&source_address.profile, |transport| {
                transport.read(&source_address.path, offset, TRANSFER_CHUNK)
            })
            .map_err(|error| error.to_string())?;
        if bytes.is_empty() {
            return Ok(());
        }
        output
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        offset = offset.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if metadata.size.is_some_and(|size| offset >= size) {
            return Ok(());
        }
    }
}

fn copy_sftp_to_sftp(
    provider: &SftpProvider,
    source: &ResourceRef,
    target: &Location,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let temporary = std::env::temp_dir().join(format!(
        "near-sftp-transfer-{}-{}",
        std::process::id(),
        source.location.as_str().len()
    ));
    let temporary_location = local_location(&temporary);
    copy_sftp_to_local(provider, source, &temporary_location, cancellation)?;
    let temporary_resource = local_resource(&temporary);
    let result = copy_local_to_sftp(provider, &temporary_resource, target, cancellation);
    if temporary.is_dir() {
        let _ = fs::remove_dir_all(&temporary);
    } else {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_transfer_source(
    provider: &SftpProvider,
    source: &ResourceRef,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if cancellation.is_cancelled() {
        return Err("SFTP move cancelled before source removal".to_owned());
    }
    if source.provider != ProviderId::from(PROVIDER_ID) {
        let path = local_path(&source.location).map_err(|error| error.to_string())?;
        return if path.is_dir() {
            fs::remove_dir_all(path).map_err(|error| error.to_string())
        } else {
            fs::remove_file(path).map_err(|error| error.to_string())
        };
    }
    let address = provider
        .address(&source.location)
        .map_err(|error| error.to_string())?;
    let metadata = provider
        .with_retry(&address.profile, |transport| transport.stat(&address.path))
        .map_err(|error| error.to_string())?;
    if metadata.kind == ResourceKind::Directory {
        let children = provider
            .with_retry(&address.profile, |transport| transport.list(&address.path))
            .map_err(|error| error.to_string())?;
        for (name, _) in children {
            remove_transfer_source(
                provider,
                &ResourceRef {
                    provider: ProviderId::from(PROVIDER_ID),
                    location: address
                        .child(&name)
                        .map_err(|error| error.to_string())?
                        .location(),
                },
                cancellation,
            )?;
        }
    }
    provider
        .with_retry(&address.profile, |transport| {
            transport.remove(&address.path, metadata.kind == ResourceKind::Directory)
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
fn tests_block_on<T>(mut future: ProviderFuture<'_, T>) -> Result<T, ProviderError> {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match Future::poll(future.as_mut(), &mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use near_core::{
        CancellationToken, ListRequest, ListingGeneration, OpenRequest, ResourceProvider,
    };

    use super::*;

    #[derive(Default)]
    struct FakeState {
        connects: usize,
        disconnects: usize,
        fail_next_list: bool,
        files: BTreeMap<String, Vec<u8>>,
        directories: BTreeSet<String>,
    }

    struct FakeTransport {
        state: Arc<Mutex<FakeState>>,
    }

    impl SftpTransport for FakeTransport {
        fn list(&self, path: &str) -> Result<Vec<(String, SftpMetadata)>, SftpError> {
            let mut state = self.state.lock().unwrap();
            if state.fail_next_list {
                state.fail_next_list = false;
                return Err(SftpError::connection("connection dropped"));
            }
            let prefix = format!("{}/", path.trim_end_matches('/'));
            Ok(state
                .files
                .iter()
                .filter_map(|(file, bytes)| {
                    let name = file.strip_prefix(&prefix)?;
                    (!name.contains('/')).then(|| {
                        (
                            name.to_owned(),
                            SftpMetadata {
                                kind: ResourceKind::File,
                                size: Some(bytes.len() as u64),
                                ..SftpMetadata::default()
                            },
                        )
                    })
                })
                .chain(state.directories.iter().filter_map(|directory| {
                    let name = directory.strip_prefix(&prefix)?;
                    (!name.is_empty() && !name.contains('/')).then(|| {
                        (
                            name.to_owned(),
                            SftpMetadata {
                                kind: ResourceKind::Directory,
                                ..SftpMetadata::default()
                            },
                        )
                    })
                }))
                .collect())
        }

        fn stat(&self, path: &str) -> Result<SftpMetadata, SftpError> {
            let state = self.state.lock().unwrap();
            if let Some(bytes) = state.files.get(path) {
                return Ok(SftpMetadata {
                    kind: ResourceKind::File,
                    size: Some(bytes.len() as u64),
                    ..SftpMetadata::default()
                });
            }
            if path == "/srv/data" || state.directories.contains(path) {
                return Ok(SftpMetadata {
                    kind: ResourceKind::Directory,
                    ..SftpMetadata::default()
                });
            }
            Err(SftpError::resource("not found"))
        }

        fn read(&self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>, SftpError> {
            let state = self.state.lock().unwrap();
            let bytes = state
                .files
                .get(path)
                .ok_or_else(|| SftpError::resource("not found"))?;
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            Ok(bytes[start..bytes.len().min(start.saturating_add(length))].to_vec())
        }

        fn write_chunk(
            &self,
            path: &str,
            offset: u64,
            bytes: &[u8],
            truncate: bool,
        ) -> Result<(), SftpError> {
            let mut state = self.state.lock().unwrap();
            let file = state.files.entry(path.to_owned()).or_default();
            if truncate {
                file.clear();
            }
            let offset = usize::try_from(offset).unwrap_or(usize::MAX);
            if file.len() < offset {
                file.resize(offset, 0);
            }
            let end = offset.saturating_add(bytes.len());
            if file.len() < end {
                file.resize(end, 0);
            }
            file[offset..end].copy_from_slice(bytes);
            Ok(())
        }

        fn create_directory(&self, path: &str) -> Result<(), SftpError> {
            self.state
                .lock()
                .unwrap()
                .directories
                .insert(path.to_owned());
            Ok(())
        }

        fn rename(&self, source: &str, destination: &str) -> Result<(), SftpError> {
            let mut state = self.state.lock().unwrap();
            let bytes = state
                .files
                .remove(source)
                .ok_or_else(|| SftpError::resource("not found"))?;
            state.files.insert(destination.to_owned(), bytes);
            Ok(())
        }

        fn remove(&self, path: &str, directory: bool) -> Result<(), SftpError> {
            let mut state = self.state.lock().unwrap();
            if directory {
                state.directories.remove(path);
            } else {
                state.files.remove(path);
            }
            Ok(())
        }

        fn disconnect(&self) {
            self.state.lock().unwrap().disconnects += 1;
        }
    }

    struct FakeFactory {
        state: Arc<Mutex<FakeState>>,
    }

    impl SftpTransportFactory for FakeFactory {
        fn connect(
            &self,
            _profile: &SftpConnectionProfile,
        ) -> Result<Arc<dyn SftpTransport>, SftpError> {
            self.state.lock().unwrap().connects += 1;
            Ok(Arc::new(FakeTransport {
                state: Arc::clone(&self.state),
            }))
        }
    }

    fn provider(state: Arc<Mutex<FakeState>>) -> SftpProvider {
        SftpProvider::with_factory(
            SftpConnectionDocument {
                schema: connection_schema(),
                connection: vec![SftpConnectionProfile {
                    id: "prod".to_owned(),
                    label: "Production".to_owned(),
                    host: "example.test".to_owned(),
                    port: 22,
                    username: "near".to_owned(),
                    root: "/srv/data".to_owned(),
                    known_hosts: "/secure/known_hosts".to_owned(),
                }],
            },
            Arc::new(FakeFactory { state }),
        )
        .unwrap()
    }

    #[test]
    fn catalog_rejects_plaintext_credentials_duplicate_ids_and_unsafe_roots() {
        let source = r#"
            [[connection]]
            id = "prod"
            label = "Production"
            host = "example.test"
            username = "near"
            root = "/srv/data"
            known_hosts = "/secure/known_hosts"
            password = "must-not-be-accepted"
        "#;
        assert!(SftpConnectionDocument::from_toml(source).is_err());
        let duplicate = source.replace("password = \"must-not-be-accepted\"", "")
            + &source.replace("password = \"must-not-be-accepted\"", "");
        assert!(SftpConnectionDocument::from_toml(&duplicate).is_err());
        let unsafe_root = source
            .replace("password = \"must-not-be-accepted\"", "")
            .replace("root = \"/srv/data\"", "root = \"../../etc\"");
        assert!(SftpConnectionDocument::from_toml(&unsafe_root).is_err());
    }

    #[test]
    fn navigation_reads_and_reconnect_preserve_provider_locations() {
        let state = Arc::new(Mutex::new(FakeState {
            fail_next_list: true,
            files: BTreeMap::from([("/srv/data/report.txt".to_owned(), b"remote report".to_vec())]),
            ..FakeState::default()
        }));
        let provider = provider(Arc::clone(&state));
        let root = provider.locations()[0].location.clone();
        assert_eq!(root.as_str(), "sftp://prod/srv/data");
        let page = crate::tests_block_on(provider.list(
            &root,
            ListRequest {
                generation: ListingGeneration(3),
                continuation: None,
                page_size: 20,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(state.lock().unwrap().connects, 2);
        let resource = page.entries[0].resource.clone();
        let stream = crate::tests_block_on(provider.open(
            &resource,
            OpenRequest {
                offset: 7,
                length: 6,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(stream.bytes, b"report");
        assert!(provider.disconnect("prod"));
        assert_eq!(provider.locations()[0].location, root);
        provider.retry("prod").unwrap();
        assert_eq!(state.lock().unwrap().disconnects, 2);
        assert_eq!(state.lock().unwrap().connects, 3);
    }

    #[test]
    fn profile_root_cannot_be_escaped_by_a_forged_location() {
        let provider = provider(Arc::new(Mutex::new(FakeState::default())));
        let forged = Location::new("sftp://prod/etc/passwd");
        assert!(provider.address(&forged).is_err());
    }

    #[test]
    fn immutable_plans_copy_and_move_between_local_and_sftp_resources() {
        use near_local_fs::LocalOperationService;
        use near_ops::{DecisionScope, OperationIntent, OperationJournal};

        let root = std::env::temp_dir().join(format!("near-sftp-operation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("download")).unwrap();
        fs::write(root.join("upload.txt"), b"bounded remote transfer").unwrap();
        let state = Arc::new(Mutex::new(FakeState::default()));
        let provider = Arc::new(provider(Arc::clone(&state)));
        let mut service = SftpOperationService::new(
            LocalOperationService::platform_default(OperationJournal::memory()),
            Arc::clone(&provider),
            OperationJournal::memory(),
        );
        let generation = ListingGeneration(9);
        let upload = service
            .plan(
                OperationIntent::CopyTo {
                    sources: vec![local_resource(&root.join("upload.txt"))],
                    destination: Location::new("sftp://prod/srv/data"),
                },
                generation,
            )
            .unwrap();
        let authorization = ExecutionAuthorization {
            context_generation: generation,
            confirmed: true,
            high_impact_confirmed: false,
        };
        let conflict = ConflictDecision {
            action: ConflictAction::Replace,
            scope: DecisionScope::Once,
        };
        let summary = service
            .execute(
                upload.id(),
                authorization,
                &CancellationToken::default(),
                conflict,
            )
            .unwrap();
        assert_eq!(summary.completed(), 1);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .files
                .get("/srv/data/upload.txt")
                .unwrap(),
            b"bounded remote transfer"
        );

        let download = service
            .plan(
                OperationIntent::MoveTo {
                    sources: vec![ResourceRef {
                        provider: ProviderId::from(PROVIDER_ID),
                        location: Location::new("sftp://prod/srv/data/upload.txt"),
                    }],
                    destination: local_location(&root.join("download")),
                },
                generation,
            )
            .unwrap();
        let summary = service
            .execute(
                download.id(),
                authorization,
                &CancellationToken::default(),
                conflict,
            )
            .unwrap();
        assert_eq!(summary.completed(), 1);
        assert_eq!(
            fs::read(root.join("download/upload.txt")).unwrap(),
            b"bounded remote transfer"
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .files
                .contains_key("/srv/data/upload.txt")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
