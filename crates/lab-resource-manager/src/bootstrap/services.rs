//! リポジトリとユースケースの組み立て
//!
//! 入口（Slack・MCP）と定期ジョブが共有する依存をここで一度だけ作る。

use super::BoxError;
use crate::application::idle_notice_log::IdleNoticeLog;
use crate::application::usecases::describe_monitoring::{
    DescribeMonitoringUseCase, MonitoringSettings,
};
use crate::application::usecases::{
    AcceptReservationProposalUseCase, CancelReservedSlotUseCase, CheckResourceAvailabilityUseCase,
    CreateReservationUseCase, EndReservedSlotEarlyUseCase, GrantUserResourceAccessUseCase,
    UpdateReservedSlotUseCase,
};
use crate::domain::aggregates::reservation::ResourceCatalog;
use crate::domain::ports::ResourceUsageObserver;
use crate::infrastructure::config::{AppConfig, ResourceConfig, StorageConfig};
use crate::infrastructure::repositories::{
    identity_link::JsonFileIdentityLinkRepository, mcp_token::JsonFileMcpTokenRepository,
    reservation::google_calendar::GoogleCalendarReservationRepository,
};
use crate::infrastructure::resource_collection_access::GoogleCalendarAccessService;
use crate::infrastructure::resource_usage_observer::SharedFileResourceUsageObserver;
use chrono::Duration;
use std::sync::Arc;

/// 予約を保存するリポジトリ
pub(super) type Repository = GoogleCalendarReservationRepository;

/// 入口と定期ジョブが共有する依存
pub(super) struct Services {
    pub resource_config: Arc<ResourceConfig>,
    pub reservation_repo: Arc<Repository>,
    pub identity_repo: Arc<JsonFileIdentityLinkRepository>,
    pub mcp_token_repo: Arc<JsonFileMcpTokenRepository>,
    pub grant_access: Arc<GrantUserResourceAccessUseCase>,
    pub reservations: ReservationUseCases,
    /// 実利用の観測（`GPU_USAGE_REPORTS_DIR`が未設定なら`None`）
    pub observer: Option<Arc<SharedFileResourceUsageObserver>>,
    pub describe_monitoring: Arc<DescribeMonitoringUseCase>,
    /// 予約者が「まだ使う」と答えたことを、Slackの応答と観測とで共有する記録
    pub idle_notices: Arc<IdleNoticeLog>,
}

/// SlackとMCPの両方から呼ばれる、予約を操作するユースケース
pub(super) struct ReservationUseCases {
    pub create: Arc<CreateReservationUseCase<Repository>>,
    pub accept_proposal: Arc<AcceptReservationProposalUseCase<Repository>>,
    pub update_slot: Arc<UpdateReservedSlotUseCase<Repository>>,
    pub cancel_slot: Arc<CancelReservedSlotUseCase<Repository>>,
    pub end_slot_early: Arc<EndReservedSlotEarlyUseCase<Repository>>,
    pub check_availability: Arc<CheckResourceAvailabilityUseCase<Repository>>,
}

impl Services {
    /// 設定から依存を組み立てる
    ///
    /// # Errors
    /// サービスアカウントの鍵を読めない、Google Calendarにつなげないなど
    pub(super) async fn build(
        app_config: &AppConfig,
        resource_config: ResourceConfig,
        storage_config: StorageConfig,
    ) -> Result<Self, BoxError> {
        let resource_config = Arc::new(resource_config);
        let service_account_key = app_config
            .google_service_account_key_path
            .to_str()
            .ok_or("サービスアカウントキーパスが不正なUTF-8です")?;

        let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(
            app_config.identity_links_file.clone(),
        ));
        let reservation_repo = Arc::new(
            GoogleCalendarReservationRepository::new(
                service_account_key,
                resource_config.as_ref().clone(),
                identity_repo.clone(),
                storage_config.clone(),
            )
            .await
            .map_err(|e| format!("予約のカレンダーにつなげません: {e}"))?,
        );
        let grant_access =
            build_grant_access(service_account_key, &storage_config, &identity_repo).await?;
        let observer = build_observer(app_config, &resource_config);

        Ok(Self {
            describe_monitoring: build_describe_monitoring(app_config, observer.clone()),
            reservations: ReservationUseCases::new(
                &reservation_repo,
                Arc::new(resource_config.catalog()),
            ),
            mcp_token_repo: Arc::new(JsonFileMcpTokenRepository::new(
                app_config.mcp_tokens_file.clone(),
            )),
            idle_notices: Arc::new(IdleNoticeLog::new(seconds(
                app_config.idle_notice_silence_secs,
            ))),
            resource_config,
            reservation_repo,
            identity_repo,
            grant_access,
            observer,
        })
    }
}

impl ReservationUseCases {
    fn new(repo: &Arc<Repository>, catalog: Arc<ResourceCatalog>) -> Self {
        Self {
            create: Arc::new(CreateReservationUseCase::new(repo.clone(), catalog.clone())),
            accept_proposal: Arc::new(AcceptReservationProposalUseCase::new(
                repo.clone(),
                catalog.clone(),
            )),
            update_slot: Arc::new(UpdateReservedSlotUseCase::new(repo.clone())),
            cancel_slot: Arc::new(CancelReservedSlotUseCase::new(repo.clone())),
            end_slot_early: Arc::new(EndReservedSlotEarlyUseCase::new(repo.clone())),
            check_availability: Arc::new(CheckResourceAvailabilityUseCase::new(
                repo.clone(),
                catalog,
            )),
        }
    }
}

/// 利用者の登録時に、全リソースのカレンダーへのアクセス権を渡すユースケース
async fn build_grant_access(
    service_account_key: &str,
    storage_config: &StorageConfig,
    identity_repo: &Arc<JsonFileIdentityLinkRepository>,
) -> Result<Arc<GrantUserResourceAccessUseCase>, BoxError> {
    let collection_ids: Vec<String> = storage_config
        .google_calendar_mappings()
        .ok_or("Google Calendarバックエンドの設定が見つかりません")?
        .values()
        .cloned()
        .collect();

    Ok(Arc::new(GrantUserResourceAccessUseCase::new(
        identity_repo.clone(),
        Arc::new(
            GoogleCalendarAccessService::new(service_account_key)
                .await
                .map_err(|e| format!("カレンダーの共有設定を扱えません: {e}"))?,
        ),
        collection_ids,
    )))
}

/// 実利用の観測（レポートの置き場が設定されていなければ観測しない）
fn build_observer(
    app_config: &AppConfig,
    resource_config: &Arc<ResourceConfig>,
) -> Option<Arc<SharedFileResourceUsageObserver>> {
    app_config.gpu_usage_reports_dir.clone().map(|dir| {
        Arc::new(SharedFileResourceUsageObserver::new(
            dir,
            resource_config.clone(),
            seconds(app_config.gpu_usage_max_staleness_secs),
        ))
    })
}

fn build_describe_monitoring(
    app_config: &AppConfig,
    observer: Option<Arc<SharedFileResourceUsageObserver>>,
) -> Arc<DescribeMonitoringUseCase> {
    Arc::new(DescribeMonitoringUseCase::new(
        observer.map(|observer| observer as Arc<dyn ResourceUsageObserver>),
        MonitoringSettings {
            polling_interval: seconds(app_config.polling_interval_secs),
            idle_threshold: seconds(app_config.idle_reservation_threshold_secs),
            held_gpu_threshold: seconds(app_config.idle_held_gpu_threshold_secs),
            max_staleness: seconds(app_config.gpu_usage_max_staleness_secs),
        },
    ))
}

/// 設定の秒数を期間にする
pub(super) fn seconds(secs: u64) -> Duration {
    Duration::seconds(secs as i64)
}
