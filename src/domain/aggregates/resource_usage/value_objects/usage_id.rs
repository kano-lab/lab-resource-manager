/// リソース使用予定の識別子
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsageId(String);

impl UsageId {
    /// 新しい使用予定IDを作成
    ///
    /// # Arguments
    /// * `id` - ID文字列
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
