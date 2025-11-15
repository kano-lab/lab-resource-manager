//! Domain ID と Google Calendar Event ID のマッピング

use crate::domain::ports::repositories::RepositoryError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// イベントIDマッピング
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMapping {
    /// インフラの種類 (例: "google_calendar", "outlook", "notion")
    pub infrastructure: String,
    /// 外部システムのEvent ID
    pub event_id: String,
    /// Calendar ID (どのカレンダーに属するか)
    pub calendar_id: String,
}

/// Domain ID と Event ID のマッピングを管理
pub trait IdMapper: Send + Sync {
    /// マッピングを保存
    fn save_mapping(
        &self,
        domain_id: &str,
        infrastructure: &str,
        event_id: &str,
        calendar_id: &str,
    ) -> Result<(), RepositoryError>;

    /// Domain ID から Event ID を取得
    fn get_event_id(&self, domain_id: &str) -> Result<Option<EventMapping>, RepositoryError>;

    /// Event ID から Domain ID を取得（逆引き）
    fn get_domain_id(&self, event_id: &str) -> Result<Option<String>, RepositoryError>;

    /// マッピングを削除
    fn delete_mapping(&self, domain_id: &str) -> Result<(), RepositoryError>;
}

/// JSONファイルベースのIDマッパー
pub struct JsonFileIdMapper {
    file_path: PathBuf,
    mappings: Arc<Mutex<HashMap<String, EventMapping>>>,
}

impl JsonFileIdMapper {
    /// 新しいJsonFileIdMapperを作成
    ///
    /// # Arguments
    /// * `file_path` - マッピングファイルのパス
    pub fn new(file_path: PathBuf) -> Result<Self, RepositoryError> {
        let mappings = if file_path.exists() {
            Self::load_from_file(&file_path)?
        } else {
            // ファイルが存在しない場合は空のマッピング
            HashMap::new()
        };

        Ok(Self {
            file_path,
            mappings: Arc::new(Mutex::new(mappings)),
        })
    }

    /// ファイルから全データを読み込み
    fn load_from_file(
        file_path: &PathBuf,
    ) -> Result<HashMap<String, EventMapping>, RepositoryError> {
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

        // ディレクトリが存在しない場合は作成
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

impl IdMapper for JsonFileIdMapper {
    fn save_mapping(
        &self,
        domain_id: &str,
        infrastructure: &str,
        event_id: &str,
        calendar_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.insert(
            domain_id.to_string(),
            EventMapping {
                infrastructure: infrastructure.to_string(),
                event_id: event_id.to_string(),
                calendar_id: calendar_id.to_string(),
            },
        );
        drop(mappings); // ロック解放

        self.save_to_file()?;
        Ok(())
    }

    fn get_event_id(&self, domain_id: &str) -> Result<Option<EventMapping>, RepositoryError> {
        let mappings = self.mappings.lock().unwrap();
        Ok(mappings.get(domain_id).cloned())
    }

    fn get_domain_id(&self, event_id: &str) -> Result<Option<String>, RepositoryError> {
        let mappings = self.mappings.lock().unwrap();
        // 全マッピングを走査して event_id が一致するものを探す
        for (domain_id, mapping) in mappings.iter() {
            if mapping.event_id == event_id {
                return Ok(Some(domain_id.clone()));
            }
        }
        Ok(None)
    }

    fn delete_mapping(&self, domain_id: &str) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.remove(domain_id);
        drop(mappings);

        self.save_to_file()?;
        Ok(())
    }
}
