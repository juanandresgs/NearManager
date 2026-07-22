use serde::{Deserialize, Serialize};

use crate::ResourceRef;
use crate::{Location, ProviderId};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderLocationEntry {
    pub provider: ProviderId,
    pub location: Location,
    pub label: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

impl FolderLocationEntry {
    pub fn new(provider: ProviderId, location: Location, label: impl Into<String>) -> Self {
        Self {
            provider,
            location,
            label: label.into(),
            last_error: None,
            locked: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderNavigationState {
    #[serde(default)]
    pub history: Vec<FolderLocationEntry>,
    #[serde(default)]
    pub shortcuts: Vec<Option<FolderLocationEntry>>,
    #[serde(default = "default_folder_history_limit")]
    pub max_unlocked: usize,
}

impl Default for FolderNavigationState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            shortcuts: Vec::new(),
            max_unlocked: default_folder_history_limit(),
        }
    }
}

const fn default_folder_history_limit() -> usize {
    200
}

pub trait FolderNavigationStore: Send + Sync {
    /// Loads folder history and shortcuts.
    ///
    /// # Errors
    ///
    /// Returns storage or decoding failures.
    fn load(&self) -> Result<FolderNavigationState, String>;
    /// Saves folder history and shortcuts atomically.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn save(&self, state: &FolderNavigationState) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorPositionEntry {
    pub provider: ProviderId,
    pub location: Location,
    pub row: usize,
    pub column: usize,
    pub top: usize,
}

pub trait EditorPositionStore: Send + Sync {
    /// Loads provider-scoped editor positions.
    ///
    /// # Errors
    ///
    /// Returns storage or decoding failures.
    fn load(&self) -> Result<Vec<EditorPositionEntry>, String>;
    /// Saves provider-scoped editor positions atomically.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn save(&self, entries: &[EditorPositionEntry]) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ViewerStateEntry {
    pub provider: ProviderId,
    pub location: Location,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub bookmarks: std::collections::BTreeMap<u8, u64>,
    #[serde(default)]
    pub navigation_history: Vec<u64>,
    #[serde(default)]
    pub navigation_index: usize,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub wrap: Option<bool>,
    #[serde(default)]
    pub hex: Option<bool>,
}

pub trait ViewerStateStore: Send + Sync {
    /// Loads provider-scoped viewer state.
    ///
    /// # Errors
    ///
    /// Returns storage or decoding failures.
    fn load(&self) -> Result<Vec<ViewerStateEntry>, String>;
    /// Saves provider-scoped viewer state atomically.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn save(&self, entries: &[ViewerStateEntry]) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceHistoryEntry {
    pub resource: ResourceRef,
    pub label: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub use_count: u64,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl ResourceHistoryEntry {
    pub fn new(resource: ResourceRef, label: impl Into<String>) -> Self {
        Self {
            resource,
            label: label.into(),
            locked: false,
            use_count: 1,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceHistoryState {
    #[serde(default)]
    pub viewed: Vec<ResourceHistoryEntry>,
    #[serde(default)]
    pub edited: Vec<ResourceHistoryEntry>,
    #[serde(default = "default_resource_history_limit")]
    pub max_unlocked: usize,
}

impl Default for ResourceHistoryState {
    fn default() -> Self {
        Self {
            viewed: Vec::new(),
            edited: Vec::new(),
            max_unlocked: default_resource_history_limit(),
        }
    }
}

const fn default_resource_history_limit() -> usize {
    100
}

pub trait ResourceHistoryStore: Send + Sync {
    /// Loads viewed and edited resource history.
    ///
    /// # Errors
    ///
    /// Returns storage or decoding failures.
    fn load(&self) -> Result<ResourceHistoryState, String>;
    /// Saves viewed and edited resource history atomically.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn save(&self, state: &ResourceHistoryState) -> Result<(), String>;
}
