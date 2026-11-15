//! # lab-resource-manager-core
//!
//! The domain model and use cases of [lab-resource-manager]: reservations of GPUs
//! and rooms, the slots they hold, conflicts between them, and how the reservations
//! compare with what is actually running on the servers.
//!
//! Nothing here knows about Google Calendar, Slack or any other service. Storage,
//! notifications and observation of the servers are ports (traits in
//! [`domain::ports`]) that an application implements. lab-resource-manager itself is
//! one such application.
//!
//! ## Layers
//!
//! - [`domain`]: aggregates, value objects, domain services and the ports
//! - [`application`]: use cases that drive the domain through the ports
//! - [`testing`]: in-memory implementations of the ports, for tests and prototypes
//!
//! ## Versioning
//!
//! This crate follows semantic versioning of its Rust API. It is versioned apart from
//! the lab-resource-manager application, whose version follows what operators rely on
//! (configuration, data files, Slack commands, MCP tools).
//!
//! [lab-resource-manager]: https://github.com/kano-lab/lab-resource-manager

pub mod application;
pub mod domain;
pub mod testing;
