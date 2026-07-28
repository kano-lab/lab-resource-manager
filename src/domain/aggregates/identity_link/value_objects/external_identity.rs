use super::ExternalSystem;
use serde::{Deserialize, Serialize};

/// あるシステムにおけるメンバーの識別子
///
/// 「どのシステムの、どの名前か」で決まる。いつ結びつけたかは識別子そのものの性質では
/// ないため、`ExternalIdentityBindingRecord`が持つ。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalIdentity {
    /// システムの種類
    system: ExternalSystem,
    /// そのシステムでのユーザーID
    user_id: String,
}

impl ExternalIdentity {
    /// 新しい外部システム識別情報を作成
    ///
    /// # Arguments
    /// * `system` - 外部システムの種類
    /// * `user_id` - 外部システムでのユーザーID
    pub fn new(system: ExternalSystem, user_id: String) -> Self {
        Self { system, user_id }
    }

    /// 外部システムの種類を取得
    pub fn system(&self) -> &ExternalSystem {
        &self.system
    }

    /// そのシステムでのユーザーIDを取得
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn os_system() -> ExternalSystem {
        ExternalSystem::Os {
            server: "Thalys".to_string(),
        }
    }

    #[test]
    fn the_same_user_on_the_same_system_is_the_same_identity() {
        let a = ExternalIdentity::new(os_system(), "kkawaguchi".to_string());
        let b = ExternalIdentity::new(os_system(), "kkawaguchi".to_string());

        assert_eq!(a, b);
    }

    #[test]
    fn identities_can_be_used_as_a_map_key() {
        let mut set = HashSet::new();
        set.insert(ExternalIdentity::new(os_system(), "kkawaguchi".to_string()));
        set.insert(ExternalIdentity::new(os_system(), "kkawaguchi".to_string()));

        assert_eq!(set.len(), 1, "同じ利用者を表す値はキーとして一致するべき");
    }

    #[test]
    fn the_os_namespace_is_per_server() {
        let on_thalys = ExternalIdentity::new(os_system(), "kkawaguchi".to_string());
        let on_freccia = ExternalIdentity::new(
            ExternalSystem::Os {
                server: "Freccia".to_string(),
            },
            "kkawaguchi".to_string(),
        );

        assert_ne!(
            on_thalys, on_freccia,
            "OSユーザー名の名前空間はサーバーごとに独立している"
        );
    }

    #[test]
    fn different_systems_are_different_identities() {
        let os = ExternalIdentity::new(os_system(), "kkawaguchi".to_string());
        let slack = ExternalIdentity::new(ExternalSystem::Slack, "kkawaguchi".to_string());

        assert_ne!(os, slack);
    }
}
