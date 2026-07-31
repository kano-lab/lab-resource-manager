use super::errors::ResourceUsageError;
use super::value_objects::*;
use crate::domain::common::EmailAddress;
use chrono::{DateTime, Utc};

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
    /// 新しいリソース使用予定を作成する
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
            id: UsageId::new(),
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

    /// 予約を`at`の時点で締める（早期終了）
    ///
    /// 取り消しが予約そのものを無かったことにするのに対し、早期終了は
    /// 開始から`at`までを使った事実として残し、残りの時間だけを他の利用者へ開く。
    ///
    /// # Errors
    /// - `at`が開始時刻以前の場合、`ResourceUsageError::NotYetStarted`
    /// - `at`が終了時刻以降の場合、`ResourceUsageError::AlreadyEnded`
    pub fn release_early(&mut self, at: DateTime<Utc>) -> Result<(), ResourceUsageError> {
        if at <= self.time_period.start() {
            return Err(ResourceUsageError::NotYetStarted {
                start: self.time_period.start(),
                at,
            });
        }
        if at >= self.time_period.end() {
            return Err(ResourceUsageError::AlreadyEnded {
                end: self.time_period.end(),
                at,
            });
        }

        self.time_period = TimePeriod::new(self.time_period.start(), at)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn usage_running_from(start: DateTime<Utc>, end: DateTime<Utc>) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new("owner@example.com".to_string()).unwrap(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[test]
    fn releasing_early_shortens_the_reservation_to_that_moment() {
        let start = Utc::now() - Duration::hours(1);
        let end = start + Duration::hours(4);
        let mut usage = usage_running_from(start, end);
        let now = start + Duration::hours(1);

        usage.release_early(now).unwrap();

        assert_eq!(
            usage.time_period().start(),
            start,
            "使い始めた時刻は実績として残る"
        );
        assert_eq!(usage.time_period().end(), now);
    }

    #[test]
    fn a_reservation_that_has_not_started_cannot_be_released_early() {
        let start = Utc::now() + Duration::hours(1);
        let mut usage = usage_running_from(start, start + Duration::hours(2));

        let error = usage.release_early(Utc::now()).unwrap_err();

        assert!(
            matches!(error, ResourceUsageError::NotYetStarted { .. }),
            "使った時間がない予約は締められない（取り消しの領分）: {:?}",
            error
        );
    }

    #[test]
    fn releasing_exactly_at_the_start_leaves_nothing_to_record() {
        let start = Utc::now() - Duration::hours(1);
        let mut usage = usage_running_from(start, start + Duration::hours(2));

        let error = usage.release_early(start).unwrap_err();

        assert!(
            matches!(error, ResourceUsageError::NotYetStarted { .. }),
            "長さ0の予約は残せない: {:?}",
            error
        );
    }

    #[test]
    fn a_reservation_that_has_already_ended_cannot_be_released_early() {
        let end = Utc::now() - Duration::hours(1);
        let mut usage = usage_running_from(end - Duration::hours(2), end);

        let error = usage.release_early(Utc::now()).unwrap_err();

        assert!(
            matches!(error, ResourceUsageError::AlreadyEnded { .. }),
            "開く残り時間がない: {:?}",
            error
        );
    }

    #[test]
    fn releasing_early_leaves_the_other_attributes_untouched() {
        let start = Utc::now() - Duration::hours(1);
        let mut usage = usage_running_from(start, start + Duration::hours(4));
        let id = usage.id().clone();
        let resources = usage.resources().clone();

        usage.release_early(start + Duration::minutes(30)).unwrap();

        assert_eq!(usage.id(), &id, "同じ予約であり続ける");
        assert_eq!(usage.resources(), &resources);
        assert_eq!(usage.owner_email().as_str(), "owner@example.com");
    }
}
