/// 予約の識別子
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReservationId(String);

impl Default for ReservationId {
    fn default() -> Self {
        Self::new()
    }
}

impl ReservationId {
    /// 新しい予約IDを生成する（UUID v4、ハイフン無しの32桁）
    ///
    /// カレンダーのイベントIDに使える文字はbase32hex（英小文字a-vと数字0-9）に限られる。
    /// ハイフン無しのUUIDはその範囲に収まるので、予約IDをそのままイベントIDにできる。
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    /// 既存のID文字列から予約IDを再構築
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 予約の中で回を区別する鍵
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OccurrenceKey {
    /// 単発予約の唯一の回
    Only,
}

/// 予約枠ID（予約ID ＋ 回の鍵）
///
/// 予約枠は予約の中の回であり、予約の外から指すときはこのIDを使う。
/// 文字列にすると、単発予約の予約枠は予約IDと同じになる。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReservedSlotId {
    reservation_id: ReservationId,
    key: OccurrenceKey,
    text: String,
}

impl ReservedSlotId {
    /// 予約IDと回の鍵から予約枠IDを作る
    pub fn new(reservation_id: ReservationId, key: OccurrenceKey) -> Self {
        let text = match &key {
            OccurrenceKey::Only => reservation_id.as_str().to_string(),
        };
        Self {
            reservation_id,
            key,
            text,
        }
    }

    /// 文字列表現から予約枠IDを読み取る
    ///
    /// 単発予約の予約枠IDは、その予約IDと同じ文字列である。
    pub fn from_string(text: String) -> Self {
        Self::new(ReservationId::from_string(text), OccurrenceKey::Only)
    }

    /// この予約枠が属する予約のID
    pub fn reservation_id(&self) -> &ReservationId {
        &self.reservation_id
    }

    /// 予約の中で回を区別する鍵
    pub fn key(&self) -> &OccurrenceKey {
        &self.key
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// 保存先での版
///
/// 予約を読んだときの版を覚えておき、書き戻すときに保存先の版と比べる。
/// 読んでから書くまでの間に誰かが変えたり取り消したりしていれば、版が違うので書かない。
/// Google カレンダーではイベントの etag にあたる。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(String);

impl Version {
    /// 版の文字列から作る
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_reservation_id_can_be_used_as_a_calendar_event_id() {
        let id = ReservationId::new();

        assert_eq!(id.as_str().len(), 32);
        assert!(
            id.as_str()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='v').contains(&c)),
            "base32hex の範囲に収まる: {}",
            id.as_str()
        );
    }

    #[test]
    fn the_slot_of_a_one_time_reservation_reads_as_the_reservation_id() {
        let reservation_id = ReservationId::from_string("abc123".to_string());

        let slot_id = ReservedSlotId::new(reservation_id.clone(), OccurrenceKey::Only);

        assert_eq!(slot_id.as_str(), "abc123");
        assert_eq!(ReservedSlotId::from_string("abc123".to_string()), slot_id);
        assert_eq!(slot_id.reservation_id(), &reservation_id);
    }
}
