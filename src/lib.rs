//! # lab-resource-manager
//!
//! GPU and room resource management system with Google Calendar and Slack integration.
//!
//! This crate provides a resource management system designed for research labs,
//! tracking GPU servers and meeting room reservations through Google Calendar
//! and sending notifications via Slack.
//!
//! ## Architecture
//!
//! Built with Clean Architecture (DDD + Hexagonal Architecture):
//!
//! - **Domain Layer**: Core business logic, aggregates, and value objects
//! - **Application Layer**: Use cases that orchestrate domain logic
//! - **Infrastructure Layer**: External system integrations (Google Calendar, Slack)
//!
//! ## Usage as a Binary
//!
//! The primary use case is running the watcher service:
//!
//! ```bash
//! # Run with Google Calendar and Slack
//! cargo run --bin watcher
//!
//! # Run with mock implementations for testing
//! cargo run --bin watcher --notifier mock --repository mock
//!
//! # Customize polling interval (default: 60 seconds)
//! cargo run --bin watcher --interval 30
//! ```
//!
//! ## Usage as a Library
//!
//! You can also use this crate as a library to build custom resource management systems:
//!
//! ```rust,no_run
//! use lab_resource_manager::{
//!     NotifyResourceUsageChangesUseCase,
//!     GoogleCalendarUsageRepository,
//!     NotificationRouter,
//!     JsonFileIdentityLinkRepository,
//!     load_config,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration
//! let config = load_config("config/resources.toml")?;
//!
//! // Create repository and notifier
//! let repository = GoogleCalendarUsageRepository::new(
//!     "secrets/service-account.json",
//!     config.clone(),
//! ).await?;
//! // Create identity link repository for Slack user mapping
//! let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new("data/identity_links.json".into()));
//! // NotificationRouter automatically supports all configured notification types
//! // (Slack, Mock, etc.) based on config/resources.toml
//! let notifier = NotificationRouter::new(config, identity_repo);
//!
//! // Create and run use case
//! let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier).await?;
//! usecase.poll_once().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Device Specification Format
//!
//! Calendar event titles support flexible device specification:
//!
//! - Single: `0` → Device 0
//! - Range: `0-2` → Devices 0, 1, 2
//! - Multiple: `0,2,5` → Devices 0, 2, 5
//! - Mixed: `0-1,6-7` → Devices 0, 1, 6, 7
//!
//! ## Features
//!
//! - Google Calendar integration for resource reservations
//! - Slack notifications for create/update/delete events
//! - DDD Factory Pattern for device specification parsing
//! - Mock implementations for testing
//! - CLI arguments for flexible deployment
//!
//! ## Setup
//!
//! See the [README](https://github.com/kano-lab/lab-resource-manager) for detailed setup instructions.

// Module declarations
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;

/// Commonly used types for building resource management systems
///
/// This prelude re-exports the most frequently used types and traits,
/// allowing users to import everything they need with a single use statement:
///
/// ```rust
/// use lab_resource_manager::prelude::*;
/// ```
pub mod prelude {
    // Use cases
    pub use crate::application::usecases::NotifyResourceUsageChangesUseCase;

    // Application errors
    pub use crate::application::error::ApplicationError;

    // Domain types
    pub use crate::domain::aggregates::resource_usage::{
        entity::ResourceUsage,
        errors::ResourceUsageError,
        factory::{ResourceFactory, ResourceFactoryError},
        value_objects::{Gpu, Resource, TimePeriod, UsageId},
    };

    // Ports (traits)
    pub use crate::domain::ports::{
        notifier::{NotificationError, NotificationEvent, Notifier},
        repositories::{RepositoryError, ResourceUsageRepository},
    };

    // Infrastructure implementations
    pub use crate::infrastructure::{
        config::{DeviceConfig, ResourceConfig, RoomConfig, ServerConfig, load_config},
        notifier::{
            router::NotificationRouter,
            senders::{MockSender, SlackSender},
        },
        repositories::{
            identity_link::JsonFileIdentityLinkRepository,
            resource_usage::{
                google_calendar::GoogleCalendarUsageRepository, mock::MockUsageRepository,
            },
        },
    };
}

// Convenience re-exports at crate root
pub use application::{error::ApplicationError, usecases::NotifyResourceUsageChangesUseCase};
pub use domain::ports::{
    notifier::{NotificationError, NotificationEvent, Notifier},
    repositories::{RepositoryError, ResourceUsageRepository},
};
pub use infrastructure::{
    config::load_config,
    notifier::{
        router::NotificationRouter,
        senders::{MockSender, SlackSender},
    },
    repositories::{
        identity_link::JsonFileIdentityLinkRepository,
        resource_usage::{
            google_calendar::GoogleCalendarUsageRepository, mock::MockUsageRepository,
        },
    },
};
