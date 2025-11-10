//! Button interaction handlers
//!
//! This module contains handlers for button clicks (cancel, edit, etc.)
//! TODO: Refactor the actual implementations from commands.rs into these functions.

use crate::application::usecases::delete_resource_usage::DeleteResourceUsageUseCase;
use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::infrastructure::config::ResourceConfig;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Handle cancel reservation button click
///
/// TODO: Move implementation from SlackCommandHandler::handle_cancel_reservation
pub async fn handle_cancel_reservation<R: ResourceUsageRepository + Send + Sync + 'static>(
    _slack_user_id: &SlackUserId,
    _usage_id: &UsageId,
    _delete_usage_usecase: &Arc<DeleteResourceUsageUseCase<R>>,
    _identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Implementation to be moved from commands.rs
    unimplemented!("To be refactored from SlackCommandHandler")
}

/// Handle edit reservation button click
///
/// TODO: Move implementation from SlackCommandHandler::handle_edit_reservation
pub async fn handle_edit_reservation<R: ResourceUsageRepository + Send + Sync + 'static>(
    _slack_user_id: &SlackUserId,
    _usage_id_str: &str,
    _trigger_id: &SlackTriggerId,
    _slack_client: &Arc<SlackHyperClient>,
    _bot_token: &SlackApiToken,
    _identity_repo: &Arc<dyn IdentityLinkRepository>,
    _config: &Arc<ResourceConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Implementation to be moved from commands.rs
    unimplemented!("To be refactored from SlackCommandHandler")
}
