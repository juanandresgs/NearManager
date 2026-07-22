use std::collections::{BTreeMap, BTreeSet};
use std::{borrow::Cow, fmt};

use serde::{Deserialize, Serialize};

use crate::{CapabilityId, ProviderId};

pub const RESOURCE_DESCRIPTION_KEY: &str = "near.description";

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Location(String);

impl Location {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn display_compact(&self) -> Cow<'_, str> {
        self.0.strip_prefix("file://").map_or_else(
            || Cow::Borrowed(self.as_str()),
            |path| Cow::Owned(decode_uri_path(path)),
        )
    }
}

fn decode_uri_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(value) = bytes
                .get(index + 1..index + 3)
                .and_then(|digits| std::str::from_utf8(digits).ok())
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
        {
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub provider: ProviderId,
    pub location: Location,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ResourceIdentity {
    provider: ProviderId,
    stable_id: String,
}

impl ResourceIdentity {
    pub fn new(provider: ProviderId, stable_id: impl Into<String>) -> Self {
        Self {
            provider,
            stable_id: stable_id.into(),
        }
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.provider, self.location.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResourceKind {
    File,
    Directory,
    Package,
    Symlink,
    Virtual,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResourceClassification {
    Ordinary,
    FilesystemRoot,
    MountRoot,
    ProviderRoot,
    VirtualRoot,
    RemovableDevice,
    Symlink,
    UnsupportedSpecial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MutationKind {
    Trash,
    Delete,
    Wipe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MutationAlternative {
    Eject,
    Unmount,
    Disconnect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationDenial {
    pub reason: String,
    pub alternative: Option<MutationAlternative>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MutationEligibility {
    Allowed,
    Denied(MutationDenial),
}

impl MutationEligibility {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn denial(&self) -> Option<&MutationDenial> {
        match self {
            Self::Allowed => None,
            Self::Denied(denial) => Some(denial),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceMetadata {
    pub name: String,
    pub kind: ResourceKind,
    pub size: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    pub created_unix_ms: Option<i64>,
    pub accessed_unix_ms: Option<i64>,
    pub stable_id: Option<String>,
    pub permissions: Option<PermissionSummary>,
    pub owner: Option<OwnerSummary>,
    pub hidden: Option<bool>,
    pub link_target: Option<Location>,
    pub extensions: BTreeMap<String, MetadataValue>,
    pub field_errors: BTreeMap<String, String>,
}

impl ResourceMetadata {
    /// Returns provider-scoped durable identity when the provider supplied one.
    ///
    /// Locations and display names are deliberately excluded because either may change while the
    /// underlying resource remains the same.
    pub fn identity_for(&self, resource: &ResourceRef) -> Option<ResourceIdentity> {
        self.stable_id
            .as_ref()
            .map(|stable_id| ResourceIdentity::new(resource.provider.clone(), stable_id))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PermissionSummary {
    pub unix_mode: Option<u32>,
    pub readonly: bool,
    pub executable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSummary {
    pub user_id: Option<u32>,
    pub group_id: Option<u32>,
    pub user_name: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetadataValue {
    Boolean(bool),
    Integer(i64),
    String(String),
    Strings(Vec<String>),
    Bytes(Vec<u8>),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySet(BTreeSet<CapabilityId>);

impl CapabilitySet {
    pub fn insert(&mut self, capability: impl Into<CapabilityId>) -> bool {
        self.0.insert(capability.into())
    }

    pub fn contains(&self, capability: &CapabilityId) -> bool {
        self.0.contains(capability)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityId> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{Location, ResourceMetadata, ResourceRef};
    use crate::ProviderId;

    #[test]
    fn compact_display_decodes_local_paths_without_changing_remote_locations() {
        assert_eq!(
            Location::new("file:///Users/test/My%20Files").display_compact(),
            "/Users/test/My Files"
        );
        assert_eq!(
            Location::new("sftp://host/path").display_compact(),
            "sftp://host/path"
        );
    }

    #[test]
    fn durable_identity_is_provider_scoped_and_ignores_display_location() {
        let metadata = ResourceMetadata {
            name: "display name".to_owned(),
            stable_id: Some("device:42:inode:7".to_owned()),
            ..ResourceMetadata::default()
        };
        let before = ResourceRef {
            provider: ProviderId::from("near.local-file"),
            location: Location::new("file:///before.txt"),
        };
        let after = ResourceRef {
            provider: ProviderId::from("near.local-file"),
            location: Location::new("file:///after.txt"),
        };
        assert_eq!(
            metadata.identity_for(&before),
            metadata.identity_for(&after)
        );
        let identity = metadata.identity_for(&before).unwrap();
        assert_eq!(identity.provider().as_str(), "near.local-file");
        assert_eq!(identity.stable_id(), "device:42:inode:7");
    }
}
