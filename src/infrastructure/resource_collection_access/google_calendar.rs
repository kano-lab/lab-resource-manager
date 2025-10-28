use crate::domain::aggregates::identity_link::value_objects::EmailAddress;
use crate::domain::ports::resource_collection_access::{
    ResourceCollectionAccessError, ResourceCollectionAccessService,
};
use async_trait::async_trait;
use google_calendar3::{
    CalendarHub,
    api::{AclRule, AclRuleScope},
    hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
    hyper_util::{
        client::legacy::{Client, connect::HttpConnector},
        rt::TokioExecutor,
    },
    yup_oauth2,
};

/// Google Calendar API を使用したリソースコレクションアクセスサービス
///
/// GoogleカレンダーをResourceUsageのコレクションとして利用し、
/// ACL（Access Control List）を通じてユーザーのアクセス権限を管理する。
pub struct GoogleCalendarAccessService {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
}

impl GoogleCalendarAccessService {
    /// サービスアカウントキーから新しいインスタンスを作成
    ///
    /// # 引数
    /// * `service_account_key` - サービスアカウントキーのJSONファイルパス
    pub async fn new(service_account_key: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let secret = yup_oauth2::read_service_account_key(service_account_key).await?;

        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret)
            .build()
            .await?;

        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(connector);

        let hub = CalendarHub::new(client, auth);

        Ok(Self { hub })
    }
}

#[async_trait]
impl ResourceCollectionAccessService for GoogleCalendarAccessService {
    async fn grant_access(
        &self,
        calendar_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError> {
        let scope = AclRuleScope {
            type_: Some("user".to_string()),
            value: Some(email.as_str().to_string()),
        };

        let rule = AclRule {
            role: Some("writer".to_string()),
            scope: Some(scope),
            ..Default::default()
        };

        self.hub
            .acl()
            .insert(rule, calendar_id)
            .doit()
            .await
            .map_err(|e| {
                ResourceCollectionAccessError::ApiError(format!(
                    "Failed to share calendar '{}' with {}: {}",
                    calendar_id,
                    email.as_str(),
                    e
                ))
            })?;

        Ok(())
    }

    async fn revoke_access(
        &self,
        calendar_id: &str,
        email: &EmailAddress,
    ) -> Result<(), ResourceCollectionAccessError> {
        // First, find the ACL rule ID for this email
        let acl_list = self.hub.acl().list(calendar_id).doit().await.map_err(|e| {
            ResourceCollectionAccessError::ApiError(format!(
                "Failed to list ACL for calendar '{}': {}",
                calendar_id, e
            ))
        })?;

        let rule_id = acl_list
            .1
            .items
            .and_then(|items| {
                items.into_iter().find(|rule| {
                    rule.scope
                        .as_ref()
                        .and_then(|scope| scope.value.as_ref())
                        .map(|value| value == email.as_str())
                        .unwrap_or(false)
                })
            })
            .and_then(|rule| rule.id)
            .ok_or_else(|| {
                ResourceCollectionAccessError::Unknown(format!(
                    "No ACL rule found for {} on calendar '{}'",
                    email.as_str(),
                    calendar_id
                ))
            })?;

        // Delete the ACL rule
        self.hub
            .acl()
            .delete(calendar_id, &rule_id)
            .doit()
            .await
            .map_err(|e| {
                ResourceCollectionAccessError::ApiError(format!(
                    "Failed to revoke access for {} on calendar '{}': {}",
                    email.as_str(),
                    calendar_id,
                    e
                ))
            })?;

        Ok(())
    }
}
