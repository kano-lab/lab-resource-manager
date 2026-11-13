//! ユースケースのテストで共有する部品

use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::{ExternalIdentity, ExternalSystem},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, RepositoryError, ReservationRepository, ReservedSlotQuery,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

/// メモリ上でID紐付けを保持するテスト用リポジトリ
#[derive(Default)]
pub struct InMemoryIdentityLinkRepository {
    links: Mutex<HashMap<String, IdentityLink>>,
}

impl InMemoryIdentityLinkRepository {
    /// メールアドレスと外部システムの識別子を紐付ける
    ///
    /// 同じメールアドレスに複数の外部システムを紐付けられる（Slackと各サーバーのOSユーザーなど）。
    pub fn add_link(&self, email: &str, system: ExternalSystem, user_id: &str) {
        let email_addr = EmailAddress::new(email.to_string()).unwrap();
        let identity = ExternalIdentity::new(system, user_id.to_string());

        let mut links = self.links.lock().unwrap();
        let link = links
            .entry(email_addr.as_str().to_string())
            .or_insert_with(|| IdentityLink::new(email_addr.clone()));
        link.link_external_identity(identity).ok();
    }
}

#[async_trait]
impl IdentityLinkRepository for InMemoryIdentityLinkRepository {
    async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        Ok(self.links.lock().unwrap().get(email.as_str()).cloned())
    }

    async fn find_by_external_user_id(
        &self,
        system: &ExternalSystem,
        user_id: &str,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        let links = self.links.lock().unwrap();
        let found = links.values().find(|link| {
            link.get_identity_for_system(system)
                .is_some_and(|identity| identity.user_id() == user_id)
        });
        Ok(found.cloned())
    }

    async fn save(&self, identity_link: IdentityLink) -> Result<(), RepositoryError> {
        self.links
            .lock()
            .unwrap()
            .insert(identity_link.email().as_str().to_string(), identity_link);
        Ok(())
    }
}

/// 予約者`holder@example.com`の、`first_start`から2時間を毎週4回繰り返す定期予約（東京）
pub fn weekly_reservation(
    first_start: chrono::DateTime<chrono::Utc>,
) -> crate::domain::aggregates::reservation::Reservation {
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::aggregates::reservation::{
        RecurrenceRule, RecurringSchedule, Reservation, ReservationId, Schedule,
    };
    use crate::domain::common::TimePeriod;
    Reservation::reconstruct(
        ReservationId::from_string("weekly".to_string()),
        EmailAddress::new("holder@example.com".to_string()).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        Some("週次の学習".to_string()),
        Schedule::Recurring(RecurringSchedule::new(
            TimePeriod::new(first_start, first_start + chrono::Duration::hours(2)).unwrap(),
            RecurrenceRule::parse(vec!["RRULE:FREQ=WEEKLY;COUNT=4".to_string()]).unwrap(),
            chrono_tz::Asia::Tokyo,
            std::collections::BTreeMap::new(),
        )),
        None,
    )
    .unwrap()
}

/// 読まれた直後に、ほかの誰かが同じ予約を保存し直すリポジトリ
///
/// 読んだ予約の版は、書き戻すときにはもう古い。ユースケースが衝突をどう伝えるかを確かめる。
pub struct ChangedAfterReading(
    pub crate::infrastructure::repositories::reservation::in_memory::InMemoryReservationRepository,
);

#[async_trait]
impl ReservationRepository for ChangedAfterReading {
    async fn find(
        &self,
        id: &crate::domain::aggregates::reservation::ReservationId,
    ) -> Result<Option<crate::domain::aggregates::reservation::Reservation>, RepositoryError> {
        let found = self.0.find(id).await?;
        if let Some(reservation) = &found {
            // 保存し直すと、保存先の版が進む
            self.0.put_reservation(reservation);
        }
        Ok(found)
    }

    async fn find_overlapping(
        &self,
        time_period: &crate::domain::common::TimePeriod,
    ) -> Result<Vec<crate::domain::aggregates::reservation::Reservation>, RepositoryError> {
        self.0.find_overlapping(time_period).await
    }

    async fn save(
        &self,
        reservation: &mut crate::domain::aggregates::reservation::Reservation,
    ) -> Result<(), RepositoryError> {
        self.0.save(reservation).await
    }
}

#[async_trait]
impl ReservedSlotQuery for ChangedAfterReading {
    async fn find_slots_overlapping(
        &self,
        time_period: &crate::domain::common::TimePeriod,
    ) -> Result<Vec<crate::domain::read_models::ReservedSlotView>, RepositoryError> {
        self.0.find_slots_overlapping(time_period).await
    }

    async fn find_slots_held_by(
        &self,
        holder_email: &EmailAddress,
        time_period: &crate::domain::common::TimePeriod,
    ) -> Result<Vec<crate::domain::read_models::ReservedSlotView>, RepositoryError> {
        self.0.find_slots_held_by(holder_email, time_period).await
    }
}
