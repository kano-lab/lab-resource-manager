//! 予約枠への操作の結果を利用者に伝える文面
//!
//! 同じ操作でも入口はいくつかある（予約通知のボタン、未使用のお知らせのボタン）。
//! 断られる理由は入口ではなく操作で決まるため、文面もここにまとめる。
//! 内部のエラー文字列をそのまま見せず、次に何をすればよいかが分かる言葉にする。

use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservationEvent;
use crate::domain::aggregates::reservation::errors::ReservationError;
use crate::domain::ports::repositories::RepositoryError;

/// 予約枠を今の時点で終了できなかった理由
pub fn end_early_message(failure: &ApplicationError) -> String {
    match failure {
        ApplicationError::Reservation(ReservationError::NotYetStarted { .. }) => {
            "ℹ️ この予約枠はまだ始まっていません。使わないのであれば「❌ 取り消す」で取り消してください。"
                .to_string()
        }
        ApplicationError::Reservation(ReservationError::AlreadyEnded { .. }) => {
            "ℹ️ この予約枠はすでに終わっています。".to_string()
        }
        ApplicationError::Repository(RepositoryError::NotFound) => {
            "❌ この予約枠は見つかりませんでした。すでに取り消されている可能性があります。"
                .to_string()
        }
        ApplicationError::Unauthorized(_) => "❌ 自分の予約枠だけを終了できます。".to_string(),
        other => format!("❌ 予約枠を終了できませんでした: {}", other),
    }
}

/// 予約枠を取り消したことを伝える文面
///
/// 単発予約の予約枠を取り消すと予約ごと無くなるが、定期予約の予約枠を取り消しても、
/// 定期予約と他の予約枠は残る。何が無くなり何が残ったのかを取り違えないよう、
/// 取り消しで起きた出来事に合わせて言い分ける。
pub fn cancelled_message(event: &ReservationEvent) -> String {
    match event {
        ReservationEvent::SlotCancelled { .. } => {
            "✅ この予約枠を取り消しました。定期予約の他の予約枠はそのまま残っています。"
                .to_string()
        }
        _ => "✅ 予約を取り消しました。".to_string(),
    }
}

/// 予約枠を取り消せなかった理由
pub fn cancel_message(failure: &ApplicationError) -> String {
    match failure {
        ApplicationError::Repository(RepositoryError::NotFound) => {
            "ℹ️ この予約枠はすでに取り消されています。".to_string()
        }
        ApplicationError::Unauthorized(_) => "❌ 自分の予約枠だけを取り消せます。".to_string(),
        other => format!("❌ 予約枠を取り消せませんでした: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn a_reservation_that_has_not_started_points_at_cancelling() {
        let message = end_early_message(&ApplicationError::Reservation(
            ReservationError::NotYetStarted {
                start: Utc::now(),
                at: Utc::now(),
            },
        ));

        assert!(
            message.contains("取り消"),
            "取り消しが正しい操作であることを伝えるべき: {message}"
        );
    }

    #[test]
    fn a_reservation_that_has_ended_is_reported_as_settled() {
        let message = end_early_message(&ApplicationError::Reservation(
            ReservationError::AlreadyEnded {
                end: Utc::now(),
                at: Utc::now(),
            },
        ));

        assert!(message.contains("すでに終わっています"), "{message}");
    }

    #[test]
    fn a_reservation_that_is_already_gone_is_reported_as_such() {
        let message = cancel_message(&ApplicationError::Repository(RepositoryError::NotFound));

        assert!(message.contains("すでに取り消されています"), "{message}");
    }

    #[test]
    fn someone_elses_reservation_is_refused_in_plain_words() {
        for message in [
            end_early_message(&ApplicationError::Unauthorized("forbidden".to_string())),
            cancel_message(&ApplicationError::Unauthorized("forbidden".to_string())),
        ] {
            assert!(
                !message.contains("forbidden"),
                "内部のエラー文字列をそのまま見せない: {message}"
            );
        }
    }

    #[test]
    fn cancelling_a_slot_of_a_recurring_reservation_says_the_rest_remains() {
        use crate::domain::aggregates::reservation::{
            OccurrenceKey, RecurrenceRule, RecurringSchedule, Reservation, ReservationId, Schedule,
            value_objects::{Gpu, Resource},
        };
        use crate::domain::common::{EmailAddress, TimePeriod};
        use chrono::Duration;

        let holder = EmailAddress::new("holder@example.com".to_string()).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];
        let start = Utc::now() + Duration::days(1);
        let period = TimePeriod::new(start, start + Duration::hours(1)).unwrap();
        let mut recurring = Reservation::reconstruct(
            ReservationId::from_string("weekly".to_string()),
            holder.clone(),
            resources.clone(),
            None,
            Schedule::Recurring(RecurringSchedule::new(
                period.clone(),
                RecurrenceRule::parse(vec!["RRULE:FREQ=WEEKLY".to_string()]).unwrap(),
                chrono_tz::UTC,
                Default::default(),
            )),
            None,
        )
        .unwrap();
        let mut one_time = Reservation::one_time(holder.clone(), period, resources, None).unwrap();

        let slot_cancelled = recurring
            .cancel_slot(&OccurrenceKey::At(start), &holder)
            .unwrap();
        let reservation_cancelled = one_time.cancel_slot(&OccurrenceKey::Only, &holder).unwrap();

        assert!(
            cancelled_message(&slot_cancelled).contains("残っています"),
            "定期予約ごと消えたと受け取られてはいけない"
        );
        assert_eq!(
            cancelled_message(&reservation_cancelled),
            "✅ 予約を取り消しました。"
        );
    }
}
