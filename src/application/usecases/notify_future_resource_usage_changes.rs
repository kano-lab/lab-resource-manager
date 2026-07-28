use crate::application::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::ports::{NotificationEvent, Notifier};
use std::collections::HashMap;
use std::sync::Arc;

/// 未来および進行中のリソース使用状況の変更を監視し、通知するユースケース
///
/// このユースケースは以下の変更を検知して通知します:
/// - 新規作成: 新しいリソース使用予約が追加された
/// - 更新: 既存の予約内容が変更された
/// - 削除: **未来の予約**がキャンセル/削除された
///
/// # スコープ
/// このユースケースは「未来および進行中」のリソース使用のみを監視対象とします。
/// 予約期間が終了したリソースは自然に監視対象外となり、削除通知は送信されません。
pub struct NotifyFutureResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    repository: Arc<R>,
    notifier: N,
    previous_state: tokio::sync::Mutex<HashMap<String, ResourceUsage>>,
}

impl<R, N> NotifyFutureResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    /// 新しいインスタンスを作成し、初期状態を取得する
    ///
    /// # Arguments
    /// * `repository` - リソース使用リポジトリ（Arc で共有）
    /// * `notifier` - 通知サービス
    ///
    /// # Errors
    /// リポジトリから初期状態の取得に失敗した場合
    pub async fn new(repository: Arc<R>, notifier: N) -> Result<Self, ApplicationError> {
        let instance = Self {
            repository,
            notifier,
            previous_state: tokio::sync::Mutex::new(HashMap::new()),
        };

        let current_usages = instance.fetch_current_usages().await?;
        *instance.previous_state.lock().await = current_usages;

        Ok(instance)
    }

    /// 一度だけポーリングを実行し、変更を検知して通知する
    ///
    /// 前回の状態と現在の状態を比較し、作成・更新・削除された予約を検知して通知します。
    ///
    /// # Errors
    /// リポジトリアクセスまたは通知送信に失敗した場合
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let current_usages = self.fetch_current_usages().await?;
        let mut previous_usages = self.previous_state.lock().await;

        self.detect_and_notify_created_usages(&previous_usages, &current_usages)
            .await?;
        self.detect_and_notify_updated_usages(&previous_usages, &current_usages)
            .await?;
        self.detect_and_notify_deleted_usages(&previous_usages, &current_usages)
            .await?;

        *previous_usages = current_usages;

        Ok(())
    }

    async fn fetch_current_usages(
        &self,
    ) -> Result<HashMap<String, ResourceUsage>, ApplicationError> {
        let usages = self.repository.find_future().await?;
        Ok(usages
            .into_iter()
            .map(|usage| (usage.id().as_str().to_string(), usage))
            .collect())
    }

    async fn detect_and_notify_created_usages(
        &self,
        previous: &HashMap<String, ResourceUsage>,
        current: &HashMap<String, ResourceUsage>,
    ) -> Result<(), ApplicationError> {
        for (id, usage) in current {
            if !previous.contains_key(id) {
                self.notify_created(usage.clone()).await?;
            }
        }
        Ok(())
    }

    async fn detect_and_notify_updated_usages(
        &self,
        previous: &HashMap<String, ResourceUsage>,
        current: &HashMap<String, ResourceUsage>,
    ) -> Result<(), ApplicationError> {
        for (id, current_usage) in current {
            if let Some(previous_usage) = previous.get(id)
                && previous_usage != current_usage
            {
                self.notify_updated(current_usage.clone()).await?;
            }
        }
        Ok(())
    }

    async fn detect_and_notify_deleted_usages(
        &self,
        previous: &HashMap<String, ResourceUsage>,
        current: &HashMap<String, ResourceUsage>,
    ) -> Result<(), ApplicationError> {
        let now = chrono::Utc::now();

        // previousを現在時刻基準で「まだ未来」のものだけに絞る
        // (currentと同じ時間軸に合わせることで、自然な期限切れを削除と誤検知しない)
        let previous_still_future: HashMap<_, _> = previous
            .iter()
            .filter(|(_, usage)| usage.time_period().end() > now)
            .collect();

        // フィルタリング後のpreviousとcurrentを比較
        for (id, usage) in previous_still_future {
            if !current.contains_key(id) {
                self.notify_deleted(usage.clone()).await?;
            }
        }
        Ok(())
    }

    async fn notify_created(&self, usage: ResourceUsage) -> Result<(), ApplicationError> {
        let event = NotificationEvent::ResourceUsageCreated(usage);
        self.notifier.notify(event).await?;
        Ok(())
    }

    async fn notify_updated(&self, usage: ResourceUsage) -> Result<(), ApplicationError> {
        let event = NotificationEvent::ResourceUsageUpdated(usage);
        self.notifier.notify(event).await?;
        Ok(())
    }

    async fn notify_deleted(&self, usage: ResourceUsage) -> Result<(), ApplicationError> {
        let event = NotificationEvent::ResourceUsageDeleted(usage);
        self.notifier.notify(event).await?;
        Ok(())
    }
}
