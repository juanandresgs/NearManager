use near_core::ViewerStateEntry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceOpenPolicy {
    #[default]
    Internal,
    External,
    Association,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewerEncoding {
    #[default]
    Auto,
    Utf8Lossy,
    Utf16Le,
    Utf16Be,
    Latin1,
}

impl ViewerEncoding {
    pub const fn config_name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Utf8Lossy => "utf8-lossy",
            Self::Utf16Le => "utf16le",
            Self::Utf16Be => "utf16be",
            Self::Latin1 => "latin1",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "utf8-lossy" | "utf-8" | "utf8" => Some(Self::Utf8Lossy),
            "utf16le" | "utf-16le" => Some(Self::Utf16Le),
            "utf16be" | "utf-16be" => Some(Self::Utf16Be),
            "latin1" | "latin-1" => Some(Self::Latin1),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Utf8Lossy => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Latin1 => "latin-1",
        }
    }

    pub fn detect(bytes: &[u8]) -> Self {
        if bytes.starts_with(&[0xff, 0xfe]) {
            return Self::Utf16Le;
        }
        if bytes.starts_with(&[0xfe, 0xff]) {
            return Self::Utf16Be;
        }
        let sample = &bytes[..bytes.len().min(256)];
        let (even_nuls, odd_nuls) = sample.iter().enumerate().fold(
            (0_usize, 0_usize),
            |(even, odd), (index, byte)| match (index % 2, byte) {
                (0, 0) => (even + 1, odd),
                (1, 0) => (even, odd + 1),
                _ => (even, odd),
            },
        );
        if odd_nuls >= 2 && odd_nuls > even_nuls.saturating_mul(2) {
            Self::Utf16Le
        } else if even_nuls >= 2 && even_nuls > odd_nuls.saturating_mul(2) {
            Self::Utf16Be
        } else {
            Self::Utf8Lossy
        }
    }

    #[must_use]
    pub fn resolved(self, bytes: &[u8]) -> Self {
        if self == Self::Auto {
            Self::detect(bytes)
        } else {
            self
        }
    }

    pub const fn newline_bytes(self) -> &'static [u8] {
        match self {
            Self::Utf16Le => b"\n\0",
            Self::Utf16Be => b"\0\n",
            Self::Auto | Self::Utf8Lossy | Self::Latin1 => b"\n",
        }
    }

    pub fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Auto => Self::detect(bytes).decode(bytes),
            Self::Utf8Lossy => {
                String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
                    .into_owned()
            }
            Self::Utf16Le => decode_utf16(bytes, true),
            Self::Utf16Be => decode_utf16(bytes, false),
            Self::Latin1 => bytes.iter().map(|byte| char::from(*byte)).collect(),
        }
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Self::Auto | Self::Utf8Lossy => text.as_bytes().to_vec(),
            Self::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Self::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            Self::Latin1 => text
                .chars()
                .filter_map(|character| u8::try_from(u32::from(character)).ok())
                .collect(),
        }
    }

    pub fn byte_column(self, bytes: &[u8], character_column: usize) -> usize {
        let bom = match self {
            Self::Utf8Lossy if bytes.starts_with(&[0xef, 0xbb, 0xbf]) => 3,
            Self::Utf16Le if bytes.starts_with(&[0xff, 0xfe]) => 2,
            Self::Utf16Be if bytes.starts_with(&[0xfe, 0xff]) => 2,
            _ => 0,
        };
        let decoded = self.decode(bytes);
        bom + self
            .encode(&decoded.chars().take(character_column).collect::<String>())
            .len()
    }

    pub fn line_bytes(self, bytes: &[u8], offset: u64) -> Vec<u8> {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let tail = &bytes[start..];
        tail[..find_bytes(tail, self.newline_bytes()).unwrap_or(tail.len())].to_vec()
    }

    pub fn next_line(self, bytes: &[u8], offset: u64) -> u64 {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        find_bytes(&bytes[start..], self.newline_bytes()).map_or(offset, |index| {
            offset.saturating_add(
                u64::try_from(index + self.newline_bytes().len()).unwrap_or(u64::MAX),
            )
        })
    }

    pub fn previous_line(self, bytes: &[u8], offset: u64) -> u64 {
        let newline = self.newline_bytes();
        let end = usize::try_from(offset)
            .unwrap_or(bytes.len())
            .min(bytes.len());
        let slice = &bytes[..end.saturating_sub(newline.len())];
        rfind_bytes(slice, newline).map_or(0, |position| {
            u64::try_from(position + newline.len()).unwrap_or(0)
        })
    }

    pub fn line_offset(self, bytes: &[u8], line: u64) -> Option<u64> {
        if line <= 1 {
            return Some(0);
        }
        let mut current_line = 1_u64;
        let mut relative = 0;
        while let Some(index) = find_bytes(&bytes[relative..], self.newline_bytes()) {
            current_line = current_line.saturating_add(1);
            relative += index + self.newline_bytes().len();
            if current_line == line {
                return u64::try_from(relative).ok();
            }
        }
        None
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty())
        .then(|| {
            haystack
                .windows(needle.len())
                .position(|window| window == needle)
        })
        .flatten()
}

fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty())
        .then(|| {
            haystack
                .windows(needle.len())
                .rposition(|window| window == needle)
        })
        .flatten()
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let bytes = if little_endian {
        bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes)
    } else {
        bytes.strip_prefix(&[0xfe, 0xff]).unwrap_or(bytes)
    };
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    String::from_utf16_lossy(&units.collect::<Vec<_>>())
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ViewerSettings {
    pub schema: u32,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub hex: bool,
    #[serde(default = "default_true")]
    pub detect_binary: bool,
    #[serde(default)]
    pub encoding: ViewerEncoding,
    #[serde(default)]
    pub open_policy: ResourceOpenPolicy,
    #[serde(default = "default_true")]
    pub remember_per_resource: bool,
    #[serde(default = "default_true")]
    pub remember_position: bool,
    #[serde(default = "default_true")]
    pub remember_bookmarks: bool,
    #[serde(default = "default_true")]
    pub remember_encoding: bool,
    #[serde(default = "default_true")]
    pub remember_view_mode: bool,
}

impl ViewerSettings {
    /// Parses and validates a versioned viewer policy document.
    ///
    /// # Errors
    ///
    /// Returns a parse error or an unsupported-schema diagnostic.
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let settings: Self = toml::from_str(source).map_err(|error| error.to_string())?;
        if settings.schema != 1 {
            return Err(format!(
                "unsupported viewer settings schema {}",
                settings.schema
            ));
        }
        Ok(settings)
    }

    #[must_use]
    pub fn filter_state(self, mut state: ViewerStateEntry) -> Option<ViewerStateEntry> {
        if !self.remember_per_resource {
            return None;
        }
        if !self.remember_position {
            state.offset = 0;
            state.navigation_history = vec![0];
            state.navigation_index = 0;
        }
        if !self.remember_bookmarks {
            state.bookmarks.clear();
        }
        if !self.remember_encoding {
            state.encoding = None;
        }
        if !self.remember_view_mode {
            state.wrap = None;
            state.hex = None;
        }
        Some(state)
    }

    pub fn is_binary(bytes: &[u8]) -> bool {
        let sample = &bytes[..bytes.len().min(4096)];
        sample.contains(&0)
            || (!sample.is_empty()
                && sample
                    .iter()
                    .filter(|byte| **byte < 0x09 || (0x0e..0x20).contains(*byte))
                    .count()
                    .saturating_mul(8)
                    > sample.len())
    }
}

impl Default for ViewerSettings {
    fn default() -> Self {
        Self {
            schema: 1,
            wrap: false,
            hex: false,
            detect_binary: true,
            encoding: ViewerEncoding::Auto,
            open_policy: ResourceOpenPolicy::Internal,
            remember_per_resource: true,
            remember_position: true,
            remember_bookmarks: true,
            remember_encoding: true,
            remember_view_mode: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct EditorSettings {
    pub schema: u32,
    #[serde(default)]
    pub persistent_blocks: bool,
    #[serde(default)]
    pub open_policy: ResourceOpenPolicy,
    #[serde(default = "default_tab_size")]
    pub tab_size: u8,
    #[serde(default)]
    pub expand_tabs: bool,
}

impl EditorSettings {
    /// Parses and validates a versioned editor policy document.
    ///
    /// # Errors
    ///
    /// Returns a parse, unsupported-schema, or invalid tab-size diagnostic.
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let settings: Self = toml::from_str(source).map_err(|error| error.to_string())?;
        if settings.schema != 1 {
            return Err(format!(
                "unsupported editor settings schema {}",
                settings.schema
            ));
        }
        if !(1..=16).contains(&settings.tab_size) {
            return Err("editor tab_size must be between 1 and 16".to_owned());
        }
        Ok(settings)
    }
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            schema: 1,
            persistent_blocks: false,
            open_policy: ResourceOpenPolicy::Internal,
            tab_size: default_tab_size(),
            expand_tabs: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

const fn default_tab_size() -> u8 {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_policies_validate_versions_and_ranges() {
        let viewer = ViewerSettings::from_toml(
            "schema = 1\nwrap = true\nencoding = 'latin1'\nopen_policy = 'external'\n",
        )
        .unwrap();
        assert!(viewer.wrap);
        assert_eq!(viewer.encoding, ViewerEncoding::Latin1);
        assert_eq!(viewer.open_policy, ResourceOpenPolicy::External);
        let editor = EditorSettings::from_toml(
            "schema = 1\ntab_size = 8\nexpand_tabs = true\nopen_policy = 'association'\n",
        )
        .unwrap();
        assert_eq!(editor.tab_size, 8);
        assert!(editor.expand_tabs);
        assert_eq!(editor.open_policy, ResourceOpenPolicy::Association);
        assert!(ViewerSettings::from_toml("schema = 2\n").is_err());
        assert!(EditorSettings::from_toml("schema = 1\ntab_size = 0\n").is_err());
        let utf16le = [0xff, 0xfe, b'N', 0, b'e', 0, b'a', 0, b'r', 0, b'\n', 0];
        assert_eq!(ViewerEncoding::detect(&utf16le), ViewerEncoding::Utf16Le);
        assert_eq!(
            ViewerEncoding::Auto.resolved(&utf16le),
            ViewerEncoding::Utf16Le
        );
        assert_eq!(ViewerEncoding::Utf16Le.decode(&utf16le), "Near\n");
        assert_eq!(ViewerEncoding::Utf16Le.encode("Near\n"), utf16le[2..]);
        assert_eq!(ViewerEncoding::Utf16Le.newline_bytes(), b"\n\0");
        assert_eq!(ViewerEncoding::Utf16Le.byte_column(&utf16le, 2), 6);
        assert!(ViewerSettings::is_binary(b"text\0binary"));
        assert!(!ViewerSettings::is_binary(b"ordinary text\n"));
        let state = ViewerStateEntry {
            provider: "test".into(),
            location: near_core::Location::new("test://resource"),
            offset: 42,
            bookmarks: [(1, 21)].into_iter().collect(),
            navigation_history: vec![0, 42],
            navigation_index: 1,
            encoding: Some("utf-16le".to_owned()),
            wrap: Some(true),
            hex: Some(false),
        };
        assert!(
            ViewerSettings {
                remember_per_resource: false,
                ..ViewerSettings::default()
            }
            .filter_state(state.clone())
            .is_none()
        );
        let filtered = ViewerSettings {
            remember_position: false,
            remember_bookmarks: false,
            ..ViewerSettings::default()
        }
        .filter_state(state)
        .unwrap();
        assert_eq!(filtered.offset, 0);
        assert!(filtered.bookmarks.is_empty());
        assert_eq!(filtered.encoding.as_deref(), Some("utf-16le"));
    }
}
