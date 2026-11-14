//! Shared model for all drivers: values, schema, traits and SQL utilities. No I/O.

pub mod dialect;
pub mod driver;
pub mod error;
pub mod result;
pub mod schema;
pub mod sql;
pub mod value;

pub use error::{DbError, DbErrorCode};
pub use value::{DEFAULT_DISPLAY_LIMIT, Value, WireValue};
