use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::resource_usage::errors::{ConflictCheckError, ResourceConflictError};
use std::collections::HashMap;

/// リソース競合チェックサービス
///
/// 指定された時間帯とリソースが既存の予約と競合しないかをチェックする
///
/// # 判定の範囲
/// 判定の対象は予約である。永続化層にあって予約として解釈されないレコードは
/// 予約ではないため、競合の対象にならない（`ResourceUsageRepository`を参照）。
///
/// したがって競合なしとは「予約と重ならない」ことであり、そのリソースが実際に
/// 使われていないことは意味しない。予約されないまま使われるリソースは
/// 実利用の観測側で扱う。
#[derive(Debug, Clone, Default)]
pub struct ResourceConflictChecker;

impl ResourceConflictChecker {
    pub fn new() -> Self {
        Self
    }

    /// リソース競合をチェック
    ///
    /// # Arguments
    /// * `repository` - リソース使用リポジトリ
    /// * `time_period` - チェック対象の時間帯
    /// * `resources` - チェック対象のリソースリスト
    /// * `exclude_usage_id` - チェックから除外するUsageID（更新時に自分自身を除外するため）
    ///
    /// # Returns
    /// 競合がない場合はOk(())、競合がある場合はエラー。
    ///
    /// 競合は既存予約ごとに1件へまとめ、その予約と重なったリソースを併せて返す。
    /// 打ち切りは行わない。複数のリソースが競合していれば全件を、ひとつのリソースが
    /// 複数の予約と競合していれば（既存の予約同士が重複している場合に起こりうる）
    /// その全ての予約を返す。時間をずらせば解決するのか、他にも障害があるのかを
    /// 呼び出し側が判断できる必要がある。
    ///
    /// # Errors
    /// - 競合するリソースがある場合
    /// - リポジトリエラー
    pub async fn check_conflicts<R: ResourceUsageRepository>(
        &self,
        repository: &R,
        time_period: &TimePeriod,
        resources: &[Resource],
        exclude_usage_id: Option<&UsageId>,
    ) -> Result<(), ConflictCheckError> {
        // 指定期間と重複する予約を検索
        let overlapping = repository.find_overlapping(time_period).await?;

        // 競合は既存予約ごとに1件へまとめる。ひとつの予約が複数のリソースを押さえている
        // 場合、リソースごとに分けて返すと受け取った側がまとめ直すことになる。
        // 検出した順序を保つため、予約IDから収集先の位置を引く。
        let mut conflicts: Vec<ResourceConflictError> = Vec::new();
        let mut position_of: HashMap<UsageId, usize> = HashMap::new();

        for new_resource in resources {
            for existing_usage in &overlapping {
                // 除外対象の場合はスキップ
                if let Some(exclude_id) = exclude_usage_id
                    && existing_usage.id() == exclude_id
                {
                    continue;
                }

                let conflicts_with_this_usage = existing_usage
                    .resources()
                    .iter()
                    .any(|existing_resource| new_resource.conflicts_with(existing_resource));

                if conflicts_with_this_usage {
                    match position_of.get(existing_usage.id()) {
                        Some(&position) => {
                            conflicts[position].resources.push(new_resource.clone());
                        }
                        None => {
                            position_of.insert(existing_usage.id().clone(), conflicts.len());
                            conflicts.push(ResourceConflictError::new(
                                vec![new_resource.clone()],
                                existing_usage.clone(),
                            ));
                        }
                    }
                }
            }
        }

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(ConflictCheckError::Conflict(conflicts))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use crate::domain::ports::repositories::RepositoryError;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};

    /// `find_overlapping`の戻り値だけを固定できるテスト用リポジトリ
    struct StubRepository {
        overlapping: Vec<ResourceUsage>,
    }

    #[async_trait]
    impl ResourceUsageRepository for StubRepository {
        async fn find_by_id(
            &self,
            _id: &UsageId,
        ) -> Result<Option<ResourceUsage>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn find_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<ResourceUsage>, RepositoryError> {
            Ok(self.overlapping.clone())
        }

        async fn find_by_owner(
            &self,
            _owner_email: &EmailAddress,
            _time_period: &TimePeriod,
        ) -> Result<Vec<ResourceUsage>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save(&self, _usage: &ResourceUsage) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn delete(&self, _id: &UsageId) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    fn test_period() -> TimePeriod {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        TimePeriod::new(start, end).unwrap()
    }

    fn usage_with(owner: &str, resources: Vec<Resource>) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new(owner.to_string()).unwrap(),
            test_period(),
            resources,
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn collects_all_conflicting_resources_not_just_the_first() {
        let alice_usage = usage_with(
            "alice@example.com",
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
        );
        let bob_usage = usage_with(
            "bob@example.com",
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                1,
                "A100".to_string(),
            ))],
        );
        let repository = StubRepository {
            overlapping: vec![alice_usage, bob_usage],
        };

        // GPU:0(alice)とGPU:1(bob)の両方をまとめて予約しようとして、両方競合する
        let requested = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
        ];

        let checker = ResourceConflictChecker::new();
        let result = checker
            .check_conflicts(&repository, &test_period(), &requested, None)
            .await;

        let Err(ConflictCheckError::Conflict(conflicts)) = result else {
            panic!("expected Conflict, got {result:?}");
        };

        // 最初の1件で打ち切らず、両方の競合が報告される
        assert_eq!(conflicts.len(), 2);
        let owners: Vec<&str> = conflicts
            .iter()
            .map(|c| c.existing_usage.owner_email().as_str())
            .collect();
        assert!(owners.contains(&"alice@example.com"));
        assert!(owners.contains(&"bob@example.com"));
    }

    #[tokio::test]
    async fn no_conflict_when_resources_are_disjoint() {
        let alice_usage = usage_with(
            "alice@example.com",
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
        );
        let repository = StubRepository {
            overlapping: vec![alice_usage],
        };

        let requested = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            1,
            "A100".to_string(),
        ))];

        let checker = ResourceConflictChecker::new();
        let result = checker
            .check_conflicts(&repository, &test_period(), &requested, None)
            .await;

        assert!(result.is_ok());
    }
    #[tokio::test]
    async fn conflicts_with_one_reservation_are_reported_as_a_single_entry() {
        // ひとつの予約が3枚を押さえていて、その3枚をまとめて予約しようとする
        let existing = usage_with(
            "alice@example.com",
            vec![
                Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
                Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
                Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "A100".to_string())),
            ],
        );
        let repository = StubRepository {
            overlapping: vec![existing],
        };

        let requested = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "A100".to_string())),
        ];

        let checker = ResourceConflictChecker::new();
        let result = checker
            .check_conflicts(&repository, &test_period(), &requested, None)
            .await;

        let Err(ConflictCheckError::Conflict(conflicts)) = result else {
            panic!("expected Conflict, got {result:?}");
        };

        assert_eq!(
            conflicts.len(),
            1,
            "同じ予約との競合は1件にまとめるべき（受け取った側がまとめ直さずに済むように）"
        );
        assert_eq!(
            conflicts[0].resources.len(),
            3,
            "競合した全リソースを持つべき"
        );
    }

    #[tokio::test]
    async fn a_resource_conflicting_with_several_reservations_reports_all_of_them() {
        // 既存の予約同士が重複している状況（同じGPUを2件が押さえている）
        let gpu0 = || Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()));
        let repository = StubRepository {
            overlapping: vec![
                usage_with("alice@example.com", vec![gpu0()]),
                usage_with("bob@example.com", vec![gpu0()]),
            ],
        };

        let checker = ResourceConflictChecker::new();
        let result = checker
            .check_conflicts(&repository, &test_period(), &[gpu0()], None)
            .await;

        let Err(ConflictCheckError::Conflict(conflicts)) = result else {
            panic!("expected Conflict, got {result:?}");
        };

        // 片方だけ知らせると、時間をずらしてもまた失敗する
        assert_eq!(
            conflicts.len(),
            2,
            "ひとつのリソースが複数の予約と競合するなら、その全てを知らせるべき"
        );
        let owners: Vec<&str> = conflicts
            .iter()
            .map(|c| c.existing_usage.owner_email().as_str())
            .collect();
        assert!(owners.contains(&"alice@example.com"));
        assert!(owners.contains(&"bob@example.com"));
    }
}
