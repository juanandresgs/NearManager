#![allow(clippy::missing_errors_doc)]

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use near_config::{ViewerEncoding, ViewerSettings};
use near_core::{
    CancellationToken, CapabilitySet, Clipboard, ContextId, OpenRequest, ProviderError,
    ProviderFuture, ResourceProvider, ResourceRef, ResourceStream, SurfaceId, ViewerStateEntry,
};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfacePresentation, SurfaceState,
    TextAlignment, UpdateContext, UpdateResult,
};

const DEFAULT_WINDOW_SIZE: usize = 64 * 1024;
const DEFAULT_PAGE_ROWS: isize = 16;
const MAX_COPY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ViewerPrompt {
    #[default]
    None,
    Search,
    GoTo,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ViewerSelectionMode {
    #[default]
    Stream,
    Column,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ViewerPoint {
    offset: u64,
    column: usize,
}

#[derive(Clone, Debug)]
pub struct ViewerLoadTicket {
    generation: u64,
    cancellation: CancellationToken,
}

impl ViewerLoadTicket {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ViewerRequestTracker {
    generation: u64,
    active: Option<CancellationToken>,
}

impl ViewerRequestTracker {
    pub fn begin(&mut self) -> ViewerLoadTicket {
        if let Some(active) = self.active.take() {
            active.cancel();
        }
        self.generation = self.generation.saturating_add(1);
        let cancellation = CancellationToken::default();
        self.active = Some(cancellation.clone());
        ViewerLoadTicket {
            generation: self.generation,
            cancellation,
        }
    }

    pub fn is_current(&self, ticket: &ViewerLoadTicket) -> bool {
        ticket.generation == self.generation && !ticket.cancellation.is_cancelled()
    }

    pub fn cancel(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel();
        }
        self.generation = self.generation.saturating_add(1);
    }
}

struct ViewerDocument {
    provider: Arc<dyn ResourceProvider>,
    resource: ResourceRef,
    window_offset: u64,
    bytes: Vec<u8>,
    total_size: Option<u64>,
    complete: bool,
    window_size: usize,
    cancellation: CancellationToken,
}

impl ViewerDocument {
    fn open(
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderError> {
        let mut document = Self {
            provider,
            resource,
            window_offset: 0,
            bytes: Vec::new(),
            total_size: None,
            complete: false,
            window_size: DEFAULT_WINDOW_SIZE,
            cancellation,
        };
        document.load(0)?;
        Ok(document)
    }

    fn from_stream(
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        cancellation: CancellationToken,
        stream: ResourceStream,
    ) -> Self {
        Self {
            provider,
            resource,
            window_offset: stream.offset,
            bytes: stream.bytes,
            total_size: stream.total_size,
            complete: stream.complete,
            window_size: DEFAULT_WINDOW_SIZE,
            cancellation,
        }
    }

    fn load(&mut self, offset: u64) -> Result<(), ProviderError> {
        if self.cancellation.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let stream = block_on_provider(self.provider.open(
            &self.resource,
            OpenRequest {
                offset,
                length: self.window_size,
                cancellation: self.cancellation.clone(),
            },
        ))?;
        if self.cancellation.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        self.window_offset = stream.offset;
        self.bytes = stream.bytes;
        self.total_size = stream.total_size;
        self.complete = stream.complete;
        Ok(())
    }

    fn contains(&self, offset: u64) -> bool {
        let end = self
            .window_offset
            .saturating_add(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX));
        offset >= self.window_offset && (offset < end || (self.bytes.is_empty() && offset == end))
    }

    fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>, ProviderError> {
        let requested = usize::try_from(end.saturating_sub(start)).unwrap_or(usize::MAX);
        if requested > MAX_COPY_BYTES {
            return Err(ProviderError::Unsupported(format!(
                "viewer copy is limited to {MAX_COPY_BYTES} bytes"
            )));
        }
        let mut bytes = Vec::with_capacity(requested);
        let mut offset = start;
        while offset < end {
            if self.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let remaining = usize::try_from(end.saturating_sub(offset)).unwrap_or(usize::MAX);
            let stream = block_on_provider(self.provider.open(
                &self.resource,
                OpenRequest {
                    offset,
                    length: remaining.min(DEFAULT_WINDOW_SIZE),
                    cancellation: self.cancellation.clone(),
                },
            ))?;
            if stream.bytes.is_empty() {
                break;
            }
            offset = offset.saturating_add(u64::try_from(stream.bytes.len()).unwrap_or(u64::MAX));
            bytes.extend_from_slice(&stream.bytes);
        }
        bytes.truncate(requested);
        Ok(bytes)
    }

    fn line_bytes(&self, offset: u64, newline: &[u8]) -> Result<Vec<u8>, ProviderError> {
        let end = self.total_size.unwrap_or_else(|| {
            offset.saturating_add(u64::try_from(MAX_COPY_BYTES).unwrap_or(u64::MAX))
        });
        let bytes = self.read_range(
            offset,
            end.min(offset.saturating_add(u64::try_from(MAX_COPY_BYTES).unwrap_or(u64::MAX))),
        )?;
        let length = find_bytes(&bytes, newline).unwrap_or(bytes.len());
        Ok(bytes[..length].to_vec())
    }

    fn ensure(&mut self, offset: u64) -> Result<(), ProviderError> {
        if self.contains(offset) {
            Ok(())
        } else {
            self.load(offset)
        }
    }

    fn relative(&self, offset: u64) -> usize {
        usize::try_from(offset.saturating_sub(self.window_offset)).unwrap_or(usize::MAX)
    }

    fn next_line(&mut self, offset: u64, newline: &[u8]) -> Result<u64, ProviderError> {
        self.ensure(offset)?;
        let relative = self.relative(offset).min(self.bytes.len());
        if let Some(index) = find_bytes(&self.bytes[relative..], newline) {
            return Ok(self.window_offset.saturating_add(
                u64::try_from(relative + index + newline.len()).unwrap_or(u64::MAX),
            ));
        }
        if self.complete {
            return Ok(offset);
        }
        let next = self
            .window_offset
            .saturating_add(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX));
        self.load(next)?;
        Ok(next)
    }

    fn previous_line(&mut self, offset: u64, newline: &[u8]) -> Result<u64, ProviderError> {
        if offset == 0 {
            return Ok(0);
        }
        let mut search_end = offset.saturating_sub(1);
        loop {
            self.ensure(search_end)?;
            let relative = self.relative(search_end).min(self.bytes.len());
            let slice = &self.bytes[..relative];
            if let Some(index) = rfind_bytes(slice, newline) {
                let before = &slice[..index];
                let start =
                    rfind_bytes(before, newline).map_or(0, |position| position + newline.len());
                return Ok(self
                    .window_offset
                    .saturating_add(u64::try_from(start).unwrap_or(0)));
            }
            if self.window_offset == 0 {
                return Ok(0);
            }
            search_end = self.window_offset.saturating_sub(1);
            self.load(
                self.window_offset
                    .saturating_sub(u64::try_from(self.window_size).unwrap_or(u64::MAX)),
            )?;
        }
    }

    fn search_forward(&mut self, start: u64, pattern: &[u8]) -> Result<Option<u64>, ProviderError> {
        if pattern.is_empty() {
            return Ok(Some(start));
        }
        let mut offset = start;
        loop {
            self.load(offset)?;
            if let Some(index) = find_bytes(&self.bytes, pattern) {
                return Ok(Some(
                    self.window_offset
                        .saturating_add(u64::try_from(index).unwrap_or(u64::MAX)),
                ));
            }
            if self.complete || self.bytes.is_empty() {
                return Ok(None);
            }
            let overlap = pattern.len().saturating_sub(1).min(self.bytes.len());
            offset = self
                .window_offset
                .saturating_add(u64::try_from(self.bytes.len() - overlap).unwrap_or(u64::MAX));
        }
    }

    fn search_backward(
        &mut self,
        start: u64,
        pattern: &[u8],
    ) -> Result<Option<u64>, ProviderError> {
        if pattern.is_empty() {
            return Ok(Some(start));
        }
        let mut end = start.min(self.total_size.unwrap_or(start));
        loop {
            let window_start =
                end.saturating_sub(u64::try_from(self.window_size).unwrap_or(u64::MAX));
            self.load(window_start)?;
            let visible_end = usize::try_from(end.saturating_sub(self.window_offset))
                .unwrap_or(usize::MAX)
                .min(self.bytes.len());
            if let Some(index) = rfind_bytes(&self.bytes[..visible_end], pattern) {
                return Ok(Some(
                    self.window_offset
                        .saturating_add(u64::try_from(index).unwrap_or(u64::MAX)),
                ));
            }
            if window_start == 0 {
                return Ok(None);
            }
            end = window_start
                .saturating_add(u64::try_from(pattern.len().saturating_sub(1)).unwrap_or(u64::MAX));
        }
    }

    fn line_offset(&mut self, line: u64, newline: &[u8]) -> Result<Option<u64>, ProviderError> {
        if line <= 1 {
            return Ok(Some(0));
        }
        let mut current_line = 1_u64;
        let mut offset = 0_u64;
        loop {
            self.load(offset)?;
            let mut relative = 0;
            while let Some(index) = find_bytes(&self.bytes[relative..], newline) {
                current_line = current_line.saturating_add(1);
                relative += index + newline.len();
                if current_line == line {
                    return Ok(Some(
                        self.window_offset
                            .saturating_add(u64::try_from(relative).unwrap_or(u64::MAX)),
                    ));
                }
            }
            if self.complete || self.bytes.is_empty() {
                return Ok(None);
            }
            offset = self
                .window_offset
                .saturating_add(u64::try_from(self.bytes.len()).unwrap_or(u64::MAX));
        }
    }
}

pub struct ViewerSurface {
    id: SurfaceId,
    title: String,
    resource: Option<ResourceRef>,
    document: Option<ViewerDocument>,
    static_bytes: Vec<u8>,
    offset: u64,
    column: usize,
    wrap: bool,
    hex: bool,
    encoding: ViewerEncoding,
    search: Option<String>,
    last_search_match: Option<u64>,
    prompt: ViewerPrompt,
    prompt_input: String,
    bookmarks: BTreeMap<u8, u64>,
    navigation_history: Vec<u64>,
    navigation_index: usize,
    selection_anchor: Option<ViewerPoint>,
    selection_mode: ViewerSelectionMode,
    clipboard: Option<Arc<dyn Clipboard>>,
    error: Option<String>,
}

impl ViewerSurface {
    #[must_use]
    pub fn with_settings(mut self, settings: ViewerSettings) -> Self {
        self.wrap = settings.wrap;
        let bytes = self
            .document
            .as_ref()
            .map_or(self.static_bytes.as_slice(), |document| {
                document.bytes.as_slice()
            });
        let encoding = settings.encoding.resolved(bytes);
        self.hex = settings.hex
            || settings.detect_binary
                && !matches!(encoding, ViewerEncoding::Utf16Le | ViewerEncoding::Utf16Be)
                && ViewerSettings::is_binary(bytes);
        self.encoding = encoding;
        self
    }

    pub fn text(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        content: impl AsRef<str>,
    ) -> Self {
        Self::bytes(id, title, content.as_ref().as_bytes().to_vec())
    }

    pub fn bytes(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        content: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            resource: None,
            document: None,
            static_bytes: content.into(),
            offset: 0,
            column: 0,
            wrap: false,
            hex: false,
            encoding: ViewerEncoding::default(),
            search: None,
            last_search_match: None,
            prompt: ViewerPrompt::None,
            prompt_input: String::new(),
            bookmarks: BTreeMap::new(),
            navigation_history: vec![0],
            navigation_index: 0,
            selection_anchor: None,
            selection_mode: ViewerSelectionMode::Stream,
            clipboard: None,
            error: None,
        }
    }

    /// Opens a provider resource using a bounded initial window.
    ///
    /// # Errors
    ///
    /// Returns a provider error when the first window cannot be read.
    pub fn stream(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderError> {
        let document = ViewerDocument::open(provider, resource.clone(), cancellation)?;
        Ok(Self {
            id: id.into(),
            title: title.into(),
            resource: Some(resource),
            document: Some(document),
            static_bytes: Vec::new(),
            offset: 0,
            column: 0,
            wrap: false,
            hex: false,
            encoding: ViewerEncoding::default(),
            search: None,
            last_search_match: None,
            prompt: ViewerPrompt::None,
            prompt_input: String::new(),
            bookmarks: BTreeMap::new(),
            navigation_history: vec![0],
            navigation_index: 0,
            selection_anchor: None,
            selection_mode: ViewerSelectionMode::Stream,
            clipboard: None,
            error: None,
        })
    }

    /// Constructs a streamed viewer from a window loaded by a background runtime task.
    pub fn from_stream(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        cancellation: CancellationToken,
        stream: ResourceStream,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            resource: Some(resource.clone()),
            document: Some(ViewerDocument::from_stream(
                provider,
                resource,
                cancellation,
                stream,
            )),
            static_bytes: Vec::new(),
            offset: 0,
            column: 0,
            wrap: false,
            hex: false,
            encoding: ViewerEncoding::default(),
            search: None,
            last_search_match: None,
            prompt: ViewerPrompt::None,
            prompt_input: String::new(),
            bookmarks: BTreeMap::new(),
            navigation_history: vec![0],
            navigation_index: 0,
            selection_anchor: None,
            selection_mode: ViewerSelectionMode::Stream,
            clipboard: None,
            error: None,
        }
    }

    #[must_use]
    pub fn with_resource(mut self, resource: ResourceRef) -> Self {
        self.resource = Some(resource);
        self
    }

    #[must_use]
    pub fn with_clipboard(mut self, clipboard: Arc<dyn Clipboard>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }

    pub fn restore_state(&mut self, state: &ViewerStateEntry) {
        if self.resource.as_ref().is_none_or(|resource| {
            resource.provider != state.provider || resource.location != state.location
        }) {
            return;
        }
        self.bookmarks.clone_from(&state.bookmarks);
        self.navigation_history = if state.navigation_history.is_empty() {
            vec![state.offset]
        } else {
            state.navigation_history.clone()
        };
        self.navigation_index = state
            .navigation_index
            .min(self.navigation_history.len().saturating_sub(1));
        if let Some(encoding) = state.encoding.as_deref().and_then(ViewerEncoding::parse) {
            self.encoding = encoding;
        }
        if let Some(wrap) = state.wrap {
            self.wrap = wrap;
        }
        if let Some(hex) = state.hex {
            self.hex = hex;
        }
        self.offset = state.offset;
        self.column = 0;
        self.clamp_offset();
        if let Err(error) = self.ensure_offset() {
            self.error = Some(error.to_string());
        }
    }

    pub fn state_entry(&self) -> Option<ViewerStateEntry> {
        let resource = self.resource.as_ref()?;
        Some(ViewerStateEntry {
            provider: resource.provider.clone(),
            location: resource.location.clone(),
            offset: self.offset,
            bookmarks: self.bookmarks.clone(),
            navigation_history: self.navigation_history.clone(),
            navigation_index: self.navigation_index,
            encoding: Some(self.encoding.label().to_owned()),
            wrap: Some(self.wrap),
            hex: Some(self.hex),
        })
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    #[allow(clippy::naive_bytecount)]
    pub fn scroll(&self) -> usize {
        let offset = usize::try_from(self.offset).unwrap_or(usize::MAX);
        if let Some(document) = &self.document {
            let relative = offset
                .saturating_sub(usize::try_from(document.window_offset).unwrap_or(usize::MAX))
                .min(document.bytes.len());
            document.bytes[..relative]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
        } else {
            self.static_bytes[..offset.min(self.static_bytes.len())]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
        }
    }

    pub fn is_wrapped(&self) -> bool {
        self.wrap
    }

    pub fn is_hex(&self) -> bool {
        self.hex
    }

    pub fn encoding(&self) -> ViewerEncoding {
        self.encoding
    }

    pub fn buffered_bytes(&self) -> usize {
        self.document
            .as_ref()
            .map_or(self.static_bytes.len(), |document| document.bytes.len())
    }

    pub fn total_size(&self) -> Option<u64> {
        self.document
            .as_ref()
            .and_then(|document| document.total_size)
            .or_else(|| u64::try_from(self.static_bytes.len()).ok())
    }

    pub fn set_search(&mut self, search: Option<String>) {
        self.search = search;
        self.last_search_match = None;
    }

    pub fn set_bookmark(&mut self, slot: u8) {
        self.bookmarks.insert(slot, self.offset);
    }

    pub fn jump_to_bookmark(&mut self, slot: u8) -> bool {
        self.bookmarks.get(&slot).copied().is_some_and(|offset| {
            self.navigate_to(offset);
            true
        })
    }

    pub fn cancel(&mut self) {
        if let Some(document) = &self.document {
            document.cancellation.cancel();
        }
    }

    pub fn search_next(&mut self) -> bool {
        let Some(query) = self.search.clone() else {
            return false;
        };
        let pattern = match self.search_pattern(&query) {
            Ok(pattern) => pattern,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        let start = if self.last_search_match == Some(self.offset) {
            self.offset.saturating_add(1)
        } else {
            self.offset
        };
        let result = if let Some(document) = &mut self.document {
            document.search_forward(start, &pattern)
        } else {
            Ok(find_bytes(
                self.static_bytes
                    .get(usize::try_from(start).unwrap_or(usize::MAX)..)
                    .unwrap_or_default(),
                &pattern,
            )
            .and_then(|index| start.checked_add(u64::try_from(index).ok()?)))
        };
        match result {
            Ok(Some(offset)) => {
                self.navigate_to(offset);
                self.last_search_match = Some(offset);
                self.error = None;
                true
            }
            Ok(None) => {
                self.error = Some(format!("not found: {query}"));
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    pub fn search_previous(&mut self) -> bool {
        let Some(query) = self.search.clone() else {
            return false;
        };
        let pattern = match self.search_pattern(&query) {
            Ok(pattern) => pattern,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        let start = if self.last_search_match == Some(self.offset) {
            self.offset
        } else {
            self.offset
                .saturating_add(u64::try_from(pattern.len()).unwrap_or(u64::MAX))
        };
        let result = if let Some(document) = &mut self.document {
            document.search_backward(start, &pattern)
        } else {
            let end = usize::try_from(start)
                .unwrap_or(usize::MAX)
                .min(self.static_bytes.len());
            Ok(rfind_bytes(&self.static_bytes[..end], &pattern)
                .and_then(|index| index.try_into().ok()))
        };
        match result {
            Ok(Some(offset)) => {
                self.navigate_to(offset);
                self.last_search_match = Some(offset);
                self.error = None;
                true
            }
            Ok(None) => {
                self.error = Some(format!("not found: {query}"));
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn search_pattern(&self, query: &str) -> Result<Vec<u8>, String> {
        if self.hex {
            parse_hex_query(query)
        } else {
            Ok(self.encoding.encode(query))
        }
    }

    fn navigate_to(&mut self, offset: u64) {
        self.offset = offset;
        self.column = 0;
        self.clamp_offset();
        if let Err(error) = self.ensure_offset() {
            self.error = Some(error.to_string());
            return;
        }
        if self.navigation_history.get(self.navigation_index).copied() == Some(self.offset) {
            return;
        }
        self.navigation_history
            .truncate(self.navigation_index.saturating_add(1));
        self.navigation_history.push(self.offset);
        self.navigation_index = self.navigation_history.len().saturating_sub(1);
    }

    fn history_back(&mut self) {
        if self.navigation_index == 0 {
            return;
        }
        self.navigation_index -= 1;
        self.offset = self.navigation_history[self.navigation_index];
        self.column = 0;
        if let Err(error) = self.ensure_offset() {
            self.error = Some(error.to_string());
        }
    }

    fn history_forward(&mut self) {
        if self.navigation_index.saturating_add(1) >= self.navigation_history.len() {
            return;
        }
        self.navigation_index += 1;
        self.offset = self.navigation_history[self.navigation_index];
        self.column = 0;
        if let Err(error) = self.ensure_offset() {
            self.error = Some(error.to_string());
        }
    }

    fn confirm_prompt(&mut self) {
        match self.prompt {
            ViewerPrompt::None => {}
            ViewerPrompt::Search => {
                self.search = Some(self.prompt_input.clone());
                self.last_search_match = None;
                self.prompt = ViewerPrompt::None;
                self.search_next();
            }
            ViewerPrompt::GoTo => {
                let input = self.prompt_input.clone();
                self.prompt = ViewerPrompt::None;
                match self.resolve_position(&input) {
                    Ok(offset) => {
                        self.navigate_to(offset);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }

    fn resolve_position(&mut self, input: &str) -> Result<u64, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("go-to position is empty".to_owned());
        }
        if let Some(percent) = input.strip_suffix('%') {
            let percent = percent
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("invalid percentage: {input}"))?;
            if percent > 100 {
                return Err("percentage must be between 0 and 100".to_owned());
            }
            let total = self
                .total_size()
                .ok_or_else(|| "resource size is unknown".to_owned())?;
            return Ok(total.saturating_mul(percent) / 100);
        }
        if let Some(line) = input
            .strip_prefix('L')
            .or_else(|| input.strip_prefix('l'))
            .or_else(|| input.strip_prefix("line:"))
        {
            let line = line
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("invalid line: {input}"))?;
            if line == 0 {
                return Err("line numbers start at 1".to_owned());
            }
            let offset = if let Some(document) = &mut self.document {
                document
                    .line_offset(line, self.encoding.newline_bytes())
                    .map_err(|error| error.to_string())?
            } else {
                self.encoding.line_offset(&self.static_bytes, line)
            };
            return offset.ok_or_else(|| format!("line {line} is beyond the resource"));
        }
        if let Some(relative) = input.strip_prefix('+') {
            let distance = parse_offset(relative)?;
            return Ok(self.offset.saturating_add(distance));
        }
        if let Some(relative) = input.strip_prefix('-') {
            let distance = parse_offset(relative)?;
            return Ok(self.offset.saturating_sub(distance));
        }
        parse_offset(input)
    }

    fn move_by(&mut self, rows: isize) {
        let result = if self.hex {
            let rows = i64::try_from(rows).unwrap_or(if rows.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            });
            let distance = rows.saturating_mul(16);
            self.offset = self.offset.saturating_add_signed(distance);
            self.ensure_offset()
        } else if rows >= 0 {
            (0..rows.unsigned_abs()).try_for_each(|_| self.move_next_line())
        } else {
            (0..rows.unsigned_abs()).try_for_each(|_| self.move_previous_line())
        };
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
        self.clamp_offset();
    }

    fn begin_selection(&mut self, mode: ViewerSelectionMode) {
        if self.selection_anchor.is_none() || self.selection_mode != mode {
            self.selection_anchor = Some(ViewerPoint {
                offset: self.offset,
                column: self.column,
            });
            self.selection_mode = mode;
        }
    }

    fn handle_selection_command(&mut self, command: &str) -> bool {
        let (mode, vertical, amount) = match command {
            "near.viewer.select-up" => (ViewerSelectionMode::Stream, true, -1),
            "near.viewer.select-down" => (ViewerSelectionMode::Stream, true, 1),
            "near.viewer.select-left" => (ViewerSelectionMode::Stream, false, -1),
            "near.viewer.select-right" => (ViewerSelectionMode::Stream, false, 1),
            "near.viewer.column-select-up" => (ViewerSelectionMode::Column, true, -1),
            "near.viewer.column-select-down" => (ViewerSelectionMode::Column, true, 1),
            "near.viewer.column-select-left" => (ViewerSelectionMode::Column, false, -1),
            "near.viewer.column-select-right" => (ViewerSelectionMode::Column, false, 1),
            _ => return false,
        };
        self.begin_selection(mode);
        if vertical {
            self.move_by(amount);
        } else {
            self.move_horizontal(amount);
        }
        true
    }

    fn move_horizontal(&mut self, columns: isize) {
        let line_length = self
            .current_line_bytes()
            .map_or(0, |line| self.encoding.decode(&line).chars().count());
        self.column = self.column.saturating_add_signed(columns).min(line_length);
    }

    fn current_line_bytes(&self) -> Result<Vec<u8>, ProviderError> {
        if let Some(document) = &self.document {
            document.line_bytes(self.offset, self.encoding.newline_bytes())
        } else {
            Ok(self.encoding.line_bytes(&self.static_bytes, self.offset))
        }
    }

    fn point_byte_offset(&self, point: ViewerPoint) -> Result<u64, ProviderError> {
        let line = if let Some(document) = &self.document {
            document.line_bytes(point.offset, self.encoding.newline_bytes())?
        } else {
            self.encoding.line_bytes(&self.static_bytes, point.offset)
        };
        let byte_column = self
            .encoding
            .byte_column(&line, point.column)
            .min(line.len());
        Ok(point
            .offset
            .saturating_add(u64::try_from(byte_column).unwrap_or(u64::MAX)))
    }

    fn bytes_between(&self, start: u64, end: u64) -> Result<Vec<u8>, ProviderError> {
        let length = usize::try_from(end.saturating_sub(start)).unwrap_or(usize::MAX);
        if length > MAX_COPY_BYTES {
            return Err(ProviderError::Unsupported(format!(
                "viewer copy is limited to {MAX_COPY_BYTES} bytes"
            )));
        }
        if let Some(document) = &self.document {
            document.read_range(start, end)
        } else {
            let start = usize::try_from(start)
                .unwrap_or(usize::MAX)
                .min(self.static_bytes.len());
            let end = usize::try_from(end)
                .unwrap_or(usize::MAX)
                .min(self.static_bytes.len());
            Ok(self.static_bytes[start.min(end)..end].to_vec())
        }
    }

    fn stream_selection_text(
        &self,
        anchor: ViewerPoint,
        current: ViewerPoint,
    ) -> Result<String, ProviderError> {
        let anchor = self.point_byte_offset(anchor)?;
        let current = self.point_byte_offset(current)?;
        let (start, end) = if anchor <= current {
            (anchor, current)
        } else {
            (current, anchor)
        };
        Ok(self.encoding.decode(&self.bytes_between(start, end)?))
    }

    fn column_selection_text(
        &mut self,
        anchor: ViewerPoint,
        current: ViewerPoint,
    ) -> Result<String, ProviderError> {
        let (start_offset, end_offset) = if anchor.offset <= current.offset {
            (anchor.offset, current.offset)
        } else {
            (current.offset, anchor.offset)
        };
        let start_column = anchor.column.min(current.column);
        let end_column = anchor.column.max(current.column);
        let mut rows = Vec::new();
        let mut offset = start_offset;
        loop {
            let line = if let Some(document) = &self.document {
                document.line_bytes(offset, self.encoding.newline_bytes())?
            } else {
                self.encoding.line_bytes(&self.static_bytes, offset)
            };
            rows.push(
                self.encoding
                    .decode(&line)
                    .chars()
                    .skip(start_column)
                    .take(end_column.saturating_sub(start_column))
                    .collect::<String>(),
            );
            if offset >= end_offset {
                break;
            }
            let next = if let Some(document) = &mut self.document {
                document.next_line(offset, self.encoding.newline_bytes())?
            } else {
                self.encoding.next_line(&self.static_bytes, offset)
            };
            if next <= offset {
                break;
            }
            offset = next;
        }
        Ok(rows.join("\n"))
    }

    fn copy_selection(&mut self) {
        let Some(anchor) = self.selection_anchor else {
            self.error = Some("no viewer selection".to_owned());
            return;
        };
        let current = ViewerPoint {
            offset: self.offset,
            column: self.column,
        };
        let selected = match self.selection_mode {
            ViewerSelectionMode::Stream => self.stream_selection_text(anchor, current),
            ViewerSelectionMode::Column => self.column_selection_text(anchor, current),
        };
        let selected = match selected {
            Ok(selected) if !selected.is_empty() => selected,
            Ok(_) => {
                self.error = Some("viewer selection is empty".to_owned());
                return;
            }
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let Some(clipboard) = &self.clipboard else {
            self.error = Some("platform clipboard is unavailable".to_owned());
            return;
        };
        match clipboard.set_text(&selected) {
            Ok(()) => {
                self.error = Some(format!("copied {} characters", selected.chars().count()));
            }
            Err(error) => self.error = Some(format!("clipboard failed: {error}")),
        }
    }

    fn current_line_selection(&self) -> Option<(usize, String)> {
        if self.hex || self.wrap {
            return None;
        }
        let anchor = self.selection_anchor?;
        let current = ViewerPoint {
            offset: self.offset,
            column: self.column,
        };
        let line = self.current_line_bytes().ok()?;
        let decoded = self.encoding.decode(&line);
        let line_length = decoded.chars().count();
        let (start, end) = match self.selection_mode {
            ViewerSelectionMode::Column => (
                anchor.column.min(current.column),
                anchor.column.max(current.column),
            ),
            ViewerSelectionMode::Stream if anchor.offset == current.offset => (
                anchor.column.min(current.column),
                anchor.column.max(current.column),
            ),
            ViewerSelectionMode::Stream if anchor.offset < current.offset => (0, current.column),
            ViewerSelectionMode::Stream => (current.column, line_length),
        };
        let start = start.min(line_length);
        let selected = decoded
            .chars()
            .skip(start)
            .take(end.min(line_length).saturating_sub(start))
            .collect::<String>();
        (!selected.is_empty()).then_some((start, selected))
    }

    fn move_next_line(&mut self) -> Result<(), ProviderError> {
        self.offset = if let Some(document) = &mut self.document {
            document.next_line(self.offset, self.encoding.newline_bytes())?
        } else {
            self.encoding.next_line(&self.static_bytes, self.offset)
        };
        Ok(())
    }

    fn move_previous_line(&mut self) -> Result<(), ProviderError> {
        self.offset = if let Some(document) = &mut self.document {
            document.previous_line(self.offset, self.encoding.newline_bytes())?
        } else {
            self.encoding.previous_line(&self.static_bytes, self.offset)
        };
        Ok(())
    }

    fn ensure_offset(&mut self) -> Result<(), ProviderError> {
        if let Some(document) = &mut self.document {
            document.ensure(self.offset)?;
        }
        Ok(())
    }

    fn clamp_offset(&mut self) {
        if let Some(total) = self.total_size() {
            self.offset = self.offset.min(total.saturating_sub(u64::from(total > 0)));
        }
    }

    fn go_end(&mut self) {
        self.offset = self.total_size().unwrap_or(0).saturating_sub(1);
        if let Err(error) = self.ensure_offset() {
            self.error = Some(error.to_string());
        }
    }

    fn visible_bytes(&self) -> (&[u8], u64) {
        if let Some(document) = &self.document {
            let start = document.relative(self.offset).min(document.bytes.len());
            (&document.bytes[start..], self.offset)
        } else {
            let start = usize::try_from(self.offset)
                .unwrap_or(usize::MAX)
                .min(self.static_bytes.len());
            (&self.static_bytes[start..], self.offset)
        }
    }

    fn visible_text(&self, width: usize, height: usize) -> String {
        let (bytes, offset) = self.visible_bytes();
        if self.hex {
            return hex_rows(bytes, offset, height);
        }
        let decoded = self.encoding.decode(bytes);
        if self.wrap && width > 0 {
            wrap_lines(&decoded, width, height)
        } else {
            decoded.lines().take(height).collect::<Vec<_>>().join("\n")
        }
    }
}

impl Drop for ViewerSurface {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Surface for ViewerSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.viewer")]
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState {
            current: self.resource.clone(),
            selected: Vec::new(),
            location: self
                .resource
                .as_ref()
                .map(|resource| resource.location.clone()),
        }
    }

    fn presentation(&self) -> SurfacePresentation {
        SurfacePresentation::FullScreen
    }

    fn viewer_state(&self) -> Option<ViewerStateEntry> {
        self.state_entry()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        if self.prompt != ViewerPrompt::None {
            match event {
                SurfaceEvent::Text(text) => {
                    self.prompt_input.push_str(text);
                    return UpdateResult::handled();
                }
                SurfaceEvent::Backspace => {
                    self.prompt_input.pop();
                    return UpdateResult::handled();
                }
                SurfaceEvent::Command(invocation)
                    if invocation.id.as_str() == "near.overlay.cancel" =>
                {
                    self.prompt = ViewerPrompt::None;
                    self.prompt_input.clear();
                    return UpdateResult::handled();
                }
                _ => {}
            }
        }
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        if self.handle_selection_command(invocation.id.as_str()) {
            return UpdateResult::handled();
        }
        match invocation.id.as_str() {
            "near.viewer.up" => self.move_by(-1),
            "near.viewer.down" => self.move_by(1),
            "near.viewer.left" => self.move_horizontal(-1),
            "near.viewer.right" => self.move_horizontal(1),
            "near.viewer.page-up" => self.move_by(-DEFAULT_PAGE_ROWS),
            "near.viewer.page-down" => self.move_by(DEFAULT_PAGE_ROWS),
            "near.viewer.selection-clear" => {
                self.selection_anchor = None;
                self.error = Some("selection cleared".to_owned());
            }
            "near.viewer.copy" => self.copy_selection(),
            "near.viewer.home" => self.navigate_to(0),
            "near.viewer.end" => {
                self.go_end();
                self.navigate_to(self.offset);
            }
            "near.viewer.toggle-wrap" => self.wrap = !self.wrap,
            "near.viewer.toggle-hex" => self.hex = !self.hex,
            "near.viewer.cycle-encoding" => {
                self.encoding = match self.encoding {
                    ViewerEncoding::Auto => ViewerEncoding::Utf8Lossy,
                    ViewerEncoding::Utf8Lossy => ViewerEncoding::Latin1,
                    ViewerEncoding::Latin1 => ViewerEncoding::Utf16Le,
                    ViewerEncoding::Utf16Le => ViewerEncoding::Utf16Be,
                    ViewerEncoding::Utf16Be => ViewerEncoding::Auto,
                };
            }
            "near.viewer.search-next" => {
                self.search_next();
            }
            "near.viewer.search-previous" => {
                self.search_previous();
            }
            "near.viewer.search-start" => {
                self.prompt = ViewerPrompt::Search;
                self.prompt_input = self.search.clone().unwrap_or_default();
            }
            "near.viewer.search-confirm" | "near.viewer.goto-confirm" => self.confirm_prompt(),
            "near.viewer.goto-start" => {
                self.prompt = ViewerPrompt::GoTo;
                self.prompt_input.clear();
            }
            "near.viewer.history-back" => {
                self.history_back();
            }
            "near.viewer.history-forward" => {
                self.history_forward();
            }
            "near.viewer.bookmark-set" => {
                if let Some(slot) = invocation
                    .arguments
                    .get("slot")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|slot| u8::try_from(slot).ok())
                {
                    self.set_bookmark(slot);
                }
            }
            "near.viewer.bookmark-jump" => {
                if let Some(slot) = invocation
                    .arguments
                    .get("slot")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|slot| u8::try_from(slot).ok())
                {
                    self.jump_to_bookmark(slot);
                }
            }
            _ => return UpdateResult::ignored(),
        }
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "viewer.background");
        scene.border(area, Some(format!(" {} ", self.title)), "viewer.border");
        let inner = area.inset(1);
        let body_height = inner.height.saturating_sub(1);
        scene.text(
            SceneRect::new(inner.x, inner.y, inner.width, body_height),
            self.visible_text(usize::from(inner.width), usize::from(body_height)),
            "viewer.text",
        );
        if let Some((column, selected)) = self.current_line_selection()
            && let Ok(column) = u16::try_from(column)
            && column < inner.width
        {
            scene.text(
                SceneRect::new(
                    inner.x.saturating_add(column),
                    inner.y,
                    u16::try_from(selected.chars().count())
                        .unwrap_or(u16::MAX)
                        .min(inner.width.saturating_sub(column)),
                    1,
                ),
                selected,
                "viewer.selected",
            );
        }
        let search = self.search.as_deref().unwrap_or("-");
        let prompt = match self.prompt {
            ViewerPrompt::None => format!("search:{search}"),
            ViewerPrompt::Search => format!("search>{}", self.prompt_input),
            ViewerPrompt::GoTo => format!("goto>{}", self.prompt_input),
        };
        let error = self.error.as_deref().unwrap_or("");
        let selection = self.selection_anchor.map_or_else(String::new, |_| {
            format!(
                " selection:{}@{}:{}",
                match self.selection_mode {
                    ViewerSelectionMode::Stream => "stream",
                    ViewerSelectionMode::Column => "column",
                },
                self.offset,
                self.column
            )
        });
        scene.aligned_text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            format!(
                "offset {}:{} / {}  wrap:{}  hex:{}  encoding:{}  history:{}/{}{}  {prompt} {error}",
                self.offset,
                self.column,
                self.total_size()
                    .map_or_else(|| "?".to_owned(), |size| size.to_string()),
                self.wrap,
                self.hex,
                self.encoding.label(),
                self.navigation_index.saturating_add(1),
                self.navigation_history.len(),
                selection,
            ),
            "viewer.status",
            TextAlignment::Right,
        );
        scene
    }
}

fn block_on_provider<T>(mut future: ProviderFuture<'_, T>) -> Result<T, ProviderError> {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match Future::poll(future.as_mut(), &mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn parse_hex_query(query: &str) -> Result<Vec<u8>, String> {
    let compact = query
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    if compact.is_empty() {
        return Ok(Vec::new());
    }
    if !compact
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err("hex search accepts only hexadecimal byte pairs".to_owned());
    }
    if compact.len() % 2 != 0 {
        return Err("hex search requires complete byte pairs".to_owned());
    }
    (0..compact.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&compact[index..index + 2], 16)
                .map_err(|_| format!("invalid hex byte: {}", &compact[index..index + 2]))
        })
        .collect()
}

fn parse_offset(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|_| format!("invalid offset: {input}"))
    } else {
        input
            .parse::<u64>()
            .map_err(|_| format!("invalid offset: {input}"))
    }
}

fn hex_rows(bytes: &[u8], offset: u64, height: usize) -> String {
    let mut output = String::new();
    for (row, chunk) in bytes.chunks(16).take(height).enumerate() {
        if row > 0 {
            output.push('\n');
        }
        let row_offset = offset.saturating_add(u64::try_from(row * 16).unwrap_or(u64::MAX));
        let _ = write!(output, "{row_offset:08x}  ");
        for index in 0..16 {
            if let Some(byte) = chunk.get(index) {
                let _ = write!(output, "{byte:02x} ");
            } else {
                output.push_str("   ");
            }
        }
        output.push(' ');
        output.extend(chunk.iter().map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        }));
    }
    output
}

fn wrap_lines(content: &str, width: usize, height: usize) -> String {
    let mut output = Vec::new();
    for line in content.lines() {
        if line.is_empty() {
            output.push(String::new());
        } else {
            let characters: Vec<_> = line.chars().collect();
            output.extend(
                characters
                    .chunks(width)
                    .map(|chunk| chunk.iter().collect::<String>()),
            );
        }
        if output.len() >= height {
            break;
        }
    }
    output
        .into_iter()
        .take(height)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use near_core::{
        CapabilitySet, ListPage, ListRequest, Location, ProviderFuture, ProviderId,
        ResourceMetadata, ResourceStream,
    };

    use super::*;

    #[test]
    fn viewer_settings_are_versioned_and_apply_initial_display_defaults() {
        let settings = ViewerSettings::from_toml(
            "schema = 1\nwrap = true\nhex = true\nencoding = 'latin1'\nopen_policy = 'external'\nremember_per_resource = false\n",
        )
        .unwrap();
        let viewer = ViewerSurface::text("viewer", "Settings", "content").with_settings(settings);
        assert!(viewer.wrap);
        assert!(viewer.hex);
        assert_eq!(viewer.encoding, ViewerEncoding::Latin1);
        assert_eq!(settings.open_policy, crate::ResourceOpenPolicy::External);
        assert!(!settings.remember_per_resource);
        assert!(ViewerSettings::from_toml("schema = 2\n").is_err());
        let binary = ViewerSurface::bytes("viewer", "Binary", b"text\0binary".to_vec())
            .with_settings(ViewerSettings::default());
        assert!(binary.is_hex());
    }

    struct MemoryProvider {
        bytes: Arc<Vec<u8>>,
    }

    #[derive(Default)]
    struct RecordingClipboard {
        text: Mutex<String>,
    }

    impl Clipboard for RecordingClipboard {
        fn set_text(&self, text: &str) -> Result<(), String> {
            text.clone_into(&mut self.text.lock().unwrap());
            Ok(())
        }
    }

    impl ResourceProvider for MemoryProvider {
        fn id(&self) -> ProviderId {
            "near.viewer-memory".into()
        }

        fn schemes(&self) -> &[&str] {
            &["memory"]
        }

        fn list<'a>(
            &'a self,
            _location: &'a Location,
            _request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            Box::pin(async { Err(ProviderError::Unsupported("list".to_owned())) })
        }

        fn stat<'a>(&'a self, _resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            Box::pin(async { Ok(ResourceMetadata::default()) })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async move {
                if request.cancellation.is_cancelled() {
                    return Err(ProviderError::Cancelled);
                }
                let start = usize::try_from(request.offset)
                    .unwrap_or(usize::MAX)
                    .min(self.bytes.len());
                let end = start.saturating_add(request.length).min(self.bytes.len());
                Ok(ResourceStream {
                    offset: request.offset,
                    bytes: self.bytes[start..end].to_vec(),
                    total_size: u64::try_from(self.bytes.len()).ok(),
                    complete: end == self.bytes.len(),
                })
            })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }
    }

    fn resource() -> ResourceRef {
        ResourceRef {
            provider: "near.viewer-memory".into(),
            location: Location::new("memory:///large"),
        }
    }

    fn invocation(id: &str) -> near_core::CommandInvocation {
        near_core::CommandInvocation {
            id: id.into(),
            arguments: BTreeMap::new(),
        }
    }

    #[test]
    fn large_stream_keeps_a_bounded_window_shared_by_text_and_hex() {
        let mut bytes = vec![b'a'; DEFAULT_WINDOW_SIZE * 8];
        bytes[DEFAULT_WINDOW_SIZE * 4..DEFAULT_WINDOW_SIZE * 4 + 7].copy_from_slice(b"needle\n");
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(bytes),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "Large",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        assert!(viewer.buffered_bytes() <= DEFAULT_WINDOW_SIZE);
        viewer.set_search(Some("needle".to_owned()));
        assert!(viewer.search_next());
        assert!(viewer.offset() >= u64::try_from(DEFAULT_WINDOW_SIZE * 4).unwrap());
        assert!(viewer.buffered_bytes() <= DEFAULT_WINDOW_SIZE);
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.toggle-hex")),
            &mut UpdateContext {
                action: &near_core::ActionContext::default(),
            },
        );
        assert!(viewer.is_hex());
        assert!(viewer.buffered_bytes() <= DEFAULT_WINDOW_SIZE);
    }

    #[test]
    fn encodings_bookmarks_and_offsets_are_preserved() {
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(b"first\nsecond\nthird\n".to_vec()),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "Text",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap()
        .with_settings(ViewerSettings::default());
        viewer.move_by(1);
        viewer.set_bookmark(3);
        viewer.move_by(1);
        assert!(viewer.jump_to_bookmark(3));
        assert_eq!(viewer.offset(), 6);
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.cycle-encoding")),
            &mut UpdateContext {
                action: &near_core::ActionContext::default(),
            },
        );
        assert_eq!(viewer.encoding(), ViewerEncoding::Latin1);
    }

    #[test]
    fn automatic_utf16_viewing_preserves_encoded_navigation_and_search_offsets() {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend("first\nsecond\n".encode_utf16().flat_map(u16::to_le_bytes));
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(bytes),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "UTF-16LE",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap()
        .with_settings(ViewerSettings::default());
        assert_eq!(viewer.encoding(), ViewerEncoding::Utf16Le);
        assert!(!viewer.is_hex());
        assert!(viewer.visible_text(80, 2).contains("first\nsecond"));
        viewer.move_by(1);
        assert_eq!(viewer.offset(), 14);
        assert_eq!(
            viewer.current_line_bytes().unwrap(),
            ViewerEncoding::Utf16Le.encode("second")
        );
        viewer.search = Some("second".to_owned());
        viewer.offset = 0;
        viewer.search_next();
        assert_eq!(viewer.offset(), 14);
    }

    #[test]
    fn stream_and_column_selections_copy_to_the_injected_clipboard() {
        let action = near_core::ActionContext::default();
        let stream_clipboard = Arc::new(RecordingClipboard::default());
        let mut stream = ViewerSurface::text("viewer", "Stream", "alpha beta")
            .with_clipboard(stream_clipboard.clone());
        for _ in 0..5 {
            stream.update(
                &SurfaceEvent::Command(invocation("near.viewer.select-right")),
                &mut UpdateContext { action: &action },
            );
        }
        stream.update(
            &SurfaceEvent::Command(invocation("near.viewer.copy")),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(&*stream_clipboard.text.lock().unwrap(), "alpha");

        let column_clipboard = Arc::new(RecordingClipboard::default());
        let mut column = ViewerSurface::text("viewer", "Column", "abcde\nABCDE\n")
            .with_clipboard(column_clipboard.clone());
        column.move_horizontal(1);
        column.update(
            &SurfaceEvent::Command(invocation("near.viewer.column-select-right")),
            &mut UpdateContext { action: &action },
        );
        column.update(
            &SurfaceEvent::Command(invocation("near.viewer.column-select-right")),
            &mut UpdateContext { action: &action },
        );
        column.update(
            &SurfaceEvent::Command(invocation("near.viewer.column-select-down")),
            &mut UpdateContext { action: &action },
        );
        column.update(
            &SurfaceEvent::Command(invocation("near.viewer.copy")),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(&*column_clipboard.text.lock().unwrap(), "bc\nBC");
    }

    #[test]
    fn active_selection_is_semantically_visible() {
        let action = near_core::ActionContext::default();
        let mut viewer = ViewerSurface::text("viewer", "Visible", "alpha beta");
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.select-right")),
            &mut UpdateContext { action: &action },
        );
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.select-right")),
            &mut UpdateContext { action: &action },
        );

        let scene = viewer.scene(
            SceneRect::new(0, 0, 40, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            crate::ScenePrimitive::Text { content, role, .. }
                if content == "al" && role.as_str() == "viewer.selected"
        )));
    }

    #[test]
    fn viewer_copy_rejects_unbounded_selections() {
        let clipboard = Arc::new(RecordingClipboard::default());
        let mut viewer = ViewerSurface::bytes(
            "viewer",
            "Bounded copy",
            vec![b'x'; MAX_COPY_BYTES.saturating_add(1)],
        )
        .with_clipboard(clipboard.clone());
        viewer.selection_anchor = Some(ViewerPoint {
            offset: 0,
            column: 0,
        });
        viewer.column = MAX_COPY_BYTES.saturating_add(1);

        viewer.copy_selection();

        assert_eq!(&*clipboard.text.lock().unwrap(), "");
        assert!(
            viewer
                .error
                .as_deref()
                .is_some_and(|error| error.contains("limited"))
        );
    }

    #[test]
    fn provider_scoped_bookmarks_and_navigation_history_restore() {
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(b"first\nsecond\nthird\n".to_vec()),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "Text",
            provider.clone(),
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        viewer.navigate_to(6);
        viewer.set_bookmark(4);
        viewer.navigate_to(13);
        let state = viewer.state_entry().unwrap();

        let mut restored = ViewerSurface::stream(
            "viewer",
            "Text",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        restored.restore_state(&state);
        assert_eq!(restored.offset(), 13);
        assert!(restored.jump_to_bookmark(4));
        assert_eq!(restored.offset(), 6);
        restored.history_back();
        assert_eq!(restored.offset(), 13);
    }

    #[test]
    fn stale_quick_view_tickets_cancel_and_cannot_commit() {
        let mut tracker = ViewerRequestTracker::default();
        let first = tracker.begin();
        let second = tracker.begin();
        assert!(first.cancellation().is_cancelled());
        assert!(!tracker.is_current(&first));
        assert!(tracker.is_current(&second));
        tracker.cancel();
        assert!(second.cancellation().is_cancelled());
        assert!(!tracker.is_current(&second));
    }

    #[test]
    fn search_entry_accepts_text_and_finds_across_windows() {
        let mut bytes = vec![b'x'; DEFAULT_WINDOW_SIZE + 32];
        bytes[DEFAULT_WINDOW_SIZE + 8..DEFAULT_WINDOW_SIZE + 14].copy_from_slice(b"target");
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(bytes),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "Search",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        let action = near_core::ActionContext::default();
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.search-start")),
            &mut UpdateContext { action: &action },
        );
        viewer.update(
            &SurfaceEvent::Text("target".to_owned()),
            &mut UpdateContext { action: &action },
        );
        viewer.update(
            &SurfaceEvent::Command(invocation("near.viewer.search-confirm")),
            &mut UpdateContext { action: &action },
        );
        assert_eq!(
            viewer.offset(),
            u64::try_from(DEFAULT_WINDOW_SIZE + 8).unwrap()
        );
        assert!(viewer.buffered_bytes() <= DEFAULT_WINDOW_SIZE);
    }

    #[test]
    fn goto_accepts_absolute_relative_percentage_hex_and_line_positions() {
        let mut viewer = ViewerSurface::bytes(
            "viewer",
            "Positions",
            b"zero\none\ntwo\nthree\nfour\n".to_vec(),
        );
        let action = near_core::ActionContext::default();
        let go_to = |viewer: &mut ViewerSurface, position: &str| {
            viewer.update(
                &SurfaceEvent::Command(invocation("near.viewer.goto-start")),
                &mut UpdateContext { action: &action },
            );
            viewer.update(
                &SurfaceEvent::Text(position.to_owned()),
                &mut UpdateContext { action: &action },
            );
            viewer.update(
                &SurfaceEvent::Command(invocation("near.viewer.search-confirm")),
                &mut UpdateContext { action: &action },
            );
        };

        go_to(&mut viewer, "5");
        assert_eq!(viewer.offset(), 5);
        go_to(&mut viewer, "+3");
        assert_eq!(viewer.offset(), 8);
        go_to(&mut viewer, "-2");
        assert_eq!(viewer.offset(), 6);
        go_to(&mut viewer, "0x0a");
        assert_eq!(viewer.offset(), 10);
        go_to(&mut viewer, "50%");
        assert_eq!(viewer.offset(), 12);
        go_to(&mut viewer, "L4");
        assert_eq!(viewer.offset(), 13);
    }

    #[test]
    fn text_and_hex_search_move_both_directions_and_record_history() {
        let mut viewer = ViewerSurface::bytes("viewer", "Search", b"alpha beta alpha".to_vec());
        viewer.set_search(Some("alpha".to_owned()));
        assert!(viewer.search_next());
        assert_eq!(viewer.offset(), 0);
        assert!(viewer.search_next());
        assert_eq!(viewer.offset(), 11);
        assert!(viewer.search_previous());
        assert_eq!(viewer.offset(), 0);
        viewer.history_back();
        assert_eq!(viewer.offset(), 11);
        viewer.history_forward();
        assert_eq!(viewer.offset(), 0);

        viewer.hex = true;
        viewer.navigate_to(16);
        viewer.set_search(Some("61 6c 70 68 61".to_owned()));
        assert!(viewer.search_previous());
        assert_eq!(viewer.offset(), 11);
        viewer.set_search(Some("abc".to_owned()));
        assert!(!viewer.search_previous());
        assert_eq!(
            viewer.error.as_deref(),
            Some("hex search requires complete byte pairs")
        );
    }

    #[test]
    fn streamed_line_goto_remains_bounded() {
        let mut bytes = Vec::new();
        for line in 1..20_000 {
            bytes.extend_from_slice(format!("line {line}\n").as_bytes());
        }
        let provider = Arc::new(MemoryProvider {
            bytes: Arc::new(bytes),
        });
        let mut viewer = ViewerSurface::stream(
            "viewer",
            "Lines",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        let offset = viewer.resolve_position("L15000").unwrap();
        viewer.navigate_to(offset);
        assert!(viewer.visible_text(80, 1).starts_with("line 15000"));
        assert!(viewer.buffered_bytes() <= DEFAULT_WINDOW_SIZE);
    }
}
