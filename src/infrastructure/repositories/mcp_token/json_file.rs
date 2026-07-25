use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{McpTokenRepository, RepositoryError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use uuid::Uuid;

/// JSON file storage for MCP access tokens
///
/// ファイルフォーマット:
/// ```json
/// {
///   "<token>": {
///     "email": "user@example.com",
///     "issued_at": "2024-01-01T00:00:00Z"
///   }
/// }
/// ```
pub struct JsonFileMcpTokenRepository {
    file_path: PathBuf,
    cache: RwLock<HashMap<String, McpTokenDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpTokenDto {
    email: String,
    issued_at: chrono::DateTime<chrono::Utc>,
}

impl JsonFileMcpTokenRepository {
    /// 新しいJSONファイルベースのリポジトリを作成
    ///
    /// # Arguments
    /// * `file_path` - JSONファイルのパス
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
                )));
            }
        };

        let data: HashMap<String, McpTokenDto> = serde_json::from_str(&content)
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
impl McpTokenRepository for JsonFileMcpTokenRepository {
    async fn issue_token(&self, email: &EmailAddress) -> Result<String, RepositoryError> {
        self.ensure_loaded().await?;

        let token = Uuid::new_v4().to_string();
        let dto = McpTokenDto {
            email: email.as_str().to_string(),
            issued_at: chrono::Utc::now(),
        };

        {
            let mut cache = self.cache.write().await;
            cache.retain(|_, existing| existing.email != email.as_str());
            cache.insert(token.clone(), dto);
        }

        self.save_to_file().await?;

        Ok(token)
    }

    async fn resolve(&self, token: &str) -> Result<Option<EmailAddress>, RepositoryError> {
        self.ensure_loaded().await?;

        let cache = self.cache.read().await;
        match cache.get(token) {
            Some(dto) => Ok(Some(EmailAddress::new(dto.email.clone())?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file() -> PathBuf {
        std::env::temp_dir().join(format!("lrm_test_mcp_tokens_{}.json", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn issue_token_can_be_resolved_back_to_the_email() {
        let repo = JsonFileMcpTokenRepository::new(temp_file());
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();

        let token = repo.issue_token(&email).await.unwrap();
        let resolved = repo.resolve(&token).await.unwrap();

        assert_eq!(resolved, Some(email));
    }

    #[tokio::test]
    async fn resolve_unknown_token_returns_none() {
        let repo = JsonFileMcpTokenRepository::new(temp_file());

        let resolved = repo.resolve("not-a-real-token").await.unwrap();

        assert_eq!(resolved, None);
    }

    #[tokio::test]
    async fn reissuing_a_token_revokes_the_previous_one() {
        let repo = JsonFileMcpTokenRepository::new(temp_file());
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();

        let old_token = repo.issue_token(&email).await.unwrap();
        let new_token = repo.issue_token(&email).await.unwrap();

        assert_ne!(old_token, new_token);
        assert_eq!(repo.resolve(&old_token).await.unwrap(), None);
        assert_eq!(repo.resolve(&new_token).await.unwrap(), Some(email));
    }

    #[tokio::test]
    async fn token_survives_a_reload_from_disk() {
        let path = temp_file();
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();

        let token = {
            let repo = JsonFileMcpTokenRepository::new(path.clone());
            repo.issue_token(&email).await.unwrap()
        };

        let repo = JsonFileMcpTokenRepository::new(path);
        let resolved = repo.resolve(&token).await.unwrap();

        assert_eq!(resolved, Some(email));
    }
}
