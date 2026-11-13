# lab-resource-manager

[![Crates.io](https://img.shields.io/crates/v/lab-resource-manager)](https://crates.io/crates/lab-resource-manager)
[![Documentation](https://docs.rs/lab-resource-manager/badge.svg)](https://docs.rs/lab-resource-manager)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](README.md#license)

A resource reservation management system for laboratories.

[日本語 README](README_ja.md)

## Overview

- **Resource Reservation Management**: Manage schedules for resources such as GPU servers and rooms
- **Change Notifications**: Get notified when reservations are created, updated, or cancelled
  (a recurring reservation is announced once, not once per reserved slot)
- **Identity Linking**: Map user identities across different systems for enhanced notifications

### Default Implementations

| Component | Implementation |
| --- | --- |
| Resource Repository | Google Calendar |
| Notifications | Slack |
| User Interface | Slack Bot |
| Access Control | Google Calendar ACL |

## Quick Start

- **Users**: See [User Guide](docs/USER_GUIDE.md)
- **Administrators**: See [Admin Guide](docs/ADMIN_GUIDE.md)

## Architecture

This project follows Clean Architecture (DDD + Hexagonal Architecture):

```text
src/
├── domain/                  # Domain layer (business logic)
│   ├── aggregates/          # Aggregates (Reservation, IdentityLink)
│   ├── common/              # Shared Kernel (EmailAddress, etc.)
│   ├── ports/               # Ports (Repository, Notifier traits)
│   ├── read_models/         # Read models (ReservedSlotView: reserved slots looked up by period)
│   └── errors.rs            # Domain errors
├── application/             # Application layer (use cases)
│   └── usecases/            # Notify, GrantAccess use cases
├── infrastructure/          # Infrastructure layer (implementations)
│   ├── repositories/        # Repository implementations
│   ├── notifier/            # Notification implementations
│   ├── resource_collection_access/  # Access control implementations
│   └── config/              # Configuration management
├── interface/               # Interface layer
│   └── slack/               # Slack bot interface
└── bin/
    └── lab-resource-manager.rs  # Entry point
```

## Development

### Prerequisites

- Rust 1.90+

### Build & Test

```bash
cargo build
cargo test
cargo fmt
cargo clippy
```

### Google Calendar Contract Tests

The Google Calendar adapter's assumptions about the API are checked by contract tests. They
run against an in-memory fake with `cargo test`, and against a real test calendar weekly, on
pull requests that change the adapter, and on demand.
For the real calendar, use a calendar that holds nothing else and a service account that can
make changes to its events. Copy `.env.contract.example` to `.env.contract`, fill in the
calendar ID and the key file's path (a key under `secrets/` stays out of git), and run:

```bash
cargo test --all-features contract_tests -- --ignored
```

`LRM_CONTRACT_CALENDAR_ID` and `LRM_CONTRACT_SERVICE_ACCOUNT_KEY` set in the environment take
precedence over the file.

The `Calendar Contract` workflow runs the same tests from GitHub Actions: every Monday, on
pull requests from this repository that change the adapter, and when started by hand. It
reads the `LRM_CONTRACT_CALENDAR_ID` and `LRM_CONTRACT_SERVICE_ACCOUNT_KEY` (the key file's
contents) repository secrets, and runs one at a time.

## Roadmap

- [x] Resource-based notification routing
- [x] Identity Linking
- [x] Access management interface
- [x] Reservation creation via Slack commands
- [ ] Natural language resource management (LLM agent)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
