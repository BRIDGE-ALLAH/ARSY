# Changelog

All notable changes to this project are documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) with
Semantic Versioning.

## [Unreleased]

### Added
- Cargo workspace with `arsy-core`, `arsy-infra`, and `arsy-server` crates.
- Axum HTTP API with `/healthz` and versioned `/v1/users` endpoints.
- In-memory user repository implementing the `UserRepository` port.
- CI workflow (fmt, clippy, test, build, cargo-deny), Dockerfile, justfile.
