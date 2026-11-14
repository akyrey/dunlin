//! Per-session metadata cache (task 4.7). Everything is read through the session's metadata
//! connection, so a long statement in an editor tab never blocks the tree. The cache is
//! cleared by `schema_refresh` and after any DDL statement runs in the session.

use std::collections::HashMap;
use std::sync::Mutex;

use dbc_core::DbError;
use dbc_core::schema::{Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef};

#[derive(Default)]
struct Entries {
    /// Bumped by every clear, so a load that started before it is not stored.
    generation: u64,
    namespaces: Option<Vec<Namespace>>,
    objects: HashMap<NamespaceRef, Vec<ObjectSummary>>,
    tables: HashMap<TableRef, TableInfo>,
    ddl: HashMap<TableRef, String>,
}

#[derive(Default)]
pub struct SchemaCache {
    entries: Mutex<Entries>,
}

impl SchemaCache {
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            let generation = entries.generation.wrapping_add(1);
            *entries = Entries {
                generation,
                ..Entries::default()
            };
        }
    }

    /// The cached value, or `load`'s result, stored unless the cache was cleared meanwhile.
    async fn cached<T, F>(
        &self,
        get: impl FnOnce(&Entries) -> Option<T>,
        put: impl FnOnce(&mut Entries, T),
        load: F,
    ) -> Result<T, DbError>
    where
        T: Clone,
        F: Future<Output = Result<T, DbError>>,
    {
        // A poisoned cache is only a missed cache: the value is loaded again.
        let (hit, generation) = match self.entries.lock() {
            Ok(entries) => (get(&entries), Some(entries.generation)),
            Err(_) => (None, None),
        };
        if let Some(hit) = hit {
            return Ok(hit);
        }
        let loaded = load.await?;
        if let Ok(mut entries) = self.entries.lock()
            && Some(entries.generation) == generation
        {
            put(&mut entries, loaded.clone());
        }
        Ok(loaded)
    }

    pub async fn namespaces(
        &self,
        load: impl Future<Output = Result<Vec<Namespace>, DbError>>,
    ) -> Result<Vec<Namespace>, DbError> {
        self.cached(
            |e| e.namespaces.clone(),
            |e, v| e.namespaces = Some(v),
            load,
        )
        .await
    }

    pub async fn objects(
        &self,
        namespace: &NamespaceRef,
        load: impl Future<Output = Result<Vec<ObjectSummary>, DbError>>,
    ) -> Result<Vec<ObjectSummary>, DbError> {
        let get = |e: &Entries| e.objects.get(namespace).cloned();
        let put = |e: &mut Entries, v| {
            e.objects.insert(namespace.clone(), v);
        };
        self.cached(get, put, load).await
    }

    pub async fn table(
        &self,
        table: &TableRef,
        load: impl Future<Output = Result<TableInfo, DbError>>,
    ) -> Result<TableInfo, DbError> {
        let get = |e: &Entries| e.tables.get(table).cloned();
        let put = |e: &mut Entries, v| {
            e.tables.insert(table.clone(), v);
        };
        self.cached(get, put, load).await
    }

    pub async fn ddl(
        &self,
        table: &TableRef,
        load: impl Future<Output = Result<String, DbError>>,
    ) -> Result<String, DbError> {
        let get = |e: &Entries| e.ddl.get(table).cloned();
        let put = |e: &mut Entries, v| {
            e.ddl.insert(table.clone(), v);
        };
        self.cached(get, put, load).await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::atomic::{AtomicUsize, Ordering};

    use dbc_core::DbErrorCode;

    use super::*;

    fn namespace(name: &str) -> NamespaceRef {
        NamespaceRef {
            catalog: None,
            schema: Some(name.into()),
        }
    }

    #[tokio::test]
    async fn loads_once_until_cleared() {
        let cache = SchemaCache::default();
        let loads = AtomicUsize::new(0);
        let load = || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        };

        cache.objects(&namespace("a"), load()).await.unwrap();
        cache.objects(&namespace("a"), load()).await.unwrap();
        cache.objects(&namespace("b"), load()).await.unwrap();
        cache.clear();
        cache.objects(&namespace("a"), load()).await.unwrap();

        assert_eq!(loads.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let cache = SchemaCache::default();

        let failed = cache
            .namespaces(async { Err(DbError::new(DbErrorCode::Permission, "no")) })
            .await;
        let loaded = cache.namespaces(async { Ok(Vec::new()) }).await;

        assert_eq!(failed.unwrap_err().code, DbErrorCode::Permission);
        assert!(loaded.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_load_overtaken_by_a_clear_is_not_stored() {
        let cache = SchemaCache::default();

        let stale = cache
            .namespaces(async {
                cache.clear();
                Ok(vec![Namespace {
                    namespace: namespace("old"),
                    name: "old".into(),
                    system: false,
                }])
            })
            .await
            .unwrap();
        let fresh = cache.namespaces(async { Ok(Vec::new()) }).await.unwrap();

        assert_eq!(stale.len(), 1);
        assert!(fresh.is_empty());
    }
}
