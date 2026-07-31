//! 使われていない予約について、同じ予約で繰り返し声をかけないための記録

use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use std::collections::HashSet;
use std::sync::Mutex;

/// 使われていない予約について、同じ予約で繰り返し声をかけないための記録
///
/// ビジネス上の不変条件ではなく、通知がうるさくならないようにするための状態である。
/// プロセス内メモリのみで保持し永続化しない（再起動後に改めて知らせても実害はない）。
/// 予約者が「まだ使う」と答えたときにも、その予約を黙らせるために使う。
#[derive(Debug, Default)]
pub struct IdleNoticeLog {
    silenced: Mutex<HashSet<String>>,
}

impl IdleNoticeLog {
    /// この予約について当面は声をかけないようにする
    pub fn silence(&self, usage_id: &UsageId) {
        self.silenced
            .lock()
            .unwrap()
            .insert(usage_id.as_str().to_string());
    }

    /// この予約について声をかけずにいるか
    pub fn is_silenced(&self, usage_id: &UsageId) -> bool {
        self.silenced.lock().unwrap().contains(usage_id.as_str())
    }

    /// ふたたび声をかける対象に戻す
    pub fn resume(&self, usage_id: &UsageId) {
        self.silenced.lock().unwrap().remove(usage_id.as_str());
    }

    /// いま覚えておく意味のある予約についてだけ記録を残す
    ///
    /// 終わった予約について黙っていることに意味はない。放っておくと、
    /// プロセスが動き続けるあいだ記録だけが増えていく。
    pub fn retain_only(&self, is_worth_remembering: impl Fn(&str) -> bool) {
        self.silenced
            .lock()
            .unwrap()
            .retain(|usage_id| is_worth_remembering(usage_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_log_keeps_only_what_is_worth_remembering() {
        let kept = UsageId::from_string("running".to_string());
        let dropped = UsageId::from_string("finished".to_string());
        let log = IdleNoticeLog::default();
        log.silence(&kept);
        log.silence(&dropped);

        log.retain_only(|usage_id| usage_id == kept.as_str());

        assert!(log.is_silenced(&kept));
        assert!(
            !log.is_silenced(&dropped),
            "終わった予約について黙り続ける意味はない"
        );
    }

    #[test]
    fn silencing_and_resuming_are_reversible() {
        let usage_id = UsageId::from_string("running".to_string());
        let log = IdleNoticeLog::default();

        assert!(!log.is_silenced(&usage_id));
        log.silence(&usage_id);
        assert!(log.is_silenced(&usage_id));
        log.resume(&usage_id);
        assert!(!log.is_silenced(&usage_id));
    }
}
