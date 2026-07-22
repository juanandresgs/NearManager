use std::collections::BTreeMap;

use near_core::{
    CorrelationId, DiagnosticDomain, DiagnosticExport, DiagnosticJournal, DiagnosticPhase,
};
use near_runtime::TaskHandle;

use crate::FarWorkspace;

#[derive(Default)]
pub(crate) struct WorkspaceDiagnostics {
    pub(crate) journal: DiagnosticJournal,
    pub(crate) active: Option<CorrelationId>,
    pub(crate) terminal: Option<CorrelationId>,
    pub(crate) tasks: BTreeMap<u64, (CorrelationId, Option<CorrelationId>)>,
}

impl FarWorkspace {
    pub fn diagnostic_export(&self) -> DiagnosticExport {
        self.diagnostics.journal.export(
            env!("CARGO_PKG_VERSION"),
            self.action_context()
                .capabilities
                .iter()
                .map(near_core::CapabilityId::as_str),
        )
    }

    pub(crate) fn record_terminal_session(&mut self, phase: DiagnosticPhase) {
        if phase == DiagnosticPhase::Started {
            self.diagnostics.terminal = Some(self.diagnostics.journal.begin(
                DiagnosticDomain::Terminal,
                "interactive-session",
                None,
            ));
            return;
        }
        let correlation = self.diagnostics.terminal.take().unwrap_or_else(|| {
            self.diagnostics
                .journal
                .begin(DiagnosticDomain::Terminal, "interactive-session", None)
        });
        self.diagnostics.journal.record(
            correlation,
            None,
            DiagnosticDomain::Terminal,
            phase,
            "interactive-session",
            BTreeMap::new(),
        );
    }

    pub(crate) fn track_task(&mut self, task: &TaskHandle, name: &str) {
        let parent = self.diagnostics.active;
        let correlation = self
            .diagnostics
            .journal
            .begin(DiagnosticDomain::Task, name, parent);
        self.diagnostics
            .tasks
            .insert(task.id().0, (correlation, parent));
    }
}
