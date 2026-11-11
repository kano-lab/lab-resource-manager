use super::value_objects::{RecurrenceRule, RecurringReservationId};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::domain::common::EmailAddress;

/// 決まった間隔で繰り返される予約（定期予約）を表す集約ルート
///
/// 各回は`ResourceUsage`として個別に存在し、この集約はそれらをまとめる取り決めを表す。
/// 定期予約を1件作ることは利用者にとって1つの行為であり、回の数だけの行為ではない。
/// 作成や取り消しを人に伝えるときに、回ではなくこの単位で語れるようにするためのモデルである。
#[derive(Debug, Clone, PartialEq)]
pub struct RecurringReservation {
    id: RecurringReservationId,
    owner_email: EmailAddress,
    resources: Vec<Resource>,
    notes: Option<String>,
    /// 各回の時間帯を示す代表の回（把握できるかぎり初回）
    representative_period: TimePeriod,
    /// 繰り返しの間隔。永続化層の表現を読み取れなかった場合は`None`
    rule: Option<RecurrenceRule>,
}

impl RecurringReservation {
    /// 定期予約を、その代表の回と繰り返しの間隔から組み立てる
    ///
    /// 予約者・リソース・備考・時間帯は回から引き継ぐ。
    ///
    /// # Arguments
    /// * `id` - 定期予約ID
    /// * `representative` - 各回の内容を示す代表の回（把握できるかぎり初回）
    /// * `rule` - 繰り返しの間隔（読み取れなかった場合は`None`）
    pub fn from_occurrence(
        id: RecurringReservationId,
        representative: &ResourceUsage,
        rule: Option<RecurrenceRule>,
    ) -> Self {
        Self {
            id,
            owner_email: representative.owner_email().clone(),
            resources: representative.resources().clone(),
            notes: representative.notes().cloned(),
            representative_period: representative.time_period().clone(),
            rule,
        }
    }

    /// 定期予約IDを取得
    pub fn id(&self) -> &RecurringReservationId {
        &self.id
    }

    /// 所有者のメールアドレスを取得
    pub fn owner_email(&self) -> &EmailAddress {
        &self.owner_email
    }

    /// 各回が使用するリソースのリストを取得
    pub fn resources(&self) -> &Vec<Resource> {
        &self.resources
    }

    /// 備考を取得
    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }

    /// 各回の時間帯を示す代表の回（把握できるかぎり初回）の期間を取得
    pub fn representative_period(&self) -> &TimePeriod {
        &self.representative_period
    }

    /// 繰り返しの間隔を取得（読み取れなかった場合は`None`）
    pub fn rule(&self) -> Option<&RecurrenceRule> {
        self.rule.as_ref()
    }
}
