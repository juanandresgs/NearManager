//! Immutable operation planning, execution policy, and audit journal contracts.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use near_core::{
    CancellationToken, ListingGeneration, Location, OperationId, ResourceRef, SafetyClass,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationKind {
    Copy,
    Move,
    Rename,
    HardLink,
    SymbolicLink,
    Trash,
    Restore,
    Delete,
    Wipe,
    CreateDirectory,
    Touch,
    SetAttributes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationDecision {
    Execute,
    Cancel,
    Replace,
    Skip,
    Rename,
    ConfirmHighImpact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationPresentation {
    pub title: String,
    pub confirmation: String,
    pub explanation: String,
    pub details: Vec<String>,
    pub reversible: bool,
    pub allowed_decisions: Vec<OperationDecision>,
    pub default_decision: OperationDecision,
    pub denial_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationFailurePresentation {
    pub title: String,
    pub body: String,
}

impl OperationFailurePresentation {
    pub fn execution_error(error: impl Into<String>) -> Self {
        Self {
            title: "Operation Failed".to_owned(),
            body: format!(
                "The operation could not complete.\n\n{}\n\nClose this message and inspect the retained task history.",
                error.into()
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConflictPolicy {
    Ask,
    Replace,
    Skip,
    Rename,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MetadataPolicy {
    Preserve,
    Portable,
    ContentsOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VerificationPolicy {
    None,
    SizeAndTime,
    Hash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecoveryPolicy {
    Trash,
    Backup,
    JournalOnly,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CrossDeviceBehavior {
    AtomicRename,
    CopyThenDelete,
    Reject,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SymlinkPolicy {
    Preserve,
    Follow,
    Reject,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannedItem {
    pub source: Option<ResourceRef>,
    pub target: Location,
    pub conflict_expected: bool,
    pub recursive: bool,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanPolicies {
    pub conflict: ConflictPolicy,
    pub metadata: MetadataPolicy,
    pub verification: VerificationPolicy,
    pub recovery: RecoveryPolicy,
    pub cross_device: CrossDeviceBehavior,
    pub symlink: SymlinkPolicy,
}

impl Default for PlanPolicies {
    fn default() -> Self {
        Self {
            conflict: ConflictPolicy::Ask,
            metadata: MetadataPolicy::Preserve,
            verification: VerificationPolicy::SizeAndTime,
            recovery: RecoveryPolicy::JournalOnly,
            cross_device: CrossDeviceBehavior::NotApplicable,
            symlink: SymlinkPolicy::Preserve,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationPlan {
    id: OperationId,
    kind: OperationKind,
    items: Vec<PlannedItem>,
    destination: Option<Location>,
    policies: PlanPolicies,
    safety: SafetyClass,
    context_generation: ListingGeneration,
    high_impact: bool,
}

impl OperationPlan {
    pub fn id(&self) -> &OperationId {
        &self.id
    }

    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    pub fn items(&self) -> &[PlannedItem] {
        &self.items
    }

    pub fn destination(&self) -> Option<&Location> {
        self.destination.as_ref()
    }

    pub fn policies(&self) -> &PlanPolicies {
        &self.policies
    }

    pub fn safety(&self) -> SafetyClass {
        self.safety
    }

    pub fn context_generation(&self) -> ListingGeneration {
        self.context_generation
    }

    pub fn high_impact(&self) -> bool {
        self.high_impact
    }

    pub fn conflict_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.conflict_expected)
            .count()
    }

    pub fn presentation(&self) -> OperationPresentation {
        let has_conflicts = self.conflict_count() > 0;
        let reversible = !matches!(self.policies.recovery, RecoveryPolicy::None);
        let mut allowed_decisions = vec![OperationDecision::Execute, OperationDecision::Cancel];
        if self.high_impact {
            allowed_decisions.insert(0, OperationDecision::ConfirmHighImpact);
        } else if has_conflicts && self.kind != OperationKind::Trash {
            allowed_decisions.extend([
                OperationDecision::Skip,
                OperationDecision::Replace,
                OperationDecision::Rename,
            ]);
        }
        let item_count = self.items.len();
        let item_label = if item_count == 1 { "item" } else { "items" };
        let (title, confirmation, explanation, details) = match self.kind {
            OperationKind::Trash => (
                "Move to Trash".to_owned(),
                format!("Move {item_count} {item_label} to Trash?"),
                if has_conflicts {
                    "Items remain restorable; existing Trash names are preserved with unique names."
                        .to_owned()
                } else {
                    "Items remain restorable from the platform Trash.".to_owned()
                },
                Vec::new(),
            ),
            OperationKind::Restore => (
                "Restore from Trash".to_owned(),
                format!("Restore {item_count} {item_label} from Trash?"),
                "Items return to their recorded original locations. Existing destinations require an explicit conflict decision.".to_owned(),
                self.preview_lines(),
            ),
            OperationKind::Delete => (
                "Permanently Delete".to_owned(),
                format!("Permanently delete {item_count} {item_label}?"),
                "This operation cannot be restored from Trash.".to_owned(),
                self.preview_lines(),
            ),
            OperationKind::Wipe => (
                "Securely Wipe".to_owned(),
                format!("Securely wipe {item_count} {item_label}?"),
                "File contents are overwritten before permanent deletion.".to_owned(),
                self.preview_lines(),
            ),
            _ => (
                "Operation Preview".to_owned(),
                format!("Execute {:?} for {item_count} {item_label}?", self.kind),
                format!("Review the {:?} operation before execution.", self.kind),
                self.preview_lines(),
            ),
        };
        OperationPresentation {
            title,
            confirmation,
            explanation,
            details,
            reversible,
            allowed_decisions,
            default_decision: OperationDecision::Execute,
            denial_reason: None,
        }
    }

    pub fn preview_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("plan: {}", self.id),
            format!("operation: {:?}", self.kind),
            format!("sources: {}", self.items.len()),
            format!(
                "destination: {}",
                self.destination.as_ref().map_or("-", Location::as_str)
            ),
            format!(
                "conflicts: {} ({:?})",
                self.conflict_count(),
                self.policies.conflict
            ),
            format!("metadata: {:?}", self.policies.metadata),
            format!("verification: {:?}", self.policies.verification),
            format!("recovery: {:?}", self.policies.recovery),
            format!("cross-device: {:?}", self.policies.cross_device),
            format!("symlinks: {:?}", self.policies.symlink),
            format!("safety: {:?}", self.safety),
        ];
        if self.kind == OperationKind::Wipe
            && let Some(passes) = self
                .items
                .first()
                .and_then(|item| item.parameters.get("passes"))
        {
            lines.push(format!("wipe-passes: {passes}"));
        }
        lines.extend(self.items.iter().enumerate().map(|(index, item)| {
            let source = item
                .source
                .as_ref()
                .map_or("-", |source| source.location.as_str());
            let conflict = if item.conflict_expected {
                " [conflict]"
            } else {
                ""
            };
            let parameters = if item.parameters.is_empty() {
                String::new()
            } else {
                format!(" {:?}", item.parameters)
            };
            format!(
                "{}. {} -> {}{}{}",
                index.saturating_add(1),
                source,
                item.target.as_str(),
                conflict,
                parameters
            )
        }));
        lines
    }
}

pub struct OperationPlanner {
    next_id: AtomicU64,
    prefix: String,
}

impl Default for OperationPlanner {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            prefix: "near.operation".to_owned(),
        }
    }
}

impl OperationPlanner {
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            prefix: prefix.into(),
        }
    }

    /// Resolves a request into an immutable operation plan.
    ///
    /// # Errors
    ///
    /// Returns an error when no items are present or a source-required item lacks a source.
    pub fn plan(&self, request: PlanRequest) -> Result<OperationPlan, PlanError> {
        if request.items.is_empty() {
            return Err(PlanError::Empty);
        }
        if request.kind.requires_source() && request.items.iter().any(|item| item.source.is_none())
        {
            return Err(PlanError::MissingSource);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        Ok(OperationPlan {
            id: OperationId::from(format!("{}.{id}", self.prefix)),
            kind: request.kind,
            items: request.items,
            destination: request.destination,
            policies: request.policies,
            safety: request.safety,
            context_generation: request.context_generation,
            high_impact: request.high_impact,
        })
    }

    /// Builds a new plan containing failed and pending items from an earlier summary.
    ///
    /// # Errors
    ///
    /// Returns an error when the summary has no retryable items.
    pub fn retry(
        &self,
        original: &OperationPlan,
        summary: &ExecutionSummary,
    ) -> Result<OperationPlan, PlanError> {
        let retryable = summary
            .items
            .iter()
            .filter(|outcome| matches!(outcome.status, ItemStatus::Failed(_) | ItemStatus::Pending))
            .map(|outcome| outcome.item.clone())
            .collect();
        self.plan(PlanRequest {
            kind: original.kind,
            items: retryable,
            destination: original.destination.clone(),
            policies: original.policies.clone(),
            safety: original.safety,
            context_generation: original.context_generation,
            high_impact: original.high_impact,
        })
    }
}

impl OperationKind {
    fn requires_source(self) -> bool {
        !matches!(self, Self::CreateDirectory | Self::Touch)
    }
}

#[derive(Clone, Debug)]
pub struct PlanRequest {
    pub kind: OperationKind,
    pub items: Vec<PlannedItem>,
    pub destination: Option<Location>,
    pub policies: PlanPolicies,
    pub safety: SafetyClass,
    pub context_generation: ListingGeneration,
    pub high_impact: bool,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PlanError {
    #[error("operation plan has no items")]
    Empty,
    #[error("operation plan item is missing a source")]
    MissingSource,
    #[error("invalid operation plan: {0}")]
    Invalid(String),
}

#[derive(Default)]
pub struct PlanStore {
    plans: BTreeMap<OperationId, OperationPlan>,
}

impl PlanStore {
    pub fn insert(&mut self, plan: OperationPlan) -> OperationId {
        let id = plan.id.clone();
        self.plans.insert(id.clone(), plan);
        id
    }

    pub fn get(&self, id: &OperationId) -> Option<&OperationPlan> {
        self.plans.get(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConflictAction {
    Replace,
    Skip,
    Rename,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DecisionScope {
    Once,
    Remaining,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConflictDecision {
    pub action: ConflictAction,
    pub scope: DecisionScope,
}

pub trait ConflictResolver {
    fn decide(&mut self, plan: &OperationPlan, item: &PlannedItem) -> ConflictDecision;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationIntent {
    CopyTo {
        sources: Vec<ResourceRef>,
        destination: Location,
    },
    MoveTo {
        sources: Vec<ResourceRef>,
        destination: Location,
    },
    Trash {
        sources: Vec<ResourceRef>,
    },
    Restore {
        items: Vec<(ResourceRef, Location)>,
    },
    Delete {
        sources: Vec<ResourceRef>,
        recursive: bool,
    },
    Wipe {
        sources: Vec<ResourceRef>,
        passes: u8,
    },
    CreateDirectory {
        parent: Location,
        name: String,
    },
    Rename {
        items: Vec<(ResourceRef, String)>,
    },
    CreateLink {
        source: ResourceRef,
        name: String,
        kind: LinkKind,
    },
    SetAttributes {
        sources: Vec<ResourceRef>,
        update: AttributeUpdate,
        recursive: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkKind {
    Hard,
    Symbolic,
    Junction,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AttributeUpdate {
    pub readonly: Option<bool>,
    pub unix_mode: Option<u32>,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    pub modified_unix_ms: Option<i64>,
    pub accessed_unix_ms: Option<i64>,
}

impl AttributeUpdate {
    pub fn is_empty(&self) -> bool {
        self.readonly.is_none()
            && self.unix_mode.is_none()
            && self.owner.is_none()
            && self.group.is_none()
            && self.modified_unix_ms.is_none()
            && self.accessed_unix_ms.is_none()
    }
}

pub trait OperationService: Send {
    /// Produces and records a provider-specific immutable plan.
    ///
    /// # Errors
    ///
    /// Returns a provider or planning error when the intent cannot be resolved safely.
    fn plan(
        &mut self,
        intent: OperationIntent,
        generation: ListingGeneration,
    ) -> Result<OperationPlan, String>;

    /// Executes a previously recorded plan identifier.
    ///
    /// # Errors
    ///
    /// Returns an authorization, stale-context, journal, or provider execution error.
    fn execute(
        &mut self,
        plan: &OperationId,
        authorization: ExecutionAuthorization,
        cancellation: &CancellationToken,
        conflict: ConflictDecision,
    ) -> Result<ExecutionSummary, String>;

    /// Executes the exact recorded plan through a platform-native elevation broker.
    ///
    /// # Errors
    ///
    /// Returns an unsupported error by default or broker, authorization, and execution failures.
    fn execute_elevated(
        &mut self,
        _plan: &OperationId,
        _authorization: ExecutionAuthorization,
        _conflict: ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        Err("privileged retry is unsupported by this operation service".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionAuthorization {
    pub context_generation: ListingGeneration,
    pub confirmed: bool,
    pub high_impact_confirmed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ItemStatus {
    Completed,
    Skipped(String),
    Failed(String),
    Pending,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ItemOutcome {
    pub item: PlannedItem,
    pub status: ItemStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub plan: OperationId,
    pub kind: OperationKind,
    pub items: Vec<ItemOutcome>,
    pub cancelled: bool,
}

impl ExecutionSummary {
    pub fn failure_presentation(
        &self,
        retry_available: bool,
    ) -> Option<OperationFailurePresentation> {
        let failures = self
            .items
            .iter()
            .filter_map(|outcome| match &outcome.status {
                ItemStatus::Failed(error) => Some(format!(
                    "{}\n  {error}",
                    outcome
                        .item
                        .source
                        .as_ref()
                        .map_or(&outcome.item.target, |source| &source.location)
                        .as_str()
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        (!failures.is_empty()).then(|| OperationFailurePresentation {
            title: "Operation Failed".to_owned(),
            body: format!(
                "Operation {} failed for {} item(s).\n\n{}\n\n{}",
                self.plan,
                failures.len(),
                failures.join("\n\n"),
                if retry_available {
                    "Close this message, then authorize the exact retained plan with the elevated retry command."
                } else {
                    "Close this message and inspect the retained task history."
                }
            ),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionEffect {
    pub target: Option<Location>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElevatedOperationRequest {
    pub plan: OperationPlan,
    pub authorization: ExecutionAuthorization,
    pub conflict: ConflictDecision,
}

impl ElevatedOperationRequest {
    /// Serializes an elevated request for the privileged helper boundary.
    ///
    /// # Errors
    ///
    /// Returns serialization failures.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|error| error.to_string())
    }

    /// Parses an elevated request received by the privileged helper.
    ///
    /// # Errors
    ///
    /// Returns decoding or schema failures.
    pub fn from_toml(input: &str) -> Result<Self, String> {
        toml::from_str(input).map_err(|error| error.to_string())
    }
}

impl ExecutionSummary {
    /// Serializes an execution summary returned by a privileged helper.
    ///
    /// # Errors
    ///
    /// Returns serialization failures.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|error| error.to_string())
    }

    /// Parses an execution summary returned by a privileged helper.
    ///
    /// # Errors
    ///
    /// Returns decoding or schema failures.
    pub fn from_toml(input: &str) -> Result<Self, String> {
        toml::from_str(input).map_err(|error| error.to_string())
    }
}

impl ExecutionSummary {
    pub fn completed(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ItemStatus::Completed)
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.status, ItemStatus::Skipped(_)))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.status, ItemStatus::Failed(_)))
            .count()
    }

    pub fn pending(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ItemStatus::Pending)
            .count()
    }
}

pub trait OperationBackend {
    fn target_exists(&self, target: &Location) -> bool;
    /// Applies one resolved item from an authorized plan.
    ///
    /// # Errors
    ///
    /// Returns a precise item failure without aborting summary generation for other items.
    fn execute(
        &mut self,
        plan: &OperationPlan,
        item: &PlannedItem,
        action: Option<ConflictAction>,
        cancellation: &CancellationToken,
    ) -> Result<ExecutionEffect, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalEvent {
    Planned(OperationId),
    Elevated(OperationId),
    Started(OperationId),
    Decision {
        plan: OperationId,
        action: ConflictAction,
        scope: DecisionScope,
    },
    Item {
        plan: OperationId,
        index: usize,
        status: ItemStatus,
    },
    Finished(ExecutionSummary),
}

pub struct OperationJournal {
    path: Option<PathBuf>,
    events: Vec<JournalEvent>,
    summaries: BTreeMap<OperationId, ExecutionSummary>,
}

impl OperationJournal {
    pub fn memory() -> Self {
        Self {
            path: None,
            events: Vec::new(),
            summaries: BTreeMap::new(),
        }
    }

    pub fn append_file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            events: Vec::new(),
            summaries: BTreeMap::new(),
        }
    }

    pub fn events(&self) -> &[JournalEvent] {
        &self.events
    }

    pub fn summary(&self, plan: &OperationId) -> Option<&ExecutionSummary> {
        self.summaries.get(plan)
    }

    fn append(&mut self, event: JournalEvent) -> Result<(), ExecutionError> {
        if let JournalEvent::Finished(summary) = &event {
            self.summaries.insert(summary.plan.clone(), summary.clone());
        }
        if let Some(path) = &self.path {
            append_journal_line(path, &event)?;
        }
        self.events.push(event);
        Ok(())
    }
}

pub struct OperationEngine<B> {
    backend: B,
    plans: PlanStore,
    journal: OperationJournal,
}

impl<B: OperationBackend> OperationEngine<B> {
    pub fn new(backend: B, journal: OperationJournal) -> Self {
        Self {
            backend,
            plans: PlanStore::default(),
            journal,
        }
    }

    /// Stores an immutable plan and appends its planning journal event.
    ///
    /// # Errors
    ///
    /// Returns an error when the append-only journal cannot be written.
    pub fn record(&mut self, plan: OperationPlan) -> Result<OperationId, ExecutionError> {
        let id = self.plans.insert(plan);
        self.journal.append(JournalEvent::Planned(id.clone()))?;
        Ok(id)
    }

    /// Stores an elevated immutable plan and records both planning and elevation events.
    ///
    /// # Errors
    ///
    /// Returns an error when either append-only journal event cannot be written.
    pub fn record_elevated(&mut self, plan: OperationPlan) -> Result<OperationId, ExecutionError> {
        let id = self.record(plan)?;
        self.journal.append(JournalEvent::Elevated(id.clone()))?;
        Ok(id)
    }

    pub fn plan(&self, id: &OperationId) -> Option<&OperationPlan> {
        self.plans.get(id)
    }

    pub fn journal(&self) -> &OperationJournal {
        &self.journal
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Authorizes and executes a stored plan by stable identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown plans, stale generations, missing confirmation, or journal
    /// failures. Individual backend failures remain in the returned summary.
    pub fn execute(
        &mut self,
        id: &OperationId,
        authorization: ExecutionAuthorization,
        cancellation: &CancellationToken,
        resolver: &mut dyn ConflictResolver,
    ) -> Result<ExecutionSummary, ExecutionError> {
        let plan = self
            .plans
            .get(id)
            .cloned()
            .ok_or_else(|| ExecutionError::UnknownPlan(id.clone()))?;
        authorize(&plan, authorization)?;
        self.journal.append(JournalEvent::Started(id.clone()))?;
        let mut outcomes = Vec::with_capacity(plan.items.len());
        let mut remaining_decision = None;
        let mut cancelled = false;
        for (index, item) in plan.items.iter().enumerate() {
            if cancellation.is_cancelled() {
                cancelled = true;
                outcomes.extend(plan.items[index..].iter().cloned().map(|item| ItemOutcome {
                    item,
                    status: ItemStatus::Pending,
                }));
                break;
            }
            let action = self.conflict_action(&plan, item, resolver, &mut remaining_decision)?;
            if action == Some(ConflictAction::Cancel) {
                cancelled = true;
                outcomes.extend(plan.items[index..].iter().cloned().map(|item| ItemOutcome {
                    item,
                    status: ItemStatus::Pending,
                }));
                break;
            }
            let mut outcome_item = item.clone();
            let status = if action == Some(ConflictAction::Skip) {
                ItemStatus::Skipped("conflict policy".to_owned())
            } else {
                self.backend
                    .execute(&plan, item, action, cancellation)
                    .map_or_else(ItemStatus::Failed, |effect| {
                        if let Some(target) = effect.target {
                            outcome_item.target = target;
                        }
                        ItemStatus::Completed
                    })
            };
            self.journal.append(JournalEvent::Item {
                plan: id.clone(),
                index,
                status: status.clone(),
            })?;
            outcomes.push(ItemOutcome {
                item: outcome_item,
                status,
            });
        }
        let summary = ExecutionSummary {
            plan: id.clone(),
            kind: plan.kind(),
            items: outcomes,
            cancelled,
        };
        self.journal
            .append(JournalEvent::Finished(summary.clone()))?;
        Ok(summary)
    }

    fn conflict_action(
        &mut self,
        plan: &OperationPlan,
        item: &PlannedItem,
        resolver: &mut dyn ConflictResolver,
        remaining: &mut Option<ConflictAction>,
    ) -> Result<Option<ConflictAction>, ExecutionError> {
        if item
            .source
            .as_ref()
            .is_some_and(|source| source.location == item.target)
        {
            return Ok(None);
        }
        if !item.conflict_expected && !self.backend.target_exists(&item.target) {
            return Ok(None);
        }
        let decision = match plan.policies.conflict {
            ConflictPolicy::Replace => ConflictDecision {
                action: ConflictAction::Replace,
                scope: DecisionScope::Remaining,
            },
            ConflictPolicy::Skip => ConflictDecision {
                action: ConflictAction::Skip,
                scope: DecisionScope::Remaining,
            },
            ConflictPolicy::Rename => ConflictDecision {
                action: ConflictAction::Rename,
                scope: DecisionScope::Remaining,
            },
            ConflictPolicy::Ask => remaining.map_or_else(
                || resolver.decide(plan, item),
                |action| ConflictDecision {
                    action,
                    scope: DecisionScope::Remaining,
                },
            ),
        };
        if decision.scope == DecisionScope::Remaining {
            *remaining = Some(decision.action);
        }
        self.journal.append(JournalEvent::Decision {
            plan: plan.id.clone(),
            action: decision.action,
            scope: decision.scope,
        })?;
        Ok(Some(decision.action))
    }
}

fn authorize(
    plan: &OperationPlan,
    authorization: ExecutionAuthorization,
) -> Result<(), ExecutionError> {
    if authorization.context_generation != plan.context_generation {
        return Err(ExecutionError::StaleContext {
            planned: plan.context_generation,
            actual: authorization.context_generation,
        });
    }
    if matches!(
        plan.safety,
        SafetyClass::Confirmable | SafetyClass::Destructive
    ) && !authorization.confirmed
    {
        return Err(ExecutionError::ConfirmationRequired);
    }
    if plan.high_impact && !authorization.high_impact_confirmed {
        return Err(ExecutionError::HighImpactConfirmationRequired);
    }
    Ok(())
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ExecutionError {
    #[error("unknown operation plan: {0}")]
    UnknownPlan(OperationId),
    #[error("operation context is stale: planned {planned:?}, actual {actual:?}")]
    StaleContext {
        planned: ListingGeneration,
        actual: ListingGeneration,
    },
    #[error("operation confirmation is required")]
    ConfirmationRequired,
    #[error("high-impact confirmation is required")]
    HighImpactConfirmationRequired,
    #[error("operation journal failed: {0}")]
    Journal(String),
}

fn append_journal_line(path: &Path, event: &JournalEvent) -> Result<(), ExecutionError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| ExecutionError::Journal(error.to_string()))?;
    writeln!(file, "{event:?}").map_err(|error| ExecutionError::Journal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use near_core::{ProviderId, ResourceRef};

    use super::*;

    fn resource(name: &str) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from("test.provider"),
            location: Location::new(format!("test:///{name}")),
        }
    }

    fn request(items: Vec<PlannedItem>) -> PlanRequest {
        PlanRequest {
            kind: OperationKind::Copy,
            items,
            destination: Some(Location::new("test:///destination")),
            policies: PlanPolicies {
                conflict: ConflictPolicy::Ask,
                ..PlanPolicies::default()
            },
            safety: SafetyClass::Confirmable,
            context_generation: ListingGeneration(7),
            high_impact: false,
        }
    }

    fn item(name: &str, conflict: bool) -> PlannedItem {
        PlannedItem {
            source: Some(resource(name)),
            target: Location::new(format!("test:///destination/{name}")),
            conflict_expected: conflict,
            recursive: false,
            parameters: BTreeMap::default(),
        }
    }

    struct Resolver {
        calls: AtomicUsize,
        decision: ConflictDecision,
    }

    impl ConflictResolver for Resolver {
        fn decide(&mut self, _plan: &OperationPlan, _item: &PlannedItem) -> ConflictDecision {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.decision
        }
    }

    struct Backend {
        fail: Option<String>,
        cancel_after: Option<usize>,
        executed: usize,
        cancellation: CancellationToken,
    }

    impl OperationBackend for Backend {
        fn target_exists(&self, _target: &Location) -> bool {
            false
        }

        fn execute(
            &mut self,
            _plan: &OperationPlan,
            item: &PlannedItem,
            _action: Option<ConflictAction>,
            _cancellation: &CancellationToken,
        ) -> Result<ExecutionEffect, String> {
            self.executed += 1;
            if self.cancel_after == Some(self.executed) {
                self.cancellation.cancel();
            }
            let name = item.source.as_ref().unwrap().location.as_str();
            if self
                .fail
                .as_ref()
                .is_some_and(|failure| name.ends_with(failure))
            {
                Err("injected failure".to_owned())
            } else {
                Ok(ExecutionEffect::default())
            }
        }
    }

    #[test]
    fn execution_rejects_stale_and_unconfirmed_plans() {
        let planner = OperationPlanner::default();
        let plan = planner.plan(request(vec![item("one", false)])).unwrap();
        let id = plan.id().clone();
        let mut engine = OperationEngine::new(
            Backend {
                fail: None,
                cancel_after: None,
                executed: 0,
                cancellation: CancellationToken::default(),
            },
            OperationJournal::memory(),
        );
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Replace,
                scope: DecisionScope::Once,
            },
        };
        let cancellation = CancellationToken::default();
        assert!(matches!(
            engine.execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(8),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &cancellation,
                &mut resolver,
            ),
            Err(ExecutionError::StaleContext { .. })
        ));
        assert_eq!(
            engine.execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: false,
                    high_impact_confirmed: false,
                },
                &cancellation,
                &mut resolver,
            ),
            Err(ExecutionError::ConfirmationRequired)
        );
    }

    #[test]
    fn conflicts_apply_to_remaining_and_journal_exact_outcomes() {
        let planner = OperationPlanner::default();
        let plan = planner
            .plan(request(vec![item("one", true), item("two", true)]))
            .unwrap();
        let id = plan.id().clone();
        let mut engine = OperationEngine::new(
            Backend {
                fail: Some("two".to_owned()),
                cancel_after: None,
                executed: 0,
                cancellation: CancellationToken::default(),
            },
            OperationJournal::memory(),
        );
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Replace,
                scope: DecisionScope::Remaining,
            },
        };
        let summary = engine
            .execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                &mut resolver,
            )
            .unwrap();
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1);
        assert_eq!(summary.completed(), 1);
        assert_eq!(summary.failed(), 1);
        assert_eq!(engine.journal().summary(&id), Some(&summary));
        let retry = planner.retry(engine.plan(&id).unwrap(), &summary).unwrap();
        assert_eq!(retry.items().len(), 1);
        assert!(
            retry.items()[0]
                .source
                .as_ref()
                .unwrap()
                .location
                .as_str()
                .ends_with("two")
        );
    }

    #[test]
    fn conflict_decisions_can_apply_once() {
        let planner = OperationPlanner::default();
        let plan = planner
            .plan(request(vec![item("one", true), item("two", true)]))
            .unwrap();
        let id = plan.id().clone();
        let mut engine = OperationEngine::new(
            Backend {
                fail: None,
                cancel_after: None,
                executed: 0,
                cancellation: CancellationToken::default(),
            },
            OperationJournal::memory(),
        );
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Replace,
                scope: DecisionScope::Once,
            },
        };
        let summary = engine
            .execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                &mut resolver,
            )
            .unwrap();
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 2);
        assert_eq!(summary.completed(), 2);
    }

    #[test]
    fn cancellation_marks_unstarted_items_pending_and_persists_journal() {
        let planner = OperationPlanner::default();
        let plan = planner
            .plan(request(vec![
                item("one", false),
                item("two", false),
                item("three", false),
            ]))
            .unwrap();
        let id = plan.id().clone();
        let cancellation = CancellationToken::default();
        let path = std::env::temp_dir().join(format!("near-journal-{}.log", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut engine = OperationEngine::new(
            Backend {
                fail: None,
                cancel_after: Some(1),
                executed: 0,
                cancellation: cancellation.clone(),
            },
            OperationJournal::append_file(&path),
        );
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Skip,
                scope: DecisionScope::Once,
            },
        };
        let summary = engine
            .execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &cancellation,
                &mut resolver,
            )
            .unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.completed(), 1);
        assert_eq!(summary.pending(), 2);
        assert!(fs::read_to_string(&path).unwrap().contains("Finished"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn recursive_delete_requires_high_impact_confirmation() {
        let planner = OperationPlanner::default();
        let mut delete = request(vec![item("tree", false)]);
        delete.kind = OperationKind::Delete;
        delete.safety = SafetyClass::Destructive;
        delete.high_impact = true;
        let plan = planner.plan(delete).unwrap();
        let id = plan.id().clone();
        let mut engine = OperationEngine::new(
            Backend {
                fail: None,
                cancel_after: None,
                executed: 0,
                cancellation: CancellationToken::default(),
            },
            OperationJournal::memory(),
        );
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Cancel,
                scope: DecisionScope::Once,
            },
        };
        assert_eq!(
            engine.execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                &mut resolver,
            ),
            Err(ExecutionError::HighImpactConfirmationRequired)
        );
    }

    #[test]
    fn preview_explicitly_reports_copy_then_delete_moves() {
        let planner = OperationPlanner::default();
        let mut move_request = request(vec![item("one", false)]);
        move_request.kind = OperationKind::Move;
        move_request.policies.cross_device = CrossDeviceBehavior::CopyThenDelete;
        let plan = planner.plan(move_request).unwrap();
        assert!(
            plan.preview_lines()
                .iter()
                .any(|line| line == "cross-device: CopyThenDelete")
        );
    }

    #[test]
    fn backend_selected_target_is_recorded_in_summary_and_journal() {
        struct TargetSelectingBackend;

        impl OperationBackend for TargetSelectingBackend {
            fn target_exists(&self, _target: &Location) -> bool {
                false
            }

            fn execute(
                &mut self,
                _plan: &OperationPlan,
                _item: &PlannedItem,
                _action: Option<ConflictAction>,
                _cancellation: &CancellationToken,
            ) -> Result<ExecutionEffect, String> {
                Ok(ExecutionEffect {
                    target: Some(Location::new("test:///platform-selected-name")),
                })
            }
        }

        let planner = OperationPlanner::default();
        let plan = planner.plan(request(vec![item("one", false)])).unwrap();
        let id = plan.id().clone();
        let mut engine = OperationEngine::new(TargetSelectingBackend, OperationJournal::memory());
        engine.record(plan).unwrap();
        let mut resolver = Resolver {
            calls: AtomicUsize::new(0),
            decision: ConflictDecision {
                action: ConflictAction::Cancel,
                scope: DecisionScope::Once,
            },
        };

        let summary = engine
            .execute(
                &id,
                ExecutionAuthorization {
                    context_generation: ListingGeneration(7),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                &CancellationToken::default(),
                &mut resolver,
            )
            .unwrap();

        assert_eq!(summary.kind, OperationKind::Copy);
        assert_eq!(
            summary.items[0].item.target.as_str(),
            "test:///platform-selected-name"
        );
        assert_eq!(engine.journal().summary(&id), Some(&summary));
    }

    #[test]
    fn failure_presentation_retains_exact_resource_and_error() {
        let summary = ExecutionSummary {
            plan: "failed-plan".into(),
            kind: OperationKind::Trash,
            items: vec![ItemOutcome {
                item: item("folder", false),
                status: ItemStatus::Failed("permission denied".to_owned()),
            }],
            cancelled: false,
        };
        let presentation = summary.failure_presentation(false).unwrap();
        assert_eq!(presentation.title, "Operation Failed");
        assert!(presentation.body.contains("test:///folder"));
        assert!(presentation.body.contains("permission denied"));
    }
}
