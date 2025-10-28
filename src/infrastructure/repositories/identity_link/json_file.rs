use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::SlackUserId,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{IdentityLinkRepository, RepositoryError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;

/// JSON file storage for IdentityLink
///
/// ファイルフォーマット:
/// ```json
/// {
///   "user@example.com": {
///     "email": "user@example.com",
///     "slack_user_id": "U12345678",
///     "created_at": "2024-01-01T00:00:00Z",
///     "slack_linked_at": "2024-01-01T00:00:00Z"
///   }
/// }
/// ```
pub struct JsonFileIdentityLinkRepository {
    file_path: PathBuf,
    cache: RwLock<HashMap<String, IdentityLinkDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityLinkDto {
    email: String,
    slack_user_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    slack_linked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl IdentityLinkDto {
    fn from_entity(entity: &IdentityLink) -> Self {
        Self {
            email: entity.email().as_str().to_string(),
            slack_user_id: entity.slack_user_id().map(|id| id.as_str().to_string()),
            created_at: entity.created_at(),
            slack_linked_at: entity.slack_linked_at(),
        }
    }

    fn to_entity(&self) -> Result<IdentityLink, RepositoryError> {
        let email = EmailAddress::new(self.email.clone())
            .map_err(|e| RepositoryError::Unknown(format!("Invalid email: {}", e)))?;

        let slack_user_id = self
            .slack_user_id
            .as_ref()
            .map(|id| SlackUserId::new(id.clone()));

        let identity =
            IdentityLink::reconstitute(email, slack_user_id, self.created_at, self.slack_linked_at);

        Ok(identity)
    }
}

impl JsonFileIdentityLinkRepository {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            cache: RwLock::new(HashMap::new()),
        }
    }

    async fn load(&self) -> Result<(), RepositoryError> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| RepositoryError::Unknown(format!("Failed to read file: {}", e)))?;

        let data: HashMap<String, IdentityLinkDto> = serde_json::from_str(&content)
            .map_err(|e| RepositoryError::Unknown(format!("Failed to parse JSON: {}", e)))?;

        let mut cache = self.cache.write().await;
        *cache = data;

        Ok(())
    }

    async fn save_to_file(&self) -> Result<(), RepositoryError> {
        let cache = self.cache.read().await;

        let content = serde_json::to_string_pretty(&*cache)
            .map_err(|e| RepositoryError::Unknown(format!("Failed to serialize JSON: {}", e)))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                RepositoryError::Unknown(format!("Failed to create directory: {}", e))
            })?;
        }

        tokio::fs::write(&self.file_path, content)
            .await
            .map_err(|e| RepositoryError::Unknown(format!("Failed to write file: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl IdentityLinkRepository for JsonFileIdentityLinkRepository {
    async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }

        let cache = self.cache.read().await;
        match cache.get(email.as_str()) {
            Some(dto) => Ok(Some(dto.to_entity()?)),
            None => Ok(None),
        }
    }

    async fn find_by_slack_user_id(
        &self,
        slack_user_id: &SlackUserId,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        // Load from file if cache is empty
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }

        let cache = self.cache.read().await;
        for dto in cache.values() {
            if let Some(id) = &dto.slack_user_id {
                if id == slack_user_id.as_str() {
                    return Ok(Some(dto.to_entity()?));
                }
            }
        }

        Ok(None)
    }

    async fn save(&self, identity: IdentityLink) -> Result<(), RepositoryError> {
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }

        let dto = IdentityLinkDto::from_entity(&identity);
        let email_key = identity.email().as_str().to_string();

        {
            let mut cache = self.cache.write().await;
            cache.insert(email_key, dto);
        }

        self.save_to_file().await?;

        Ok(())
    }

    async fn find_all(&self) -> Result<Vec<IdentityLink>, RepositoryError> {
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }

        let cache = self.cache.read().await;
        let mut result = Vec::new();

        for dto in cache.values() {
            result.push(dto.to_entity()?);
        }

        Ok(result)
    }

    async fn delete(&self, email: &EmailAddress) -> Result<(), RepositoryError> {
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }

        {
            let mut cache = self.cache.write().await;
            cache.remove(email.as_str());
        }

        self.save_to_file().await?;

        Ok(())
    }
}
