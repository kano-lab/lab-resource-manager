# lab-resource-manager-core

The domain model and use cases of [lab-resource-manager](https://github.com/kano-lab/lab-resource-manager): reservations of GPUs and rooms, the slots they hold, conflicts between them, and how they compare with what is actually running on the servers.

The crate does not depend on Google Calendar, Slack or any other service. Storage, notifications and observation of the servers are ports (traits under `domain::ports`) that an application implements. The `testing` module provides in-memory implementations of them.

This crate follows semantic versioning of its Rust API, separately from the lab-resource-manager application.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
