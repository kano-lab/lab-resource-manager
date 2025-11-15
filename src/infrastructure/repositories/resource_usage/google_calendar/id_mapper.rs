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
    /// 逆引きマップ: event_id -> domain_id (O(1)検索用)
    reverse_mappings: Arc<Mutex<HashMap<String, String>>>,
}

impl IdMapper {
    /// 新しいIdMapperを作成
    pub(super) fn new(file_path: PathBuf) -> Result<Self, RepositoryError> {
        let mappings = if file_path.exists() {
            Self::load_from_file(&file_path)?
        } else {
            HashMap::new()
        };

        // 逆引きマップを構築
        let reverse_mappings: HashMap<String, String> = mappings
            .iter()
            .map(|(domain_id, external_id)| (external_id.event_id.clone(), domain_id.clone()))
            .collect();

        Ok(Self {
            file_path,
            mappings: Arc::new(Mutex::new(mappings)),
            reverse_mappings: Arc::new(Mutex::new(reverse_mappings)),
        })
    }

    /// マッピングを保存
    pub(super) fn save_mapping(
        &self,
        domain_id: &str,
        external_id: ExternalId,
    ) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        let mut reverse_mappings = self.reverse_mappings.lock().unwrap();

        // 既存のマッピングがある場合は逆引きマップから削除
        if let Some(old_external_id) = mappings.get(domain_id) {
            reverse_mappings.remove(&old_external_id.event_id);
        }

        // 新しいマッピングを追加
        reverse_mappings.insert(external_id.event_id.clone(), domain_id.to_string());
        mappings.insert(domain_id.to_string(), external_id);

        drop(mappings);
        drop(reverse_mappings);

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
        let reverse_mappings = self.reverse_mappings.lock().unwrap();
        Ok(reverse_mappings.get(event_id).cloned())
    }

    /// マッピングを削除
    pub(super) fn delete_mapping(&self, domain_id: &str) -> Result<(), RepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        let mut reverse_mappings = self.reverse_mappings.lock().unwrap();

        // 逆引きマップからも削除
        if let Some(external_id) = mappings.get(domain_id) {
            reverse_mappings.remove(&external_id.event_id);
        }

        mappings.remove(domain_id);

        drop(mappings);
        drop(reverse_mappings);

        self.save_to_file()?;
        Ok(())
    }

    /// ファイルから全データを読み込み
    fn load_from_file(file_path: &PathBuf) -> Result<HashMap<String, ExternalId>, RepositoryError> {
        // TODO(#41): 同期的I/Oを非同期化 (tokio::fs) またはキャッシング戦略を検討
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

        // TODO(#41): 同期的I/Oを非同期化 (tokio::fs) またはキャッシング戦略を検討
        std::fs::write(&self.file_path, json).map_err(|e| {
            RepositoryError::ConnectionError(format!("マッピングファイルの書き込みに失敗: {}", e))
        })?;

        Ok(())
    }
}
