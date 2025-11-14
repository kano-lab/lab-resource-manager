//! リソース使用ドメインサービスのエラー

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::errors::DomainError;
use std::fmt;

/// リソース競合エラー
#[derive(Debug)]
pub struct ResourceConflictError {
    /// 競合しているリソースの説明
    pub resource_description: String,
    /// 競合している既存の使用予定ID
    pub conflicting_usage_id: UsageId,
}

impl ResourceConflictError {
    pub fn new(resource_description: impl Into<String>, conflicting_usage_id: UsageId) -> Self {
        Self {
            resource_description: resource_description.into(),
            conflicting_usage_id,
        }
    }
}

impl fmt::Display for ResourceConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "リソース競合: {} (競合する予約ID: {})",
            self.resource_description,
            self.conflicting_usage_id.as_str()
        )
    }
}

impl std::error::Error for ResourceConflictError {}

impl DomainError for ResourceConflictError {}
