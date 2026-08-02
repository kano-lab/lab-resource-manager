//! ユースケースのテストで共有する部品

use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::{ExternalIdentity, ExternalSystem},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{IdentityLinkRepository, RepositoryError};
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
