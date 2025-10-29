use serde::{Deserialize, Serialize};

/// 外部システムの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalSystem {
    Slack,
}

impl ExternalSystem {
    pub fn as_str(&self) -> &str {
        match self {
            ExternalSystem::Slack => "slack",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "slack" => Some(ExternalSystem::Slack),
            _ => None,
        }
    }
}
