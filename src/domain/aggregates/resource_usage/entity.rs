use super::errors::ResourceUsageError;
use super::value_objects::*;
use crate::domain::common::EmailAddress;

/// リソース使用予定を表す集約ルート
///
/// GPU、部屋などのリソースの使用予定情報を管理する。
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceUsage {
    id: UsageId,
    owner_email: EmailAddress,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

impl ResourceUsage {
    /// 新しいリソース使用予定を作成する（UUID自動生成）
    ///
    /// # Arguments
    /// * `owner_email` - 所有者のメールアドレス
    /// * `time_period` - 使用期間
    /// * `resources` - 使用するリソースのリスト
    /// * `notes` - 備考（オプション）
    ///
    /// # Errors
    /// リソースが空の場合、`ResourceUsageError::NoResourceItems`を返す
    pub fn new(
        owner_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ResourceUsageError> {
        if resources.is_empty() {
            return Err(ResourceUsageError::NoResourceItems);
        }

        Ok(Self {
            id: UsageId::new(), // UUIDを自動生成
            owner_email,
            time_period,
            resources,
            notes,
        })
    }

    /// リポジトリからの再構築用（既存IDを指定）
    ///
    /// # Arguments
    /// * `id` - 既存の使用予定ID
    /// * `owner_email` - 所有者のメールアドレス
    /// * `time_period` - 使用期間
    /// * `resources` - 使用するリソースのリスト
    /// * `notes` - 備考（オプション）
    ///
    /// # Errors
    /// リソースが空の場合、`ResourceUsageError::NoResourceItems`を返す
    pub fn reconstruct(
        id: UsageId,
        owner_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ResourceUsageError> {
        if resources.is_empty() {
            return Err(ResourceUsageError::NoResourceItems);
        }

        Ok(Self {
            id,
            owner_email,
            time_period,
            resources,
            notes,
        })
    }

    /// 使用予定IDを取得
    pub fn id(&self) -> &UsageId {
        &self.id
    }

    /// 所有者のメールアドレスを取得
    pub fn owner_email(&self) -> &EmailAddress {
        &self.owner_email
    }

    /// 使用期間を取得
    pub fn time_period(&self) -> &TimePeriod {
        &self.time_period
    }

    /// 使用するリソースのリストを取得
    pub fn resources(&self) -> &Vec<Resource> {
        &self.resources
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }

    /// 使用期間を更新する
    pub fn update_time_period(&mut self, new_time_period: TimePeriod) {
        self.time_period = new_time_period;
    }

    /// 備考を更新する
    pub fn update_notes(&mut self, notes: String) {
        self.notes = Some(notes);
    }
}
