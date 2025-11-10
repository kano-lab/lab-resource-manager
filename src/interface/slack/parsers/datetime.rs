//! Date and time parsing utilities

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};

/// 日付文字列と時刻文字列をUTC DateTimeにパース
///
/// # Arguments
/// * `date_str` - 日付文字列 (YYYY-MM-DD形式)
/// * `time_str` - 時刻文字列 (HH:MM形式)
///
/// # Returns
/// パースされたUTC DateTime
///
/// # Errors
/// - 日付または時刻のフォーマットが不正な場合
/// - 無効な日時の場合
pub fn parse_datetime(
    date_str: &str,
    time_str: &str,
) -> Result<DateTime<Utc>, Box<dyn std::error::Error + Send + Sync>> {
    // 日付をパース (YYYY-MM-DD形式)
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| format!("日付のパースに失敗: {} ({})", date_str, e))?;

    // 時刻をパース (HH:MM形式)
    let time = NaiveTime::parse_from_str(time_str, "%H:%M")
        .map_err(|e| format!("時刻のパースに失敗: {} ({})", time_str, e))?;

    // 日付と時刻を結合
    let naive_datetime = date.and_time(time);

    // ローカルタイムゾーンでDateTime<Local>を作成してからUTCに変換
    let local_datetime = Local
        .from_local_datetime(&naive_datetime)
        .single()
        .ok_or_else(|| format!("無効な日時: {} {}", date_str, time_str))?;

    Ok(local_datetime.with_timezone(&Utc))
}
