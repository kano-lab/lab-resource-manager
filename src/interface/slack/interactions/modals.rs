//! Modal submission processing logic
//!
//! This module contains the business logic for processing modal submissions.
//! TODO: Refactor the actual implementations from commands.rs into these functions.

use crate::application::usecases::{
    create_resource_usage::CreateResourceUsageUseCase,
    grant_user_resource_access::GrantUserResourceAccessUseCase,
    update_resource_usage::UpdateResourceUsageUseCase,
};
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::infrastructure::config::ResourceConfig;
use slack_morphism::prelude::*;
use std::sync::Arc;

/// Process new resource reservation submission
///
/// TODO: Move implementation from SlackCommandHandler::process_reservation_submission
pub async fn process_reservation_submission<R: ResourceUsageRepository + Send + Sync + 'static>(
    _view_submission: &SlackInteractionViewSubmissionEvent,
    _create_usage_usecase: &Arc<CreateResourceUsageUseCase<R>>,
    _identity_repo: &Arc<dyn IdentityLinkRepository>,
    _config: &Arc<ResourceConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Implementation to be moved from commands.rs
    unimplemented!("To be refactored from SlackCommandHandler")
}

/// Process resource reservation update submission
///
/// TODO: Move implementation from SlackCommandHandler::process_update_submission
pub async fn process_update_submission<R: ResourceUsageRepository + Send + Sync + 'static>(
    _view_submission: &SlackInteractionViewSubmissionEvent,
    _update_usage_usecase: &Arc<UpdateResourceUsageUseCase<R>>,
    _identity_repo: &Arc<dyn IdentityLinkRepository>,
    _config: &Arc<ResourceConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Implementation to be moved from commands.rs
    unimplemented!("To be refactored from SlackCommandHandler")
}

/// Process email registration submission
///
/// TODO: Move implementation from SlackCommandHandler::process_registration_submission
pub async fn process_registration_submission(
    _view_submission: &SlackInteractionViewSubmissionEvent,
    _grant_access_usecase: &Arc<GrantUserResourceAccessUseCase>,
    _slack_client: &Arc<SlackHyperClient>,
    _bot_token: &SlackApiToken,
    _config: &Arc<ResourceConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Implementation to be moved from commands.rs
    unimplemented!("To be refactored from SlackCommandHandler")
}
