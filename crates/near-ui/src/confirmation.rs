use near_core::SafetyClass;
use near_ops::OperationPlan;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmationPolicy {
    confirm_reversible: bool,
    confirm_confirmable: bool,
}

impl Default for ConfirmationPolicy {
    fn default() -> Self {
        Self {
            confirm_reversible: true,
            confirm_confirmable: true,
        }
    }
}

impl ConfirmationPolicy {
    pub const fn reversible_preview(&self) -> bool {
        self.confirm_reversible
    }

    pub const fn confirmable_preview(&self) -> bool {
        self.confirm_confirmable
    }

    pub fn set_reversible_preview(&mut self, enabled: bool) {
        self.confirm_reversible = enabled;
    }

    pub fn set_confirmable_preview(&mut self, enabled: bool) {
        self.confirm_confirmable = enabled;
    }

    /// Serializes the complete versioned policy with mandatory safeguards intact.
    ///
    /// # Errors
    ///
    /// Returns a TOML serialization failure.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(&ConfirmationDocument {
            schema: 1,
            confirmations: ConfirmationSettings {
                reversible: decision(self.confirm_reversible),
                confirmable: decision(self.confirm_confirmable),
                ..ConfirmationSettings::default()
            },
        })
    }

    /// Parses a versioned confirmation policy.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed documents, unsupported schemas, or attempts to disable
    /// mandatory destructive safeguards.
    pub fn from_toml(source: &str) -> Result<Self, ConfirmationPolicyError> {
        let document: ConfirmationDocument = toml::from_str(source)?;
        if document.schema != 1 {
            return Err(ConfirmationPolicyError::UnsupportedSchema(document.schema));
        }
        if document.confirmations.destructive != ConfirmationDecision::Preview {
            return Err(ConfirmationPolicyError::MandatorySafeguard("destructive"));
        }
        if document.confirmations.privileged != ConfirmationDecision::Preview {
            return Err(ConfirmationPolicyError::MandatorySafeguard("privileged"));
        }
        if document.confirmations.high_impact != ConfirmationDecision::Preview {
            return Err(ConfirmationPolicyError::MandatorySafeguard("high-impact"));
        }
        Ok(Self {
            confirm_reversible: document.confirmations.reversible == ConfirmationDecision::Preview,
            confirm_confirmable: document.confirmations.confirmable
                == ConfirmationDecision::Preview,
        })
    }

    pub fn requires_preview(&self, plan: &OperationPlan) -> bool {
        if plan.high_impact() {
            return true;
        }
        match plan.safety() {
            SafetyClass::ReadOnly => false,
            SafetyClass::Reversible => self.confirm_reversible,
            SafetyClass::Confirmable => self.confirm_confirmable,
            _ => true,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfirmationPolicyError {
    #[error("confirmation policy TOML is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unsupported confirmation policy schema {0}")]
    UnsupportedSchema(u32),
    #[error("the {0} confirmation safeguard cannot be disabled")]
    MandatorySafeguard(&'static str),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfirmationDocument {
    schema: u32,
    confirmations: ConfirmationSettings,
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct ConfirmationSettings {
    reversible: ConfirmationDecision,
    confirmable: ConfirmationDecision,
    destructive: ConfirmationDecision,
    privileged: ConfirmationDecision,
    high_impact: ConfirmationDecision,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ConfirmationDecision {
    Preview,
    Execute,
}

const fn decision(preview: bool) -> ConfirmationDecision {
    if preview {
        ConfirmationDecision::Preview
    } else {
        ConfirmationDecision::Execute
    }
}

impl Default for ConfirmationSettings {
    fn default() -> Self {
        Self {
            reversible: ConfirmationDecision::Preview,
            confirmable: ConfirmationDecision::Preview,
            destructive: ConfirmationDecision::Preview,
            privileged: ConfirmationDecision::Preview,
            high_impact: ConfirmationDecision::Preview,
        }
    }
}
