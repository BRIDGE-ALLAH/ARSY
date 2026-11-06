//! Infrastructure adapters for ARSY.
//!
//! Implements the storage ports defined in `arsy-core`. Swap the in-memory
//! repositories for SQL/NoSQL backends or add HTTP clients under [`clients`]
//! without touching the domain layer.

pub mod clients;
pub mod db;
