//! Counter infrastructure layer — concrete repository implementations.
//!
//! This module bridges the abstract `CounterRepository` port to
//! concrete storage backends.

pub mod libsql_adapter;
pub mod surrealdb_adapter;

pub use libsql_adapter::*;
pub use surrealdb_adapter::*;
