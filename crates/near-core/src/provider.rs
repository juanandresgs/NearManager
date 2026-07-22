use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CapabilitySet, Location, MutationEligibility, MutationKind, ProviderId, ResourceClassification,
    ResourceMetadata, ResourceRef,
};

pub type ProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send + 'a>>;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct ListingGeneration(pub u64);

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct ListRequest {
    pub generation: ListingGeneration,
    pub continuation: Option<String>,
    pub page_size: usize,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceEntry {
    pub resource: ResourceRef,
    pub metadata: ResourceMetadata,
    pub details: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderLocation {
    pub location: Location,
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListPage {
    pub generation: ListingGeneration,
    pub entries: Vec<ResourceEntry>,
    pub continuation: Option<String>,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub struct OpenRequest {
    pub offset: u64,
    pub length: usize,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug)]
pub struct WriteRequest {
    pub bytes: Vec<u8>,
    pub expected: Option<ResourceVersion>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceVersion {
    pub size: Option<u64>,
    pub modified_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceStream {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub total_size: Option<u64>,
    pub complete: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("provider request was cancelled")]
    Cancelled,
    #[error("resource was not found: {0}")]
    NotFound(ResourceRef),
    #[error("resource changed since it was opened: {0}")]
    Conflict(String),
    #[error("provider does not support this operation: {0}")]
    Unsupported(String),
    #[error("provider failed: {0}")]
    Failed(String),
}

pub trait ResourceProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn schemes(&self) -> &[&str];
    fn list<'a>(
        &'a self,
        location: &'a Location,
        request: ListRequest,
    ) -> ProviderFuture<'a, ListPage>;
    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata>;
    fn open<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream>;
    fn streams<'a>(
        &'a self,
        resource: &'a ResourceRef,
        _request: ListRequest,
    ) -> ProviderFuture<'a, ListPage> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "{} cannot enumerate alternate streams for {}",
                self.id(),
                resource.location.as_str()
            )))
        })
    }
    fn write<'a>(
        &'a self,
        resource: &'a ResourceRef,
        _request: WriteRequest,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "{} cannot write {}",
                self.id(),
                resource.location.as_str()
            )))
        })
    }

    fn set_description<'a>(
        &'a self,
        resource: &'a ResourceRef,
        _description: Option<String>,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "{} cannot update descriptions for {}",
                self.id(),
                resource.location.as_str()
            )))
        })
    }

    fn folder_description<'a>(
        &'a self,
        location: &'a Location,
        _create: bool,
    ) -> ProviderFuture<'a, Option<ResourceRef>> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(format!(
                "{} has no folder-description contract for {}",
                self.id(),
                location.as_str()
            )))
        })
    }
    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet;

    /// Parses a provider-native reference supplied by an operator or interchange document.
    ///
    /// Provider-qualified locations are handled by the application before this method is called.
    /// Implementations should return `Ok(None)` when the text does not belong to them and must not
    /// silently reinterpret relative text as a resource.
    ///
    /// # Errors
    ///
    /// Returns a provider-specific error when the text is recognized but malformed.
    fn parse_native_reference(
        &self,
        _reference: &str,
    ) -> Result<Option<ResourceRef>, ProviderError> {
        Ok(None)
    }

    /// Classifies a resource before provider-neutral mutation policy is applied.
    ///
    /// Providers with roots, mounts, removable devices, virtual collections, or special
    /// resources should override this method rather than relying on the ordinary-resource
    /// default.
    ///
    /// # Errors
    ///
    /// Returns a provider-specific error when the resource cannot be classified safely.
    fn classify_resource(
        &self,
        _resource: &ResourceRef,
    ) -> Result<ResourceClassification, ProviderError> {
        Ok(ResourceClassification::Ordinary)
    }

    /// Reports whether a resource may enter mutation planning.
    ///
    /// The default preserves compatibility for providers that already enforce mutation safety in
    /// their operation service. Providers with protected resource classes should override this so
    /// applications can deny the workflow visibly before a plan is created.
    fn mutation_eligibility(
        &self,
        _resource: &ResourceRef,
        _mutation: MutationKind,
    ) -> MutationEligibility {
        MutationEligibility::Allowed
    }

    fn command_prefixes(&self) -> Vec<crate::CommandPrefixDescriptor> {
        Vec::new()
    }

    /// Resolves a provider-owned command prefix into a location.
    ///
    /// # Errors
    ///
    /// Returns an unsupported or provider-specific resolution failure.
    fn resolve_command_prefix(
        &self,
        prefix: &str,
        _arguments: &str,
        _current: Option<&Location>,
    ) -> Result<Location, ProviderError> {
        Err(ProviderError::Unsupported(format!(
            "{} does not own command prefix {prefix}",
            self.id()
        )))
    }

    fn locations(&self) -> Vec<ProviderLocation> {
        self.schemes()
            .iter()
            .map(|scheme| ProviderLocation {
                location: Location::new(format!("{scheme}://")),
                label: format!("{scheme} root"),
                detail: format!("{} provider root", self.id()),
            })
            .collect()
    }

    fn location_label(&self, location: &Location) -> String {
        location.as_str().to_owned()
    }

    fn parent(&self, _location: &Location) -> Option<Location> {
        None
    }

    /// Closes a live connection while retaining provider-addressable state.
    ///
    /// # Errors
    ///
    /// Returns a provider-specific connection shutdown error.
    fn disconnect(&self, _location: &Location) -> Result<bool, ProviderError> {
        Ok(false)
    }

    /// Re-establishes a connection for a retained location.
    ///
    /// # Errors
    ///
    /// Returns authentication, host-verification, or connection errors.
    fn reconnect(&self, _location: &Location) -> Result<bool, ProviderError> {
        Ok(false)
    }

    /// Resolves a foreign resource into a collection location owned by this provider.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider recognizes the resource but cannot safely mount it.
    fn mount(&self, _resource: &ResourceRef) -> Result<Option<Location>, ProviderError> {
        Ok(None)
    }

    /// Resolves a new named container below a foreign parent location.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is recognized but invalid or unsafe for this provider.
    fn create_container(
        &self,
        _parent: &Location,
        _name: &str,
    ) -> Result<Option<Location>, ProviderError> {
        Ok(None)
    }

    fn container_capabilities(&self, _location: &Location) -> CapabilitySet {
        CapabilitySet::default()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProviderRegistryError {
    #[error("provider {0} is already registered")]
    Duplicate(ProviderId),
}

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, Arc<dyn ResourceProvider>>,
}

pub type ProviderCollectionResolution = (Arc<dyn ResourceProvider>, Location);

impl ProviderRegistry {
    /// Registers a provider by its stable provider ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider ID is already registered.
    pub fn register(
        &mut self,
        provider: Arc<dyn ResourceProvider>,
    ) -> Result<(), ProviderRegistryError> {
        let id = provider.id();
        if self.providers.contains_key(&id) {
            return Err(ProviderRegistryError::Duplicate(id));
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn get(&self, id: &ProviderId) -> Option<Arc<dyn ResourceProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn for_location(&self, location: &Location) -> Option<Arc<dyn ResourceProvider>> {
        let scheme = location.as_str().split_once(':')?.0;
        self.providers
            .values()
            .find(|provider| provider.schemes().contains(&scheme))
            .cloned()
    }

    pub fn providers(&self) -> impl Iterator<Item = Arc<dyn ResourceProvider>> + '_ {
        self.providers.values().cloned()
    }

    /// Finds the first registered provider that can mount `resource` as a collection.
    ///
    /// # Errors
    ///
    /// Returns the claiming provider's mount error instead of silently opening the resource as a
    /// byte stream.
    pub fn mount(
        &self,
        resource: &ResourceRef,
    ) -> Result<Option<ProviderCollectionResolution>, ProviderError> {
        for provider in self.providers.values() {
            if let Some(location) = provider.mount(resource)? {
                return Ok(Some((Arc::clone(provider), location)));
            }
        }
        Ok(None)
    }

    /// Finds a provider capable of creating a named container below `parent`.
    ///
    /// # Errors
    ///
    /// Returns the claiming provider's validation error.
    pub fn create_container(
        &self,
        parent: &Location,
        name: &str,
    ) -> Result<Option<ProviderCollectionResolution>, ProviderError> {
        for provider in self.providers.values() {
            if let Some(location) = provider.create_container(parent, name)? {
                return Ok(Some((Arc::clone(provider), location)));
            }
        }
        Ok(None)
    }

    pub fn container_capabilities(&self, location: &Location) -> CapabilitySet {
        let mut capabilities = CapabilitySet::default();
        for provider in self.providers.values() {
            for capability in provider.container_capabilities(location).iter() {
                capabilities.insert(capability.clone());
            }
        }
        capabilities
    }
}
