//! Accepts a column of any type and keeps its binary payload for our own decoding.

use tokio_postgres::types::{FromSql, Type};

pub(crate) struct RawCell<'a>(pub Option<&'a [u8]>);

impl<'a> FromSql<'a> for RawCell<'a> {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self(Some(raw)))
    }

    fn from_sql_null(_ty: &Type) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self(None))
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }
}
