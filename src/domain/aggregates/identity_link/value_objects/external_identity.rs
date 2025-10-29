use super::ExternalSystem;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 外部システムでのユーザー識別情報
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalIdentity {
    /// 外部システムの種類
    system: ExternalSystem,
    /// 外部システムでのユーザーID
    user_id: String,
    /// 紐付けた日時
    linked_at: DateTime<Utc>,
}

impl ExternalIdentity {
    pub fn new(system: ExternalSystem, user_id: String) -> Self {
        Self {
            system,
            user_id,
            linked_at: Utc::now(),
        }
    }

    /// 永続化層からの復元用
    pub(crate) fn reconstitute(
        system: ExternalSystem,
        user_id: String,
        linked_at: DateTime<Utc>,
    ) -> Self {
        Self {
            system,
            user_id,
            linked_at,
        }
    }

    pub fn system(&self) -> &ExternalSystem {
        &self.system
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn linked_at(&self) -> DateTime<Utc> {
        self.linked_at
    }
}
