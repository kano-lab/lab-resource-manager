use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// 外部システムの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalSystem {
    /// Slack
    Slack,
}

impl ExternalSystem {
    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        match self {
            ExternalSystem::Slack => "slack",
        }
    }
}

impl FromStr for ExternalSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "slack" => Ok(ExternalSystem::Slack),
            _ => Err(format!("Unknown external system: {}", s)),
        }
    }
}
