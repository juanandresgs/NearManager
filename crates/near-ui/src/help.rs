#![allow(clippy::default_trait_access, clippy::needless_pass_by_value)]

use std::cell::Cell;

use near_core::{CapabilitySet, ContextId, SurfaceId};

use crate::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState, UpdateContext,
    UpdateResult,
    selection_search::{SelectionSearch, searchable_text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpEntry {
    pub keys: String,
    pub command: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpLink {
    pub label: String,
    pub target: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpTopic {
    pub id: String,
    pub title: String,
    pub introduction: String,
    pub links: Vec<HelpLink>,
    pub entries: Vec<HelpEntry>,
    pub source: String,
}

impl HelpTopic {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        introduction: impl Into<String>,
        entries: Vec<HelpEntry>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            introduction: introduction.into(),
            links: Vec::new(),
            entries,
            source: "core".to_owned(),
        }
    }

    #[must_use]
    pub fn with_links(mut self, links: Vec<HelpLink>) -> Self {
        self.links = links;
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HelpSearchResult {
    topic: String,
    title: String,
    excerpt: String,
}

pub struct HelpSurface {
    id: SurfaceId,
    topics: Vec<HelpTopic>,
    active: usize,
    history: Vec<usize>,
    scroll: usize,
    link_cursor: usize,
    searching: bool,
    query: String,
    search_results: Vec<HelpSearchResult>,
    visible_rows: Cell<usize>,
}

impl HelpSurface {
    pub fn new(
        id: impl Into<SurfaceId>,
        title: impl Into<String>,
        introduction: impl Into<String>,
        entries: Vec<HelpEntry>,
    ) -> Self {
        Self::with_topics(
            id,
            vec![HelpTopic::new("context", title, introduction, entries)],
            "context",
        )
    }

    pub fn with_topics(
        id: impl Into<SurfaceId>,
        mut topics: Vec<HelpTopic>,
        start_topic: &str,
    ) -> Self {
        if topics.is_empty() {
            topics.push(HelpTopic::new(
                "contents",
                "Help",
                "No help topics are registered.",
                Vec::new(),
            ));
        }
        let active = topics
            .iter()
            .position(|topic| topic.id == start_topic)
            .unwrap_or_default();
        Self {
            id: id.into(),
            topics,
            active,
            history: Vec::new(),
            scroll: 0,
            link_cursor: 0,
            searching: false,
            query: String::new(),
            search_results: Vec::new(),
            visible_rows: Cell::new(1),
        }
    }

    fn topic(&self) -> &HelpTopic {
        &self.topics[self.active]
    }

    fn navigate(&mut self, target: &str) -> bool {
        let Some(index) = self.topics.iter().position(|topic| topic.id == target) else {
            return false;
        };
        if index != self.active {
            self.history.push(self.active);
            self.active = index;
        }
        self.scroll = 0;
        self.link_cursor = 0;
        self.searching = false;
        true
    }

    fn back(&mut self) {
        if self.searching {
            self.searching = false;
            self.query.clear();
            self.search_results.clear();
        } else if let Some(previous) = self.history.pop() {
            self.active = previous;
            self.scroll = 0;
            self.link_cursor = 0;
        }
    }

    fn update_search(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.search_results.clear();
        if query.is_empty() {
            return;
        }
        for topic in &self.topics {
            let mut excerpt = None;
            if topic.title.to_lowercase().contains(&query)
                || topic.introduction.to_lowercase().contains(&query)
                || topic.source.to_lowercase().contains(&query)
            {
                excerpt = Some(topic.introduction.clone());
            }
            if excerpt.is_none() {
                excerpt = topic.entries.iter().find_map(|entry| {
                    let searchable =
                        format!("{} {} {}", entry.keys, entry.command, entry.description)
                            .to_lowercase();
                    searchable
                        .contains(&query)
                        .then(|| entry.description.clone())
                });
            }
            if excerpt.is_none() {
                excerpt = topic.links.iter().find_map(|link| {
                    let searchable = format!("{} {}", link.label, link.description).to_lowercase();
                    searchable
                        .contains(&query)
                        .then(|| link.description.clone())
                });
            }
            if let Some(excerpt) = excerpt {
                self.search_results.push(HelpSearchResult {
                    topic: topic.id.clone(),
                    title: topic.title.clone(),
                    excerpt,
                });
            }
        }
        self.link_cursor = self
            .link_cursor
            .min(self.search_results.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.link_cursor);
    }

    fn move_cursor(&mut self, delta: isize) {
        let length = if self.searching {
            self.search_results.len()
        } else {
            self.topic().links.len()
        };
        if length == 0 {
            return;
        }
        self.link_cursor = self
            .link_cursor
            .saturating_add_signed(delta)
            .min(length - 1);
        if self.link_cursor < self.scroll {
            self.scroll = self.link_cursor;
        }
    }

    fn move_row(&mut self, delta: isize) {
        if self.searching {
            self.move_cursor(delta);
            return;
        }
        if delta < 0 {
            if self.link_cursor > 0 {
                self.link_cursor -= 1;
            } else {
                self.scroll = self.scroll.saturating_sub(1);
            }
        } else if self.link_cursor + 1 < self.topic().links.len() {
            self.link_cursor += 1;
        } else {
            self.scroll = self
                .scroll
                .saturating_add(1)
                .min(self.topic().entries.len().saturating_sub(1));
        }
    }

    fn activate(&mut self) {
        let target = if self.searching {
            self.search_results
                .get(self.link_cursor)
                .map(|result| result.topic.clone())
        } else {
            self.topic()
                .links
                .get(self.link_cursor)
                .map(|link| link.target.clone())
        };
        if let Some(target) = target {
            self.navigate(&target);
        }
    }
}

impl Surface for HelpSurface {
    fn id(&self) -> SurfaceId {
        self.id.clone()
    }
    fn contexts(&self) -> Vec<ContextId> {
        vec![ContextId::from("surface.help")]
    }
    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }
    fn state(&self) -> SurfaceState {
        SurfaceState::default()
    }

    fn update(&mut self, event: &SurfaceEvent, _context: &mut UpdateContext<'_>) -> UpdateResult {
        match event {
            SurfaceEvent::Text(text) if self.searching => {
                self.query.push_str(text);
                self.update_search();
            }
            SurfaceEvent::Paste(text) if self.searching => {
                self.query.push_str(text);
                self.update_search();
            }
            SurfaceEvent::SelectionSearchText(text) => {
                if !self.searching {
                    self.searching = true;
                    self.query.clear();
                    self.search_results.clear();
                    self.scroll = 0;
                    self.link_cursor = 0;
                }
                self.query.push_str(text);
                self.update_search();
            }
            SurfaceEvent::SelectionSearchBackspace if self.searching => {
                self.query.pop();
                self.update_search();
            }
            SurfaceEvent::Backspace if self.searching && !self.query.is_empty() => {
                self.query.pop();
                self.update_search();
            }
            SurfaceEvent::Backspace => self.back(),
            SurfaceEvent::Command(invocation) => match invocation.id.as_str() {
                "near.help.up" => self.move_row(-1),
                "near.help.down" => self.move_row(1),
                "near.help.previous-link" => self.move_cursor(-1),
                "near.help.next-link" | "near.help.search-next" => self.move_cursor(1),
                "near.help.home" => {
                    self.scroll = 0;
                    self.link_cursor = 0;
                }
                "near.help.end" => {
                    if self.searching {
                        self.link_cursor = self.search_results.len().saturating_sub(1);
                        self.scroll = self
                            .search_results
                            .len()
                            .saturating_sub(self.visible_rows.get().max(1));
                    } else {
                        self.link_cursor = self.topic().links.len().saturating_sub(1);
                        self.scroll = self
                            .topic()
                            .entries
                            .len()
                            .saturating_sub(self.visible_rows.get().max(1));
                    }
                }
                "near.help.page-up" => {
                    let rows = self.visible_rows.get().max(1);
                    if self.searching {
                        self.move_cursor(-isize::try_from(rows).unwrap_or(isize::MAX));
                    } else {
                        self.scroll = self.scroll.saturating_sub(rows);
                    }
                }
                "near.help.page-down" => {
                    let rows = self.visible_rows.get().max(1);
                    if self.searching {
                        self.move_cursor(isize::try_from(rows).unwrap_or(isize::MAX));
                    } else {
                        self.scroll = self
                            .scroll
                            .saturating_add(rows)
                            .min(self.topic().entries.len().saturating_sub(1));
                    }
                }
                "near.help.contents" => {
                    self.navigate("contents");
                }
                "near.help.extensions" => {
                    self.navigate("extensions");
                }
                "near.help.back" if self.searching && !self.query.is_empty() => {
                    self.query.pop();
                    self.update_search();
                }
                "near.help.back" => self.back(),
                "near.help.activate" => self.activate(),
                "near.help.search" => {
                    self.searching = true;
                    self.query.clear();
                    self.search_results.clear();
                    self.scroll = 0;
                    self.link_cursor = 0;
                }
                _ => return UpdateResult::ignored(),
            },
            _ => return UpdateResult::ignored(),
        }
        UpdateResult::handled()
    }

    fn scene(&self, area: SceneRect, _context: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.fill(area, "dialog.background");
        let title = if self.searching {
            "Help Search".to_owned()
        } else {
            self.topic().title.clone()
        };
        scene.border(area, Some(format!(" {title} ")), "dialog.border");
        let inner = area.inset(1);
        self.visible_rows
            .set(usize::from(inner.height.saturating_sub(3)).max(1));
        if self.searching {
            let mut selection_search = SelectionSearch::default();
            selection_search.push(&self.query);
            scene.text(
                SceneRect::new(inner.x, inner.y, inner.width, 1),
                format!("Search: {}_", self.query),
                "control.focused",
            );
            scene.text(
                SceneRect::new(inner.x, inner.y + 1, inner.width, 1),
                format!("{} matching topics", self.search_results.len()),
                "text",
            );
            for (offset, result) in self.search_results.iter().skip(self.scroll).enumerate() {
                let Ok(row) = u16::try_from(offset) else {
                    break;
                };
                let row = row.saturating_add(3);
                if row >= inner.height {
                    break;
                }
                let index = self.scroll + offset;
                let marker = if index == self.link_cursor { ">" } else { " " };
                let content = format!("{marker} {} — {}", result.title, result.excerpt);
                searchable_text(
                    &mut scene,
                    SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                    &content,
                    if index == self.link_cursor {
                        "control.focused"
                    } else {
                        "text"
                    },
                    &selection_search,
                );
            }
            return scene;
        }

        let topic = self.topic();
        scene.text(
            SceneRect::new(inner.x, inner.y, inner.width, 2.min(inner.height)),
            topic.introduction.clone(),
            "text",
        );
        let mut row = 2_u16;
        for (index, link) in topic.links.iter().enumerate() {
            if row >= inner.height {
                break;
            }
            let marker = if index == self.link_cursor { ">" } else { " " };
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                format!("{marker} {} — {}", link.label, link.description),
                if index == self.link_cursor {
                    "control.focused"
                } else {
                    "text"
                },
            );
            row = row.saturating_add(1);
        }
        if !topic.links.is_empty() {
            row = row.saturating_add(1);
        }
        for entry in topic.entries.iter().skip(self.scroll) {
            if row >= inner.height {
                break;
            }
            scene.text(
                SceneRect::new(inner.x, inner.y + row, inner.width, 1),
                format!(
                    "{:<14} {:<30} {}",
                    entry.keys, entry.command, entry.description
                ),
                "text",
            );
            row = row.saturating_add(1);
        }
        if inner.height > 0 {
            scene.text(
                SceneRect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
                format!(
                    "Source: {}  Alt+type Search  F7 Search  Shift+F1 Contents  Alt+F1 Back",
                    topic.source
                ),
                "text",
            );
        }
        scene
    }
}

#[cfg(test)]
mod tests {
    use near_core::{ActionContext, CommandId, CommandInvocation};

    use super::*;

    fn command(id: &str) -> SurfaceEvent {
        SurfaceEvent::Command(CommandInvocation {
            id: CommandId::from(id),
            arguments: Default::default(),
        })
    }

    fn update(help: &mut HelpSurface, event: SurfaceEvent) {
        help.update(
            &event,
            &mut UpdateContext {
                action: &ActionContext::default(),
            },
        );
    }

    #[test]
    fn topics_links_search_and_back_navigation_compose() {
        let contents =
            HelpTopic::new("contents", "Contents", "All help", Vec::new()).with_links(vec![
                HelpLink {
                    label: "Editor".to_owned(),
                    target: "editor".to_owned(),
                    description: "Editing files".to_owned(),
                },
            ]);
        let editor = HelpTopic::new(
            "editor",
            "Editor",
            "Internal editor",
            vec![HelpEntry {
                keys: "F2".to_owned(),
                command: "near.editor.save".to_owned(),
                description: "Save document".to_owned(),
            }],
        );
        let mut help = HelpSurface::with_topics("help", vec![contents, editor], "contents");
        update(&mut help, command("near.help.activate"));
        assert_eq!(help.topic().id, "editor");
        update(&mut help, command("near.help.back"));
        assert_eq!(help.topic().id, "contents");
        update(&mut help, command("near.help.search"));
        update(&mut help, SurfaceEvent::Text("save".to_owned()));
        assert_eq!(help.search_results.len(), 1);
        update(&mut help, command("near.help.activate"));
        assert_eq!(help.topic().id, "editor");
    }

    #[test]
    fn surface_interaction_conformance_help_pages_home_and_end() {
        let entries = (0..20)
            .map(|index| HelpEntry {
                keys: format!("Key {index:02}"),
                command: format!("command.{index:02}"),
                description: format!("Description {index:02}"),
            })
            .collect();
        let mut help = HelpSurface::new("help", "Help", "Introduction", entries);
        let action = ActionContext::default();
        help.scene(
            SceneRect::new(0, 0, 70, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        update(&mut help, command("near.help.page-down"));
        assert!(help.scroll > 0);
        update(&mut help, command("near.help.end"));
        let scene = help.scene(
            SceneRect::new(0, 0, 70, 8),
            &RenderContext {
                focused: true,
                action: &action,
            },
        );
        assert!(scene.primitives().iter().any(|primitive| {
            matches!(primitive, crate::ScenePrimitive::Text { content, .. } if content.contains("command.19"))
        }));
        update(&mut help, command("near.help.home"));
        assert_eq!(help.scroll, 0);
    }
}
