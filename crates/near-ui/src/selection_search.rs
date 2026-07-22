use crate::{Scene, SceneRect};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SelectionSearch {
    query: String,
}

impl SelectionSearch {
    pub(crate) fn push(&mut self, text: &str) {
        self.query.push_str(text);
    }

    pub(crate) fn pop(&mut self) {
        self.query.pop();
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    pub(crate) fn matches(&self, values: impl IntoIterator<Item = impl AsRef<str>>) -> bool {
        if self.query.is_empty() {
            return true;
        }
        let query = self.query.to_lowercase();
        values
            .into_iter()
            .any(|value| value.as_ref().to_lowercase().contains(&query))
    }

    pub(crate) fn prompt(&self) -> String {
        format!("Find: {}_  Alt+Backspace edit", self.query)
    }

    pub(crate) fn segments<'a>(&self, content: &'a str) -> Vec<(&'a str, bool)> {
        if self.query.is_empty() {
            return vec![(content, false)];
        }
        let content_lower = content.to_ascii_lowercase();
        let query_lower = self.query.to_ascii_lowercase();
        let mut segments = Vec::new();
        let mut offset = 0;
        while let Some(relative) = content_lower[offset..].find(&query_lower) {
            let start = offset + relative;
            let end = start + query_lower.len();
            if start > offset {
                segments.push((&content[offset..start], false));
            }
            segments.push((&content[start..end], true));
            offset = end;
        }
        if offset < content.len() {
            segments.push((&content[offset..], false));
        }
        segments
    }
}

pub(crate) fn searchable_text(
    scene: &mut Scene,
    area: SceneRect,
    content: &str,
    role: &str,
    search: &SelectionSearch,
) {
    scene.text(area, content, role);
    let mut column = 0_u16;
    for (segment, matched) in search.segments(content) {
        let width = u16::try_from(segment.chars().count()).unwrap_or(u16::MAX);
        if matched && column < area.width {
            scene.text(
                SceneRect::new(
                    area.x.saturating_add(column),
                    area.y,
                    width.min(area.width.saturating_sub(column)),
                    1,
                ),
                segment,
                "selection.match",
            );
        }
        column = column.saturating_add(width);
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionSearch;

    #[test]
    fn segments_mark_case_insensitive_matches() {
        let mut search = SelectionSearch::default();
        search.push("tri");
        assert_eq!(
            search.segments("[A]ttributes"),
            vec![("[A]t", false), ("tri", true), ("butes", false)]
        );
    }
}
