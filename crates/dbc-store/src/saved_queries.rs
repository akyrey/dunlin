//! Saved queries (task 8.6): named SQL, for one connection or for all of them.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::connections::now_millis;
use crate::{Store, StoreError};

const MAX_NAME_LENGTH: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    /// `None`: shown for every connection.
    pub connection_id: Option<String>,
    pub name: String,
    pub sql: String,
    pub updated_at: i64,
}

/// A query to create (`id` absent) or update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryDraft {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
    pub name: String,
    pub sql: String,
}

impl Store {
    pub fn list_saved_queries(&self) -> Result<Vec<SavedQuery>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, connection_id, name, sql, updated_at FROM saved_queries
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SavedQuery {
                id: row.get(0)?,
                connection_id: row.get(1)?,
                name: row.get(2)?,
                sql: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn save_query(&self, draft: SavedQueryDraft) -> Result<SavedQuery, StoreError> {
        let name = draft.name.trim().to_owned();
        if name.is_empty() {
            return Err(StoreError::Invalid("The query needs a name".into()));
        }
        if name.chars().count() > MAX_NAME_LENGTH {
            return Err(StoreError::Invalid(format!(
                "The name is longer than {MAX_NAME_LENGTH} characters"
            )));
        }
        let now = now_millis();
        let id = match &draft.id {
            Some(id) => {
                let changed = self.connection.execute(
                    "UPDATE saved_queries SET connection_id = ?2, name = ?3, sql = ?4, updated_at = ?5 WHERE id = ?1",
                    params![id, draft.connection_id, name, draft.sql, now],
                )?;
                if changed == 0 {
                    return Err(StoreError::NotFound(id.clone()));
                }
                id.clone()
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.connection.execute(
                    "INSERT INTO saved_queries (id, connection_id, name, sql, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    params![id, draft.connection_id, name, draft.sql, now],
                )?;
                id
            }
        };
        self.get_saved_query(&id)?.ok_or(StoreError::NotFound(id))
    }

    fn get_saved_query(&self, id: &str) -> Result<Option<SavedQuery>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, connection_id, name, sql, updated_at FROM saved_queries WHERE id = ?1",
                [id],
                |row| {
                    Ok(SavedQuery {
                        id: row.get(0)?,
                        connection_id: row.get(1)?,
                        name: row.get(2)?,
                        sql: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn delete_saved_query(&self, id: &str) -> Result<(), StoreError> {
        let deleted = self
            .connection
            .execute("DELETE FROM saved_queries WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(StoreError::NotFound(id.to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::BTreeMap;

    use dbc_core::driver::DriverKind;

    use super::*;
    use crate::{ConnectionDraft, TlsSettings};

    fn store() -> (tempfile::TempDir, Store, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store.db")).unwrap();
        let profile = store
            .save_connection(ConnectionDraft {
                id: None,
                name: "local".into(),
                driver: DriverKind::Sqlite,
                color: None,
                read_only: false,
                host: None,
                port: None,
                database: None,
                user: None,
                file_path: Some("/tmp/x.db".into()),
                options: BTreeMap::new(),
                tls: TlsSettings::default(),
                ssh: None,
                always_preview: false,
            })
            .unwrap();
        (dir, store, profile.id)
    }

    fn draft(name: &str, connection: Option<&str>) -> SavedQueryDraft {
        SavedQueryDraft {
            id: None,
            connection_id: connection.map(str::to_owned),
            name: name.into(),
            sql: "SELECT 1".into(),
        }
    }

    #[test]
    fn saves_lists_updates_and_deletes() {
        let (_dir, store, connection) = store();

        let global = store.save_query(draft("  b global ", None)).unwrap();
        let local = store
            .save_query(draft("A local", Some(&connection)))
            .unwrap();
        let updated = store
            .save_query(SavedQueryDraft {
                id: Some(local.id.clone()),
                sql: "SELECT 2".into(),
                ..draft("A renamed", Some(&connection))
            })
            .unwrap();

        assert_eq!(global.name, "b global");
        assert_eq!(updated.sql, "SELECT 2");
        assert_eq!(
            store
                .list_saved_queries()
                .unwrap()
                .iter()
                .map(|q| q.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A renamed", "b global"]
        );
        store.delete_saved_query(&global.id).unwrap();
        assert!(matches!(
            store.delete_saved_query(&global.id),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn refuses_bad_names_and_unknown_ids_and_follows_its_connection() {
        let (_dir, store, connection) = store();
        store.save_query(draft("kept", Some(&connection))).unwrap();

        assert!(matches!(
            store.save_query(draft(" ", None)),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            store.save_query(draft(&"x".repeat(201), None)),
            Err(StoreError::Invalid(_))
        ));
        let missing = SavedQueryDraft {
            id: Some("nope".into()),
            ..draft("x", None)
        };
        assert!(matches!(
            store.save_query(missing),
            Err(StoreError::NotFound(_))
        ));
        store.delete_connection(&connection).unwrap();
        assert!(store.list_saved_queries().unwrap().is_empty());
    }
}
