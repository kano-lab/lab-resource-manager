/// リソース使用予定の識別子
///
/// ドメイン層で管理する一意なID（UUID v4）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsageId(String);

impl Default for UsageId {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageId {
    /// 新しいUsageIdを生成
    ///
    /// UUID v4を使用して一意なIDを生成します。
    ///
    /// # Returns
    /// UUID v4形式の新しいUsageId
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 既存のID文字列からUsageIdを再構築
    ///
    /// リポジトリから読み込んだデータを復元する際に使用します。
    /// 入力文字列が有効なUUID形式であることを検証します。
    ///
    /// # Arguments
    /// * `id` - UUID形式のID文字列
    ///
    /// # Errors
    /// 入力文字列が有効なUUID形式でない場合、エラーメッセージを返します。
    pub fn from_string(id: String) -> Result<Self, String> {
        // UUID形式のバリデーション
        uuid::Uuid::parse_str(&id)
            .map(|_| Self(id))
            .map_err(|e| format!("Invalid UUID format: {}", e))
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
