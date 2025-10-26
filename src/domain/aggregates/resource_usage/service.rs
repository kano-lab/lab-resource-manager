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

fn format_resource_item(item: &Resource) -> String {
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
