use super::entity::ResourceUsage;
use super::errors::ResourceUsageError;
use super::value_objects::Resource;

pub struct UsageConflictChecker;

// TODO@KinjiKawaguchi: もう少し自明なコードを書いてインラインコメントを減らす
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
                            conflicting_user: existing.user().name().to_string(),
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
/// "YYYY-MM-DD HH:MM - YYYY-MM-DD HH:MM" の形式で返す
pub fn format_time_period(period: &super::value_objects::TimePeriod) -> String {
    format!(
        "{} - {}",
        period.start().format("%Y-%m-%d %H:%M"),
        period.end().format("%Y-%m-%d %H:%M")
    )
}
