#![allow(
    clippy::assigning_clones,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::nonminimal_bool,
    clippy::if_not_else,
    clippy::redundant_closure_for_method_calls,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

use std::{
    collections::BTreeMap,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use near_config::EditorSettings;
use near_core::{
    CancellationToken, CapabilitySet, ContextId, OpenRequest, ProviderError, ProviderFuture,
    ResourceProvider, ResourceRef, ResourceStream, ResourceVersion, SurfaceId, WriteRequest,
};
use regex::Regex;

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfacePresentation, SurfaceState,
    TextAlignment, UpdateContext, UpdateResult,
};

const READ_CHUNK: usize = 1024 * 1024;
const MAX_EDITOR_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
struct Revision {
    lines: Vec<String>,
    row: usize,
    column: usize,
    selection_anchor: Option<(usize, usize)>,
    selection_mode: SelectionMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SelectionMode {
    #[default]
    Stream,
    Column,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum EditorPrompt {
    #[default]
    None,
    Find,
    ReplaceFind,
    ReplaceWith,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorEncoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    Latin1,
}

impl EditorEncoding {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Latin1 => "Latin-1",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "utf-8" | "utf8" => Some(Self::Utf8),
            "utf-16le" | "utf16le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" => Some(Self::Utf16Be),
            "latin-1" | "latin1" | "ansi" => Some(Self::Latin1),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorLineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
}

impl EditorLineEnding {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
            Self::Cr => "CR",
        }
    }

    pub const fn separator(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lf" | "unix" => Some(Self::Lf),
            "crlf" | "windows" => Some(Self::CrLf),
            "cr" | "mac" => Some(Self::Cr),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorSaveFormat {
    pub encoding: EditorEncoding,
    pub bom: bool,
    pub line_ending: EditorLineEnding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorSaveOutcome {
    Saved,
    LossyConfirmationRequired,
    ExternalChange,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditorMatch {
    row: usize,
    start: usize,
    end: usize,
    preview: String,
}

pub struct EditorSurface {
    id: SurfaceId,
    title: String,
    provider: Arc<dyn ResourceProvider>,
    resource: ResourceRef,
    version: ResourceVersion,
    format: EditorSaveFormat,
    lines: Vec<String>,
    row: usize,
    column: usize,
    selection_anchor: Option<(usize, usize)>,
    selection_mode: SelectionMode,
    persistent_blocks: bool,
    tab_size: usize,
    expand_tabs: bool,
    clipboard: String,
    clipboard_column: bool,
    top: usize,
    dirty: bool,
    undo: Vec<Revision>,
    redo: Vec<Revision>,
    search: String,
    replacement: String,
    prompt: EditorPrompt,
    regex_search: bool,
    preserve_style: bool,
    find_results: Vec<EditorMatch>,
    find_result_index: usize,
    find_all_open: bool,
    last_search_match: Option<(usize, usize)>,
    close_armed: bool,
    message: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditorPosition {
    pub row: usize,
    pub column: usize,
    pub top: usize,
}

impl EditorSurface {
    pub fn open(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderError> {
        let metadata = block_on_provider(provider.stat(&resource))?;
        let version = ResourceVersion {
            size: metadata.size,
            modified_unix_ms: metadata.modified_unix_ms,
        };
        let bytes = read_all(provider.as_ref(), &resource, cancellation)?;
        let (text, encoding, bom) = decode_editor_bytes(&bytes)?;
        let line_ending = detect_line_ending(&text);
        let lines = split_lines(&text);
        Ok(Self {
            id: id.into(),
            title: title.into(),
            provider,
            resource,
            version,
            format: EditorSaveFormat {
                encoding,
                bom,
                line_ending,
            },
            lines,
            row: 0,
            column: 0,
            selection_anchor: None,
            selection_mode: SelectionMode::Stream,
            persistent_blocks: false,
            tab_size: 4,
            expand_tabs: false,
            clipboard: String::new(),
            clipboard_column: false,
            top: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            search: String::new(),
            replacement: String::new(),
            prompt: EditorPrompt::None,
            regex_search: false,
            preserve_style: false,
            find_results: Vec::new(),
            find_result_index: 0,
            find_all_open: false,
            last_search_match: None,
            close_armed: false,
            message: format!(
                "{}{} • {}",
                encoding.label(),
                if bom { " BOM" } else { "" },
                line_ending.label()
            ),
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    pub const fn position(&self) -> EditorPosition {
        EditorPosition {
            row: self.row,
            column: self.column,
            top: self.top,
        }
    }

    pub fn restore_position(&mut self, position: EditorPosition) {
        self.row = position.row.min(self.lines.len().saturating_sub(1));
        self.column = position.column;
        self.clamp_column();
        self.top = position.top.min(self.row);
    }

    pub fn set_persistent_blocks(&mut self, persistent: bool) {
        self.persistent_blocks = persistent;
    }

    #[must_use]
    pub fn with_settings(mut self, settings: EditorSettings) -> Self {
        self.persistent_blocks = settings.persistent_blocks;
        self.tab_size = usize::from(settings.tab_size);
        self.expand_tabs = settings.expand_tabs;
        self.message = format!(
            "{} • tab:{}:{}",
            self.message,
            self.tab_size,
            if self.expand_tabs {
                "spaces"
            } else {
                "literal"
            }
        );
        self
    }

    fn snapshot(&self) -> Revision {
        Revision {
            lines: self.lines.clone(),
            row: self.row,
            column: self.column,
            selection_anchor: self.selection_anchor,
            selection_mode: self.selection_mode,
        }
    }

    fn begin_edit(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.close_armed = false;
    }

    fn current_chars(&self) -> Vec<char> {
        self.lines[self.row].chars().collect()
    }

    fn line_len(&self) -> usize {
        self.lines[self.row].chars().count()
    }

    fn clamp_column(&mut self) {
        self.column = self.column.min(self.line_len());
    }

    fn prepare_regular_movement(&mut self) {
        if !self.persistent_blocks {
            self.selection_anchor = None;
        }
    }

    fn begin_selection(&mut self, mode: SelectionMode) {
        if self.selection_anchor.is_none() || self.selection_mode != mode {
            self.selection_anchor = Some((self.row, self.column));
        }
        self.selection_mode = mode;
    }

    fn move_vertical(&mut self, rows: isize) {
        self.row = self
            .row
            .saturating_add_signed(rows)
            .min(self.lines.len().saturating_sub(1));
        self.clamp_column();
    }

    fn move_vertical_preserving_column(&mut self, rows: isize) {
        self.row = self
            .row
            .saturating_add_signed(rows)
            .min(self.lines.len().saturating_sub(1));
    }

    fn move_horizontal(&mut self, columns: isize) {
        if columns < 0 {
            if self.column > 0 {
                self.column -= 1;
            } else if self.row > 0 {
                self.row -= 1;
                self.column = self.line_len();
            }
        } else if self.column < self.line_len() {
            self.column += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.column = 0;
        }
    }

    fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.begin_edit();
        self.remove_selection_without_revision();
        for character in text.chars() {
            if character == '\n' {
                self.newline_without_revision();
            } else if character != '\r' {
                let mut chars = self.current_chars();
                chars.insert(self.column, character);
                self.lines[self.row] = chars.into_iter().collect();
                self.column += 1;
            }
        }
    }

    fn newline_without_revision(&mut self) {
        let chars = self.current_chars();
        let left: String = chars[..self.column].iter().collect();
        let right: String = chars[self.column..].iter().collect();
        self.lines[self.row] = left;
        self.row += 1;
        self.lines.insert(self.row, right);
        self.column = 0;
    }

    fn newline(&mut self) {
        self.begin_edit();
        self.newline_without_revision();
    }

    fn insert_tab(&mut self) {
        if self.expand_tabs {
            let column = display_width(
                self.lines[self.row].chars().take(self.column),
                self.tab_size,
            );
            self.insert(&" ".repeat(self.tab_size - column % self.tab_size));
        } else {
            self.insert("\t");
        }
    }

    fn backspace(&mut self) {
        if self.selection_anchor.is_some() {
            self.begin_edit();
            self.remove_selection_without_revision();
            return;
        }
        if self.row == 0 && self.column == 0 {
            return;
        }
        self.begin_edit();
        if self.column > 0 {
            let mut chars = self.current_chars();
            chars.remove(self.column - 1);
            self.lines[self.row] = chars.into_iter().collect();
            self.column -= 1;
        } else {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.column = self.line_len();
            self.lines[self.row].push_str(&current);
        }
    }

    fn delete(&mut self) {
        if self.selection_anchor.is_some() {
            self.begin_edit();
            self.remove_selection_without_revision();
            return;
        }
        if self.column < self.line_len() {
            self.begin_edit();
            let mut chars = self.current_chars();
            chars.remove(self.column);
            self.lines[self.row] = chars.into_iter().collect();
        } else if self.row + 1 < self.lines.len() {
            self.begin_edit();
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        }
    }

    fn restore(&mut self, revision: Revision) {
        self.lines = revision.lines;
        self.row = revision.row;
        self.column = revision.column;
        self.selection_anchor = revision.selection_anchor;
        self.selection_mode = revision.selection_mode;
        self.dirty = true;
    }

    fn undo(&mut self) {
        if let Some(revision) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(revision);
            "Undo".clone_into(&mut self.message);
        }
    }

    fn redo(&mut self) {
        if let Some(revision) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(revision);
            "Redo".clone_into(&mut self.message);
        }
    }

    pub const fn save_format(&self) -> EditorSaveFormat {
        self.format
    }

    pub fn save(&mut self) -> EditorSaveOutcome {
        self.save_to(
            Arc::clone(&self.provider),
            self.resource.clone(),
            self.save_format(),
            false,
            false,
        )
    }

    pub fn save_as(
        &mut self,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        format: EditorSaveFormat,
        confirm_lossy: bool,
    ) -> EditorSaveOutcome {
        self.save_to(provider, resource, format, confirm_lossy, true)
    }

    pub fn force_save_after_external_change(&mut self) -> EditorSaveOutcome {
        self.save_to(
            Arc::clone(&self.provider),
            self.resource.clone(),
            self.save_format(),
            true,
            true,
        )
    }

    pub fn confirm_lossy_save(&mut self) -> EditorSaveOutcome {
        self.save_to(
            Arc::clone(&self.provider),
            self.resource.clone(),
            self.save_format(),
            true,
            false,
        )
    }

    fn save_to(
        &mut self,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        format: EditorSaveFormat,
        confirm_lossy: bool,
        replace_identity: bool,
    ) -> EditorSaveOutcome {
        let (bytes, lossy) =
            encode_editor_text(&self.lines.join(format.line_ending.separator()), format);
        if lossy && !confirm_lossy {
            "Lossy save requires explicit confirmation".clone_into(&mut self.message);
            return EditorSaveOutcome::LossyConfirmationRequired;
        }
        let request = WriteRequest {
            bytes,
            expected: (!replace_identity).then_some(self.version),
            cancellation: CancellationToken::default(),
        };
        match block_on_provider(provider.write(&resource, request)) {
            Ok(()) => {
                if replace_identity {
                    self.provider = provider;
                    self.resource = resource;
                    self.title = self.resource.location.as_str().to_owned();
                }
                if let Ok(metadata) = block_on_provider(self.provider.stat(&self.resource)) {
                    self.version = ResourceVersion {
                        size: metadata.size,
                        modified_unix_ms: metadata.modified_unix_ms,
                    };
                }
                self.format = format;
                self.dirty = false;
                self.message = format!(
                    "Saved • {}{} • {}",
                    format.encoding.label(),
                    if format.bom { " BOM" } else { "" },
                    format.line_ending.label()
                );
                EditorSaveOutcome::Saved
            }
            Err(ProviderError::Conflict(_)) => {
                "Resource changed externally • choose reload, compare, or keep local"
                    .clone_into(&mut self.message);
                EditorSaveOutcome::ExternalChange
            }
            Err(error) => {
                self.message = format!("Save failed: {error}");
                EditorSaveOutcome::Failed(error.to_string())
            }
        }
    }

    pub fn reload_external(&mut self) -> Result<(), ProviderError> {
        let metadata = block_on_provider(self.provider.stat(&self.resource))?;
        let bytes = read_all(
            self.provider.as_ref(),
            &self.resource,
            CancellationToken::default(),
        )?;
        let (text, encoding, bom) = decode_editor_bytes(&bytes)?;
        self.lines = split_lines(&text);
        self.format = EditorSaveFormat {
            encoding,
            bom,
            line_ending: detect_line_ending(&text),
        };
        self.version = ResourceVersion {
            size: metadata.size,
            modified_unix_ms: metadata.modified_unix_ms,
        };
        self.row = self.row.min(self.lines.len().saturating_sub(1));
        self.column = self.column.min(self.line_len());
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        "Reloaded external version".clone_into(&mut self.message);
        Ok(())
    }

    pub fn external_comparison(&self) -> Result<String, ProviderError> {
        let bytes = read_all(
            self.provider.as_ref(),
            &self.resource,
            CancellationToken::default(),
        )?;
        let (external, _, _) = decode_editor_bytes(&bytes)?;
        Ok(format_comparison(&self.document_text(), &external))
    }

    fn document_text(&self) -> String {
        self.lines.join("\n")
    }

    fn position_offset(&self, row: usize, column: usize) -> usize {
        self.lines
            .iter()
            .take(row)
            .map(|line| line.chars().count() + 1)
            .sum::<usize>()
            .saturating_add(column)
    }

    fn selection_offsets(&self) -> Option<(usize, usize)> {
        if self.selection_mode != SelectionMode::Stream {
            return None;
        }
        let anchor = self.selection_anchor?;
        let anchor = self.position_offset(anchor.0, anchor.1);
        let cursor = self.position_offset(self.row, self.column);
        (anchor != cursor).then_some((anchor.min(cursor), anchor.max(cursor)))
    }

    fn selected_text(&self) -> Option<String> {
        if self.selection_mode == SelectionMode::Column {
            let ((top, left), (bottom, right)) = self.column_bounds()?;
            if left == right {
                return None;
            }
            return Some(
                self.lines[top..=bottom]
                    .iter()
                    .map(|line| {
                        let mut selected = line
                            .chars()
                            .skip(left)
                            .take(right - left)
                            .collect::<String>();
                        let selected_width = selected.chars().count();
                        selected.extend(std::iter::repeat_n(' ', right - left - selected_width));
                        selected
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let (start, end) = self.selection_offsets()?;
        Some(
            self.document_text()
                .chars()
                .skip(start)
                .take(end - start)
                .collect(),
        )
    }

    fn remove_selection_without_revision(&mut self) {
        if self.selection_mode == SelectionMode::Column {
            let Some(((top, left), (bottom, right))) = self.column_bounds() else {
                self.selection_anchor = None;
                return;
            };
            if left == right {
                self.selection_anchor = None;
                return;
            }
            for line in &mut self.lines[top..=bottom] {
                let mut characters = line.chars().collect::<Vec<_>>();
                let start = left.min(characters.len());
                let end = right.min(characters.len());
                characters.drain(start..end);
                *line = characters.into_iter().collect();
            }
            self.row = top;
            self.column = left.min(self.lines[top].chars().count());
            self.selection_anchor = None;
            return;
        }
        let Some((start, end)) = self.selection_offsets() else {
            self.selection_anchor = None;
            return;
        };
        let text = self.document_text();
        let retained = text
            .chars()
            .take(start)
            .chain(text.chars().skip(end))
            .collect::<String>();
        self.lines = split_lines(&retained);
        let prefix = text.chars().take(start).collect::<String>();
        self.row = prefix
            .chars()
            .filter(|character| *character == '\n')
            .count();
        self.column = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count();
        self.selection_anchor = None;
    }

    fn column_bounds(&self) -> Option<((usize, usize), (usize, usize))> {
        let (anchor_row, anchor_column) = self.selection_anchor?;
        Some((
            (anchor_row.min(self.row), anchor_column.min(self.column)),
            (anchor_row.max(self.row), anchor_column.max(self.column)),
        ))
    }

    fn copy_selection(&mut self) {
        if let Some(selected) = self.selected_text() {
            self.clipboard = selected;
            self.clipboard_column = self.selection_mode == SelectionMode::Column;
            self.message = format!("Copied {} characters", self.clipboard.chars().count());
            if !self.persistent_blocks {
                self.selection_anchor = None;
            }
        } else {
            self.clipboard.clone_from(&self.lines[self.row]);
            self.clipboard.push('\n');
            self.clipboard_column = false;
            self.message = format!("Copied line {}", self.row + 1);
        }
    }

    fn cut_selection(&mut self) {
        let Some(selected) = self.selected_text() else {
            "No active selection".clone_into(&mut self.message);
            return;
        };
        self.begin_edit();
        self.clipboard = selected;
        self.clipboard_column = self.selection_mode == SelectionMode::Column;
        self.remove_selection_without_revision();
        self.message = format!("Cut {} characters", self.clipboard.chars().count());
    }

    fn paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            "Editor clipboard is empty".clone_into(&mut self.message);
        } else if self.clipboard_column {
            let clipboard = self.clipboard.clone();
            self.begin_edit();
            self.remove_selection_without_revision();
            let rows = clipboard.split('\n').collect::<Vec<_>>();
            while self.lines.len() < self.row + rows.len() {
                self.lines.push(String::new());
            }
            for (offset, value) in rows.iter().enumerate() {
                let line = &mut self.lines[self.row + offset];
                let mut characters = line.chars().collect::<Vec<_>>();
                if characters.len() < self.column {
                    characters.resize(self.column, ' ');
                }
                characters.splice(self.column..self.column, value.chars());
                *line = characters.into_iter().collect();
            }
            self.message = format!("Pasted {} column rows", rows.len());
        } else {
            let clipboard = self.clipboard.clone();
            self.insert(&clipboard);
        }
    }

    fn search_matches(&self) -> Result<Vec<EditorMatch>, String> {
        if self.search.is_empty() {
            return Err("Search text is empty".to_owned());
        }
        let mut matches = Vec::new();
        if self.regex_search {
            let regex = Regex::new(&self.search).map_err(|error| error.to_string())?;
            for (row, line) in self.lines.iter().enumerate() {
                for found in regex.find_iter(line) {
                    matches.push(EditorMatch {
                        row,
                        start: line[..found.start()].chars().count(),
                        end: line[..found.end()].chars().count(),
                        preview: line.clone(),
                    });
                }
            }
        } else {
            for (row, line) in self.lines.iter().enumerate() {
                for (byte, found) in line.match_indices(&self.search) {
                    matches.push(EditorMatch {
                        row,
                        start: line[..byte].chars().count(),
                        end: line[..byte + found.len()].chars().count(),
                        preview: line.clone(),
                    });
                }
            }
        }
        Ok(matches)
    }

    fn next_match(&self) -> Result<Option<EditorMatch>, String> {
        let matches = self.search_matches()?;
        let origin = self.last_search_match.unwrap_or((self.row, self.column));
        let include_origin = self.last_search_match.is_none();
        Ok(matches
            .iter()
            .find(|found| {
                found.row > origin.0
                    || (found.row == origin.0
                        && if include_origin {
                            found.start >= origin.1
                        } else {
                            found.start > origin.1
                        })
            })
            .or_else(|| matches.first())
            .cloned())
    }

    fn search_next(&mut self) {
        match self.next_match() {
            Ok(Some(found)) => {
                self.row = found.row;
                self.column = found.start;
                self.last_search_match = Some((found.row, found.start));
                self.message = format!("Found: {}", self.search);
            }
            Ok(None) => self.message = format!("Not found: {}", self.search),
            Err(error) => self.message = format!("Invalid search: {error}"),
        }
    }

    fn replacement_for(&self, found: &EditorMatch) -> Result<String, String> {
        let replacement = if self.regex_search {
            let regex = Regex::new(&self.search).map_err(|error| error.to_string())?;
            let line = &self.lines[found.row];
            let byte_start = char_to_byte(line, found.start);
            let captures = regex
                .captures_at(line, byte_start)
                .filter(|captures| {
                    captures
                        .get(0)
                        .is_some_and(|matched| matched.start() == byte_start)
                })
                .ok_or_else(|| "regex match became stale".to_owned())?;
            let mut expanded = String::new();
            captures.expand(&self.replacement, &mut expanded);
            expanded
        } else {
            self.replacement.clone()
        };
        if self.preserve_style {
            let matched = self.lines[found.row]
                .chars()
                .skip(found.start)
                .take(found.end - found.start)
                .collect::<String>();
            Ok(preserve_replacement_style(&matched, &replacement))
        } else {
            Ok(replacement)
        }
    }

    fn replace_match(&mut self, found: &EditorMatch, replacement: &str) {
        let line = &mut self.lines[found.row];
        let mut characters = line.chars().collect::<Vec<_>>();
        characters.splice(found.start..found.end, replacement.chars());
        *line = characters.into_iter().collect();
        self.row = found.row;
        self.column = found.start + replacement.chars().count();
    }

    fn replace_next(&mut self) {
        let found = match self.next_match() {
            Ok(Some(found)) => found,
            Ok(None) => {
                self.message = format!("Not found: {}", self.search);
                return;
            }
            Err(error) => {
                self.message = format!("Invalid search: {error}");
                return;
            }
        };
        let replacement = match self.replacement_for(&found) {
            Ok(replacement) => replacement,
            Err(error) => {
                self.message = format!("Invalid replacement: {error}");
                return;
            }
        };
        self.begin_edit();
        self.replace_match(&found, &replacement);
        self.last_search_match = Some((found.row, found.start));
        self.message = "Replaced 1 match".to_owned();
    }

    fn replace_all(&mut self) {
        let matches = match self.search_matches() {
            Ok(matches) => matches,
            Err(error) => {
                self.message = format!("Invalid search: {error}");
                return;
            }
        };
        if matches.is_empty() {
            self.message = format!("Not found: {}", self.search);
            return;
        }
        let replacements = match matches
            .iter()
            .map(|found| {
                self.replacement_for(found)
                    .map(|replacement| (found.clone(), replacement))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(replacements) => replacements,
            Err(error) => {
                self.message = format!("Invalid replacement: {error}");
                return;
            }
        };
        self.begin_edit();
        for (found, replacement) in replacements.iter().rev() {
            self.replace_match(found, replacement);
        }
        self.message = format!("Replaced {} matches", replacements.len());
    }

    fn open_find_all(&mut self) {
        match self.search_matches() {
            Ok(results) if results.is_empty() => {
                self.find_results.clear();
                self.find_all_open = false;
                self.message = format!("Not found: {}", self.search);
            }
            Ok(results) => {
                self.find_results = results;
                self.find_result_index = 0;
                self.find_all_open = true;
                self.message = format!("Find All: {} matches", self.find_results.len());
            }
            Err(error) => self.message = format!("Invalid search: {error}"),
        }
    }

    fn move_find_result(&mut self, rows: isize) {
        if self.find_results.is_empty() {
            return;
        }
        self.find_result_index = self
            .find_result_index
            .saturating_add_signed(rows)
            .min(self.find_results.len() - 1);
    }

    fn activate_find_result(&mut self) {
        let Some(found) = self.find_results.get(self.find_result_index) else {
            return;
        };
        self.row = found.row;
        self.column = found.start;
        self.find_all_open = false;
        self.message = format!(
            "Match {}/{}",
            self.find_result_index + 1,
            self.find_results.len()
        );
    }

    fn confirm_prompt(&mut self) {
        match self.prompt {
            EditorPrompt::Find => {
                self.prompt = EditorPrompt::None;
                self.search_next();
            }
            EditorPrompt::ReplaceFind => {
                self.prompt = EditorPrompt::ReplaceWith;
                "Replacement text>".clone_into(&mut self.message);
            }
            EditorPrompt::ReplaceWith => {
                self.prompt = EditorPrompt::None;
                self.replace_next();
            }
            EditorPrompt::None => {}
        }
    }

    fn prompt_buffer_mut(&mut self) -> Option<&mut String> {
        match self.prompt {
            EditorPrompt::Find | EditorPrompt::ReplaceFind => Some(&mut self.search),
            EditorPrompt::ReplaceWith => Some(&mut self.replacement),
            EditorPrompt::None => None,
        }
    }

    fn find_all_text(&self, height: usize) -> String {
        self.find_results
            .iter()
            .enumerate()
            .skip(
                self.find_result_index
                    .saturating_sub(height.saturating_sub(1)),
            )
            .take(height)
            .map(|(index, found)| {
                format!(
                    "{} {:>5}:{:<4} {}",
                    if index == self.find_result_index {
                        '▶'
                    } else {
                        ' '
                    },
                    found.row + 1,
                    found.start + 1,
                    found.preview
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn visible_text(&self, width: usize, height: usize) -> String {
        let gutter = self.lines.len().to_string().len().max(3);
        let content_width = width.saturating_sub(gutter + 2);
        self.lines
            .iter()
            .enumerate()
            .skip(self.top)
            .take(height)
            .map(|(row, line)| {
                let visible = display_editor_line(
                    line,
                    (row == self.row).then_some(self.column),
                    self.tab_size,
                    content_width,
                );
                format!("{:>gutter$} │ {}", row + 1, visible)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn keep_cursor_visible(&mut self, height: usize) {
        if self.row < self.top {
            self.top = self.row;
        } else if height > 0 && self.row >= self.top + height {
            self.top = self.row + 1 - height;
        }
    }
}

impl Surface for EditorSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.editor")]
    }

    fn capabilities(&self) -> CapabilitySet {
        self.provider.capabilities(&self.resource)
    }

    fn state(&self) -> SurfaceState {
        SurfaceState {
            current: Some(self.resource.clone()),
            selected: Vec::new(),
            location: Some(self.resource.location.clone()),
        }
    }

    fn presentation(&self) -> SurfacePresentation {
        SurfacePresentation::FullScreen
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        if self.find_all_open {
            let SurfaceEvent::Command(invocation) = event else {
                return UpdateResult::handled();
            };
            match invocation.id.as_str() {
                "near.editor.up" | "near.editor.page-up" => self.move_find_result(-1),
                "near.editor.down" | "near.editor.page-down" => self.move_find_result(1),
                "near.editor.newline" => self.activate_find_result(),
                "near.editor.close" => self.find_all_open = false,
                _ => return UpdateResult::ignored(),
            }
            return UpdateResult::handled();
        }
        if self.prompt != EditorPrompt::None {
            match event {
                SurfaceEvent::Text(text) | SurfaceEvent::Paste(text) => {
                    self.prompt_buffer_mut()
                        .expect("prompt buffer")
                        .push_str(text);
                    return UpdateResult::handled();
                }
                SurfaceEvent::Backspace => {
                    self.prompt_buffer_mut().expect("prompt buffer").pop();
                    return UpdateResult::handled();
                }
                _ => {}
            }
        } else {
            match event {
                SurfaceEvent::Text(text) | SurfaceEvent::Paste(text) => {
                    self.insert(text);
                    return UpdateResult::handled();
                }
                SurfaceEvent::Backspace => {
                    self.backspace();
                    return UpdateResult::handled();
                }
                _ => {}
            }
        }
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        match invocation.id.as_str() {
            "near.editor.up" => {
                self.prepare_regular_movement();
                self.move_vertical(-1);
            }
            "near.editor.down" => {
                self.prepare_regular_movement();
                self.move_vertical(1);
            }
            "near.editor.left" => {
                self.prepare_regular_movement();
                self.move_horizontal(-1);
            }
            "near.editor.right" => {
                self.prepare_regular_movement();
                self.move_horizontal(1);
            }
            "near.editor.page-up" => {
                self.prepare_regular_movement();
                self.move_vertical(-20);
            }
            "near.editor.page-down" => {
                self.prepare_regular_movement();
                self.move_vertical(20);
            }
            "near.editor.home" => {
                self.prepare_regular_movement();
                self.column = 0;
            }
            "near.editor.end" => {
                self.prepare_regular_movement();
                self.column = self.line_len();
            }
            "near.editor.select-up" => {
                self.begin_selection(SelectionMode::Stream);
                self.move_vertical(-1);
            }
            "near.editor.select-down" => {
                self.begin_selection(SelectionMode::Stream);
                self.move_vertical(1);
            }
            "near.editor.select-left" => {
                self.begin_selection(SelectionMode::Stream);
                self.move_horizontal(-1);
            }
            "near.editor.select-right" => {
                self.begin_selection(SelectionMode::Stream);
                self.move_horizontal(1);
            }
            "near.editor.column-select-up" => {
                self.begin_selection(SelectionMode::Column);
                self.move_vertical_preserving_column(-1);
            }
            "near.editor.column-select-down" => {
                self.begin_selection(SelectionMode::Column);
                self.move_vertical_preserving_column(1);
            }
            "near.editor.column-select-left" => {
                self.begin_selection(SelectionMode::Column);
                self.move_horizontal(-1);
            }
            "near.editor.column-select-right" => {
                self.begin_selection(SelectionMode::Column);
                self.move_horizontal(1);
            }
            "near.editor.newline" if self.prompt != EditorPrompt::None => self.confirm_prompt(),
            "near.editor.newline" => self.newline(),
            "near.editor.insert-tab" if self.prompt == EditorPrompt::ReplaceFind => {
                self.prompt = EditorPrompt::ReplaceWith;
                "Replacement text>".clone_into(&mut self.message);
            }
            "near.editor.insert-tab" => self.insert_tab(),
            "near.editor.delete" => self.delete(),
            "near.editor.undo" => self.undo(),
            "near.editor.redo" => self.redo(),
            "near.editor.selection-toggle" => {
                if self.selection_anchor.take().is_none() {
                    self.selection_anchor = Some((self.row, self.column));
                    self.selection_mode = SelectionMode::Stream;
                    "Selection started".clone_into(&mut self.message);
                } else {
                    "Selection cleared".clone_into(&mut self.message);
                }
            }
            "near.editor.selection-clear" => {
                self.selection_anchor = None;
                "Selection cleared".clone_into(&mut self.message);
            }
            "near.editor.select-all" => {
                self.selection_anchor = Some((0, 0));
                self.selection_mode = SelectionMode::Stream;
                self.row = self.lines.len() - 1;
                self.column = self.line_len();
                "Selected all".clone_into(&mut self.message);
            }
            "near.editor.copy" => self.copy_selection(),
            "near.editor.cut" => self.cut_selection(),
            "near.editor.paste" => self.paste_clipboard(),
            "near.editor.toggle-persistent-blocks" => {
                self.persistent_blocks = !self.persistent_blocks;
                self.message = format!("Persistent blocks: {}", self.persistent_blocks);
            }
            "near.editor.save" => match self.save() {
                EditorSaveOutcome::ExternalChange => {
                    return UpdateResult::dispatch(near_core::CommandInvocation {
                        id: "near.editor.external-change".into(),
                        arguments: BTreeMap::default(),
                    });
                }
                EditorSaveOutcome::LossyConfirmationRequired => {
                    return UpdateResult::dispatch(near_core::CommandInvocation {
                        id: "near.editor.lossy-save-warning".into(),
                        arguments: BTreeMap::default(),
                    });
                }
                EditorSaveOutcome::Saved | EditorSaveOutcome::Failed(_) => {}
            },
            "near.editor.search-start" => {
                self.search.clear();
                self.last_search_match = None;
                self.prompt = EditorPrompt::Find;
                "Find>".clone_into(&mut self.message);
            }
            "near.editor.search-confirm" => self.confirm_prompt(),
            "near.editor.search-next" => self.search_next(),
            "near.editor.replace-start" => {
                self.search.clear();
                self.replacement.clear();
                self.last_search_match = None;
                self.prompt = EditorPrompt::ReplaceFind;
                "Replace find>".clone_into(&mut self.message);
            }
            "near.editor.replace-all" => self.replace_all(),
            "near.editor.find-all" => self.open_find_all(),
            "near.editor.toggle-regex" => {
                self.regex_search = !self.regex_search;
                self.last_search_match = None;
                self.message = format!("Regex search: {}", self.regex_search);
            }
            "near.editor.toggle-preserve-style" => {
                self.preserve_style = !self.preserve_style;
                self.message = format!("Preserve replacement style: {}", self.preserve_style);
            }
            "near.editor.close" if self.prompt != EditorPrompt::None => {
                self.prompt = EditorPrompt::None;
                "Search prompt cancelled".clone_into(&mut self.message);
            }
            "near.editor.close" if self.dirty && !self.close_armed => {
                self.close_armed = true;
                "Unsaved changes • Ctrl+S saves • Esc again discards".clone_into(&mut self.message);
            }
            "near.editor.close" => {
                return UpdateResult::dispatch(near_core::CommandInvocation {
                    id: "near.editor.close-confirmed".into(),
                    arguments: BTreeMap::default(),
                });
            }
            _ => return UpdateResult::ignored(),
        }
        self.keep_cursor_visible(20);
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "editor.background");
        let dirty = if self.dirty { " *" } else { "" };
        scene.border(
            area,
            Some(format!(" {}{} ", self.title, dirty)),
            "editor.border",
        );
        let inner = area.inset(1);
        let body_height = inner.height.saturating_sub(1);
        scene.text(
            SceneRect::new(inner.x, inner.y, inner.width, body_height),
            if self.find_all_open {
                self.find_all_text(usize::from(body_height))
            } else {
                self.visible_text(usize::from(inner.width), usize::from(body_height))
            },
            "editor.text",
        );
        let search = match self.prompt {
            EditorPrompt::Find => format!(" find>{}_", self.search),
            EditorPrompt::ReplaceFind => format!(" replace-find>{}_", self.search),
            EditorPrompt::ReplaceWith => format!(" replace-with>{}_", self.replacement),
            EditorPrompt::None => String::new(),
        };
        let selection = self.selected_text().map_or_else(String::new, |selected| {
            format!(
                " {}:{}{}",
                match self.selection_mode {
                    SelectionMode::Stream => "stream",
                    SelectionMode::Column => "column",
                },
                selected.chars().count(),
                if self.persistent_blocks {
                    "+persistent"
                } else {
                    ""
                }
            )
        });
        scene.aligned_text(
            SceneRect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            format!(
                "Ln {}, Col {}{}{} regex:{} style:{}  {}",
                self.row + 1,
                self.column + 1,
                search,
                selection,
                self.regex_search,
                self.preserve_style,
                self.message
            ),
            "editor.status",
            TextAlignment::Right,
        );
        scene
    }
}

fn display_width(characters: impl Iterator<Item = char>, tab_size: usize) -> usize {
    characters.fold(0, |column, character| {
        if character == '\t' {
            column + tab_size - column % tab_size
        } else {
            column + 1
        }
    })
}

fn display_editor_line(line: &str, cursor: Option<usize>, tab_size: usize, width: usize) -> String {
    let mut rendered = String::new();
    let mut display_column = 0;
    for (column, character) in line.chars().enumerate() {
        if cursor == Some(column) {
            rendered.push('▌');
        }
        if character == '\t' {
            let spaces = tab_size - display_column % tab_size;
            rendered.extend(std::iter::repeat_n(' ', spaces));
            display_column += spaces;
        } else {
            rendered.push(character);
            display_column += 1;
        }
        if display_column >= width {
            break;
        }
    }
    if cursor == Some(line.chars().count()) && display_column < width {
        rendered.push('▌');
    }
    rendered.chars().take(width).collect()
}

fn split_lines(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized
        .split('\n')
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn detect_line_ending(text: &str) -> EditorLineEnding {
    let bytes = text.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 2;
            }
            b'\r' => {
                cr += 1;
                index += 1;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    if crlf >= lf && crlf >= cr && crlf > 0 {
        EditorLineEnding::CrLf
    } else if cr > lf && cr > 0 {
        EditorLineEnding::Cr
    } else {
        EditorLineEnding::Lf
    }
}

fn decode_editor_bytes(bytes: &[u8]) -> Result<(String, EditorEncoding, bool), ProviderError> {
    if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(bytes.to_vec())
            .map(|text| (text, EditorEncoding::Utf8, true))
            .map_err(|error| ProviderError::Unsupported(error.to_string()));
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(bytes, true).map(|text| (text, EditorEncoding::Utf16Le, true));
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(bytes, false).map(|text| (text, EditorEncoding::Utf16Be, true));
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => Ok((text, EditorEncoding::Utf8, false)),
        Err(_) => Ok((
            bytes.iter().map(|byte| char::from(*byte)).collect(),
            EditorEncoding::Latin1,
            false,
        )),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, ProviderError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(ProviderError::Unsupported(
            "UTF-16 input has an incomplete code unit".to_owned(),
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|error| ProviderError::Unsupported(error.to_string()))
}

fn encode_editor_text(text: &str, format: EditorSaveFormat) -> (Vec<u8>, bool) {
    match format.encoding {
        EditorEncoding::Utf8 => {
            let mut bytes = Vec::with_capacity(text.len().saturating_add(3));
            if format.bom {
                bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
            }
            bytes.extend_from_slice(text.as_bytes());
            (bytes, false)
        }
        EditorEncoding::Utf16Le | EditorEncoding::Utf16Be => {
            let mut bytes = Vec::with_capacity(text.len().saturating_mul(2).saturating_add(2));
            if format.bom {
                bytes.extend_from_slice(match format.encoding {
                    EditorEncoding::Utf16Le => &[0xff, 0xfe],
                    EditorEncoding::Utf16Be => &[0xfe, 0xff],
                    _ => unreachable!(),
                });
            }
            for unit in text.encode_utf16() {
                let encoded = match format.encoding {
                    EditorEncoding::Utf16Le => unit.to_le_bytes(),
                    EditorEncoding::Utf16Be => unit.to_be_bytes(),
                    _ => unreachable!(),
                };
                bytes.extend_from_slice(&encoded);
            }
            (bytes, false)
        }
        EditorEncoding::Latin1 => {
            let mut lossy = false;
            let bytes = text
                .chars()
                .map(|character| {
                    if let Ok(byte) = u8::try_from(u32::from(character)) {
                        byte
                    } else {
                        lossy = true;
                        b'?'
                    }
                })
                .collect();
            (bytes, lossy)
        }
    }
}

fn format_comparison(local: &str, external: &str) -> String {
    let local = local.lines().collect::<Vec<_>>();
    let external = external.lines().collect::<Vec<_>>();
    let mut output = String::from("--- external\n+++ local\n");
    for index in 0..local.len().max(external.len()) {
        match (external.get(index), local.get(index)) {
            (Some(external), Some(local)) if external == local => {
                output.push(' ');
                output.push_str(local);
                output.push('\n');
            }
            (Some(external), Some(local)) => {
                output.push('-');
                output.push_str(external);
                output.push('\n');
                output.push('+');
                output.push_str(local);
                output.push('\n');
            }
            (Some(external), None) => {
                output.push('-');
                output.push_str(external);
                output.push('\n');
            }
            (None, Some(local)) => {
                output.push('+');
                output.push_str(local);
                output.push('\n');
            }
            (None, None) => {}
        }
    }
    output
}

fn char_to_byte(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map_or(text.len(), |(byte, _)| byte)
}

fn preserve_replacement_style(matched: &str, replacement: &str) -> String {
    let letters = matched
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect::<Vec<_>>();
    if !letters.is_empty() && letters.iter().all(|character| character.is_uppercase()) {
        replacement.to_uppercase()
    } else if !letters.is_empty() && letters.iter().all(|character| character.is_lowercase()) {
        replacement.to_lowercase()
    } else if matched
        .chars()
        .next()
        .is_some_and(|character| character.is_uppercase())
        && matched
            .chars()
            .skip(1)
            .all(|character| !character.is_alphabetic() || character.is_lowercase())
    {
        let mut characters = replacement.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_uppercase().collect::<String>() + characters.as_str()
        })
    } else {
        replacement.to_owned()
    }
}

fn read_all(
    provider: &dyn ResourceProvider,
    resource: &ResourceRef,
    cancellation: CancellationToken,
) -> Result<Vec<u8>, ProviderError> {
    let mut bytes = Vec::new();
    loop {
        if bytes.len() >= MAX_EDITOR_BYTES {
            return Err(ProviderError::Unsupported(format!(
                "internal editor limit is {} MiB",
                MAX_EDITOR_BYTES / 1024 / 1024
            )));
        }
        let ResourceStream {
            bytes: chunk,
            complete,
            ..
        } = block_on_provider(provider.open(
            resource,
            OpenRequest {
                offset: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                length: READ_CHUNK.min(MAX_EDITOR_BYTES - bytes.len()),
                cancellation: cancellation.clone(),
            },
        ))?;
        if chunk.is_empty() && !complete {
            return Err(ProviderError::Failed(
                "provider returned an incomplete empty editor stream".to_owned(),
            ));
        }
        bytes.extend_from_slice(&chunk);
        if complete {
            return Ok(bytes);
        }
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use near_config::ResourceOpenPolicy;
    use near_core::{ActionContext, ListPage, ListRequest, Location, ProviderId, ResourceMetadata};

    use super::*;

    struct MemoryProvider {
        bytes: Mutex<Vec<u8>>,
    }

    impl ResourceProvider for MemoryProvider {
        fn id(&self) -> ProviderId {
            "near.editor-memory".into()
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
            Box::pin(async move {
                Ok(ResourceMetadata {
                    size: u64::try_from(self.bytes.lock().unwrap().len()).ok(),
                    ..ResourceMetadata::default()
                })
            })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async move {
                let bytes = self.bytes.lock().unwrap();
                let start = usize::try_from(request.offset)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                let end = start.saturating_add(request.length).min(bytes.len());
                Ok(ResourceStream {
                    offset: request.offset,
                    bytes: bytes[start..end].to_vec(),
                    total_size: u64::try_from(bytes.len()).ok(),
                    complete: end == bytes.len(),
                })
            })
        }

        fn write<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            request: WriteRequest,
        ) -> ProviderFuture<'a, ()> {
            Box::pin(async move {
                let mut bytes = self.bytes.lock().unwrap();
                if request
                    .expected
                    .and_then(|expected| expected.size)
                    .is_some_and(|expected| {
                        expected != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
                    })
                {
                    return Err(ProviderError::Conflict("memory resource".to_owned()));
                }
                *bytes = request.bytes;
                Ok(())
            })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            let mut capabilities = CapabilitySet::default();
            capabilities.insert("resource.read");
            capabilities.insert("resource.write");
            capabilities
        }
    }

    fn resource() -> ResourceRef {
        ResourceRef {
            provider: "near.editor-memory".into(),
            location: Location::new("memory:///document.txt"),
        }
    }

    fn command(id: &str) -> SurfaceEvent {
        SurfaceEvent::Command(near_core::CommandInvocation {
            id: id.into(),
            arguments: BTreeMap::new(),
        })
    }

    fn update(editor: &mut EditorSurface, event: SurfaceEvent) -> UpdateResult {
        editor.update(
            &event,
            &mut UpdateContext {
                action: &ActionContext::default(),
            },
        )
    }

    #[test]
    fn edits_undoes_redoes_searches_and_saves_through_the_provider() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha\nbeta".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider.clone(),
            resource(),
            CancellationToken::default(),
        )
        .unwrap();

        update(&mut editor, command("near.editor.end"));
        update(&mut editor, SurfaceEvent::Text("!".to_owned()));
        assert_eq!(editor.lines[0], "alpha!");
        update(&mut editor, command("near.editor.undo"));
        assert_eq!(editor.lines[0], "alpha");
        update(&mut editor, command("near.editor.redo"));
        assert_eq!(editor.lines[0], "alpha!");

        update(&mut editor, command("near.editor.search-start"));
        update(&mut editor, SurfaceEvent::Text("beta".to_owned()));
        update(&mut editor, command("near.editor.search-confirm"));
        assert_eq!((editor.row, editor.column), (1, 0));

        update(&mut editor, command("near.editor.save"));
        assert!(!editor.is_dirty());
        assert_eq!(*provider.bytes.lock().unwrap(), b"alpha!\nbeta");
    }

    #[test]
    fn save_as_writes_selected_encoding_bom_and_line_endings() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha\nbeta".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider.clone(),
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        let target = ResourceRef {
            provider: provider.id(),
            location: Location::new("memory:///saved.txt"),
        };

        assert_eq!(
            editor.save_as(
                provider.clone(),
                target.clone(),
                EditorSaveFormat {
                    encoding: EditorEncoding::Utf16Be,
                    bom: true,
                    line_ending: EditorLineEnding::CrLf,
                },
                false,
            ),
            EditorSaveOutcome::Saved
        );
        let bytes = provider.bytes.lock().unwrap().clone();
        assert!(bytes.starts_with(&[0xfe, 0xff]));
        let (text, encoding, bom) = decode_editor_bytes(&bytes).unwrap();
        assert_eq!(text, "alpha\r\nbeta");
        assert_eq!(encoding, EditorEncoding::Utf16Be);
        assert!(bom);
        assert_eq!(editor.resource(), &target);
        assert_eq!(editor.save_format().line_ending, EditorLineEnding::CrLf);
    }

    #[test]
    fn lossy_save_requires_explicit_confirmation() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new("snowman ☃".as_bytes().to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider.clone(),
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        let format = EditorSaveFormat {
            encoding: EditorEncoding::Latin1,
            bom: false,
            line_ending: EditorLineEnding::Lf,
        };

        assert_eq!(
            editor.save_as(provider.clone(), resource(), format, false),
            EditorSaveOutcome::LossyConfirmationRequired
        );
        assert_eq!(&*provider.bytes.lock().unwrap(), "snowman ☃".as_bytes());
        assert_eq!(
            editor.save_as(provider.clone(), resource(), format, true),
            EditorSaveOutcome::Saved
        );
        assert_eq!(&*provider.bytes.lock().unwrap(), b"snowman ?");
    }

    #[test]
    fn external_change_supports_compare_reload_and_keep_local() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"original".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider.clone(),
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        editor.insert(" local");
        *provider.bytes.lock().unwrap() = b"external version".to_vec();

        assert_eq!(editor.save(), EditorSaveOutcome::ExternalChange);
        let comparison = editor.external_comparison().unwrap();
        assert!(comparison.contains("-external version"));
        assert!(comparison.contains("+ localoriginal"));
        editor.reload_external().unwrap();
        assert_eq!(editor.document_text(), "external version");
        assert!(!editor.is_dirty());

        editor.column = 0;
        editor.insert("kept ");
        *provider.bytes.lock().unwrap() = b"changed again externally".to_vec();
        assert_eq!(editor.save(), EditorSaveOutcome::ExternalChange);
        assert_eq!(
            editor.force_save_after_external_change(),
            EditorSaveOutcome::Saved
        );
        assert_eq!(&*provider.bytes.lock().unwrap(), b"kept external version");
    }

    #[test]
    fn dirty_editor_requires_a_second_close_before_discarding() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"text".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        update(&mut editor, SurfaceEvent::Text("x".to_owned()));
        let first = update(&mut editor, command("near.editor.close"));
        assert!(first.command.is_none());
        assert!(editor.message.contains("Unsaved changes"));
        let second = update(&mut editor, command("near.editor.close"));
        assert_eq!(
            second.command.unwrap().id.as_str(),
            "near.editor.close-confirmed"
        );
    }

    #[test]
    fn stream_selection_supports_copy_cut_and_paste() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha\nbeta".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();

        update(&mut editor, command("near.editor.select-all"));
        update(&mut editor, command("near.editor.toggle-persistent-blocks"));
        update(&mut editor, command("near.editor.copy"));
        assert_eq!(editor.clipboard, "alpha\nbeta");
        update(&mut editor, command("near.editor.cut"));
        assert_eq!(editor.document_text(), "");
        update(&mut editor, command("near.editor.paste"));
        assert_eq!(editor.document_text(), "alpha\nbeta");
    }

    #[test]
    fn shift_selection_and_persistent_blocks_follow_far_movement_semantics() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha\nbeta".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();

        update(&mut editor, command("near.editor.select-right"));
        update(&mut editor, command("near.editor.select-right"));
        assert_eq!(editor.selected_text().as_deref(), Some("al"));
        update(&mut editor, command("near.editor.copy"));
        assert!(editor.selection_anchor.is_none());

        update(&mut editor, command("near.editor.select-left"));
        update(&mut editor, command("near.editor.toggle-persistent-blocks"));
        update(&mut editor, command("near.editor.copy"));
        assert!(editor.selection_anchor.is_some());
        update(&mut editor, command("near.editor.down"));
        assert!(editor.selection_anchor.is_some());
        update(&mut editor, command("near.editor.toggle-persistent-blocks"));
        update(&mut editor, command("near.editor.right"));
        assert!(editor.selection_anchor.is_none());
    }

    #[test]
    fn column_selection_copies_cuts_and_pastes_rectangles() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"abcd\nABCD\nxy".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();

        update(&mut editor, command("near.editor.right"));
        update(&mut editor, command("near.editor.column-select-down"));
        update(&mut editor, command("near.editor.column-select-right"));
        update(&mut editor, command("near.editor.column-select-right"));
        assert_eq!(editor.selected_text().as_deref(), Some("bc\nBC"));
        update(&mut editor, command("near.editor.copy"));
        assert!(editor.clipboard_column);

        update(&mut editor, command("near.editor.down"));
        update(&mut editor, command("near.editor.home"));
        update(&mut editor, command("near.editor.paste"));
        assert_eq!(editor.document_text(), "abcd\nABCD\nbcxy\nBC");

        editor.row = 0;
        editor.column = 1;
        update(&mut editor, command("near.editor.column-select-down"));
        update(&mut editor, command("near.editor.column-select-right"));
        update(&mut editor, command("near.editor.column-select-right"));
        update(&mut editor, command("near.editor.cut"));
        assert_eq!(editor.document_text(), "ad\nAD\nbcxy\nBC");
    }

    #[test]
    fn editor_settings_are_versioned_and_configure_persistent_blocks() {
        let settings = EditorSettings::from_toml(
            "schema = 1\npersistent_blocks = true\nopen_policy = 'association'\ntab_size = 8\nexpand_tabs = true\n",
        )
        .unwrap();
        assert!(settings.persistent_blocks);
        assert_eq!(settings.open_policy, ResourceOpenPolicy::Association);
        assert_eq!(settings.tab_size, 8);
        assert!(settings.expand_tabs);
        assert!(EditorSettings::from_toml("schema = 2\n").is_err());
        assert!(EditorSettings::from_toml("schema = 1\ntab_size = 0\n").is_err());

        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"text".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        editor = editor.with_settings(settings);
        update(&mut editor, command("near.editor.insert-tab"));
        assert_eq!(editor.document_text(), "        text");
        assert!(editor.visible_text(40, 2).contains("│         ▌text"));
        update(&mut editor, command("near.editor.undo"));
        editor = editor.with_settings(EditorSettings {
            expand_tabs: false,
            ..settings
        });
        update(&mut editor, command("near.editor.insert-tab"));
        assert_eq!(editor.document_text(), "\ttext");
        update(&mut editor, command("near.editor.undo"));
        editor = editor.with_settings(settings);
        update(&mut editor, command("near.editor.select-right"));
        update(&mut editor, command("near.editor.copy"));
        assert!(editor.selection_anchor.is_some());
    }

    #[test]
    fn find_and_replace_prompts_confirm_through_normal_editor_keys() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha alpha".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();

        update(&mut editor, command("near.editor.search-start"));
        update(&mut editor, SurfaceEvent::Text("alpha".to_owned()));
        update(&mut editor, command("near.editor.newline"));
        assert_eq!((editor.row, editor.column), (0, 0));
        assert_eq!(editor.document_text(), "alpha alpha");

        update(&mut editor, command("near.editor.replace-start"));
        update(&mut editor, SurfaceEvent::Text("alpha".to_owned()));
        update(&mut editor, command("near.editor.insert-tab"));
        update(&mut editor, SurfaceEvent::Text("beta".to_owned()));
        update(&mut editor, command("near.editor.newline"));
        assert_eq!(editor.document_text(), "beta alpha");
        update(&mut editor, command("near.editor.replace-all"));
        assert_eq!(editor.document_text(), "beta beta");
    }

    #[test]
    fn regex_groups_style_preservation_and_invalid_patterns_are_safe() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"alpha=one\nalpha=two".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        editor.regex_search = true;
        editor.search = r"alpha=(\w+)".to_owned();
        editor.replacement = "$1:$1".to_owned();
        update(&mut editor, command("near.editor.replace-all"));
        assert_eq!(editor.document_text(), "one:one\ntwo:two");

        editor.lines = split_lines("CAT Cat cat");
        editor.search = "(?i)cat".to_owned();
        editor.replacement = "dog".to_owned();
        editor.preserve_style = true;
        update(&mut editor, command("near.editor.replace-all"));
        assert_eq!(editor.document_text(), "DOG Dog dog");

        let before = editor.document_text();
        editor.search = "(".to_owned();
        update(&mut editor, command("near.editor.replace-all"));
        assert_eq!(editor.document_text(), before);
        assert!(editor.message.contains("Invalid search"));
    }

    #[test]
    fn find_all_results_are_navigable_and_activate_source_positions() {
        let provider = Arc::new(MemoryProvider {
            bytes: Mutex::new(b"hit first\nnone\nhit second\nhit third".to_vec()),
        });
        let mut editor = EditorSurface::open(
            "editor",
            "Document",
            provider,
            resource(),
            CancellationToken::default(),
        )
        .unwrap();
        editor.search = "hit".to_owned();
        update(&mut editor, command("near.editor.find-all"));
        assert!(editor.find_all_open);
        assert_eq!(editor.find_results.len(), 3);
        assert!(editor.find_all_text(10).contains("hit second"));

        update(&mut editor, command("near.editor.down"));
        update(&mut editor, command("near.editor.newline"));
        assert!(!editor.find_all_open);
        assert_eq!((editor.row, editor.column), (2, 0));
    }
}
