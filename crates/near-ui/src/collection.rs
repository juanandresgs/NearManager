#![allow(
    clippy::cast_precision_loss,
    clippy::obfuscated_if_else,
    clippy::struct_excessive_bools,
    clippy::missing_panics_doc
)]

use std::{cell::Cell, cmp::Ordering, collections::HashMap};

use near_core::{
    CapabilitySet, CommandValue, ContextId, Location, MetadataValue, ResourceKind,
    ResourceMetadata, ResourceRef, SurfaceId,
};
use unicode_width::UnicodeWidthChar;

use crate::interaction_kernel::{
    CollectionInteractionEffect, CollectionInteractionModel, CollectionInteractionMsg,
    update_collection_interaction,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortMode {
    Unsorted,
    #[default]
    Name,
    Extension,
    Modified,
    Size,
    Created,
    Accessed,
    Kind,
    Owner,
    Permissions,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollectionTargetScope {
    #[default]
    SelectionOrCurrent,
    CurrentOnly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollectionLookupMode {
    #[default]
    Prefix,
    Contains,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CollectionLookup {
    query: String,
    mode: CollectionLookupMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LookupSpan {
    start_byte: usize,
    end_byte: usize,
    start_columns: usize,
    width: usize,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unsorted => "Unsorted",
            Self::Name => "Name",
            Self::Extension => "Extension",
            Self::Modified => "Modified",
            Self::Size => "Size",
            Self::Created => "Created",
            Self::Accessed => "Accessed",
            Self::Kind => "Kind",
            Self::Owner => "Owner",
            Self::Permissions => "Permissions",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortState {
    pub mode: SortMode,
    pub reverse: bool,
    pub numeric: bool,
    pub selected_first: bool,
    pub directories_first: bool,
    pub sort_groups: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            mode: SortMode::Unsorted,
            reverse: false,
            numeric: false,
            selected_first: false,
            directories_first: false,
            sort_groups: false,
        }
    }
}

impl SortState {
    pub fn indicator(self) -> String {
        let direction = if self.reverse { "↓" } else { "↑" };
        let mut modifiers = String::new();
        if self.numeric {
            modifiers.push('N');
        }
        if self.selected_first {
            modifiers.push('S');
        }
        if self.directories_first {
            modifiers.push('D');
        }
        if self.sort_groups {
            modifiers.push('G');
        }
        if modifiers.is_empty() {
            format!("{} {direction}", self.mode.label())
        } else {
            format!("{} {direction} {modifiers}", self.mode.label())
        }
    }
}

use crate::{
    ColumnAlignment, FileDecoration, HighlightingCatalog, PanelColumn, PanelColumnKind,
    PanelModeCatalog, PanelViewMode, RenderContext, Scene, SceneRect, Surface, SurfaceEvent,
    SurfaceState, TextAlignment, UpdateContext, UpdateResult,
};

const SELECTION_DENIAL_EXTENSION: &str = "near.ui.selection-denial";
const SORT_PRIORITY_EXTENSION: &str = "near.ui.sort-priority";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionEntry {
    pub resource: ResourceRef,
    pub metadata: ResourceMetadata,
    pub details: String,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonSelection {
    NewerOrUnique,
    BothDiffering,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FolderComparisonPolicy {
    pub case_sensitive_names: bool,
    pub compare_size: bool,
    pub compare_modified: bool,
    pub timestamp_tolerance_ms: u64,
    pub selection: ComparisonSelection,
}

impl Default for FolderComparisonPolicy {
    fn default() -> Self {
        Self {
            case_sensitive_names: false,
            compare_size: true,
            compare_modified: true,
            timestamp_tolerance_ms: 0,
            selection: ComparisonSelection::NewerOrUnique,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FolderComparisonResult {
    pub left: Vec<ResourceRef>,
    pub right: Vec<ResourceRef>,
    pub unique_left: usize,
    pub unique_right: usize,
    pub differing_pairs: usize,
    pub equal_pairs: usize,
}

impl FolderComparisonResult {
    pub fn selected_count(&self) -> usize {
        self.left.len() + self.right.len()
    }
}

impl CollectionEntry {
    pub fn new(
        resource: ResourceRef,
        metadata: ResourceMetadata,
        details: impl Into<String>,
    ) -> Self {
        Self {
            resource,
            metadata,
            details: details.into(),
            selected: false,
        }
    }

    #[must_use]
    pub fn with_selection_denial(mut self, reason: impl Into<String>) -> Self {
        self.metadata.extensions.insert(
            SELECTION_DENIAL_EXTENSION.to_owned(),
            MetadataValue::String(reason.into()),
        );
        self.selected = false;
        self
    }

    pub fn selection_denial(&self) -> Option<&str> {
        match self.metadata.extensions.get(SELECTION_DENIAL_EXTENSION) {
            Some(MetadataValue::String(reason)) => Some(reason),
            _ => None,
        }
    }

    pub fn is_selectable(&self) -> bool {
        self.selection_denial().is_none()
    }

    #[must_use]
    pub fn with_sort_priority(mut self, priority: i64) -> Self {
        self.metadata.extensions.insert(
            SORT_PRIORITY_EXTENSION.to_owned(),
            MetadataValue::Integer(priority),
        );
        self
    }

    pub fn sort_priority(&self) -> i64 {
        match self.metadata.extensions.get(SORT_PRIORITY_EXTENSION) {
            Some(MetadataValue::Integer(priority)) => *priority,
            _ => 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct CollectionViewport {
    start: Cell<usize>,
    visible_rows: Cell<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CollectionStateSnapshot {
    current: Option<ResourceRef>,
    selected: Vec<ResourceRef>,
    horizontal_offset: usize,
}

impl CollectionStateSnapshot {
    pub fn current(&self) -> Option<&ResourceRef> {
        self.current.as_ref()
    }

    pub fn selected(&self) -> &[ResourceRef] {
        &self.selected
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }
}

impl CollectionViewport {
    pub fn start(&self) -> usize {
        self.start.get()
    }

    pub fn visible_rows(&self) -> usize {
        self.visible_rows.get().max(1)
    }

    pub fn item_at_visible_row(&self, row: usize, entry_count: usize) -> Option<usize> {
        self.start()
            .checked_add(row)
            .filter(|index| *index < entry_count)
    }

    fn reset(&self) {
        self.start.set(0);
    }

    fn ensure_cursor_visible(
        &self,
        cursor: usize,
        entry_count: usize,
        visible_rows: usize,
    ) -> usize {
        let visible_rows = visible_rows.max(1);
        self.visible_rows.set(visible_rows);
        let maximum = entry_count.saturating_sub(visible_rows);
        let mut start = self.start.get().min(maximum);
        if cursor < start {
            start = cursor;
        } else if cursor >= start.saturating_add(visible_rows) {
            start = cursor.saturating_add(1).saturating_sub(visible_rows);
        }
        self.start.set(start);
        start
    }
}

pub struct CollectionSurface {
    id: SurfaceId,
    context: ContextId,
    title: String,
    location: Location,
    entries: Vec<CollectionEntry>,
    source_order: HashMap<String, usize>,
    cursor: usize,
    viewport: CollectionViewport,
    horizontal_offset: Cell<usize>,
    horizontal_limit: Cell<usize>,
    maximum_name_width: usize,
    maximum_description_width: usize,
    capabilities: CapabilitySet,
    sort: SortState,
    view_mode: PanelViewMode,
    highlighting: HighlightingCatalog,
    filter_active: bool,
    lookup: Option<CollectionLookup>,
}

impl CollectionSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        context: impl Into<ContextId>,
        title: impl Into<String>,
        location: Location,
        entries: Vec<CollectionEntry>,
    ) -> Self {
        let source_order = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.resource.to_string(), index))
            .collect();
        let (maximum_name_width, maximum_description_width) = collection_scroll_widths(&entries);
        let mut surface = Self {
            id: id.into(),
            context: context.into(),
            title: title.into(),
            location,
            entries,
            source_order,
            cursor: 0,
            viewport: CollectionViewport::default(),
            horizontal_offset: Cell::new(0),
            horizontal_limit: Cell::new(0),
            maximum_name_width,
            maximum_description_width,
            capabilities: CapabilitySet::default(),
            sort: SortState::default(),
            view_mode: PanelModeCatalog::built_in()
                .mode("medium")
                .expect("built-in medium panel mode must exist")
                .clone(),
            highlighting: HighlightingCatalog::default(),
            filter_active: false,
            lookup: None,
        };
        surface.apply_sort();
        surface
    }

    #[must_use]
    pub fn with_cursor(mut self, cursor: usize) -> Self {
        self.cursor = cursor.min(self.entries.len().saturating_sub(1));
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn entries(&self) -> &[CollectionEntry] {
        &self.entries
    }

    pub fn current(&self) -> Option<&CollectionEntry> {
        self.entries.get(self.cursor)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.entries.len().saturating_sub(1));
    }

    pub fn set_lookup_query(&mut self, query: Option<String>) {
        self.lookup = query.map(|query| CollectionLookup {
            query,
            mode: CollectionLookupMode::Prefix,
        });
    }

    pub fn set_lookup(&mut self, query: Option<String>, mode: CollectionLookupMode) {
        self.lookup = query.map(|query| CollectionLookup { query, mode });
    }

    pub fn focus_resource(&mut self, resource: &ResourceRef) -> bool {
        let Some(cursor) = self
            .entries
            .iter()
            .position(|entry| &entry.resource == resource)
        else {
            return false;
        };
        self.cursor = cursor;
        true
    }

    pub fn sort_state(&self) -> SortState {
        self.sort
    }

    pub fn view_mode(&self) -> &PanelViewMode {
        &self.view_mode
    }

    pub fn set_view_mode(&mut self, mode: PanelViewMode) {
        self.view_mode = mode;
    }

    pub fn set_highlighting(&mut self, highlighting: HighlightingCatalog) {
        self.highlighting = highlighting;
        self.apply_sort();
    }

    pub fn set_filter_active(&mut self, active: bool) {
        self.filter_active = active;
    }

    pub fn filter_active(&self) -> bool {
        self.filter_active
    }

    pub fn highlighting_report(&self) -> String {
        self.highlighting.report()
    }

    pub fn replace(&mut self, location: Location, entries: Vec<CollectionEntry>) {
        self.location = location;
        self.entries = entries;
        self.source_order = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.resource.to_string(), index))
            .collect();
        self.cursor = 0;
        self.viewport.reset();
        (self.maximum_name_width, self.maximum_description_width) =
            collection_scroll_widths(&self.entries);
        self.apply_sort();
    }

    pub fn state_snapshot(&self) -> CollectionStateSnapshot {
        CollectionStateSnapshot {
            current: self.current().map(|entry| entry.resource.clone()),
            selected: self.selected_resources(),
            horizontal_offset: self.horizontal_offset(),
        }
    }

    pub fn restore_state(&mut self, snapshot: &CollectionStateSnapshot) {
        self.restore_selection(snapshot.selected());
        self.horizontal_offset.set(snapshot.horizontal_offset());
        if let Some(current) = snapshot.current()
            && let Some(cursor) = self
                .entries
                .iter()
                .position(|entry| &entry.resource == current)
        {
            self.cursor = cursor;
        }
    }

    pub fn append(&mut self, entries: impl IntoIterator<Item = CollectionEntry>) {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let (maximum_name_width, maximum_description_width) = collection_scroll_widths(&entries);
        self.maximum_name_width = self.maximum_name_width.max(maximum_name_width);
        self.maximum_description_width = self
            .maximum_description_width
            .max(maximum_description_width);
        let next = self.source_order.len();
        self.source_order.extend(
            entries
                .iter()
                .enumerate()
                .map(|(offset, entry)| (entry.resource.to_string(), next + offset)),
        );
        self.entries.extend(entries);
        self.apply_sort();
    }

    pub fn hydrate(
        &mut self,
        resource: &ResourceRef,
        result: Result<ResourceMetadata, String>,
    ) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| &entry.resource == resource)
        else {
            return false;
        };
        match result {
            Ok(metadata) => {
                entry.details = metadata.size.map_or_else(
                    || format!("{:?}", metadata.kind).to_lowercase(),
                    |size| format!("{size} B"),
                );
                entry.metadata = metadata;
            }
            Err(error) => {
                entry.metadata.field_errors.insert("stat".to_owned(), error);
                "metadata error".clone_into(&mut entry.details);
            }
        }
        (self.maximum_name_width, self.maximum_description_width) =
            collection_scroll_widths(&self.entries);
        self.apply_sort();
        true
    }

    pub fn move_cursor(&mut self, rows: isize) {
        self.apply_interaction(CollectionInteractionMsg::Move(rows));
    }

    pub fn first(&mut self) {
        self.apply_interaction(CollectionInteractionMsg::First);
    }

    pub fn last(&mut self) {
        self.apply_interaction(CollectionInteractionMsg::Last);
    }

    pub fn viewport(&self) -> &CollectionViewport {
        &self.viewport
    }

    pub fn page_cursor(&mut self, pages: isize) {
        self.apply_interaction(CollectionInteractionMsg::Page(pages));
    }

    pub fn item_at_visible_row(&self, row: usize) -> Option<usize> {
        self.viewport.item_at_visible_row(row, self.entries.len())
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset.get()
    }

    pub fn scroll_horizontal(&mut self, columns: isize) {
        self.horizontal_offset.set(
            self.horizontal_offset
                .get()
                .saturating_add_signed(columns)
                .min(self.horizontal_limit.get()),
        );
    }

    pub fn horizontal_start(&mut self) {
        self.horizontal_offset.set(0);
    }

    pub fn horizontal_end(&mut self) {
        self.horizontal_offset.set(self.horizontal_limit.get());
    }

    pub fn toggle_selection(&mut self) {
        self.apply_interaction(CollectionInteractionMsg::ToggleCurrent);
        if self.sort.selected_first {
            self.apply_sort();
        }
    }

    pub fn toggle_selection_and_move(&mut self, rows: isize) -> bool {
        let effect = self.apply_interaction(CollectionInteractionMsg::ToggleCurrent);
        if self.sort.selected_first {
            self.apply_sort();
        }
        self.apply_interaction(CollectionInteractionMsg::Move(rows));
        effect.selection_changed
    }

    pub fn select_by_masks(&mut self, include: &str, exclude: &str, selected: bool) -> usize {
        let masks = SelectionMasks::new(include, exclude);
        let mut changed = 0;
        for entry in &mut self.entries {
            if !entry.is_selectable() || !masks.matches(&entry.metadata.name) {
                continue;
            }
            if entry.selected != selected {
                entry.selected = selected;
                changed += 1;
            }
        }
        if self.sort.selected_first {
            self.apply_sort();
        }
        changed
    }

    pub fn select_same_extension(&mut self) -> usize {
        let Some(current_extension) = self
            .current()
            .filter(|entry| entry.is_selectable())
            .map(|entry| extension(&entry.metadata.name).to_lowercase())
        else {
            return 0;
        };
        self.set_matching_selection(
            |entry| extension(&entry.metadata.name).eq_ignore_ascii_case(&current_extension),
            true,
        )
    }

    pub fn select_same_name(&mut self) -> usize {
        let Some(stem) = self
            .current()
            .filter(|entry| entry.is_selectable())
            .map(|entry| file_stem(&entry.metadata.name).to_lowercase())
        else {
            return 0;
        };
        self.set_matching_selection(
            |entry| file_stem(&entry.metadata.name).eq_ignore_ascii_case(&stem),
            true,
        )
    }

    pub fn invert_selection(&mut self) -> usize {
        let mut changed = 0;
        for entry in &mut self.entries {
            if !entry.is_selectable() {
                continue;
            }
            entry.selected = !entry.selected;
            changed += 1;
        }
        if self.sort.selected_first {
            self.apply_sort();
        }
        changed
    }

    pub fn selected_resources(&self) -> Vec<ResourceRef> {
        self.entries
            .iter()
            .filter(|entry| entry.selected && entry.is_selectable())
            .map(|entry| entry.resource.clone())
            .collect()
    }

    pub fn target_resources(&self, scope: CollectionTargetScope) -> Vec<ResourceRef> {
        self.target_entries(scope)
            .into_iter()
            .map(|entry| entry.resource.clone())
            .collect()
    }

    pub fn target_entries(&self, scope: CollectionTargetScope) -> Vec<&CollectionEntry> {
        if scope == CollectionTargetScope::SelectionOrCurrent {
            let selected = self
                .entries
                .iter()
                .filter(|entry| entry.selected && entry.is_selectable())
                .collect::<Vec<_>>();
            if !selected.is_empty() {
                return selected;
            }
        }
        self.current()
            .filter(|entry| entry.is_selectable())
            .into_iter()
            .collect()
    }

    pub fn restore_selection(&mut self, resources: &[ResourceRef]) -> usize {
        let mut selected = 0;
        for entry in &mut self.entries {
            entry.selected = entry.is_selectable()
                && resources.iter().any(|resource| resource == &entry.resource);
            selected += usize::from(entry.selected);
        }
        if self.sort.selected_first {
            self.apply_sort();
        }
        selected
    }

    fn set_matching_selection(
        &mut self,
        matches: impl Fn(&CollectionEntry) -> bool,
        selected: bool,
    ) -> usize {
        let mut changed = 0;
        for entry in &mut self.entries {
            if !entry.is_selectable() || !matches(entry) {
                continue;
            }
            if entry.selected != selected {
                entry.selected = selected;
                changed += 1;
            }
        }
        if self.sort.selected_first {
            self.apply_sort();
        }
        changed
    }

    pub fn set_sort_mode(&mut self, mode: SortMode) {
        self.sort.mode = mode;
        self.apply_sort();
    }

    pub fn toggle_reverse_sort(&mut self) {
        self.sort.reverse = !self.sort.reverse;
        self.apply_sort();
    }

    pub fn toggle_numeric_sort(&mut self) {
        self.sort.numeric = !self.sort.numeric;
        self.apply_sort();
    }

    pub fn toggle_selected_first(&mut self) {
        self.sort.selected_first = !self.sort.selected_first;
        self.apply_sort();
    }

    pub fn toggle_directories_first(&mut self) {
        self.sort.directories_first = !self.sort.directories_first;
        self.apply_sort();
    }

    pub fn toggle_sort_groups(&mut self) {
        self.sort.sort_groups = !self.sort.sort_groups;
        self.apply_sort();
    }

    fn apply_sort(&mut self) {
        let current = self.current().map(|entry| entry.resource.clone());
        let state = self.sort;
        let source_order = &self.source_order;
        self.entries.sort_by(|left, right| {
            compare_entries(left, right, state, source_order, &self.highlighting)
        });
        self.cursor = current
            .and_then(|resource| {
                self.entries
                    .iter()
                    .position(|entry| entry.resource == resource)
            })
            .unwrap_or_default();
    }

    fn navigation_model(&self) -> CollectionInteractionModel {
        CollectionInteractionModel::navigation(
            self.entries.len(),
            self.cursor,
            self.viewport.start(),
            self.viewport.visible_rows(),
        )
    }

    fn apply_interaction(
        &mut self,
        message: CollectionInteractionMsg,
    ) -> CollectionInteractionEffect {
        let original_cursor = self.cursor;
        let selection_message = matches!(
            message,
            CollectionInteractionMsg::ToggleCurrent
                | CollectionInteractionMsg::ToggleCurrentAndMove(_)
        );
        let mut model = if selection_message {
            let current = self.entries.get(original_cursor);
            CollectionInteractionModel::current_item(
                self.entries.len(),
                self.cursor,
                current.is_some_and(|entry| entry.selected),
                current.is_some_and(CollectionEntry::is_selectable),
                self.viewport.start(),
                self.viewport.visible_rows(),
            )
        } else {
            self.navigation_model()
        };
        let effect = update_collection_interaction(&mut model, message);
        self.cursor = model.cursor();
        self.viewport.start.set(model.viewport_start());
        self.viewport.visible_rows.set(model.visible_rows());
        if effect.selection_changed
            && let Some(entry) = self.entries.get_mut(original_cursor)
        {
            entry.selected = model.selected().contains(&original_cursor);
        }
        effect
    }

    fn command_rows(arguments: &std::collections::BTreeMap<String, CommandValue>) -> isize {
        arguments
            .get("rows")
            .and_then(CommandValue::as_i64)
            .and_then(|rows| isize::try_from(rows).ok())
            .unwrap_or_default()
    }

    fn command_pages(arguments: &std::collections::BTreeMap<String, CommandValue>) -> isize {
        arguments
            .get("pages")
            .and_then(CommandValue::as_i64)
            .and_then(|pages| isize::try_from(pages).ok())
            .unwrap_or_default()
    }

    fn command_columns(arguments: &std::collections::BTreeMap<String, CommandValue>) -> isize {
        arguments
            .get("columns")
            .and_then(CommandValue::as_i64)
            .and_then(|columns| isize::try_from(columns).ok())
            .unwrap_or_default()
    }
}

pub fn compare_folders(
    left: &CollectionSurface,
    right: &CollectionSurface,
    policy: FolderComparisonPolicy,
) -> FolderComparisonResult {
    let mut left_by_name = HashMap::<String, Vec<&CollectionEntry>>::new();
    let mut right_by_name = HashMap::<String, Vec<&CollectionEntry>>::new();
    for entry in left.entries().iter().filter(|entry| entry.is_selectable()) {
        left_by_name
            .entry(comparison_name(
                &entry.metadata.name,
                policy.case_sensitive_names,
            ))
            .or_default()
            .push(entry);
    }
    for entry in right.entries().iter().filter(|entry| entry.is_selectable()) {
        right_by_name
            .entry(comparison_name(
                &entry.metadata.name,
                policy.case_sensitive_names,
            ))
            .or_default()
            .push(entry);
    }

    let mut result = FolderComparisonResult::default();
    let mut names = left_by_name
        .keys()
        .chain(right_by_name.keys())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    for name in names {
        match (left_by_name.get(name), right_by_name.get(name)) {
            (Some(left_entries), None) => {
                result.unique_left += left_entries.len();
                result
                    .left
                    .extend(left_entries.iter().map(|entry| entry.resource.clone()));
            }
            (None, Some(right_entries)) => {
                result.unique_right += right_entries.len();
                result
                    .right
                    .extend(right_entries.iter().map(|entry| entry.resource.clone()));
            }
            (Some(left_entries), Some(right_entries))
                if left_entries.len() != 1 || right_entries.len() != 1 =>
            {
                result.differing_pairs += 1;
                result
                    .left
                    .extend(left_entries.iter().map(|entry| entry.resource.clone()));
                result
                    .right
                    .extend(right_entries.iter().map(|entry| entry.resource.clone()));
            }
            (Some(left_entries), Some(right_entries)) => {
                let left_entry = left_entries[0];
                let right_entry = right_entries[0];
                if !entries_differ(left_entry, right_entry, policy) {
                    result.equal_pairs += 1;
                    continue;
                }
                result.differing_pairs += 1;
                select_differing_pair(&mut result, left_entry, right_entry, policy);
            }
            (None, None) => {}
        }
    }
    result
}

fn comparison_name(name: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        name.to_owned()
    } else {
        name.to_lowercase()
    }
}

fn entries_differ(
    left: &CollectionEntry,
    right: &CollectionEntry,
    policy: FolderComparisonPolicy,
) -> bool {
    if left.metadata.kind != right.metadata.kind {
        return true;
    }
    if policy.compare_size && left.metadata.size != right.metadata.size {
        return true;
    }
    policy.compare_modified
        && timestamps_differ(
            left.metadata.modified_unix_ms,
            right.metadata.modified_unix_ms,
            policy.timestamp_tolerance_ms,
        )
}

fn timestamps_differ(left: Option<i64>, right: Option<i64>, tolerance_ms: u64) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.abs_diff(right) > tolerance_ms,
        (None, None) => false,
        _ => true,
    }
}

fn select_differing_pair(
    result: &mut FolderComparisonResult,
    left: &CollectionEntry,
    right: &CollectionEntry,
    policy: FolderComparisonPolicy,
) {
    if policy.selection == ComparisonSelection::BothDiffering {
        result.left.push(left.resource.clone());
        result.right.push(right.resource.clone());
        return;
    }
    match (
        left.metadata.modified_unix_ms,
        right.metadata.modified_unix_ms,
    ) {
        (Some(left_time), Some(right_time))
            if policy.compare_modified
                && left_time.abs_diff(right_time) > policy.timestamp_tolerance_ms
                && left_time > right_time =>
        {
            result.left.push(left.resource.clone());
        }
        (Some(left_time), Some(right_time))
            if policy.compare_modified
                && left_time.abs_diff(right_time) > policy.timestamp_tolerance_ms
                && right_time > left_time =>
        {
            result.right.push(right.resource.clone());
        }
        _ => {
            result.left.push(left.resource.clone());
            result.right.push(right.resource.clone());
        }
    }
}

fn compare_entries(
    left: &CollectionEntry,
    right: &CollectionEntry,
    state: SortState,
    source_order: &HashMap<String, usize>,
    highlighting: &HighlightingCatalog,
) -> Ordering {
    let structural = left
        .sort_priority()
        .cmp(&right.sort_priority())
        .then_with(|| {
            state
                .selected_first
                .then(|| right.selected.cmp(&left.selected))
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            state
                .sort_groups
                .then(|| {
                    highlighting
                        .decoration(&right.metadata)
                        .sort_group
                        .cmp(&highlighting.decoration(&left.metadata).sort_group)
                })
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            state
                .directories_first
                .then(|| is_directory(right.metadata.kind).cmp(&is_directory(left.metadata.kind)))
                .unwrap_or(Ordering::Equal)
        });
    if structural != Ordering::Equal {
        return structural;
    }

    let primary = match state.mode {
        SortMode::Unsorted => {
            source_position(source_order, left).cmp(&source_position(source_order, right))
        }
        SortMode::Name => compare_text(&left.metadata.name, &right.metadata.name, state.numeric),
        SortMode::Extension => compare_text(
            extension(&left.metadata.name),
            extension(&right.metadata.name),
            state.numeric,
        )
        .then_with(|| compare_text(&left.metadata.name, &right.metadata.name, state.numeric)),
        SortMode::Modified => compare_optional(
            left.metadata.modified_unix_ms,
            right.metadata.modified_unix_ms,
        ),
        SortMode::Size => compare_optional(left.metadata.size, right.metadata.size),
        SortMode::Created => compare_optional(
            left.metadata.created_unix_ms,
            right.metadata.created_unix_ms,
        ),
        SortMode::Accessed => compare_optional(
            left.metadata.accessed_unix_ms,
            right.metadata.accessed_unix_ms,
        ),
        SortMode::Kind => kind_rank(left.metadata.kind).cmp(&kind_rank(right.metadata.kind)),
        SortMode::Owner => compare_text(
            owner_key(&left.metadata).as_deref().unwrap_or(""),
            owner_key(&right.metadata).as_deref().unwrap_or(""),
            state.numeric,
        ),
        SortMode::Permissions => {
            permission_key(&left.metadata).cmp(&permission_key(&right.metadata))
        }
    };
    let primary = if state.reverse {
        primary.reverse()
    } else {
        primary
    };
    primary
        .then_with(|| compare_text(&left.metadata.name, &right.metadata.name, state.numeric))
        .then_with(|| {
            source_position(source_order, left).cmp(&source_position(source_order, right))
        })
}

fn source_position(source_order: &HashMap<String, usize>, entry: &CollectionEntry) -> usize {
    source_order
        .get(&entry.resource.to_string())
        .copied()
        .unwrap_or(usize::MAX)
}

fn is_directory(kind: ResourceKind) -> bool {
    matches!(kind, ResourceKind::Directory | ResourceKind::Package)
}

fn extension(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

fn file_stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

struct SelectionMasks {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl SelectionMasks {
    fn new(include: &str, exclude: &str) -> Self {
        let (include, inline_exclude) = include.split_once('|').unwrap_or((include, ""));
        Self {
            include: parse_masks(include),
            exclude: parse_masks(&format!("{inline_exclude};{exclude}")),
        }
    }

    fn matches(&self, name: &str) -> bool {
        let included =
            self.include.is_empty() || self.include.iter().any(|mask| wildcard_matches(mask, name));
        included && !self.exclude.iter().any(|mask| wildcard_matches(mask, name))
    }
}

fn parse_masks(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|mask| !mask.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn wildcard_matches(mask: &str, name: &str) -> bool {
    let mask = mask.as_bytes();
    let name = name.to_lowercase();
    let name = name.as_bytes();
    let (mut mask_index, mut name_index) = (0, 0);
    let (mut star, mut retry_name) = (None, 0);
    while name_index < name.len() {
        if mask_index < mask.len()
            && (mask[mask_index] == b'?' || mask[mask_index] == name[name_index])
        {
            mask_index += 1;
            name_index += 1;
        } else if mask_index < mask.len() && mask[mask_index] == b'*' {
            star = Some(mask_index);
            mask_index += 1;
            retry_name = name_index;
        } else if let Some(star_index) = star {
            mask_index = star_index + 1;
            retry_name += 1;
            name_index = retry_name;
        } else {
            return false;
        }
    }
    while mask_index < mask.len() && mask[mask_index] == b'*' {
        mask_index += 1;
    }
    mask_index == mask.len()
}

fn compare_optional<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_text(left: &str, right: &str, numeric: bool) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    if !numeric {
        return left.cmp(&right);
    }
    natural_chunks(&left)
        .cmp(&natural_chunks(&right))
        .then_with(|| left.cmp(&right))
}

fn natural_chunks(value: &str) -> Vec<(bool, u128, String)> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut digits = None;
    for character in value.chars() {
        let is_digit = character.is_ascii_digit();
        if digits.is_some_and(|current_digits| current_digits != is_digit) {
            chunks.push(natural_chunk(&current, digits.unwrap_or_default()));
            current.clear();
        }
        digits = Some(is_digit);
        current.push(character);
    }
    if let Some(is_digit) = digits {
        chunks.push(natural_chunk(&current, is_digit));
    }
    chunks
}

fn natural_chunk(value: &str, digits: bool) -> (bool, u128, String) {
    if digits {
        (true, value.parse().unwrap_or(u128::MAX), value.to_owned())
    } else {
        (false, 0, value.to_owned())
    }
}

fn kind_rank(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Directory => 0,
        ResourceKind::Package => 1,
        ResourceKind::File => 2,
        ResourceKind::Symlink => 3,
        ResourceKind::Virtual => 4,
        ResourceKind::Other => 5,
        _ => 6,
    }
}

fn owner_key(metadata: &ResourceMetadata) -> Option<String> {
    metadata.owner.as_ref().map(|owner| {
        owner
            .user_name
            .clone()
            .or_else(|| owner.user_id.map(|id| id.to_string()))
            .unwrap_or_default()
    })
}

fn permission_key(metadata: &ResourceMetadata) -> (u32, bool, bool) {
    metadata
        .permissions
        .as_ref()
        .map_or((u32::MAX, false, false), |permissions| {
            (
                permissions.unix_mode.unwrap_or(u32::MAX),
                permissions.readonly,
                permissions.executable,
            )
        })
}

impl Surface for CollectionSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }

    fn contexts(&self) -> Vec<ContextId> {
        vec![self.context.clone()]
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities.clone()
    }

    fn state(&self) -> SurfaceState {
        SurfaceState {
            current: self.current().map(|entry| entry.resource.clone()),
            selected: self
                .entries
                .iter()
                .filter(|entry| entry.selected)
                .map(|entry| entry.resource.clone())
                .collect(),
            location: Some(self.location.clone()),
        }
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        let SurfaceEvent::Command(invocation) = event else {
            return UpdateResult::ignored();
        };
        match invocation.id.as_str() {
            "near.collection.move" => self.move_cursor(Self::command_rows(&invocation.arguments)),
            "near.collection.page" => self.page_cursor(Self::command_pages(&invocation.arguments)),
            "near.collection.first" => self.first(),
            "near.collection.last" => self.last(),
            "near.collection.scroll-horizontal" => {
                self.scroll_horizontal(Self::command_columns(&invocation.arguments));
            }
            "near.collection.horizontal-start" => self.horizontal_start(),
            "near.collection.horizontal-end" => self.horizontal_end(),
            "near.collection.toggle-selection" => self.toggle_selection(),
            "near.collection.toggle-selection-move" => {
                self.toggle_selection_and_move(Self::command_rows(&invocation.arguments));
            }
            _ => return UpdateResult::ignored(),
        }
        UpdateResult::handled()
    }

    #[allow(clippy::too_many_lines)]
    fn scene(&self, area: SceneRect, context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        let border_role = if context.focused {
            "panel.border.focused"
        } else {
            "panel.border"
        };
        scene.fill(area, border_role);
        scene.border(
            area,
            Some(format!(
                " {} [{}{}] [{}] ",
                self.title,
                self.sort.indicator(),
                if self.filter_active { "*" } else { "" },
                self.view_mode.label
            )),
            border_role,
        );
        let inner = area.inset(1);
        scene.fill(inner, "panel.background");
        let widths = panel_column_widths(&self.view_mode.columns, inner.width);
        let horizontal_limit = self
            .view_mode
            .columns
            .iter()
            .zip(widths.iter().copied())
            .map(|(column, width)| {
                scrollable_column_overflow(
                    column.kind,
                    width,
                    self.maximum_name_width,
                    self.maximum_description_width,
                )
            })
            .max()
            .unwrap_or_default();
        self.horizontal_limit.set(horizontal_limit);
        self.horizontal_offset
            .set(self.horizontal_offset.get().min(horizontal_limit));
        let visible_rows = usize::from(inner.height);
        let visible_start =
            self.viewport
                .ensure_cursor_visible(self.cursor, self.entries.len(), visible_rows);
        for (index, entry) in self.entries.iter().enumerate().skip(visible_start) {
            let Ok(row) = u16::try_from(index.saturating_sub(visible_start)) else {
                break;
            };
            if row >= inner.height {
                break;
            }
            let decoration = self.highlighting.decoration(&entry.metadata);
            let role = entry_role(entry, context.focused && index == self.cursor, &decoration);
            let mut column_x = inner.x;
            for (column_index, (column, width)) in self
                .view_mode
                .columns
                .iter()
                .zip(widths.iter().copied())
                .enumerate()
            {
                if width == 0 {
                    continue;
                }
                let value = panel_column_value(
                    column.kind,
                    entry,
                    &decoration,
                    self.horizontal_offset.get(),
                );
                scene.aligned_text(
                    SceneRect::new(column_x, inner.y + row, width, 1),
                    value,
                    role.clone(),
                    column_alignment(column.alignment),
                );
                if column.kind == PanelColumnKind::Name && column.alignment == ColumnAlignment::Left
                {
                    render_lookup_match(
                        &mut scene,
                        self.lookup.as_ref(),
                        entry,
                        SceneRect::new(column_x, inner.y + row, width, 1),
                        self.horizontal_offset.get(),
                        context.focused && index == self.cursor,
                    );
                }
                column_x = column_x.saturating_add(width);
                if column_index + 1 < self.view_mode.columns.len() && column_x < inner.right() {
                    column_x = column_x.saturating_add(1);
                }
            }
        }
        if area.height > 2 {
            scene.text(
                SceneRect::new(
                    area.x.saturating_add(2),
                    area.bottom().saturating_sub(1),
                    area.width.saturating_sub(4),
                    1,
                ),
                self.location.display_compact(),
                "panel.title",
            );
        }
        scene
    }
}

fn render_lookup_match(
    scene: &mut Scene,
    lookup: Option<&CollectionLookup>,
    entry: &CollectionEntry,
    area: SceneRect,
    offset: usize,
    focused: bool,
) {
    let Some(lookup) = lookup else { return };
    let Some(span) = lookup_match_span(&entry.metadata.name, &lookup.query, lookup.mode) else {
        return;
    };
    let match_end = span.start_columns.saturating_add(span.width);
    if offset >= match_end {
        return;
    }
    let clipped = offset.saturating_sub(span.start_columns);
    let visible_start = span.start_columns.saturating_sub(offset);
    let name_width = area.width.saturating_sub(3);
    let visible_width = span.width.saturating_sub(clipped);
    let matched_text = skip_display_columns(
        &entry.metadata.name[span.start_byte..span.end_byte],
        clipped,
    );
    scene.text(
        SceneRect::new(
            area.x
                .saturating_add(3)
                .saturating_add(u16::try_from(visible_start).unwrap_or(u16::MAX)),
            area.y,
            u16::try_from(visible_width)
                .unwrap_or(u16::MAX)
                .min(name_width.saturating_sub(u16::try_from(visible_start).unwrap_or(u16::MAX))),
            1,
        ),
        matched_text,
        if focused {
            "lookup.match.focused"
        } else {
            "lookup.match"
        },
    );
}

fn entry_role(entry: &CollectionEntry, focused: bool, decoration: &FileDecoration) -> String {
    if focused && entry.selected {
        "panel.item.selected.focused".to_owned()
    } else if focused {
        "panel.item.focused".to_owned()
    } else if entry.selected {
        "panel.item.selected".to_owned()
    } else if let Some(role) = &decoration.role {
        role.clone()
    } else if is_directory(entry.metadata.kind) {
        "panel.item.directory".to_owned()
    } else {
        "panel.item".to_owned()
    }
}

fn panel_column_widths(columns: &[PanelColumn], total: u16) -> Vec<u16> {
    if columns.is_empty() || total == 0 {
        return vec![0; columns.len()];
    }
    let separators = u16::try_from(columns.len().saturating_sub(1)).unwrap_or(u16::MAX);
    let available = total.saturating_sub(separators);
    let fixed = columns
        .iter()
        .filter_map(|column| column.width)
        .fold(0_u16, u16::saturating_add);
    let flexible = columns
        .iter()
        .filter(|column| column.width.is_none())
        .count();
    let flexible_space = available.saturating_sub(fixed.min(available));
    let flexible_count = u16::try_from(flexible).unwrap_or(u16::MAX).max(1);
    let base_flexible = flexible_space / flexible_count;
    let mut remainder = flexible_space % flexible_count;
    let mut remaining = available;
    let mut widths = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        let later = u16::try_from(columns.len().saturating_sub(index + 1)).unwrap_or(u16::MAX);
        let maximum = remaining.saturating_sub(later.min(remaining));
        let desired = column.width.unwrap_or_else(|| {
            let width = base_flexible.saturating_add(u16::from(remainder > 0));
            remainder = remainder.saturating_sub(1);
            width
        });
        let width = desired.min(maximum);
        widths.push(width);
        remaining = remaining.saturating_sub(width);
    }
    if remaining > 0
        && let Some(last) = widths.last_mut()
    {
        *last = last.saturating_add(remaining);
    }
    widths
}

fn panel_column_value(
    kind: PanelColumnKind,
    entry: &CollectionEntry,
    decoration: &FileDecoration,
    horizontal_offset: usize,
) -> String {
    match kind {
        PanelColumnKind::Name => format!(
            "{}{} {}",
            if entry.selected { "√" } else { " " },
            decoration.mark.as_deref().unwrap_or(" "),
            skip_display_columns(&entry.metadata.name, horizontal_offset)
        ),
        PanelColumnKind::Extension => extension(&entry.metadata.name).to_owned(),
        PanelColumnKind::Size if is_directory(entry.metadata.kind) => {
            if entry.metadata.name == ".." {
                "Up"
            } else {
                "Folder"
            }
            .to_owned()
        }
        PanelColumnKind::Size => entry
            .metadata
            .size
            .map_or_else(|| "-".to_owned(), format_size),
        PanelColumnKind::Modified => format_timestamp(entry.metadata.modified_unix_ms),
        PanelColumnKind::Created => format_timestamp(entry.metadata.created_unix_ms),
        PanelColumnKind::Accessed => format_timestamp(entry.metadata.accessed_unix_ms),
        PanelColumnKind::Kind => format!("{:?}", entry.metadata.kind).to_lowercase(),
        PanelColumnKind::Owner => entry.metadata.owner.as_ref().map_or_else(
            || "-".to_owned(),
            |owner| {
                owner
                    .user_name
                    .clone()
                    .or_else(|| owner.user_id.map(|id| id.to_string()))
                    .unwrap_or_else(|| "-".to_owned())
            },
        ),
        PanelColumnKind::Permissions => entry.metadata.permissions.as_ref().map_or_else(
            || "-".to_owned(),
            |permissions| {
                permissions.unix_mode.map_or_else(
                    || {
                        format!(
                            "{}{}",
                            if permissions.readonly { "ro" } else { "rw" },
                            if permissions.executable { "x" } else { "-" }
                        )
                    },
                    |mode| format!("{mode:04o}"),
                )
            },
        ),
        PanelColumnKind::Description => {
            let value = entry
                .metadata
                .extensions
                .get(near_core::RESOURCE_DESCRIPTION_KEY)
                .and_then(|value| match value {
                    MetadataValue::String(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| entry.details.clone());
            skip_display_columns(&value, horizontal_offset)
        }
    }
}

fn scrollable_column_overflow(
    kind: PanelColumnKind,
    width: u16,
    maximum_name_width: usize,
    maximum_description_width: usize,
) -> usize {
    let available = match kind {
        PanelColumnKind::Name => usize::from(width).saturating_sub(4),
        PanelColumnKind::Description => usize::from(width),
        _ => return 0,
    };
    match kind {
        PanelColumnKind::Name => maximum_name_width,
        PanelColumnKind::Description => maximum_description_width,
        _ => 0,
    }
    .saturating_sub(available)
}

fn collection_scroll_widths(entries: &[CollectionEntry]) -> (usize, usize) {
    entries
        .iter()
        .fold((0, 0), |(name_width, description_width), entry| {
            let description = entry
                .metadata
                .extensions
                .get(near_core::RESOURCE_DESCRIPTION_KEY)
                .and_then(|value| match value {
                    MetadataValue::String(value) => Some(value.as_str()),
                    _ => None,
                });
            (
                name_width.max(display_width(&entry.metadata.name)),
                description_width
                    .max(description.map_or_else(|| display_width(&entry.details), display_width)),
            )
        })
}

fn display_width(value: &str) -> usize {
    value
        .chars()
        .map(|character| character.width().unwrap_or_default())
        .sum()
}

pub(crate) fn name_matches_lookup(value: &str, query: &str, mode: CollectionLookupMode) -> bool {
    lookup_match_span(value, query, mode).is_some()
}

fn lookup_match_span(value: &str, query: &str, mode: CollectionLookupMode) -> Option<LookupSpan> {
    let folded_query = query
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if folded_query.is_empty() {
        return None;
    }
    for (start_byte, _) in value.char_indices() {
        if mode == CollectionLookupMode::Prefix && start_byte != 0 {
            break;
        }
        let mut folded = String::new();
        for (relative_byte, character) in value[start_byte..].char_indices() {
            folded.extend(character.to_lowercase());
            if folded == folded_query {
                let end_byte = start_byte + relative_byte + character.len_utf8();
                return Some(LookupSpan {
                    start_byte,
                    end_byte,
                    start_columns: display_width(&value[..start_byte]),
                    width: display_width(&value[start_byte..end_byte]),
                });
            }
            if !folded_query.starts_with(&folded) {
                break;
            }
        }
    }
    None
}

fn skip_display_columns(value: &str, columns: usize) -> String {
    let mut skipped = 0;
    value
        .chars()
        .skip_while(|character| {
            if skipped >= columns {
                return false;
            }
            skipped = skipped.saturating_add(character.width().unwrap_or_default());
            true
        })
        .collect()
}

fn column_alignment(alignment: ColumnAlignment) -> TextAlignment {
    match alignment {
        ColumnAlignment::Left => TextAlignment::Left,
        ColumnAlignment::Center => TextAlignment::Center,
        ColumnAlignment::Right => TextAlignment::Right,
    }
}

fn format_timestamp(value: Option<i64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

fn format_size(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use near_core::{
        ActionContext, CommandId, CommandInvocation, CommandValue, Location, ProviderId,
        ResourceKind, ResourceMetadata, ResourceRef,
    };

    use crate::{
        CollectionEntry, CollectionLookupMode, CollectionSurface, ColumnAlignment,
        ComparisonSelection, FolderComparisonPolicy, HighlightingCatalog, PanelColumn,
        PanelColumnKind, PanelViewMode, RenderContext, ScenePrimitive, SceneRect, SemanticTheme,
        SortMode, Surface, SurfaceEvent, UpdateContext, compare_folders,
    };

    fn entry(name: &str) -> CollectionEntry {
        CollectionEntry::new(
            ResourceRef {
                provider: ProviderId::from("test"),
                location: Location::new(format!("/{name}")),
            },
            ResourceMetadata {
                name: name.to_owned(),
                kind: ResourceKind::Virtual,
                size: None,
                modified_unix_ms: None,
                ..ResourceMetadata::default()
            },
            "record",
        )
    }

    #[test]
    fn contains_lookup_renders_only_the_original_internal_span() {
        assert!(!super::name_matches_lookup(
            "docs",
            "ocs",
            CollectionLookupMode::Prefix
        ));
        let mut surface = CollectionSurface::new(
            "test.lookup",
            "test.collection",
            "Lookup",
            Location::new("test://lookup"),
            vec![entry("docs")],
        );
        surface.set_lookup(Some("ocs".to_owned()), CollectionLookupMode::Contains);
        let scene = surface.scene(
            SceneRect::new(0, 0, 40, 5),
            &RenderContext {
                focused: true,
                action: &ActionContext::default(),
            },
        );
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { area, content, role, .. }
                if area.width == 3 && content == "ocs" && role.as_str() == "lookup.match.focused"
        )));
    }

    #[test]
    fn collection_state_snapshot_restores_surviving_focus_and_selection() {
        let mut surface = CollectionSurface::new(
            "test.collection",
            "test.collection",
            "Collection",
            Location::new("test://collection"),
            vec![entry("alpha"), entry("beta"), entry("gamma")],
        );
        surface.set_cursor(1);
        surface.toggle_selection();
        surface.set_cursor(2);
        surface.toggle_selection();
        let snapshot = surface.state_snapshot();

        surface.replace(
            Location::new("test://collection"),
            vec![entry("gamma"), entry("beta"), entry("delta")],
        );
        surface.restore_state(&snapshot);

        assert_eq!(surface.current().unwrap().metadata.name, "gamma");
        assert_eq!(
            surface
                .selected_resources()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["test:/gamma", "test:/beta"]
        );
    }

    #[test]
    fn focus_resource_moves_to_exact_provider_identity_and_reports_absence() {
        let mut surface = CollectionSurface::new(
            "test.focus-resource",
            "test.collection",
            "Collection",
            Location::new("test://collection"),
            vec![entry("alpha"), entry("beta"), entry("gamma")],
        );
        let gamma = surface.entries()[2].resource.clone();
        let missing = ResourceRef {
            provider: ProviderId::from("test"),
            location: Location::new("/missing"),
        };

        assert!(surface.focus_resource(&gamma));
        assert_eq!(surface.current().unwrap().resource, gamma);
        assert!(!surface.focus_resource(&missing));
        assert_eq!(surface.current().unwrap().metadata.name, "gamma");
    }

    #[test]
    fn collection_target_scope_distinguishes_selection_from_current_only() {
        let mut surface = CollectionSurface::new(
            "test.targets",
            "test.collection",
            "Targets",
            Location::new("test://targets"),
            vec![entry("alpha"), entry("beta"), entry("gamma")],
        );
        surface.set_cursor(0);
        surface.toggle_selection();
        surface.set_cursor(2);
        surface.toggle_selection();
        surface.set_cursor(1);

        assert_eq!(
            surface
                .target_resources(crate::CollectionTargetScope::SelectionOrCurrent)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["test:/alpha", "test:/gamma"]
        );
        assert_eq!(
            surface
                .target_resources(crate::CollectionTargetScope::CurrentOnly)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["test:/beta"]
        );
    }

    fn rich_entry(
        name: &str,
        kind: ResourceKind,
        size: u64,
        modified: i64,
        owner: &str,
        mode: u32,
    ) -> CollectionEntry {
        let mut entry = entry(name);
        entry.metadata.kind = kind;
        entry.metadata.size = Some(size);
        entry.metadata.modified_unix_ms = Some(modified);
        entry.metadata.created_unix_ms = Some(modified + 10);
        entry.metadata.accessed_unix_ms = Some(modified + 20);
        entry.metadata.owner = Some(near_core::OwnerSummary {
            user_id: None,
            group_id: None,
            user_name: Some(owner.to_owned()),
            group_name: None,
        });
        entry.metadata.permissions = Some(near_core::PermissionSummary {
            unix_mode: Some(mode),
            readonly: false,
            executable: mode & 0o111 != 0,
        });
        entry
    }

    fn names(surface: &CollectionSurface) -> Vec<&str> {
        surface
            .entries()
            .iter()
            .map(|entry| entry.metadata.name.as_str())
            .collect()
    }

    fn folder(location: &str, entries: Vec<CollectionEntry>) -> CollectionSurface {
        CollectionSurface::new(
            format!("test.{location}"),
            "test.context",
            location,
            Location::new(location),
            entries,
        )
    }

    #[test]
    fn highlighting_marks_roles_and_sort_groups_apply_without_hiding_focus() {
        let catalog = HighlightingCatalog::from_toml(
            r#"
schema = 1
[[rules]]
id = "large"
priority = 10
role = "highlight.large"
mark = "L"
sort_group = 20
[rules.predicate]
schema_version = 1
minimum_size = 100
hidden = "include"
ignore = "none"
"#,
        )
        .unwrap();
        let mut small = rich_entry("small.txt", ResourceKind::File, 1, 0, "owner", 0o644);
        let large = rich_entry("large.txt", ResourceKind::File, 200, 0, "owner", 0o644);
        small.selected = true;
        let mut surface = folder("/highlight", vec![small, large]);
        surface.set_highlighting(catalog);
        surface.toggle_sort_groups();

        assert_eq!(names(&surface), ["large.txt", "small.txt"]);
        let action = ActionContext::default();
        let scene = surface.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: false,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { content, role, .. }
                if content.contains("L large.txt") && role.as_str() == "highlight.large"
        )));
        let focused = surface.scene(
            SceneRect::new(0, 0, 60, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(focused.primitives().iter().any(|primitive| matches!(
            primitive,
            ScenePrimitive::Text { role, .. }
                if role.as_str() == "panel.item.selected.focused"
        )));
    }

    #[test]
    fn folder_comparison_selects_unique_and_newer_resources_without_mutation() {
        let left = folder(
            "/left",
            vec![
                rich_entry("same.txt", ResourceKind::File, 10, 100, "owner", 0o644),
                rich_entry("changed.txt", ResourceKind::File, 20, 300, "owner", 0o644),
                rich_entry("left.txt", ResourceKind::File, 1, 100, "owner", 0o644),
            ],
        );
        let right = folder(
            "/right",
            vec![
                rich_entry("same.txt", ResourceKind::File, 10, 100, "owner", 0o644),
                rich_entry("changed.txt", ResourceKind::File, 15, 200, "owner", 0o644),
                rich_entry("right.txt", ResourceKind::File, 1, 100, "owner", 0o644),
            ],
        );

        let result = compare_folders(&left, &right, FolderComparisonPolicy::default());

        assert_eq!(result.unique_left, 1);
        assert_eq!(result.unique_right, 1);
        assert_eq!(result.differing_pairs, 1);
        assert_eq!(result.equal_pairs, 1);
        assert_eq!(result.left.len(), 2);
        assert_eq!(result.right.len(), 1);
        assert!(
            result
                .left
                .iter()
                .any(|resource| resource.location.as_str() == "/changed.txt")
        );
        assert!(left.selected_resources().is_empty());
        assert!(right.selected_resources().is_empty());
    }

    #[test]
    fn folder_comparison_policy_controls_case_tolerance_and_selection() {
        let left = folder(
            "/left",
            vec![rich_entry(
                "Report.txt",
                ResourceKind::File,
                20,
                1_000,
                "owner",
                0o644,
            )],
        );
        let right = folder(
            "/right",
            vec![rich_entry(
                "report.txt",
                ResourceKind::File,
                10,
                2_500,
                "owner",
                0o644,
            )],
        );
        let tolerant = compare_folders(
            &left,
            &right,
            FolderComparisonPolicy {
                compare_size: false,
                timestamp_tolerance_ms: 2_000,
                ..FolderComparisonPolicy::default()
            },
        );
        assert_eq!(tolerant.equal_pairs, 1);
        assert_eq!(tolerant.selected_count(), 0);

        let size_only = compare_folders(
            &left,
            &right,
            FolderComparisonPolicy {
                compare_modified: false,
                ..FolderComparisonPolicy::default()
            },
        );
        assert_eq!(size_only.differing_pairs, 1);
        assert_eq!(size_only.left.len(), 1);
        assert_eq!(size_only.right.len(), 1);

        let strict = compare_folders(
            &left,
            &right,
            FolderComparisonPolicy {
                case_sensitive_names: true,
                selection: ComparisonSelection::BothDiffering,
                ..FolderComparisonPolicy::default()
            },
        );
        assert_eq!(strict.unique_left, 1);
        assert_eq!(strict.unique_right, 1);
        assert_eq!(strict.selected_count(), 2);
    }

    #[test]
    fn commands_update_reusable_collection_model() {
        let mut surface = CollectionSurface::new(
            "test.collection",
            "test.context",
            "Records",
            Location::new("test://records"),
            vec![entry("one"), entry("two")],
        );
        let action = ActionContext::default();
        let move_command = CommandInvocation {
            id: CommandId::from("near.collection.move"),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        };
        assert!(
            surface
                .update(
                    &SurfaceEvent::Command(move_command),
                    &mut UpdateContext { action: &action }
                )
                .handled
        );
        assert_eq!(surface.current().unwrap().metadata.name, "two");

        let select = CommandInvocation {
            id: CommandId::from("near.collection.toggle-selection"),
            arguments: BTreeMap::new(),
        };
        surface.update(
            &SurfaceEvent::Command(select),
            &mut UpdateContext { action: &action },
        );
        let state = surface.state();
        assert_eq!(state.selected.len(), 1);
        assert_eq!(state.location.unwrap().as_str(), "test://records");
    }

    #[test]
    fn reusable_viewport_owns_paging_visibility_and_hit_testing() {
        let mut surface = CollectionSurface::new(
            "test.viewport",
            "test.context",
            "Records",
            Location::new("test://records"),
            (0..64)
                .map(|index| entry(&format!("item-{index:02}")))
                .collect(),
        );
        let action = ActionContext::default();
        surface.scene(
            crate::SceneRect::new(0, 0, 80, 12),
            &crate::RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert_eq!(surface.viewport().visible_rows(), 10);

        surface.page_cursor(1);
        assert_eq!(surface.cursor(), 10);
        surface.scene(
            crate::SceneRect::new(0, 0, 80, 12),
            &crate::RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert_eq!(surface.viewport().start(), 10);
        assert_eq!(surface.item_at_visible_row(0), Some(10));
        assert_eq!(surface.item_at_visible_row(9), Some(19));

        surface.move_cursor(-1);
        surface.scene(
            crate::SceneRect::new(0, 0, 80, 12),
            &crate::RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert_eq!(surface.cursor(), 9);
        assert_eq!(surface.viewport().start(), 9);

        surface.last();
        surface.scene(
            crate::SceneRect::new(0, 0, 80, 8),
            &crate::RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert_eq!(surface.viewport().visible_rows(), 6);
        assert_eq!(surface.item_at_visible_row(5), Some(63));
        assert_eq!(surface.item_at_visible_row(6), None);
    }

    #[test]
    fn reusable_collection_horizontal_scroll_preserves_marks_and_unicode_boundaries() {
        let mut surface = CollectionSurface::new(
            "test.horizontal",
            "test.context",
            "Horizontal",
            Location::new("test://horizontal"),
            vec![entry("alpha-界-bravo-charlie-delta.txt")],
        );
        surface.set_view_mode(PanelViewMode {
            id: "names".to_owned(),
            label: "Names".to_owned(),
            columns: vec![PanelColumn {
                kind: PanelColumnKind::Name,
                width: None,
                alignment: ColumnAlignment::Left,
            }],
        });
        let action = ActionContext::default();
        let theme = SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap();
        let area = crate::SceneRect::new(0, 0, 20, 4);
        let initial = crate::snapshot_scene(
            &surface.scene(
                area,
                &crate::RenderContext {
                    focused: true,
                    action: &action,
                },
            ),
            &theme,
            20,
            4,
        )
        .text_lines()
        .join("\n");
        assert!(initial.contains("alpha-界"));

        surface.horizontal_end();
        let shifted = crate::snapshot_scene(
            &surface.scene(
                area,
                &crate::RenderContext {
                    focused: true,
                    action: &action,
                },
            ),
            &theme,
            20,
            4,
        )
        .text_lines()
        .join("\n");
        assert!(surface.horizontal_offset() > 0);
        assert!(shifted.contains("delta.txt"));
        assert!(!shifted.contains("alpha-界"));

        surface.horizontal_start();
        assert_eq!(surface.horizontal_offset(), 0);
    }

    #[test]
    fn custom_columns_render_metadata_with_requested_alignment_and_widths() {
        let mut surface = CollectionSurface::new(
            "test.columns",
            "test.context",
            "Columns",
            Location::new("/columns"),
            vec![rich_entry(
                "report.txt",
                ResourceKind::File,
                2048,
                1_700_000_000_000,
                "alex",
                0o640,
            )],
        );
        surface.set_view_mode(PanelViewMode {
            id: "custom".to_owned(),
            label: "Custom".to_owned(),
            columns: vec![
                PanelColumn {
                    kind: PanelColumnKind::Name,
                    width: None,
                    alignment: ColumnAlignment::Left,
                },
                PanelColumn {
                    kind: PanelColumnKind::Size,
                    width: Some(8),
                    alignment: ColumnAlignment::Right,
                },
                PanelColumn {
                    kind: PanelColumnKind::Owner,
                    width: Some(8),
                    alignment: ColumnAlignment::Center,
                },
                PanelColumn {
                    kind: PanelColumnKind::Permissions,
                    width: Some(6),
                    alignment: ColumnAlignment::Right,
                },
            ],
        });
        let scene = surface.scene(
            crate::SceneRect::new(0, 0, 64, 5),
            &crate::RenderContext {
                focused: true,
                action: &ActionContext::default(),
            },
        );
        let theme = SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap();
        let snapshot = crate::snapshot_scene(&scene, &theme, 64, 5)
            .text_lines()
            .join("\n");
        assert!(snapshot.contains("Custom"));
        assert!(snapshot.contains("report.txt"));
        assert!(snapshot.contains("2.0 K"));
        assert!(snapshot.contains("alex"));
        assert!(snapshot.contains("0640"));
    }

    #[test]
    fn rendering_virtualizes_large_collections_to_the_viewport() {
        let entries = (0..100_000)
            .map(|index| entry(&format!("item-{index}")))
            .collect();
        let surface = CollectionSurface::new(
            "test.large",
            "test.context",
            "Large",
            Location::new("test://large"),
            entries,
        );
        let action = ActionContext::default();
        let scene = surface.scene(
            crate::SceneRect::new(0, 0, 80, 24),
            &crate::RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().len() < 60);

        let mut samples = Vec::new();
        for _ in 0..100 {
            let started = Instant::now();
            surface.scene(
                crate::SceneRect::new(0, 0, 80, 24),
                &crate::RenderContext {
                    focused: true,
                    action: &action,
                },
            );
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        assert!(samples[94] < Duration::from_millis(16));
    }

    #[test]
    fn sort_modes_cover_names_extensions_times_sizes_and_metadata() {
        let mut surface = CollectionSurface::new(
            "test.sort",
            "test.context",
            "Sort",
            Location::new("test://sort"),
            vec![
                rich_entry("file10.txt", ResourceKind::File, 30, 300, "zoe", 0o644),
                rich_entry("file2.rs", ResourceKind::Symlink, 10, 100, "amy", 0o777),
                rich_entry("file1.md", ResourceKind::Directory, 20, 200, "max", 0o600),
            ],
        );
        surface.set_sort_mode(SortMode::Name);
        assert_eq!(names(&surface), ["file1.md", "file10.txt", "file2.rs"]);
        surface.toggle_numeric_sort();
        assert_eq!(names(&surface), ["file1.md", "file2.rs", "file10.txt"]);
        surface.set_sort_mode(SortMode::Extension);
        assert_eq!(names(&surface), ["file1.md", "file2.rs", "file10.txt"]);
        surface.set_sort_mode(SortMode::Modified);
        assert_eq!(names(&surface), ["file2.rs", "file1.md", "file10.txt"]);
        surface.set_sort_mode(SortMode::Size);
        assert_eq!(names(&surface), ["file2.rs", "file1.md", "file10.txt"]);
        surface.set_sort_mode(SortMode::Created);
        assert_eq!(names(&surface), ["file2.rs", "file1.md", "file10.txt"]);
        surface.set_sort_mode(SortMode::Accessed);
        assert_eq!(names(&surface), ["file2.rs", "file1.md", "file10.txt"]);
        surface.set_sort_mode(SortMode::Kind);
        assert_eq!(names(&surface), ["file1.md", "file10.txt", "file2.rs"]);
        surface.set_sort_mode(SortMode::Owner);
        assert_eq!(names(&surface), ["file2.rs", "file1.md", "file10.txt"]);
        surface.set_sort_mode(SortMode::Permissions);
        assert_eq!(names(&surface), ["file1.md", "file10.txt", "file2.rs"]);
        surface.set_sort_mode(SortMode::Unsorted);
        assert_eq!(names(&surface), ["file10.txt", "file2.rs", "file1.md"]);
    }

    #[test]
    fn ordering_modifiers_compose_and_preserve_the_current_resource() {
        let mut selected = rich_entry("selected.txt", ResourceKind::File, 2, 2, "b", 0o644);
        selected.selected = true;
        let mut surface = CollectionSurface::new(
            "test.sort-modifiers",
            "test.context",
            "Sort",
            Location::new("test://sort"),
            vec![
                rich_entry("z-dir", ResourceKind::Directory, 0, 0, "a", 0o755),
                rich_entry("plain.txt", ResourceKind::File, 1, 1, "a", 0o644),
                selected,
            ],
        );
        surface.toggle_directories_first();
        surface.set_sort_mode(SortMode::Name);
        surface.last();
        let current = surface.current().unwrap().resource.clone();
        surface.toggle_selected_first();
        surface.toggle_reverse_sort();

        assert_eq!(names(&surface), ["selected.txt", "z-dir", "plain.txt"]);
        assert_eq!(surface.current().unwrap().resource, current);
        assert_eq!(surface.sort_state().indicator(), "Name ↓ SD");
    }

    #[test]
    fn selection_masks_groups_inversion_and_restore_compose_safely() {
        let mut parent = entry("..")
            .with_selection_denial("navigation-only")
            .with_sort_priority(i64::MIN);
        parent.metadata.kind = ResourceKind::Directory;
        let mut surface = CollectionSurface::new(
            "test.selection",
            "test.context",
            "Selection",
            Location::new("test://selection"),
            vec![
                parent,
                entry("report.rs"),
                entry("report.md"),
                entry("notes.rs"),
                entry("skip.tmp"),
            ],
        );

        assert_eq!(surface.select_by_masks("*.rs;*.md", "notes*", true), 2);
        assert_eq!(
            surface
                .selected_resources()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["test:/report.rs", "test:/report.md"]
        );
        let saved = surface.selected_resources();

        surface.restore_selection(&[]);
        surface.set_cursor(1);
        assert_eq!(surface.select_same_name(), 2);
        surface.restore_selection(&[]);
        assert_eq!(surface.select_same_extension(), 2);

        surface.invert_selection();
        assert!(!surface.entries()[0].selected);
        assert!(surface.entries()[2].selected);
        assert!(surface.entries()[4].selected);

        assert_eq!(surface.restore_selection(&saved), 2);
        assert!(!surface.entries()[0].selected);
        assert!(surface.entries()[1].selected);
        assert!(surface.entries()[2].selected);
    }

    #[test]
    fn inline_exclusion_masks_match_far_syntax() {
        let mut surface = CollectionSurface::new(
            "test.selection-inline",
            "test.context",
            "Selection",
            Location::new("test://selection"),
            vec![entry("one.rs"), entry("one.test.rs"), entry("two.md")],
        );
        assert_eq!(surface.select_by_masks("*.rs|*.test.rs", "", true), 1);
        assert!(surface.entries()[0].selected);
        assert!(!surface.entries()[1].selected);
    }
}
