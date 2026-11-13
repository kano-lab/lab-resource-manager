use chrono::{DateTime, NaiveDateTime, Utc};

/// 予約枠IDの中で、回の開始時刻を書く形式（UTC）
///
/// Google カレンダーが繰り返しイベントの個々の回に付けるIDと同じ形にしてある。
const OCCURRENCE_START_FORMAT: &str = "%Y%m%dT%H%M%SZ";

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OccurrenceKey {
    /// 単発予約の唯一の回
    Only,
    /// 定期予約の、本来この時刻に始まる回
    ///
    /// 回の時間を変えても鍵は変わらない。繰り返しの規則が決めた開始時刻で回を指す。
    At(DateTime<Utc>),
}

/// 予約枠ID（予約ID ＋ 回の鍵）
///
/// 予約枠は予約の中の回であり、予約の外から指すときはこのIDを使う。
/// 文字列にすると、単発予約の予約枠は予約IDと同じになり、定期予約の予約枠は
/// `予約ID_回の本来の開始時刻`（例: `abc123_20261116T010000Z`）になる。
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
            OccurrenceKey::At(start) => format!(
                "{}_{}",
                reservation_id.as_str(),
                start.format(OCCURRENCE_START_FORMAT)
            ),
        };
        Self {
            reservation_id,
            key,
            text,
        }
    }

    /// 文字列表現から予約枠IDを読み取る
    ///
    /// 末尾が`_回の開始時刻`なら定期予約の予約枠、そうでなければ単発予約の予約枠
    /// （予約IDと同じ文字列）として読む。予約IDに`_`は使われない。
    pub fn from_string(text: String) -> Self {
        let occurrence = text.rsplit_once('_').and_then(|(reservation_id, start)| {
            NaiveDateTime::parse_from_str(start, OCCURRENCE_START_FORMAT)
                .ok()
                .map(|start| (reservation_id.to_string(), start.and_utc()))
        });
        match occurrence {
            Some((reservation_id, start)) => Self::new(
                ReservationId::from_string(reservation_id),
                OccurrenceKey::At(start),
            ),
            None => Self::new(ReservationId::from_string(text), OccurrenceKey::Only),
        }
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

    #[test]
    fn the_slot_of_a_recurring_reservation_reads_as_the_id_and_its_original_start() {
        use chrono::TimeZone;
        let reservation_id = ReservationId::from_string("series1".to_string());
        let start = Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap();

        let slot_id = ReservedSlotId::new(reservation_id.clone(), OccurrenceKey::At(start));

        assert_eq!(slot_id.as_str(), "series1_20261116T010000Z");
        assert_eq!(
            ReservedSlotId::from_string("series1_20261116T010000Z".to_string()),
            slot_id
        );
        assert_eq!(slot_id.reservation_id(), &reservation_id);
    }

    #[test]
    fn an_id_whose_last_part_is_not_a_start_time_is_a_one_time_slot() {
        // `_`を含んでも、末尾が回の開始時刻でなければ定期予約の回ではない
        let slot_id = ReservedSlotId::from_string("legacy_event_id".to_string());

        assert_eq!(slot_id.key(), &OccurrenceKey::Only);
        assert_eq!(slot_id.reservation_id().as_str(), "legacy_event_id");
        assert_eq!(slot_id.as_str(), "legacy_event_id");
    }
}
