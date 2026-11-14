//! The user's order of connections and duplicating one (task 8.13).

use std::collections::HashSet;

use crate::connections::MAX_NAME_LENGTH;
use crate::profile::{ConnectionDraft, ConnectionProfile};
use crate::{Store, StoreError};

impl Store {
    /// Puts the connections in the given order. `ids` must name every connection exactly once,
    /// so a list the frontend loaded before another change cannot drop or duplicate one.
    pub fn reorder_connections(&self, ids: &[String]) -> Result<(), StoreError> {
        let current: HashSet<String> = self.list_connections()?.into_iter().map(|p| p.id).collect();
        let given: HashSet<&String> = ids.iter().collect();
        if given.len() != ids.len() || given.len() != current.len() {
            return Err(StoreError::Invalid(
                "The new order must list every connection once; reload and try again".into(),
            ));
        }
        if let Some(unknown) = ids.iter().find(|id| !current.contains(*id)) {
            return Err(StoreError::NotFound(unknown.clone()));
        }
        let transaction = self.connection.unchecked_transaction()?;
        for (position, id) in ids.iter().enumerate() {
            let position = i64::try_from(position).unwrap_or(i64::MAX);
            transaction.execute(
                "UPDATE connections SET position = ?2 WHERE id = ?1",
                rusqlite::params![id, position],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// A copy of the profile, named "<name> copy", listed right after it. Secrets are not
    /// copied here: they live in the keychain, which the caller copies.
    pub fn duplicate_connection(&self, id: &str) -> Result<ConnectionProfile, StoreError> {
        let original = self.get_connection(id)?;
        let draft = ConnectionDraft {
            id: None,
            name: copy_name(&original.name),
            ..ConnectionDraft::from(original)
        };
        let copy = self.save_connection(draft)?;
        let mut order: Vec<String> = self
            .list_connections()?
            .into_iter()
            .map(|p| p.id)
            .filter(|other| *other != copy.id)
            .collect();
        let after = order
            .iter()
            .position(|other| other == id)
            .map_or(order.len(), |i| i + 1);
        order.insert(after, copy.id.clone());
        self.reorder_connections(&order)?;
        Ok(copy)
    }
}

fn copy_name(name: &str) -> String {
    const SUFFIX: &str = " copy";
    let keep = MAX_NAME_LENGTH - SUFFIX.len();
    let base: String = name.chars().take(keep).collect();
    format!("{base}{SUFFIX}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_long_name_still_fits_with_copy() {
        let name = "x".repeat(MAX_NAME_LENGTH);
        assert_eq!(copy_name(&name).chars().count(), MAX_NAME_LENGTH);
        assert_eq!(copy_name("prod"), "prod copy");
    }
}
