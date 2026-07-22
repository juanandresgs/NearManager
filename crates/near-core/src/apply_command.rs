use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ApplyCommandMode {
    #[default]
    Sequential,
    Batch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyCommandTarget {
    pub label: String,
    pub resource_argument: String,
    pub name_argument: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TemplateSegment {
    Literal(String),
    Resource,
    Resources,
    Name,
    Panel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyCommandTemplate {
    source: String,
    segments: Vec<TemplateSegment>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ApplyCommandTemplateError {
    #[error("command template is empty")]
    Empty,
    #[error("unclosed template placeholder")]
    UnclosedPlaceholder,
    #[error("unknown template placeholder {{{0}}}")]
    UnknownPlaceholder(String),
    #[error("batch mode requires {{resources}} instead of {{{0}}}")]
    SequentialPlaceholderInBatch(&'static str),
}

impl ApplyCommandTemplate {
    /// Parses a command template with explicit resource placeholders.
    ///
    /// # Errors
    ///
    /// Returns an error for empty templates, unclosed braces, or unknown placeholders.
    pub fn parse(source: impl Into<String>) -> Result<Self, ApplyCommandTemplateError> {
        let source = source.into();
        if source.trim().is_empty() {
            return Err(ApplyCommandTemplateError::Empty);
        }
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut chars = source.chars().peekable();
        while let Some(character) = chars.next() {
            match character {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    if !literal.is_empty() {
                        segments.push(TemplateSegment::Literal(std::mem::take(&mut literal)));
                    }
                    let mut placeholder = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(value) => placeholder.push(value),
                            None => return Err(ApplyCommandTemplateError::UnclosedPlaceholder),
                        }
                    }
                    segments.push(match placeholder.as_str() {
                        "resource" => TemplateSegment::Resource,
                        "resources" => TemplateSegment::Resources,
                        "name" => TemplateSegment::Name,
                        "panel" => TemplateSegment::Panel,
                        _ => {
                            return Err(ApplyCommandTemplateError::UnknownPlaceholder(placeholder));
                        }
                    });
                }
                value => literal.push(value),
            }
        }
        if !literal.is_empty() {
            segments.push(TemplateSegment::Literal(literal));
        }
        Ok(Self { source, segments })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn has_resources_placeholder(&self) -> bool {
        self.segments
            .iter()
            .any(|segment| matches!(segment, TemplateSegment::Resources))
    }

    pub fn expand_sequential(&self, target: &ApplyCommandTarget, panel_argument: &str) -> String {
        self.render(
            &target.resource_argument,
            &target.resource_argument,
            &target.name_argument,
            panel_argument,
        )
    }

    /// Expands a single batch command using all resource arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when a per-resource placeholder is used in batch mode.
    pub fn expand_batch(
        &self,
        targets: &[ApplyCommandTarget],
        panel_argument: &str,
    ) -> Result<String, ApplyCommandTemplateError> {
        let resources = targets
            .iter()
            .map(|target| target.resource_argument.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for segment in &self.segments {
            match segment {
                TemplateSegment::Resource => {
                    return Err(ApplyCommandTemplateError::SequentialPlaceholderInBatch(
                        "resource",
                    ));
                }
                TemplateSegment::Name => {
                    return Err(ApplyCommandTemplateError::SequentialPlaceholderInBatch(
                        "name",
                    ));
                }
                _ => {}
            }
        }
        Ok(self.render("", &resources, "", panel_argument))
    }

    fn render(&self, resource: &str, resources: &str, name: &str, panel: &str) -> String {
        let mut command = String::new();
        for segment in &self.segments {
            match segment {
                TemplateSegment::Literal(value) => command.push_str(value),
                TemplateSegment::Resource => command.push_str(resource),
                TemplateSegment::Resources => command.push_str(resources),
                TemplateSegment::Name => command.push_str(name),
                TemplateSegment::Panel => command.push_str(panel),
            }
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplyCommandTarget, ApplyCommandTemplate, ApplyCommandTemplateError};

    fn target(label: &str, resource: &str, name: &str) -> ApplyCommandTarget {
        ApplyCommandTarget {
            label: label.to_owned(),
            resource_argument: resource.to_owned(),
            name_argument: name.to_owned(),
        }
    }

    #[test]
    fn sequential_expansion_preserves_prequoted_arguments() {
        let template = ApplyCommandTemplate::parse(
            "tool --root {panel} --input {resource} --name {name} --literal {{ok}}",
        )
        .unwrap();
        let command =
            template.expand_sequential(&target("a b.txt", "'/tmp/a b.txt'", "'a b.txt'"), "'/tmp'");
        assert_eq!(
            command,
            "tool --root '/tmp' --input '/tmp/a b.txt' --name 'a b.txt' --literal {ok}"
        );
    }

    #[test]
    fn batch_expansion_uses_one_structured_argument_per_target() {
        let template = ApplyCommandTemplate::parse("tool {resources}").unwrap();
        let command = template
            .expand_batch(
                &[
                    target("one", "'/tmp/one file'", "one"),
                    target("two", "'/tmp/two file'", "two"),
                ],
                "'/tmp'",
            )
            .unwrap();
        assert_eq!(command, "tool '/tmp/one file' '/tmp/two file'");
    }

    #[test]
    fn batch_expansion_rejects_ambiguous_per_item_placeholders() {
        let template = ApplyCommandTemplate::parse("tool {resource}").unwrap();
        assert_eq!(
            template.expand_batch(&[target("one", "one", "one")], "/tmp"),
            Err(ApplyCommandTemplateError::SequentialPlaceholderInBatch(
                "resource"
            ))
        );
    }
}
