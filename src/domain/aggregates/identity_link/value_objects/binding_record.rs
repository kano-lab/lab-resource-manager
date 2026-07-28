use super::ExternalIdentity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 識別子を結びつけた記録
///
/// 識別子そのもの（どのシステムの、どの名前か）と、それを結びつけた出来事
/// （いつ結びつけたか）は別の関心である。両者を1つの値にまとめると、同じメンバーを
/// 指す識別子が、結びつけた時刻が違うだけで別物として扱われてしまう。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalIdentityBindingRecord {
    /// 結びつけた識別子
    identity: ExternalIdentity,
    /// 結びつけた日時
    linked_at: DateTime<Utc>,
}

impl ExternalIdentityBindingRecord {
    /// 識別子を今結びつけた記録を作る
    pub fn new(identity: ExternalIdentity) -> Self {
        Self {
            identity,
            linked_at: Utc::now(),
        }
    }

    /// 永続化層からの復元用
    pub(crate) fn reconstitute(identity: ExternalIdentity, linked_at: DateTime<Utc>) -> Self {
        Self {
            identity,
            linked_at,
        }
    }

    /// 結びつけた識別子を取得
    pub fn identity(&self) -> &ExternalIdentity {
        &self.identity
    }

    /// 結びつけた日時を取得
    pub fn linked_at(&self) -> DateTime<Utc> {
        self.linked_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;

    fn os_identity() -> ExternalIdentity {
        ExternalIdentity::new(
            ExternalSystem::Os {
                server: "Thalys".to_string(),
            },
            "kkawaguchi".to_string(),
        )
    }

    #[test]
    fn records_differ_when_the_binding_time_differs() {
        // 記録としては、いつ結びつけたかまで含めて同じであるときに同じ
        let earlier = ExternalIdentityBindingRecord::reconstitute(
            os_identity(),
            Utc::now() - chrono::Duration::days(30),
        );
        let later = ExternalIdentityBindingRecord::reconstitute(os_identity(), Utc::now());

        assert_ne!(earlier, later);
        assert_eq!(
            earlier.identity(),
            later.identity(),
            "結びつけた識別子そのものは同じ"
        );
    }
}
