use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorrelationId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DiagnosticDomain {
    Command,
    Task,
    Provider,
    Operation,
    Plugin,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DiagnosticPhase {
    Started,
    Completed,
    Failed,
    Cancelled,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub correlation: CorrelationId,
    pub parent: Option<CorrelationId>,
    pub domain: DiagnosticDomain,
    pub phase: DiagnosticPhase,
    pub name: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticExport {
    pub schema: u32,
    pub near_version: String,
    pub capabilities: Vec<String>,
    pub events: Vec<DiagnosticEvent>,
}

impl DiagnosticExport {
    /// Serializes this export as stable, human-readable JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticJournal {
    next_correlation: u64,
    events: Vec<DiagnosticEvent>,
}

impl DiagnosticJournal {
    pub fn begin(
        &mut self,
        domain: DiagnosticDomain,
        name: impl Into<String>,
        parent: Option<CorrelationId>,
    ) -> CorrelationId {
        self.next_correlation = self.next_correlation.saturating_add(1);
        let correlation = CorrelationId(self.next_correlation);
        self.record(
            correlation,
            parent,
            domain,
            DiagnosticPhase::Started,
            name,
            BTreeMap::new(),
        );
        correlation
    }

    pub fn record(
        &mut self,
        correlation: CorrelationId,
        parent: Option<CorrelationId>,
        domain: DiagnosticDomain,
        phase: DiagnosticPhase,
        name: impl Into<String>,
        fields: BTreeMap<String, String>,
    ) {
        self.events.push(DiagnosticEvent {
            correlation,
            parent,
            domain,
            phase,
            name: name.into(),
            fields: redact_fields(fields),
        });
    }

    pub fn events(&self) -> &[DiagnosticEvent] {
        &self.events
    }

    pub fn export(
        &self,
        near_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> DiagnosticExport {
        DiagnosticExport {
            schema: 1,
            near_version: near_version.into(),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
            events: self.events.clone(),
        }
    }
}

fn redact_fields(fields: BTreeMap<String, String>) -> BTreeMap<String, String> {
    fields
        .into_iter()
        .map(|(key, value)| {
            let sensitive = [
                "path",
                "content",
                "token",
                "secret",
                "credential",
                "password",
            ]
            .iter()
            .any(|needle| key.to_ascii_lowercase().contains(needle));
            (
                key,
                if sensitive {
                    "<redacted>".to_owned()
                } else {
                    value
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_spans_domains_and_redacts_sensitive_fields() {
        let mut journal = DiagnosticJournal::default();
        let command = journal.begin(DiagnosticDomain::Command, "near.resource.copy", None);
        let task = journal.begin(DiagnosticDomain::Task, "copy", Some(command));
        journal.record(
            task,
            Some(command),
            DiagnosticDomain::Provider,
            DiagnosticPhase::Completed,
            "near.local-fs",
            BTreeMap::from([
                ("item-count".to_owned(), "2".to_owned()),
                ("source-path".to_owned(), "/private/user/file".to_owned()),
            ]),
        );
        let export = journal.export("0.1.0", ["resource.copy", "resource.read"]);
        assert_eq!(export.events.len(), 3);
        assert_eq!(export.events[1].parent, Some(command));
        assert_eq!(export.events[2].fields["source-path"], "<redacted>");
        assert_eq!(export.events[2].fields["item-count"], "2");
        assert_eq!(export.near_version, "0.1.0");
        assert_eq!(export.capabilities.len(), 2);
        let json = export.to_pretty_json().unwrap();
        assert!(json.contains("\"schema\": 1"));
        assert!(!json.contains("/private/user/file"));
    }
}
