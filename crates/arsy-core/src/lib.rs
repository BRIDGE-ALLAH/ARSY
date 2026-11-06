//! Domain core for ARSY.
//!
//! Holds models, errors, and service logic with no I/O or web dependencies.
//! Storage is expressed as traits ([`services::UserRepository`]) and implemented
//! by adapters in `arsy-infra`.

pub mod error;
pub mod models;
pub mod services;

pub use error::CoreError;
