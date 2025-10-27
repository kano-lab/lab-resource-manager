use crate::application::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::ports::{NotificationEvent, Notifier};
use std::collections::HashMap;

pub struct NotifyResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    repository: R,
    notifier: N,
    previous_state: tokio::sync::Mutex<HashMap<String, ResourceUsage>>,
}

impl<R, N> NotifyResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    pub async fn new(repository: R, notifier: N) -> Result<Self, ApplicationError> {
        let instance = Self {
            repository,
            notifier,
            previous_state: tokio::sync::Mutex::new(HashMap::new()),
        };

        let current_usages = instance.fetch_current_usages().await?;
        *instance.previous_state.lock().await = current_usages;

        Ok(instance)
    }

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
        let usages = self.repository.find_all().await?;
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
            if let Some(previous_usage) = previous.get(id) {
                if previous_usage != current_usage {
                    self.notify_updated(current_usage.clone()).await?;
                }
            }
        }
        Ok(())
    }

    async fn detect_and_notify_deleted_usages(
        &self,
        previous: &HashMap<String, ResourceUsage>,
        current: &HashMap<String, ResourceUsage>,
    ) -> Result<(), ApplicationError> {
        for (id, usage) in previous {
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
        let event = NotificationEvent::ResourceUsageDeleted {
            id: usage.id().clone(),
            user: usage.user().clone(),
        };
        self.notifier.notify(event).await?;
        Ok(())
    }
}
