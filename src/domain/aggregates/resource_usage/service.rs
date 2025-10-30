use super::entity::ResourceUsage;
use super::errors::ResourceUsageError;
use super::value_objects::Resource;

pub struct UsageConflictChecker;

// TODO@KinjiKawaguchi: もう少し自明なコードを書いてインラインコメントを減らす
impl Default for UsageConflictChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageConflictChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check_conflicts(
        &self,
        new_usage: &ResourceUsage,
        existing_usages: &[ResourceUsage],
    ) -> Result<(), ResourceUsageError> {
        for existing in existing_usages {
            // 同じ使用予定はスキップ（更新の場合）
            if existing.id() == new_usage.id() {
                continue;
            }

            // 時間が重複していなければスキップ
            if !new_usage
                .time_period()
                .overlaps_with(existing.time_period())
            {
                continue;
            }

            // 資源の競合をチェック
            for new_resource in new_usage.resources() {
                for existing_resource in existing.resources() {
                    if new_resource.conflicts_with(existing_resource) {
                        return Err(ResourceUsageError::UsageConflict {
                            resource: format_resource_item(new_resource),
                            conflicting_user: existing.owner_email().as_str().to_string(),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// リソースを人間が読みやすい文字列にフォーマットする
///
/// 通知メッセージやエラーメッセージで使用するための純粋関数
pub fn format_resource_item(item: &Resource) -> String {
    match item {
        Resource::Gpu(spec) => {
            format!(
                "サーバー: {}, モデル: {}, デバイスID: {}",
                spec.server(),
                spec.model(),
                spec.device_number()
            )
        }
        Resource::Room { name } => name.clone(),
    }
}

/// 複数のリソースを改行で区切り、インデントを付けて結合した文字列にフォーマットする
///
/// 通知メッセージやログ出力で使用するための純粋関数
///
/// # 出力例
/// ```text
///   - サーバー: Thalys, モデル: A100 80GB PCIe, デバイスID: 1
///   - サーバー: Thalys, モデル: A100 80GB PCIe, デバイスID: 2
/// ```
pub fn format_resources(resources: &[Resource]) -> String {
    if resources.is_empty() {
        return String::new();
    }

    resources
        .iter()
        .map(|r| format!("  - {}", format_resource_item(r)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 時間期間を人間が読みやすい文字列にフォーマットする
///
/// タイムゾーンを指定した場合、そのタイムゾーンでの時刻とタイムゾーン名が表示される
/// 指定がない場合はUTCで表示される
///
/// # 例
/// ```
/// // UTCの場合: "2024-01-15 10:00 - 2024-01-15 12:00 (UTC)"
/// // JST指定の場合: "2024-01-15 19:00 - 2024-01-15 21:00 (JST)"
/// ```
pub fn format_time_period(
    period: &super::value_objects::TimePeriod,
    timezone_str: Option<&str>,
) -> String {
    use chrono_tz::Tz;

    // タイムゾーンのパースを試みる
    let tz_result = timezone_str.and_then(|s| s.parse::<Tz>().ok());

    match tz_result {
        Some(tz) => {
            // 指定されたタイムゾーンに変換して表示
            let start = period.start().with_timezone(&tz);
            let end = period.end().with_timezone(&tz);
            format!(
                "{} - {} ({})",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M"),
                tz.name()
            )
        }
        None => {
            // タイムゾーンが指定されていないか無効な場合はUTCで表示
            format!(
                "{} - {} (UTC)",
                period.start().format("%Y-%m-%d %H:%M"),
                period.end().format("%Y-%m-%d %H:%M")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_format_time_period_utc_default() {
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = super::super::value_objects::TimePeriod::new(start, end).unwrap();

        let result = format_time_period(&period, None);
        assert_eq!(result, "2024-01-15 10:00 - 2024-01-15 12:00 (UTC)");
    }

    #[test]
    fn test_format_time_period_with_jst() {
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = super::super::value_objects::TimePeriod::new(start, end).unwrap();

        let result = format_time_period(&period, Some("Asia/Tokyo"));
        // Timezone name returns the IANA name, not the abbreviation
        assert_eq!(result, "2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo)");
    }

    #[test]
    fn test_format_time_period_with_est() {
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = super::super::value_objects::TimePeriod::new(start, end).unwrap();

        let result = format_time_period(&period, Some("America/New_York"));
        // Timezone name returns the IANA name, not the abbreviation
        assert_eq!(
            result,
            "2024-01-15 05:00 - 2024-01-15 07:00 (America/New_York)"
        );
    }

    #[test]
    fn test_format_time_period_invalid_timezone() {
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = super::super::value_objects::TimePeriod::new(start, end).unwrap();

        // 無効なタイムゾーンの場合はUTCにフォールバック
        let result = format_time_period(&period, Some("Invalid/Timezone"));
        assert_eq!(result, "2024-01-15 10:00 - 2024-01-15 12:00 (UTC)");
    }
}
