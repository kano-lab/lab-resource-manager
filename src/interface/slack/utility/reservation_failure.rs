//! 予約への操作が断られた理由を利用者に伝える文面
//!
//! 同じ操作でも入口はいくつかある（予約通知のボタン、未使用のお知らせのボタン）。
//! 断られる理由は入口ではなく操作で決まるため、文面もここにまとめる。
//! 内部のエラー文字列をそのまま見せず、次に何をすればよいかが分かる言葉にする。

use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::errors::ResourceUsageError;
use crate::domain::ports::repositories::RepositoryError;

/// 予約を今の時点で終了できなかった理由
pub fn release_message(failure: &ApplicationError) -> String {
    match failure {
        ApplicationError::ResourceUsage(ResourceUsageError::NotYetStarted { .. }) => {
            "ℹ️ この予約はまだ始まっていません。使わないのであれば「❌ キャンセル」で取り消してください。"
                .to_string()
        }
        ApplicationError::ResourceUsage(ResourceUsageError::AlreadyEnded { .. }) => {
            "ℹ️ この予約はすでに終わっています。".to_string()
        }
        ApplicationError::Repository(RepositoryError::NotFound) => {
            "❌ この予約は見つかりませんでした。すでに取り消されている可能性があります。".to_string()
        }
        ApplicationError::Unauthorized(_) => "❌ 自分の予約だけを終了できます。".to_string(),
        other => format!("❌ 予約を終了できませんでした: {}", other),
    }
}

/// 予約を取り消せなかった理由
pub fn cancel_message(failure: &ApplicationError) -> String {
    match failure {
        ApplicationError::Repository(RepositoryError::NotFound) => {
            "ℹ️ この予約はすでに取り消されています。".to_string()
        }
        ApplicationError::Unauthorized(_) => "❌ 自分の予約だけを取り消せます。".to_string(),
        other => format!("❌ 予約を取り消せませんでした: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn a_reservation_that_has_not_started_points_at_cancelling() {
        let message = release_message(&ApplicationError::ResourceUsage(
            ResourceUsageError::NotYetStarted {
                start: Utc::now(),
                at: Utc::now(),
            },
        ));

        assert!(
            message.contains("キャンセル"),
            "取り消しが正しい操作であることを伝えるべき: {message}"
        );
    }

    #[test]
    fn a_reservation_that_has_ended_is_reported_as_settled() {
        let message = release_message(&ApplicationError::ResourceUsage(
            ResourceUsageError::AlreadyEnded {
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
            release_message(&ApplicationError::Unauthorized("forbidden".to_string())),
            cancel_message(&ApplicationError::Unauthorized("forbidden".to_string())),
        ] {
            assert!(
                !message.contains("forbidden"),
                "内部のエラー文字列をそのまま見せない: {message}"
            );
        }
    }
}
