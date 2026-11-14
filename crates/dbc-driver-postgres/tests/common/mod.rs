//! A PostgreSQL container plus a fresh database per request. Needs Docker.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::atomic::{AtomicUsize, Ordering};

use dbc_core::conformance::POSTGRES_FIXTURE;
use dbc_core::driver::{ConnectionConfig, Endpoint, TlsConfig};
use secrecy::SecretString;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio_postgres::NoTls;

pub const TAG: &str = "18-alpine";
pub const PASSWORD: &str = "postgres";

/// Types used by the type table: an enum, a domain, a composite and two extensions.
const TYPE_SETUP: &str = "
    CREATE EXTENSION IF NOT EXISTS hstore;
    CREATE EXTENSION IF NOT EXISTS citext;
    CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy');
    CREATE DOMAIN positive AS integer CHECK (VALUE > 0);
    CREATE TYPE pair AS (a integer, b text);
";

pub struct Server {
    pub container: ContainerAsync<Postgres>,
    pub port: u16,
    counter: AtomicUsize,
}

impl Server {
    pub async fn start() -> Self {
        Self::from(Postgres::default().with_tag(TAG).start().await.unwrap()).await
    }

    pub async fn from(container: ContainerAsync<Postgres>) -> Self {
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        Self {
            container,
            port,
            counter: AtomicUsize::new(0),
        }
    }

    pub async fn admin(&self, database: &str) -> tokio_postgres::Client {
        let url = format!(
            "host=127.0.0.1 port={} user=postgres password={PASSWORD} dbname={database}",
            self.port
        );
        let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
        tokio::spawn(connection);
        client
    }

    /// A new database with the fixture and the type-table types loaded.
    pub async fn fresh_database(&self) -> String {
        let name = format!(
            "conformance_{}",
            self.counter.fetch_add(1, Ordering::SeqCst)
        );
        self.admin("postgres")
            .await
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .await
            .unwrap();
        let client = self.admin(&name).await;
        client.batch_execute(POSTGRES_FIXTURE).await.unwrap();
        client.batch_execute(TYPE_SETUP).await.unwrap();
        name
    }

    pub fn config(&self, database: &str, read_only: bool) -> ConnectionConfig {
        ConnectionConfig {
            endpoint: Endpoint::Tcp {
                dial_host: "127.0.0.1".into(),
                dial_port: self.port,
                server_name: "localhost".into(),
            },
            database: Some(database.into()),
            user: Some("postgres".into()),
            password: Some(SecretString::from(PASSWORD)),
            tls: TlsConfig::default(),
            read_only,
            options: Default::default(),
        }
    }
}
