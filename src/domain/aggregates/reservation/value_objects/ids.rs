/// 予約の識別子
///
/// 単発予約と定期予約のどちらも、この識別子で指す。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReservationId(String);

impl ReservationId {
    /// 既存のID文字列から予約IDを再構築
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 予約枠の識別子
///
/// 単発予約の予約枠は、その予約と同じ識別子を持つ（予約と予約枠が1対1のため）。
/// 定期予約の予約枠は、それぞれ別の識別子を持つ。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReservedSlotId(String);

impl Default for ReservedSlotId {
    fn default() -> Self {
        Self::new()
    }
}

impl ReservedSlotId {
    /// 新しい予約枠IDを生成する（UUID v4）
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 既存のID文字列から予約枠IDを再構築
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
