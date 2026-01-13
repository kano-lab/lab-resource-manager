use super::entity::ResourceUsage;
use super::errors::ResourceUsageError;
use super::value_objects::Resource;

/// リソース使用の競合をチェックするドメインサービス
pub struct UsageConflictChecker;

// TODO@KinjiKawaguchi: もう少し自明なコードを書いてインラインコメントを減らす
impl Default for UsageConflictChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageConflictChecker {
    /// 新しいチェッカーを作成
    pub fn new() -> Self {
        Self
    }

    /// リソース使用の競合をチェック
    ///
    /// # Arguments
    /// * `new_usage` - チェック対象の新しい使用予定
    /// * `existing_usages` - 既存の使用予定リスト
    ///
    /// # Errors
    /// 競合がある場合、`ResourceUsageError::UsageConflict`を返す
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
                "{} / {} / GPU:{}",
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
/// Thalys / A100 80GB PCIe / GPU:1
/// Thalys / A100 80GB PCIe / GPU:2
/// ```
pub fn format_resources(resources: &[Resource]) -> String {
    if resources.is_empty() {
        return String::new();
    }

    resources
        .iter()
        .map(format_resource_item)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 時間期間を人間が読みやすい文字列にフォーマットする
///
/// タイムゾーンを指定した場合、そのタイムゾーンでの時刻とタイムゾーン名が表示される
/// 指定がない場合はシステムのローカルタイムゾーンで表示される（取得できない場合はUTC）
///
/// # 例
/// ```text
/// システムローカル(JST)の場合: "2024-01-15 19:00 - 2024-01-15 21:00 (+09:00)"
/// タイムゾーン指定の場合: "2024-01-15 05:00 - 2024-01-15 07:00 (America/New_York)"
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
            // タイムゾーンが指定されていない場合はシステムのローカルタイムゾーンを使用
            use chrono::Local;
            let start_local = period.start().with_timezone(&Local);
            let end_local = period.end().with_timezone(&Local);

            // ローカルタイムゾーンのオフセットを取得して表示
            let offset = start_local.offset();
            let offset_str = offset.to_string();

            format!(
                "{} - {} ({})",
                start_local.format("%Y-%m-%d %H:%M"),
                end_local.format("%Y-%m-%d %H:%M"),
                offset_str
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::value_objects::Gpu;
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_format_time_period_system_default() {
        let start = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = chrono::Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = super::super::value_objects::TimePeriod::new(start, end).unwrap();

        // タイムゾーン未指定の場合はシステムのローカルタイムゾーンを使用
        // テスト環境によって異なる可能性があるため、フォーマットが正しいことのみを確認
        let result = format_time_period(&period, None);
        assert!(result.contains("2024-01-15"));
        assert!(result.contains(" - "));
        assert!(result.contains("("));
        assert!(result.contains(")"));
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

        // 無効なタイムゾーンの場合はシステムのローカルタイムゾーンにフォールバック
        let result = format_time_period(&period, Some("Invalid/Timezone"));
        // システムのタイムゾーンによって異なるため、フォーマットが正しいことのみを確認
        assert!(result.contains("2024-01-15"));
        assert!(result.contains(" - "));
        assert!(result.contains("("));
        assert!(result.contains(")"));
    }

    #[test]
    fn test_format_gpu_resource() {
        let gpu = Gpu::new("Thalys".to_string(), 0, "A100 80GB PCIe".to_string());
        let resource = Resource::Gpu(gpu);
        let formatted = format_resource_item(&resource);

        assert_eq!(formatted, "Thalys / A100 80GB PCIe / GPU:0");
    }

    #[test]
    fn test_format_room_resource() {
        let resource = Resource::Room {
            name: "会議室A".to_string(),
        };
        let formatted = format_resource_item(&resource);

        assert_eq!(formatted, "会議室A");
    }

    #[test]
    fn test_format_multiple_resources() {
        let gpu1 = Gpu::new("Thalys".to_string(), 0, "A100".to_string());
        let gpu2 = Gpu::new("Thalys".to_string(), 1, "A100".to_string());
        let resources = vec![Resource::Gpu(gpu1), Resource::Gpu(gpu2)];

        let formatted = format_resources(&resources);

        assert_eq!(formatted, "Thalys / A100 / GPU:0\nThalys / A100 / GPU:1");
    }

    #[test]
    fn test_format_empty_resources() {
        let resources: Vec<Resource> = vec![];
        let formatted = format_resources(&resources);

        assert_eq!(formatted, "");
    }
}
