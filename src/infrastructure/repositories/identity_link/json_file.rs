use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::{ExternalIdentity, ExternalSystem},
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
///     "external_identities": [
///       {
///         "system": "slack",
///         "user_id": "U12345678",
///         "linked_at": "2024-01-01T00:00:00Z"
///       }
///     ],
///     "created_at": "2024-01-01T00:00:00Z",
///     "updated_at": "2024-01-01T00:00:00Z"
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
    external_identities: Vec<ExternalIdentityDto>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExternalIdentityDto {
    system: String,
    user_id: String,
    linked_at: chrono::DateTime<chrono::Utc>,
}

impl IdentityLinkDto {
    fn from_entity(entity: &IdentityLink) -> Self {
        let external_identities = entity
            .external_identities()
            .iter()
            .map(|id| ExternalIdentityDto {
                system: id.system().as_str().to_string(),
                user_id: id.user_id().to_string(),
                linked_at: id.linked_at(),
            })
            .collect();

        Self {
            email: entity.email().as_str().to_string(),
            external_identities,
            created_at: entity.created_at(),
            updated_at: entity.updated_at(),
        }
    }

    fn to_entity(&self) -> Result<IdentityLink, RepositoryError> {
        let email = EmailAddress::new(self.email.clone())?;

        let external_identities: Vec<ExternalIdentity> = self
            .external_identities
            .iter()
            .filter_map(|dto| {
                // 現在サポートしているシステムのみ復元
                ExternalSystem::from_str(&dto.system).map(|system| {
                    ExternalIdentity::reconstitute(system, dto.user_id.clone(), dto.linked_at)
                })
            })
            .collect();

        let identity = IdentityLink::reconstitute(
            email,
            external_identities,
            self.created_at,
            self.updated_at,
        );

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
        let content = match tokio::fs::read_to_string(&self.file_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // ファイルが存在しない場合は空の状態として扱う
                return Ok(());
            }
            Err(e) => {
                return Err(RepositoryError::Unknown(format!(
                    "ファイルの読み込みに失敗: {}",
                    e
                )))
            }
        };

        let data: HashMap<String, IdentityLinkDto> = serde_json::from_str(&content)
            .map_err(|e| RepositoryError::Unknown(format!("JSONのパースに失敗: {}", e)))?;

        let mut cache = self.cache.write().await;
        *cache = data;

        Ok(())
    }

    /// キャッシュが空の場合、ファイルから読み込む
    async fn ensure_loaded(&self) -> Result<(), RepositoryError> {
        if self.cache.read().await.is_empty() {
            self.load().await?;
        }
        Ok(())
    }

    async fn save_to_file(&self) -> Result<(), RepositoryError> {
        let cache = self.cache.read().await;

        let content = serde_json::to_string_pretty(&*cache)
            .map_err(|e| RepositoryError::Unknown(format!("JSONのシリアライズに失敗: {}", e)))?;

        // 親ディレクトリが存在しない場合は作成
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                RepositoryError::Unknown(format!("ディレクトリの作成に失敗: {}", e))
            })?;
        }

        tokio::fs::write(&self.file_path, content)
            .await
            .map_err(|e| RepositoryError::Unknown(format!("ファイルの書き込みに失敗: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl IdentityLinkRepository for JsonFileIdentityLinkRepository {
    async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        self.ensure_loaded().await?;

        let cache = self.cache.read().await;
        match cache.get(email.as_str()) {
            Some(dto) => Ok(Some(dto.to_entity()?)),
            None => Ok(None),
        }
    }

    async fn find_by_external_user_id(
        &self,
        system: &ExternalSystem,
        user_id: &str,
    ) -> Result<Option<IdentityLink>, RepositoryError> {
        self.ensure_loaded().await?;

        let cache = self.cache.read().await;
        for dto in cache.values() {
            for external_id in &dto.external_identities {
                if external_id.system == system.as_str() && external_id.user_id == user_id {
                    return Ok(Some(dto.to_entity()?));
                }
            }
        }

        Ok(None)
    }

    async fn save(&self, identity: IdentityLink) -> Result<(), RepositoryError> {
        self.ensure_loaded().await?;

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
        self.ensure_loaded().await?;

        let cache = self.cache.read().await;
        let mut result = Vec::new();

        for dto in cache.values() {
            result.push(dto.to_entity()?);
        }

        Ok(result)
    }

    async fn delete(&self, email: &EmailAddress) -> Result<(), RepositoryError> {
        self.ensure_loaded().await?;

        {
            let mut cache = self.cache.write().await;
            cache.remove(email.as_str());
        }

        self.save_to_file().await?;

        Ok(())
    }
}
