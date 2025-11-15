//! Google Calendar Event ID マッピング (内部実装)

use crate::domain::ports::repositories::RepositoryError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Google Calendar の外部ID (calendar_id + event_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ExternalId {
    /// Calendar ID
    pub calendar_id: String,
    /// Event ID
    pub event_id: String,
}

/// Domain ID と Google Calendar Event ID のマッピングを管理
pub(super) struct IdMapper {
    file_path: PathBuf,
    mappings: Arc<Mutex<HashMap<String, ExternalId>>>,
}

impl IdMapper {
    /// 新しいIdMapperを作成
    pub(super) fn new(file_path: PathBuf) -> Result<Self, RepositoryError> {
        let mappings = if file_path.exists() {
            Self::load_from_file(&file_path)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            file_path,
            mappings: Arc::new(Mutex::new(mappings)),
        })
    }

    /// マッピングを保存
    pub(super) fn save_mapping(
        &self,
        domain_id: &str,
        external_id: ExternalId,
    ) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.insert(domain_id.to_string(), external_id);
        drop(mappings);

        self.save_to_file()?;
        Ok(())
    }

    /// Domain ID から外部ID を取得
    pub(super) fn get_external_id(
        &self,
        domain_id: &str,
    ) -> Result<Option<ExternalId>, RepositoryError> {
        let mappings = self.mappings.lock().unwrap();
        Ok(mappings.get(domain_id).cloned())
    }

    /// Event ID から Domain ID を取得（逆引き）
    pub(super) fn get_domain_id(&self, event_id: &str) -> Result<Option<String>, RepositoryError> {
        let mappings = self.mappings.lock().unwrap();
        for (domain_id, external_id) in mappings.iter() {
            if external_id.event_id == event_id {
                return Ok(Some(domain_id.clone()));
            }
        }
        Ok(None)
    }

    /// マッピングを削除
    pub(super) fn delete_mapping(&self, domain_id: &str) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.remove(domain_id);
        drop(mappings);

        self.save_to_file()?;
        Ok(())
    }

    /// ファイルから全データを読み込み
    fn load_from_file(file_path: &PathBuf) -> Result<HashMap<String, ExternalId>, RepositoryError> {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            RepositoryError::ConnectionError(format!("マッピングファイルの読み込みに失敗: {}", e))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            RepositoryError::Unknown(format!("マッピングファイルのパースに失敗: {}", e))
        })
    }

    /// 全データをファイルに保存
    fn save_to_file(&self) -> Result<(), RepositoryError> {
        let mappings = self.mappings.lock().unwrap();

        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RepositoryError::ConnectionError(format!("ディレクトリの作成に失敗: {}", e))
            })?;
        }

        let json = serde_json::to_string_pretty(&*mappings)
            .map_err(|e| RepositoryError::Unknown(format!("JSONのシリアライズに失敗: {}", e)))?;

        std::fs::write(&self.file_path, json).map_err(|e| {
            RepositoryError::ConnectionError(format!("マッピングファイルの書き込みに失敗: {}", e))
        })?;

        Ok(())
    }
}
