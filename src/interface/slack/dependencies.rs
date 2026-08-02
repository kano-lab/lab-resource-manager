//! Slackボットが必要とする依存
//!
//! ボットが呼び出すユースケースと参照するリポジトリを、それぞれひとつの束として扱います。
//!
//! ## 束ねる理由
//!
//! これらは常に一緒に組み立てられ、一緒に`SlackApp`へ渡ります。個別の引数として
//! 並べると、機能をひとつ足すたびにフィールド・引数・初期化・アクセサ・
//! コンポジションルートの5箇所を触ることになり、追加のたびに同じ場所を
//! 編集し続けることになります。束にすれば、触るのは束の定義と組み立て側の2箇所です。

use crate::application::usecases::{
    AcceptReservationProposalUseCase, CheckResourceAvailabilityUseCase, CreateResourceUsageUseCase,
    DeleteResourceUsageUseCase, DescribeMonitoringUseCase, GrantUserResourceAccessUseCase,
    NotifyFutureResourceUsageChangesUseCase, ReleaseResourceUsageEarlyUseCase,
    UpdateResourceUsageUseCase,
};
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{
    IdentityLinkRepository, McpTokenRepository, ResourceUsageRepository,
};
use std::sync::Arc;

/// Slackボットが呼び出すユースケース
pub struct SlackUseCases<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    /// リソースへのアクセス権を付与する
    pub grant_access: Arc<GrantUserResourceAccessUseCase>,
    /// 予約を作成する
    pub create_resource_usage: Arc<CreateResourceUsageUseCase<R>>,
    /// 事後予約の提案を受諾する
    pub accept_reservation_proposal: Arc<AcceptReservationProposalUseCase<R>>,
    /// 予約を更新する
    pub update_resource_usage: Arc<UpdateResourceUsageUseCase<R>>,
    /// 予約を取り消す
    pub delete_resource_usage: Arc<DeleteResourceUsageUseCase<R>>,
    /// 進行中の予約を今の時点で締める
    pub release_resource_usage_early: Arc<ReleaseResourceUsageEarlyUseCase<R>>,
    /// リソースの空きを調べる
    pub check_resource_availability: Arc<CheckResourceAvailabilityUseCase<R>>,
    /// 予約の変更を監視して通知する
    pub notify_resource_usage_changes: Arc<NotifyFutureResourceUsageChangesUseCase<R, N>>,
    /// 実利用の監視の稼働状況を答える
    pub describe_monitoring: Arc<DescribeMonitoringUseCase>,
}

/// Slackボットが参照するリポジトリ
///
/// 予約リポジトリはユースケースの内側にあるため、ここには現れない。
pub struct SlackRepositories {
    /// 識別情報とメールアドレスの紐付け
    pub identity_link: Arc<dyn IdentityLinkRepository>,
    /// MCPアクセストークン
    pub mcp_token: Arc<dyn McpTokenRepository>,
}
