//! Slackへの接続とSlackボットの組み立て

use super::services::{Repository, Services};
use crate::infrastructure::config::SlackTokens;
use crate::interface::slack::{SlackApp, SlackRepositories, SlackUseCases};
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Slackへの接続
///
/// ボットのほか、予約者へDMを送るジョブも使う。
pub(super) struct SlackConnection {
    pub client: Arc<SlackHyperClient>,
    /// Bot User OAuth Token（xoxb-...）
    pub bot_token: SlackApiToken,
    /// Socket Modeの接続に使うApp-Level Token（xapp-...）
    pub app_token: SlackApiToken,
}

impl SlackConnection {
    /// 設定のトークンでSlackにつなぐ準備をする
    ///
    /// # Errors
    /// HTTPクライアントを作れない場合
    pub(super) fn new(tokens: &SlackTokens) -> Result<Self, super::BoxError> {
        Ok(Self {
            client: Arc::new(SlackClient::new(SlackClientHyperConnector::new()?)),
            bot_token: SlackApiToken::new(tokens.bot_token.clone().into()),
            app_token: SlackApiToken::new(tokens.app_token.clone().into()),
        })
    }
}

/// Slackボットを組み立てる
pub(super) fn build_bot(services: &Services, slack: SlackConnection) -> Arc<SlackApp<Repository>> {
    let reservations = &services.reservations;

    Arc::new(SlackApp::new(
        services.resource_config.clone(),
        SlackRepositories {
            identity_link: services.identity_repo.clone(),
            mcp_token: services.mcp_token_repo.clone(),
        },
        SlackUseCases {
            grant_access: services.grant_access.clone(),
            create_reservation: reservations.create.clone(),
            accept_reservation_proposal: reservations.accept_proposal.clone(),
            update_reserved_slot: reservations.update_slot.clone(),
            cancel_reserved_slot: reservations.cancel_slot.clone(),
            end_reserved_slot_early: reservations.end_slot_early.clone(),
            check_resource_availability: reservations.check_availability.clone(),
            describe_monitoring: services.describe_monitoring.clone(),
        },
        services.idle_notices.clone(),
        slack.client,
        slack.bot_token,
        slack.app_token,
    ))
}
