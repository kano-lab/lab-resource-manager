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
                "🖥️ サーバー: {}, モデル: {}, デバイスID: {}",
                spec.server(),
                spec.model(),
                spec.device_number()
            )
        }
        Resource::Room { name } => format!("🚪 {}", name),
    }
}

/// 複数のリソースを改行で区切り、インデントを付けて結合した文字列にフォーマットする
///
/// 通知メッセージやログ出力で使用するための純粋関数
///
/// # 出力例
/// ```text
///   - 🖥️ サーバー: Thalys, モデル: A100 80GB PCIe, デバイスID: 1
///   - 🖥️ サーバー: Thalys, モデル: A100 80GB PCIe, デバイスID: 2
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
/// "YYYY-MM-DD HH:MM - YYYY-MM-DD HH:MM" の形式で返す
pub fn format_time_period(period: &super::value_objects::TimePeriod) -> String {
    format!(
        "{} - {}",
        period.start().format("%Y-%m-%d %H:%M"),
        period.end().format("%Y-%m-%d %H:%M")
    )
}

#[cfg(test)]
mod tests {
    use super::super::value_objects::Gpu;
    use super::*;

    #[test]
    fn test_format_gpu_resource() {
        let gpu = Gpu::new("Thalys".to_string(), 0, "A100 80GB PCIe".to_string());
        let resource = Resource::Gpu(gpu);
        let formatted = format_resource_item(&resource);

        assert!(formatted.contains("🖥️"));
        assert!(formatted.contains("Thalys"));
        assert!(formatted.contains("A100 80GB PCIe"));
        assert!(formatted.contains("0"));
    }

    #[test]
    fn test_format_room_resource() {
        let resource = Resource::Room {
            name: "会議室A".to_string(),
        };
        let formatted = format_resource_item(&resource);

        assert!(formatted.contains("🚪"));
        assert!(formatted.contains("会議室A"));
    }

    #[test]
    fn test_format_multiple_resources() {
        let gpu1 = Gpu::new("Thalys".to_string(), 0, "A100".to_string());
        let gpu2 = Gpu::new("Thalys".to_string(), 1, "A100".to_string());
        let resources = vec![Resource::Gpu(gpu1), Resource::Gpu(gpu2)];

        let formatted = format_resources(&resources);

        // 各行に絵文字が含まれることを確認
        assert!(formatted.contains("🖥️"));
        // 改行で区切られていることを確認
        assert!(formatted.contains('\n'));
        // インデントが含まれることを確認
        assert!(formatted.contains("  - "));
    }

    #[test]
    fn test_format_empty_resources() {
        let resources: Vec<Resource> = vec![];
        let formatted = format_resources(&resources);

        assert_eq!(formatted, "");
    }
}
