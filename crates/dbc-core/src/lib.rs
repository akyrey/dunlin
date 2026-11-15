//! Shared model for all drivers: values, schema, traits and SQL utilities. No I/O.

pub mod changes;
#[cfg(feature = "test-support")]
pub mod conformance;
pub mod data;
pub mod dialect;
pub mod driver;
pub mod error;
pub mod export;
#[cfg(test)]
mod fake;
pub mod import;
pub mod kv;
pub mod plan;
pub mod result;
pub mod schema;
pub mod sql;
pub mod value;

pub use error::{DbError, DbErrorCode};
pub use value::{DEFAULT_DISPLAY_LIMIT, Value, WireValue};
