//! 使われていない予約について、同じ予約で繰り返し声をかけないための記録

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

/// 使われていない予約について、しばらく声をかけないための記録
///
/// ビジネス上の不変条件ではなく、通知がうるさくならないようにするための状態である。
/// プロセス内メモリのみで保持し永続化しない（再起動後に改めて知らせても実害はない）。
/// 予約者が「まだ使う」と答えたときにも、その予約を黙らせるために使う。
///
/// 黙るのは期限つきである。「まだ使う」は使うつもりだという申告であって、
/// その予約が終わるまで何をしても構わないという意味ではない。申告どおりに
/// 使われたかどうかは、しばらく経ってからもう一度見る。
#[derive(Debug)]
pub struct IdleNoticeLog {
    silence_duration: Duration,
    silenced_until: Mutex<HashMap<String, DateTime<Utc>>>,
}

impl IdleNoticeLog {
    /// 新しい記録を作成
    ///
    /// # Arguments
    /// * `silence_duration` - 一度声をかけてから、次に声をかけるまで置く時間
    pub fn new(silence_duration: Duration) -> Self {
        Self {
            silence_duration,
            silenced_until: Mutex::new(HashMap::new()),
        }
    }

    /// この予約について、しばらく声をかけないようにする
    pub fn silence(&self, usage_id: &UsageId, now: DateTime<Utc>) {
        self.silenced_until
            .lock()
            .unwrap()
            .insert(usage_id.as_str().to_string(), now + self.silence_duration);
    }

    /// この予約について、いま声をかけずにいるか
    pub fn is_silenced(&self, usage_id: &UsageId, now: DateTime<Utc>) -> bool {
        self.silenced_until
            .lock()
            .unwrap()
            .get(usage_id.as_str())
            .is_some_and(|until| now < *until)
    }

    /// ふたたび声をかける対象に戻す
    pub fn resume(&self, usage_id: &UsageId) {
        self.silenced_until
            .lock()
            .unwrap()
            .remove(usage_id.as_str());
    }

    /// いま覚えておく意味のある予約についてだけ記録を残す
    ///
    /// 終わった予約について黙っていることに意味はない。放っておくと、
    /// プロセスが動き続けるあいだ記録だけが増えていく。
    pub fn retain_only(&self, is_worth_remembering: impl Fn(&str) -> bool) {
        self.silenced_until
            .lock()
            .unwrap()
            .retain(|usage_id, _| is_worth_remembering(usage_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> IdleNoticeLog {
        IdleNoticeLog::new(Duration::hours(4))
    }

    #[test]
    fn a_notice_log_keeps_only_what_is_worth_remembering() {
        let now = Utc::now();
        let kept = UsageId::from_string("running".to_string());
        let dropped = UsageId::from_string("finished".to_string());
        let log = log();
        log.silence(&kept, now);
        log.silence(&dropped, now);

        log.retain_only(|usage_id| usage_id == kept.as_str());

        assert!(log.is_silenced(&kept, now));
        assert!(
            !log.is_silenced(&dropped, now),
            "終わった予約について黙り続ける意味はない"
        );
    }

    #[test]
    fn silencing_and_resuming_are_reversible() {
        let now = Utc::now();
        let usage_id = UsageId::from_string("running".to_string());
        let log = log();

        assert!(!log.is_silenced(&usage_id, now));
        log.silence(&usage_id, now);
        assert!(log.is_silenced(&usage_id, now));
        log.resume(&usage_id);
        assert!(!log.is_silenced(&usage_id, now));
    }

    #[test]
    fn silence_wears_off_so_a_promise_to_use_it_is_looked_at_again() {
        let now = Utc::now();
        let usage_id = UsageId::from_string("running".to_string());
        let log = log();

        log.silence(&usage_id, now);

        assert!(log.is_silenced(&usage_id, now + Duration::hours(3)));
        assert!(
            !log.is_silenced(&usage_id, now + Duration::hours(4)),
            "「まだ使う」と答えたきり使わない予約に、二度と声をかけられないのはおかしい"
        );
    }
}
