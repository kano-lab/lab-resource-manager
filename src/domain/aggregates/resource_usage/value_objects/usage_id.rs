/// リソース使用予定の識別子
///
/// ドメイン層で管理する一意なID。
/// デフォルトではUUID v4を生成しますが、外部システムのIDも受け入れます。
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
    ///
    /// # Arguments
    /// * `id` - ID文字列
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
