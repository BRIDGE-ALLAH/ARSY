//! Outbound integrations: HTTP/gRPC clients for third-party services.
//!
//! Add one submodule per external system, keeping each client behind a trait
//! defined in `arsy-core` so the domain stays testable.
