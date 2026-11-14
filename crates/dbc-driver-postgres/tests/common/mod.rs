//! A PostgreSQL container plus a fresh database per request. Needs Docker.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::atomic::{AtomicUsize, Ordering};

use dbc_core::conformance::POSTGRES_FIXTURE;
use dbc_core::driver::{ConnectionConfig, Endpoint, TlsConfig};
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use secrecy::SecretString;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, CopyTargetOptions, ImageExt};
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

    /// A server on Docker `network`, reachable there as `name`.
    pub async fn start_on(network: &str, name: &str) -> Self {
        let request = Postgres::default()
            .with_tag(TAG)
            .with_network(network)
            .with_container_name(name);
        Self::from(request.start().await.unwrap()).await
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

pub struct Pki {
    pub ca_pem: String,
    pub cert_pem: String,
    pub key_pem: String,
}

/// A test CA and a server certificate for `localhost` signed by it.
pub fn pki() -> Pki {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    let server_key = KeyPair::generate().unwrap();
    let server = CertificateParams::new(vec!["localhost".to_owned()])
        .unwrap()
        .signed_by(&server_key, &issuer)
        .unwrap();
    Pki {
        ca_pem: ca.pem(),
        cert_pem: server.pem(),
        key_pem: server_key.serialize_pem(),
    }
}

/// A server with TLS on, optionally on a Docker `network` as container `name`.
pub async fn tls_server(pki: &Pki, network: Option<(&str, &str)>) -> Server {
    // The copied directory is root-only and the key must belong to `postgres`. The entrypoint
    // runs a non-`postgres` command as root, so install the files, then start normally.
    const START: &str = "install -o postgres -m 644 /tls/server.crt /tmp/server.crt \\
        && install -o postgres -m 600 /tls/server.key /tmp/server.key \\
        && exec docker-entrypoint.sh postgres -c ssl=on \\
           -c ssl_cert_file=/tmp/server.crt -c ssl_key_file=/tmp/server.key";
    let request = Postgres::default()
        .with_tag(TAG)
        .with_copy_to(
            CopyTargetOptions::new("/tls/server.crt"),
            pki.cert_pem.clone().into_bytes(),
        )
        .with_copy_to(
            CopyTargetOptions::new("/tls/server.key"),
            pki.key_pem.clone().into_bytes(),
        )
        .with_cmd(["sh", "-c", START]);
    let container = match network {
        Some((network, name)) => {
            request
                .with_network(network)
                .with_container_name(name)
                .start()
                .await
        }
        None => request.start().await,
    };
    Server::from(container.unwrap()).await
}
