//! HTTP API surface for ARSY.
//!
//! Kept as a library so integration tests can build the same router the
//! binary serves; [`main`] only wires config, state, and the listener.

pub mod config;
pub mod errors;
pub mod middleware;
pub mod routes;
pub mod state;
