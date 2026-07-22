//! Provider-neutral predicates and cancellable streaming search.

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, RwLock},
};

use near_core::{
    CancellationToken, CapabilityId, CapabilitySet, ListPage, ListRequest, ListingGeneration,
    Location, MetadataValue, OpenRequest, ProviderError, ProviderFuture, ProviderId,
    ProviderRegistry, ResourceEntry, ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef,
    ResourceStream,
};
use near_runtime::block_on;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PREDICATE_SCHEMA_VERSION: u16 = 2;
const SEARCH_SCHEMES: &[&str] = &["search"];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HiddenPolicy {
    #[default]
    Exclude,
    Include,
    Only,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IgnorePolicy {
    #[default]
    None,
    VersionControl,
    Common,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "match", content = "value", rename_all = "kebab-case")]
pub enum TextPredicate {
    Contains(String),
    Exact(String),
    Glob(String),
    Regex(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentMatch {
    #[default]
    Text,
    Regex,
    Hex,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchEncoding {
    #[default]
    Auto,
    Utf8,
    Utf16Le,
    Utf16Be,
    Latin1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentPredicate {
    pub text: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub match_kind: ContentMatch,
    #[serde(default)]
    pub encoding: SearchEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourcePredicate {
    pub schema_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TextPredicate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<ResourceKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_after_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_before_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<bool>,
    #[serde(default)]
    pub hidden: HiddenPolicy,
    #[serde(default)]
    pub ignore: IgnorePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentPredicate>,
}

impl Default for ResourcePredicate {
    fn default() -> Self {
        Self {
            schema_version: PREDICATE_SCHEMA_VERSION,
            name: None,
            kinds: Vec::new(),
            minimum_size: None,
            maximum_size: None,
            modified_after_unix_ms: None,
            modified_before_unix_ms: None,
            readonly: None,
            executable: None,
            hidden: HiddenPolicy::default(),
            ignore: IgnorePolicy::default(),
            content: None,
        }
    }
}

impl ResourcePredicate {
    /// Validates the serialized predicate schema.
    ///
    /// # Errors
    ///
    /// Returns an error when the document uses an unsupported schema version or invalid field.
    pub fn validate(&self) -> Result<(), SearchError> {
        if !matches!(self.schema_version, 1 | PREDICATE_SCHEMA_VERSION) {
            return Err(SearchError::UnsupportedSchema(self.schema_version));
        }
        if self.schema_version == 1
            && (matches!(self.name, Some(TextPredicate::Regex(_)))
                || self.content.as_ref().is_some_and(|content| {
                    content.match_kind != ContentMatch::Text
                        || content.encoding != SearchEncoding::Auto
                }))
        {
            return Err(SearchError::InvalidPredicate {
                field: "schema-version".to_owned(),
                message: "regex names and advanced content modes require schema version 2"
                    .to_owned(),
            });
        }
        if self
            .minimum_size
            .zip(self.maximum_size)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Err(SearchError::InvalidPredicate {
                field: "minimum-size".to_owned(),
                message: "minimum size cannot exceed maximum size".to_owned(),
            });
        }
        if self
            .modified_after_unix_ms
            .zip(self.modified_before_unix_ms)
            .is_some_and(|(after, before)| after > before)
        {
            return Err(SearchError::InvalidPredicate {
                field: "modified-after".to_owned(),
                message: "start date cannot be after end date".to_owned(),
            });
        }
        if let Some(TextPredicate::Regex(pattern)) = &self.name {
            Regex::new(pattern).map_err(|error| SearchError::InvalidPredicate {
                field: "name".to_owned(),
                message: error.to_string(),
            })?;
        }
        if let Some(content) = &self.content {
            match content.match_kind {
                ContentMatch::Regex => {
                    Regex::new(&content.text).map_err(|error| SearchError::InvalidPredicate {
                        field: "content".to_owned(),
                        message: error.to_string(),
                    })?;
                }
                ContentMatch::Hex => {
                    parse_hex_pattern(&content.text).map_err(|message| {
                        SearchError::InvalidPredicate {
                            field: "content".to_owned(),
                            message,
                        }
                    })?;
                }
                ContentMatch::Text => {}
            }
        }
        Ok(())
    }

    pub fn matches_metadata(&self, metadata: &ResourceMetadata) -> bool {
        let hidden = metadata.hidden.unwrap_or(false);
        if matches!(self.hidden, HiddenPolicy::Exclude) && hidden
            || matches!(self.hidden, HiddenPolicy::Only) && !hidden
        {
            return false;
        }
        if let Some(name) = &self.name
            && !matches_text(name, &metadata.name)
        {
            return false;
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&metadata.kind) {
            return false;
        }
        if self
            .minimum_size
            .is_some_and(|minimum| metadata.size.is_none_or(|size| size < minimum))
        {
            return false;
        }
        if self
            .maximum_size
            .is_some_and(|maximum| metadata.size.is_none_or(|size| size > maximum))
        {
            return false;
        }
        if self
            .modified_after_unix_ms
            .is_some_and(|after| metadata.modified_unix_ms.is_none_or(|value| value < after))
        {
            return false;
        }
        if self
            .modified_before_unix_ms
            .is_some_and(|before| metadata.modified_unix_ms.is_none_or(|value| value > before))
        {
            return false;
        }
        if self.readonly.is_some_and(|readonly| {
            metadata
                .permissions
                .as_ref()
                .is_none_or(|permissions| permissions.readonly != readonly)
        }) {
            return false;
        }
        if self.executable.is_some_and(|executable| {
            metadata
                .permissions
                .as_ref()
                .is_none_or(|permissions| permissions.executable != executable)
        }) {
            return false;
        }
        true
    }

    pub fn filters<'a>(
        &'a self,
        resources: impl IntoIterator<Item = &'a ResourceEntry> + 'a,
    ) -> impl Iterator<Item = &'a ResourceEntry> + 'a {
        resources
            .into_iter()
            .filter(|entry| self.matches_metadata(&entry.metadata))
    }

    fn should_descend(&self, metadata: &ResourceMetadata) -> bool {
        if metadata.kind != ResourceKind::Directory {
            return false;
        }
        let hidden = metadata.hidden.unwrap_or(false);
        if matches!(self.hidden, HiddenPolicy::Exclude) && hidden {
            return false;
        }
        !ignored_name(self.ignore, &metadata.name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub source: ResourceRef,
    pub metadata: ResourceMetadata,
    pub details: String,
}

impl SearchHit {
    pub fn resource_entry(&self, session: &str) -> ResourceEntry {
        let mut metadata = self.metadata.clone();
        metadata.extensions.insert(
            "near.search.session".to_owned(),
            MetadataValue::String(session.to_owned()),
        );
        metadata.extensions.insert(
            "near.search.source-provider".to_owned(),
            MetadataValue::String(self.source.provider.as_str().to_owned()),
        );
        ResourceEntry {
            resource: self.source.clone(),
            metadata,
            details: self.details.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchProgress {
    pub visited: usize,
    pub matched: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchEvent {
    Batch(Vec<SearchHit>),
    Progress(SearchProgress),
    Diagnostic(SearchDiagnostic),
    Finished(SearchProgress),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchDiagnostic {
    pub provider: ProviderId,
    pub location: Location,
    pub capability: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchRoot {
    pub provider: ProviderId,
    pub location: Location,
}

impl SearchRoot {
    pub fn new(provider: ProviderId, location: Location) -> Self {
        Self { provider, location }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArchiveSearchPolicy {
    #[default]
    Exclude,
    Include,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SymlinkSearchPolicy {
    Skip,
    #[default]
    Match,
    Follow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlternateStreamSearchPolicy {
    #[default]
    Exclude,
    Include,
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub root: Location,
    pub predicate: ResourcePredicate,
    pub page_size: usize,
    pub batch_size: usize,
    pub read_chunk_size: usize,
}

impl SearchRequest {
    pub fn new(root: Location, predicate: ResourcePredicate) -> Self {
        Self {
            root,
            predicate,
            page_size: 128,
            batch_size: 32,
            read_chunk_size: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScopedSearchRequest {
    pub roots: Vec<SearchRoot>,
    pub predicate: ResourcePredicate,
    pub page_size: usize,
    pub batch_size: usize,
    pub read_chunk_size: usize,
    pub archives: ArchiveSearchPolicy,
    pub symlinks: SymlinkSearchPolicy,
    pub streams: AlternateStreamSearchPolicy,
}

impl ScopedSearchRequest {
    pub fn new(roots: Vec<SearchRoot>, predicate: ResourcePredicate) -> Self {
        Self {
            roots,
            predicate,
            page_size: 128,
            batch_size: 32,
            read_chunk_size: 64 * 1024,
            archives: ArchiveSearchPolicy::Exclude,
            symlinks: SymlinkSearchPolicy::Match,
            streams: AlternateStreamSearchPolicy::Exclude,
        }
    }

    fn provider_request(&self, root: Location) -> SearchRequest {
        SearchRequest {
            root,
            predicate: self.predicate.clone(),
            page_size: self.page_size,
            batch_size: self.batch_size,
            read_chunk_size: self.read_chunk_size,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SearchError {
    #[error("unsupported predicate schema version {0}")]
    UnsupportedSchema(u16),
    #[error("invalid search field {field}: {message}")]
    InvalidPredicate { field: String, message: String },
    #[error("search was cancelled")]
    Cancelled,
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

#[derive(Default)]
pub struct SearchService;

impl SearchService {
    /// Recursively searches a provider and emits bounded result batches.
    ///
    /// # Errors
    ///
    /// Returns validation, cancellation, or provider errors. Results emitted before an error remain
    /// valid and actionable.
    pub fn search(
        &self,
        provider: &Arc<dyn ResourceProvider>,
        request: &SearchRequest,
        cancellation: &CancellationToken,
        emit: impl FnMut(SearchEvent),
    ) -> Result<SearchProgress, SearchError> {
        let mut providers = ProviderRegistry::default();
        providers
            .register(Arc::clone(provider))
            .map_err(|error| ProviderError::Failed(error.to_string()))?;
        self.search_scoped(
            &providers,
            &ScopedSearchRequest {
                roots: vec![SearchRoot::new(provider.id(), request.root.clone())],
                predicate: request.predicate.clone(),
                page_size: request.page_size,
                batch_size: request.batch_size,
                read_chunk_size: request.read_chunk_size,
                archives: ArchiveSearchPolicy::Exclude,
                symlinks: SymlinkSearchPolicy::Match,
                streams: AlternateStreamSearchPolicy::Exclude,
            },
            cancellation,
            emit,
        )
    }

    /// Searches one or more provider roots with explicit archive, link, and stream policies.
    ///
    /// Unsupported optional capabilities are emitted as diagnostics and do not discard results
    /// produced by searchable roots.
    ///
    /// # Errors
    ///
    /// Returns validation, cancellation, registry, or provider errors.
    #[allow(clippy::too_many_lines)]
    pub fn search_scoped(
        &self,
        providers: &ProviderRegistry,
        request: &ScopedSearchRequest,
        cancellation: &CancellationToken,
        mut emit: impl FnMut(SearchEvent),
    ) -> Result<SearchProgress, SearchError> {
        request.predicate.validate()?;
        let mut directories = VecDeque::from(request.roots.clone());
        let mut visited_directories = BTreeSet::new();
        let mut diagnosed_stream_providers = BTreeSet::new();
        let mut progress = SearchProgress::default();
        let mut batch = Vec::with_capacity(request.batch_size.max(1));
        while let Some(root) = directories.pop_front() {
            if !visited_directories.insert((root.provider.clone(), root.location.clone())) {
                continue;
            }
            let Some(provider) = providers.get(&root.provider) else {
                emit(SearchEvent::Diagnostic(SearchDiagnostic {
                    provider: root.provider,
                    location: root.location,
                    capability: "resource.list".to_owned(),
                    message: "search provider is not registered".to_owned(),
                }));
                continue;
            };
            let mut continuation = None;
            loop {
                check_cancelled(cancellation)?;
                let page = match block_on(provider.list(
                    &root.location,
                    ListRequest {
                        generation: ListingGeneration(1),
                        continuation: continuation.clone(),
                        page_size: request.page_size.max(1),
                        cancellation: cancellation.clone(),
                    },
                )) {
                    Ok(page) => page,
                    Err(ProviderError::Cancelled) => return Err(SearchError::Cancelled),
                    Err(error) => {
                        emit(SearchEvent::Diagnostic(SearchDiagnostic {
                            provider: provider.id(),
                            location: root.location.clone(),
                            capability: "resource.list".to_owned(),
                            message: error.to_string(),
                        }));
                        break;
                    }
                };
                for listed in page.entries {
                    check_cancelled(cancellation)?;
                    progress.visited = progress.visited.saturating_add(1);
                    let metadata = block_on(provider.stat(&listed.resource))
                        .unwrap_or_else(|_| listed.metadata.clone());
                    let is_symlink = metadata.kind == ResourceKind::Symlink;
                    if is_symlink && request.symlinks == SymlinkSearchPolicy::Skip {
                        continue;
                    }
                    if request.predicate.should_descend(&metadata) {
                        directories.push_back(SearchRoot::new(
                            listed.resource.provider.clone(),
                            listed.resource.location.clone(),
                        ));
                    } else if is_symlink && request.symlinks == SymlinkSearchPolicy::Follow {
                        if provider
                            .capabilities(&listed.resource)
                            .contains(&CapabilityId::from("resource.list"))
                        {
                            directories.push_back(SearchRoot::new(
                                listed.resource.provider.clone(),
                                listed.resource.location.clone(),
                            ));
                        } else {
                            emit(SearchEvent::Diagnostic(SearchDiagnostic {
                                provider: listed.resource.provider.clone(),
                                location: listed.resource.location.clone(),
                                capability: "search.follow-links".to_owned(),
                                message: "provider cannot follow this symbolic link".to_owned(),
                            }));
                        }
                    }
                    if request.archives == ArchiveSearchPolicy::Include
                        && metadata.kind != ResourceKind::Directory
                    {
                        match providers.mount(&listed.resource) {
                            Ok(Some((mounted_provider, location))) => directories
                                .push_back(SearchRoot::new(mounted_provider.id(), location)),
                            Ok(None) => {}
                            Err(error) => emit(SearchEvent::Diagnostic(SearchDiagnostic {
                                provider: listed.resource.provider.clone(),
                                location: listed.resource.location.clone(),
                                capability: "archive.mount".to_owned(),
                                message: error.to_string(),
                            })),
                        }
                    }
                    if request.streams == AlternateStreamSearchPolicy::Include
                        && metadata.kind == ResourceKind::File
                    {
                        if provider
                            .capabilities(&listed.resource)
                            .contains(&CapabilityId::from("resource.streams"))
                        {
                            search_streams(
                                &provider,
                                &listed.resource,
                                request,
                                cancellation,
                                &mut progress,
                                &mut batch,
                                &mut emit,
                            )?;
                        } else if diagnosed_stream_providers.insert(provider.id()) {
                            emit(SearchEvent::Diagnostic(SearchDiagnostic {
                                provider: provider.id(),
                                location: listed.resource.location.clone(),
                                capability: "resource.streams".to_owned(),
                                message: "provider does not expose alternate streams".to_owned(),
                            }));
                        }
                    }
                    if ignored_name(request.predicate.ignore, &metadata.name)
                        || !request.predicate.matches_metadata(&metadata)
                        || !matches_content(
                            &provider,
                            &listed.resource,
                            &metadata,
                            &request.provider_request(root.location.clone()),
                            cancellation,
                        )?
                    {
                        continue;
                    }
                    progress.matched = progress.matched.saturating_add(1);
                    batch.push(SearchHit {
                        source: listed.resource,
                        metadata,
                        details: listed.details,
                    });
                    if batch.len() >= request.batch_size.max(1) {
                        emit(SearchEvent::Batch(std::mem::take(&mut batch)));
                    }
                }
                emit(SearchEvent::Progress(progress));
                continuation = page.continuation;
                if continuation.is_none() {
                    break;
                }
            }
        }
        if !batch.is_empty() {
            emit(SearchEvent::Batch(batch));
        }
        emit(SearchEvent::Finished(progress));
        Ok(progress)
    }

    /// Refines an existing result set without traversing resources that were not already matched.
    ///
    /// # Errors
    ///
    /// Returns validation, cancellation, or source-provider errors while checking content.
    pub fn refine(
        &self,
        provider: &Arc<dyn ResourceProvider>,
        hits: &[SearchHit],
        predicate: &ResourcePredicate,
        cancellation: &CancellationToken,
    ) -> Result<Vec<SearchHit>, SearchError> {
        predicate.validate()?;
        let request = SearchRequest::new(Location::new("search://refine"), predicate.clone());
        let mut refined = Vec::new();
        for hit in hits {
            check_cancelled(cancellation)?;
            if ignored_name(predicate.ignore, &hit.metadata.name)
                || !predicate.matches_metadata(&hit.metadata)
                || !matches_content(provider, &hit.source, &hit.metadata, &request, cancellation)?
            {
                continue;
            }
            refined.push(hit.clone());
        }
        Ok(refined)
    }

    /// Refines results that may originate from different registered providers.
    ///
    /// # Errors
    ///
    /// Returns validation, cancellation, unavailable-provider, or content-read errors.
    pub fn refine_scoped(
        &self,
        providers: &ProviderRegistry,
        hits: &[SearchHit],
        predicate: &ResourcePredicate,
        cancellation: &CancellationToken,
    ) -> Result<Vec<SearchHit>, SearchError> {
        predicate.validate()?;
        let request = SearchRequest::new(Location::new("search://refine"), predicate.clone());
        let mut refined = Vec::new();
        for hit in hits {
            check_cancelled(cancellation)?;
            let provider = providers.get(&hit.source.provider).ok_or_else(|| {
                ProviderError::Unsupported(format!(
                    "search provider {} is unavailable",
                    hit.source.provider
                ))
            })?;
            if ignored_name(predicate.ignore, &hit.metadata.name)
                || !predicate.matches_metadata(&hit.metadata)
                || !matches_content(
                    &provider,
                    &hit.source,
                    &hit.metadata,
                    &request,
                    cancellation,
                )?
            {
                continue;
            }
            refined.push(hit.clone());
        }
        Ok(refined)
    }
}

pub struct SearchResultsProvider {
    session: String,
    location: Location,
    results: RwLock<Vec<SearchHit>>,
}

impl SearchResultsProvider {
    pub fn new(session: impl Into<String>) -> Self {
        let session = session.into();
        Self {
            location: Location::new(format!("search://sessions/{session}")),
            session,
            results: RwLock::new(Vec::new()),
        }
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn append(&self, hits: impl IntoIterator<Item = SearchHit>) {
        self.results
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(hits);
    }

    pub fn append_unique(&self, hits: impl IntoIterator<Item = SearchHit>) -> Vec<SearchHit> {
        let mut results = self
            .results
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut appended = Vec::new();
        for hit in hits {
            if results.iter().all(|existing| existing.source != hit.source) {
                results.push(hit.clone());
                appended.push(hit);
            }
        }
        appended
    }

    pub fn replace(&self, hits: Vec<SearchHit>) {
        *self
            .results
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = hits;
    }

    pub fn snapshot(&self) -> Vec<SearchHit> {
        self.results
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn len(&self) -> usize {
        self.results
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ResourceProvider for SearchResultsProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from(format!("near.search-results.{}", self.session))
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
            let results = self
                .results
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let offset = request
                .continuation
                .as_deref()
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| ProviderError::Failed("invalid continuation".to_owned()))?;
            let end = offset
                .saturating_add(request.page_size.max(1))
                .min(results.len());
            let entries = results[offset..end]
                .iter()
                .map(|hit| hit.resource_entry(&self.session))
                .collect();
            Ok(ListPage {
                generation: request.generation,
                entries,
                continuation: (end < results.len()).then(|| end.to_string()),
                complete: end == results.len(),
            })
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

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), SearchError> {
    if cancellation.is_cancelled() {
        Err(SearchError::Cancelled)
    } else {
        Ok(())
    }
}

fn ignored_name(policy: IgnorePolicy, name: &str) -> bool {
    matches!(policy, IgnorePolicy::VersionControl | IgnorePolicy::Common)
        && matches!(name, ".git" | ".hg" | ".svn")
        || matches!(policy, IgnorePolicy::Common) && matches!(name, "node_modules" | "target")
}

fn matches_text(predicate: &TextPredicate, value: &str) -> bool {
    match predicate {
        TextPredicate::Contains(needle) => value.contains(needle),
        TextPredicate::Exact(expected) => value == expected,
        TextPredicate::Glob(pattern) => glob_matches(pattern.as_bytes(), value.as_bytes()),
        TextPredicate::Regex(pattern) => {
            Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
        }
    }
}

fn glob_matches(pattern: &[u8], value: &[u8]) -> bool {
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry) = (None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    pattern[pattern_index..].iter().all(|byte| *byte == b'*')
}

fn matches_content(
    provider: &Arc<dyn ResourceProvider>,
    resource: &ResourceRef,
    metadata: &ResourceMetadata,
    request: &SearchRequest,
    cancellation: &CancellationToken,
) -> Result<bool, SearchError> {
    let Some(predicate) = &request.predicate.content else {
        return Ok(true);
    };
    if metadata.kind != ResourceKind::File {
        return Ok(false);
    }
    let bytes = read_resource(provider, resource, request, cancellation)?;
    match predicate.match_kind {
        ContentMatch::Hex => {
            let needle = parse_hex_pattern(&predicate.text).map_err(|message| {
                SearchError::InvalidPredicate {
                    field: "content".to_owned(),
                    message,
                }
            })?;
            Ok(needle.is_empty()
                || bytes
                    .windows(needle.len())
                    .any(|window| window == needle.as_slice()))
        }
        ContentMatch::Text => {
            let haystack = decode_content(&bytes, predicate.encoding);
            if predicate.case_sensitive {
                Ok(haystack.contains(&predicate.text))
            } else {
                Ok(haystack
                    .to_lowercase()
                    .contains(&predicate.text.to_lowercase()))
            }
        }
        ContentMatch::Regex => {
            let pattern = if predicate.case_sensitive {
                predicate.text.clone()
            } else {
                format!("(?i:{})", predicate.text)
            };
            let regex = Regex::new(&pattern).map_err(|error| SearchError::InvalidPredicate {
                field: "content".to_owned(),
                message: error.to_string(),
            })?;
            Ok(regex.is_match(&decode_content(&bytes, predicate.encoding)))
        }
    }
}

fn search_streams(
    provider: &Arc<dyn ResourceProvider>,
    resource: &ResourceRef,
    request: &ScopedSearchRequest,
    cancellation: &CancellationToken,
    progress: &mut SearchProgress,
    batch: &mut Vec<SearchHit>,
    emit: &mut impl FnMut(SearchEvent),
) -> Result<(), SearchError> {
    let mut continuation = None;
    loop {
        check_cancelled(cancellation)?;
        let page = match block_on(provider.streams(
            resource,
            ListRequest {
                generation: ListingGeneration(1),
                continuation: continuation.clone(),
                page_size: request.page_size.max(1),
                cancellation: cancellation.clone(),
            },
        )) {
            Ok(page) => page,
            Err(error) => {
                emit(SearchEvent::Diagnostic(SearchDiagnostic {
                    provider: provider.id(),
                    location: resource.location.clone(),
                    capability: "resource.streams".to_owned(),
                    message: error.to_string(),
                }));
                return Ok(());
            }
        };
        for stream in page.entries {
            check_cancelled(cancellation)?;
            progress.visited = progress.visited.saturating_add(1);
            let metadata = block_on(provider.stat(&stream.resource))
                .unwrap_or_else(|_| stream.metadata.clone());
            if ignored_name(request.predicate.ignore, &metadata.name)
                || !request.predicate.matches_metadata(&metadata)
                || !matches_content(
                    provider,
                    &stream.resource,
                    &metadata,
                    &request.provider_request(resource.location.clone()),
                    cancellation,
                )?
            {
                continue;
            }
            progress.matched = progress.matched.saturating_add(1);
            batch.push(SearchHit {
                source: stream.resource,
                metadata,
                details: stream.details,
            });
            if batch.len() >= request.batch_size.max(1) {
                emit(SearchEvent::Batch(std::mem::take(batch)));
            }
        }
        continuation = page.continuation;
        if continuation.is_none() {
            return Ok(());
        }
    }
}

fn read_resource(
    provider: &Arc<dyn ResourceProvider>,
    resource: &ResourceRef,
    request: &SearchRequest,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, SearchError> {
    let mut offset = 0_u64;
    let mut bytes = Vec::new();
    loop {
        check_cancelled(cancellation)?;
        let stream = block_on(provider.open(
            resource,
            OpenRequest {
                offset,
                length: request.read_chunk_size.max(1),
                cancellation: cancellation.clone(),
            },
        ))?;
        bytes.extend_from_slice(&stream.bytes);
        if stream.complete || stream.bytes.is_empty() {
            return Ok(bytes);
        }
        offset = stream.offset.saturating_add(stream.bytes.len() as u64);
    }
}

fn parse_hex_pattern(value: &str) -> Result<Vec<u8>, String> {
    let compact = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    if compact.len() % 2 != 0 {
        return Err("hex content must contain complete byte pairs".to_owned());
    }
    (0..compact.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&compact[index..index + 2], 16)
                .map_err(|_| format!("invalid hex byte `{}`", &compact[index..index + 2]))
        })
        .collect()
}

fn decode_content(bytes: &[u8], encoding: SearchEncoding) -> String {
    match encoding {
        SearchEncoding::Auto => {
            if bytes.starts_with(&[0xff, 0xfe]) {
                decode_utf16(&bytes[2..], true)
            } else if bytes.starts_with(&[0xfe, 0xff]) {
                decode_utf16(&bytes[2..], false)
            } else if let Ok(value) =
                std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            {
                value.to_owned()
            } else {
                decode_latin1(bytes)
            }
        }
        SearchEncoding::Utf8 => {
            String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
                .into_owned()
        }
        SearchEncoding::Utf16Le => {
            decode_utf16(bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes), true)
        }
        SearchEncoding::Utf16Be => {
            decode_utf16(bytes.strip_prefix(&[0xfe, 0xff]).unwrap_or(bytes), false)
        }
        SearchEncoding::Latin1 => decode_latin1(bytes),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    char::decode_utf16(units)
        .map(|result| result.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    use near_archive::ZipArchiveProvider;
    use near_core::{
        ListRequest, ListingGeneration, PermissionSummary, ResourceEntry, ResourceProvider,
    };
    use near_local_fs::LocalFileProvider;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    fn fixture_entry(name: &str, hidden: bool, size: u64) -> ResourceEntry {
        ResourceEntry {
            resource: LocalFileProvider::resource_for_path(std::path::Path::new(name)),
            metadata: ResourceMetadata {
                name: name.to_owned(),
                kind: ResourceKind::File,
                size: Some(size),
                hidden: Some(hidden),
                ..ResourceMetadata::default()
            },
            details: String::new(),
        }
    }

    #[test]
    fn saved_predicate_has_equivalent_panel_and_operation_sets() {
        let predicate = ResourcePredicate {
            name: Some(TextPredicate::Glob("*.rs".to_owned())),
            minimum_size: Some(5),
            hidden: HiddenPolicy::Include,
            ..ResourcePredicate::default()
        };
        let entries = [
            fixture_entry("main.rs", false, 10),
            fixture_entry("tiny.rs", false, 1),
            fixture_entry("notes.md", false, 10),
            fixture_entry(".hidden.rs", true, 10),
        ];
        let panel = predicate
            .filters(&entries)
            .map(|entry| entry.resource.clone())
            .collect::<Vec<_>>();
        let operation = entries
            .iter()
            .filter(|entry| predicate.matches_metadata(&entry.metadata))
            .map(|entry| entry.resource.clone())
            .collect::<Vec<_>>();
        assert_eq!(panel, operation);
    }

    #[test]
    fn advanced_metadata_criteria_compose_and_validate_ranges() {
        let metadata = ResourceMetadata {
            name: "report-2026.txt".to_owned(),
            kind: ResourceKind::File,
            size: Some(4096),
            modified_unix_ms: Some(1_750_000_000_000),
            permissions: Some(PermissionSummary {
                unix_mode: Some(0o555),
                readonly: true,
                executable: true,
            }),
            hidden: Some(false),
            ..ResourceMetadata::default()
        };
        let predicate = ResourcePredicate {
            name: Some(TextPredicate::Regex(r"^report-\d{4}\.txt$".to_owned())),
            kinds: vec![ResourceKind::File],
            minimum_size: Some(1024),
            maximum_size: Some(8192),
            modified_after_unix_ms: Some(1_700_000_000_000),
            modified_before_unix_ms: Some(1_800_000_000_000),
            readonly: Some(true),
            executable: Some(true),
            hidden: HiddenPolicy::Exclude,
            ..ResourcePredicate::default()
        };
        predicate.validate().unwrap();
        assert!(predicate.matches_metadata(&metadata));

        let invalid = ResourcePredicate {
            minimum_size: Some(10),
            maximum_size: Some(1),
            ..ResourcePredicate::default()
        };
        assert!(matches!(
            invalid.validate(),
            Err(SearchError::InvalidPredicate { ref field, .. }) if field == "minimum-size"
        ));
        let mislabeled = ResourcePredicate {
            schema_version: 1,
            name: Some(TextPredicate::Regex("report".to_owned())),
            ..ResourcePredicate::default()
        };
        assert!(matches!(
            mislabeled.validate(),
            Err(SearchError::InvalidPredicate { ref field, .. }) if field == "schema-version"
        ));
    }

    #[test]
    fn regex_hex_and_encoding_predicates_search_real_files() {
        let root = std::env::temp_dir().join(format!(
            "near-search-advanced-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let utf16 = "Café number 42"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        fs::write(root.join("unicode.txt"), [vec![0xff, 0xfe], utf16].concat()).unwrap();
        fs::write(
            root.join("binary.dat"),
            [0x00, 0xde, 0xad, 0xbe, 0xef, 0xff],
        )
        .unwrap();
        let provider: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);

        let search = |content: ContentPredicate| {
            let mut names = Vec::new();
            SearchService
                .search(
                    &provider,
                    &SearchRequest::new(
                        LocalFileProvider::location(&root),
                        ResourcePredicate {
                            kinds: vec![ResourceKind::File],
                            content: Some(content),
                            hidden: HiddenPolicy::Include,
                            ..ResourcePredicate::default()
                        },
                    ),
                    &CancellationToken::default(),
                    |event| {
                        if let SearchEvent::Batch(batch) = event {
                            names.extend(batch.into_iter().map(|hit| hit.metadata.name));
                        }
                    },
                )
                .unwrap();
            names
        };

        assert_eq!(
            search(ContentPredicate {
                text: r"Caf. number \d+".to_owned(),
                case_sensitive: true,
                match_kind: ContentMatch::Regex,
                encoding: SearchEncoding::Utf16Le,
            }),
            vec!["unicode.txt"]
        );
        assert_eq!(
            search(ContentPredicate {
                text: "DE AD BE EF".to_owned(),
                case_sensitive: false,
                match_kind: ContentMatch::Hex,
                encoding: SearchEncoding::Auto,
            }),
            vec!["binary.dat"]
        );
        let invalid = ContentPredicate {
            text: "ABC".to_owned(),
            case_sensitive: false,
            match_kind: ContentMatch::Hex,
            encoding: SearchEncoding::Auto,
        };
        assert!(matches!(
            ResourcePredicate {
                content: Some(invalid),
                ..ResourcePredicate::default()
            }
            .validate(),
            Err(SearchError::InvalidPredicate { ref field, .. }) if field == "content"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_predicate_round_trips_through_toml() {
        let source = include_str!("../tests/fixtures/predicate-v1.toml");
        let predicate: ResourcePredicate = toml::from_str(source).unwrap();
        predicate.validate().unwrap();
        let encoded = toml::to_string_pretty(&predicate).unwrap();
        let decoded: ResourcePredicate = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, predicate);
    }

    #[test]
    fn recursive_content_search_streams_source_resources_and_keeps_cancelled_results() {
        let root = std::env::temp_dir().join(format!(
            "near-search-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("first.txt"), "needle one").unwrap();
        fs::write(root.join("nested/second.txt"), "needle two").unwrap();
        fs::write(root.join("nested/nope.txt"), "other").unwrap();
        let provider: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);
        let cancellation = CancellationToken::default();
        let mut received = Vec::new();
        let result = SearchService.search(
            &provider,
            &SearchRequest {
                batch_size: 1,
                read_chunk_size: 4,
                ..SearchRequest::new(
                    LocalFileProvider::location(&root),
                    ResourcePredicate {
                        kinds: vec![ResourceKind::File],
                        content: Some(ContentPredicate {
                            text: "needle".to_owned(),
                            case_sensitive: true,
                            match_kind: ContentMatch::Text,
                            encoding: SearchEncoding::Utf8,
                        }),
                        ..ResourcePredicate::default()
                    },
                )
            },
            &cancellation,
            |event| {
                if let SearchEvent::Batch(batch) = event {
                    received.extend(batch);
                    cancellation.cancel();
                }
            },
        );
        assert_eq!(result, Err(SearchError::Cancelled));
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].source.provider, provider.id());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scoped_search_traverses_archives_and_reports_unsupported_streams() {
        let root = std::env::temp_dir().join(format!(
            "near-search-scoped-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("outside.txt"), "outside").unwrap();
        let archive_path = root.join("fixture.zip");
        let mut archive = ZipWriter::new(fs::File::create(&archive_path).unwrap());
        let options = SimpleFileOptions::default();
        archive.start_file("inside.txt", options).unwrap();
        archive.write_all(b"inside archive").unwrap();
        archive.finish().unwrap();

        let local: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);
        let archives: Arc<dyn ResourceProvider> = Arc::new(ZipArchiveProvider);
        let mut providers = ProviderRegistry::default();
        providers.register(Arc::clone(&local)).unwrap();
        providers.register(archives).unwrap();
        let mut request = ScopedSearchRequest::new(
            vec![SearchRoot::new(
                local.id(),
                LocalFileProvider::location(&root),
            )],
            ResourcePredicate {
                name: Some(TextPredicate::Glob("*.txt".to_owned())),
                kinds: vec![ResourceKind::File],
                ..ResourcePredicate::default()
            },
        );
        request.archives = ArchiveSearchPolicy::Include;
        request.streams = AlternateStreamSearchPolicy::Include;
        let mut hits = Vec::new();
        let mut diagnostics = Vec::new();
        let progress = SearchService
            .search_scoped(
                &providers,
                &request,
                &CancellationToken::default(),
                |event| match event {
                    SearchEvent::Batch(batch) => hits.extend(batch),
                    SearchEvent::Diagnostic(diagnostic) => diagnostics.push(diagnostic),
                    SearchEvent::Progress(_) | SearchEvent::Finished(_) => {}
                },
            )
            .unwrap();
        let mut names = hits
            .iter()
            .map(|hit| hit.metadata.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["inside.txt", "outside.txt"]);
        assert_eq!(progress.matched, 2);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.provider == local.id() && diagnostic.capability == "resource.streams"
        }));
        assert!(
            hits.iter()
                .any(|hit| hit.source.provider == ProviderId::from("near.archive"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_collection_pages_exact_source_references() {
        let source = LocalFileProvider::resource_for_path(std::path::Path::new("/tmp/result"));
        let provider = SearchResultsProvider::new("test");
        provider.append([SearchHit {
            source: source.clone(),
            metadata: ResourceMetadata {
                name: "result".to_owned(),
                kind: ResourceKind::File,
                ..ResourceMetadata::default()
            },
            details: "match".to_owned(),
        }]);
        let page = block_on(provider.list(
            provider.location(),
            ListRequest {
                generation: ListingGeneration(7),
                continuation: None,
                page_size: 10,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(page.entries[0].resource, source);
    }

    #[test]
    fn search_collection_append_is_unique_and_replaceable() {
        let first = SearchHit {
            source: LocalFileProvider::resource_for_path(std::path::Path::new("/tmp/first")),
            metadata: ResourceMetadata {
                name: "first".to_owned(),
                kind: ResourceKind::File,
                ..ResourceMetadata::default()
            },
            details: "first match".to_owned(),
        };
        let second = SearchHit {
            source: LocalFileProvider::resource_for_path(std::path::Path::new("/tmp/second")),
            metadata: ResourceMetadata {
                name: "second".to_owned(),
                kind: ResourceKind::File,
                ..ResourceMetadata::default()
            },
            details: "second match".to_owned(),
        };
        let provider = SearchResultsProvider::new("test");

        assert_eq!(provider.append_unique([first.clone()]), vec![first.clone()]);
        assert!(provider.append_unique([first.clone()]).is_empty());
        assert_eq!(
            provider.append_unique([second.clone()]),
            vec![second.clone()]
        );
        assert_eq!(provider.snapshot(), vec![first.clone(), second]);

        provider.replace(vec![first.clone()]);
        assert_eq!(provider.snapshot(), vec![first]);
    }
}
