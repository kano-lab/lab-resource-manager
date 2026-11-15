# Versioning

This repository releases two crates, versioned by different contracts.

| Crate | Who relies on it | What its version follows |
|---|---|---|
| `lab-resource-manager` (application) | Operators who run it, people who use it from Slack or MCP | What a deployment relies on (below) |
| `lab-resource-manager-core` (library) | Rust code that depends on it | Its Rust API, under [Cargo's SemVer rules](https://doc.rust-lang.org/cargo/reference/semver.html) |

Releases are prepared by [release-plz](https://release-plz.dev/) (`release-plz.toml`): it keeps a release
pull request open, and merging it publishes both crates, tags them and creates the application's GitHub
release.

## The application

A change is **breaking** when an existing deployment stops working as before after only the binary is
replaced, so the operator has to do something. That is the line `deploy/auto-update.sh` draws: it applies
minor and patch releases on its own and leaves majors to an administrator.

| Part of the contract (changing it is breaking) | Not part of the contract |
|---|---|
| Environment variables and their meaning | The Rust API of the `lab-resource-manager` crate |
| The format of `resources.toml` | Log messages and log field names |
| The format of data files (`identity_links.json`, MCP tokens, state directories) | The wording of Slack messages |
| `deploy/lab-resource-manager.service` and command-line arguments | |
| Slack app scopes (adding one means reinstalling the app), slash command names and meaning | |
| MCP tool names, inputs and outputs (their callers are programs) | |

Mark a breaking change with `!` in the commit message (`feat!: ...`) and describe what operators must do in
[MIGRATION.md](MIGRATION.md). Use `!` only for this contract; see below for library changes.

`feat` makes a minor release and `fix` or `perf` a patch release. Refactors, documentation and tests do not
propose a release on their own; they ship with the next one.

## The library

Breaking API changes are found by [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
when the release pull request is prepared, so they need no marker in the commit message. While the
library is below 1.0, a breaking change raises the minor version (0.1.x → 0.2.0).

Do not mark a library-only API change with `!`. A pull request that changes the library usually also
updates the application's calls, and with squash merging both land in one commit; `!` would make the
application major although nothing changed for operators.

Every library release also gives the application a patch release, because changes in the library reach
operators only through the application's binary.

## Tags

| Crate | Tag | GitHub release |
|---|---|---|
| Application | `vX.Y.Z` | Yes, with the binaries `deploy/auto-update.sh` downloads |
| Library | `lab-resource-manager-core-vX.Y.Z` | No (crates.io and docs.rs carry it), so the application's release stays the latest |
