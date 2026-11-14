//! Turns a connection profile into a driver `ConnectionConfig`, validating it against the
//! driver's declared endpoint and options.

use std::path::PathBuf;

use dbc_core::driver::{ConnectionConfig, DriverInfo, Endpoint, EndpointKind, TlsConfig, TlsMode};
use dbc_store::{ConnectionDraft, TlsSettings};
use secrecy::SecretString;

use crate::error::AppError;

pub fn validate(draft: &ConnectionDraft, info: &DriverInfo) -> Result<(), AppError> {
    endpoint(draft, info).map(|_| ())?;
    if draft.ssh.is_some() && !info.capabilities.tunnel {
        return Err(AppError::invalid(format!(
            "{} connections cannot use an SSH tunnel",
            info.label
        )));
    }
    let mode = draft.tls.mode;
    if mode != TlsMode::Disable && !info.tls_modes.contains(&mode) {
        return Err(AppError::invalid(format!(
            "{} does not support this TLS mode",
            info.label
        )));
    }
    if let Some(key) = draft
        .options
        .keys()
        .find(|k| !info.options.iter().any(|o| &o.key == *k))
    {
        return Err(AppError::invalid(format!(
            "Unknown option for {}: {key}",
            info.label
        )));
    }
    Ok(())
}

pub fn config(
    draft: &ConnectionDraft,
    info: &DriverInfo,
    password: Option<SecretString>,
) -> Result<ConnectionConfig, AppError> {
    validate(draft, info)?;
    Ok(ConnectionConfig {
        endpoint: endpoint(draft, info)?,
        database: non_empty(draft.database.as_deref()),
        user: non_empty(draft.user.as_deref()),
        password,
        tls: tls(&draft.tls),
        read_only: draft.read_only,
        options: draft.options.clone(),
    })
}

fn tls(settings: &TlsSettings) -> TlsConfig {
    let path = |p: &Option<String>| non_empty(p.as_deref()).map(PathBuf::from);
    TlsConfig {
        mode: settings.mode,
        ca_file: path(&settings.ca_file),
        client_cert_file: path(&settings.cert_file),
        client_key_file: path(&settings.key_file),
    }
}

fn endpoint(draft: &ConnectionDraft, info: &DriverInfo) -> Result<Endpoint, AppError> {
    match info.endpoint {
        EndpointKind::File => {
            let path = non_empty(draft.file_path.as_deref())
                .ok_or_else(|| AppError::invalid("Choose a database file"))?;
            Ok(Endpoint::File {
                path: PathBuf::from(path),
            })
        }
        EndpointKind::Tcp => {
            let host = non_empty(draft.host.as_deref())
                .ok_or_else(|| AppError::invalid("The host is required"))?;
            let port = draft
                .port
                .or(info.default_port)
                .ok_or_else(|| AppError::invalid("The port is required"))?;
            Ok(Endpoint::Tcp {
                dial_host: host.clone(),
                dial_port: port,
                server_name: host,
            })
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dbc_core::driver::{Driver, DriverKind};
    use dbc_driver_mysql::MysqlDriver;
    use dbc_driver_postgres::PostgresDriver;
    use dbc_driver_sqlite::SqliteDriver;

    use super::*;

    fn draft(file_path: Option<&str>) -> ConnectionDraft {
        ConnectionDraft {
            id: None,
            name: "x".into(),
            driver: DriverKind::Sqlite,
            color: None,
            read_only: true,
            host: None,
            port: None,
            database: None,
            user: None,
            file_path: file_path.map(str::to_owned),
            options: BTreeMap::new(),
            tls: TlsSettings::default(),
            ssh: None,
        }
    }

    #[test]
    fn file_databases_cannot_use_a_tunnel() {
        let ssh = dbc_store::SshSettings {
            server: dbc_store::SshHostSettings {
                host: "bastion".into(),
                port: 22,
                user: "me".into(),
                auth: dbc_store::SshAuthMethod::Agent,
                key_file: None,
            },
            jump: None,
        };
        let draft = ConnectionDraft {
            ssh: Some(ssh),
            ..draft(Some("/a.db"))
        };

        let error = validate(&draft, &SqliteDriver.info()).err().unwrap();

        assert_eq!(error.code, "invalid_argument");
    }

    #[test]
    fn file_driver_needs_a_path() {
        let error = config(&draft(Some("  ")), &SqliteDriver.info(), None)
            .err()
            .unwrap();

        assert_eq!(error.code, "invalid_argument");
    }

    #[test]
    fn file_config_carries_path_and_read_only() {
        let config = config(&draft(Some("/tmp/a.db")), &SqliteDriver.info(), None).unwrap();

        assert_eq!(
            config.endpoint,
            Endpoint::File {
                path: "/tmp/a.db".into()
            }
        );
        assert!(config.read_only);
    }

    #[test]
    fn tls_settings_become_paths_and_unsupported_modes_are_rejected() {
        let mut draft = draft(None);
        draft.host = Some("db".into());
        draft.tls = TlsSettings {
            mode: TlsMode::VerifyCa,
            ca_file: Some("/etc/ca.pem".into()),
            cert_file: Some("  ".into()),
            key_file: None,
        };

        let pg = config(&draft, &PostgresDriver.info(), None).unwrap();
        let mysql = validate(&draft, &MysqlDriver.info());
        let sqlite = validate(
            &ConnectionDraft {
                tls: draft.tls.clone(),
                ..self::draft(Some("/a.db"))
            },
            &SqliteDriver.info(),
        );

        assert_eq!(pg.tls.ca_file, Some(PathBuf::from("/etc/ca.pem")));
        assert_eq!(pg.tls.client_cert_file, None);
        assert_eq!(
            mysql.err().map(|e| e.code).as_deref(),
            Some("invalid_argument")
        );
        assert_eq!(
            sqlite.err().map(|e| e.code).as_deref(),
            Some("invalid_argument")
        );
    }

    #[test]
    fn unknown_options_are_rejected() {
        let mut draft = draft(Some("/tmp/a.db"));
        draft.options.insert("bogus".into(), "1".into());

        let error = validate(&draft, &SqliteDriver.info()).err().unwrap();

        assert!(error.message.contains("bogus"));
    }

    #[test]
    fn tcp_endpoint_requires_a_host_and_uses_the_default_port() {
        let info = PostgresDriver.info();
        let mut draft = draft(None);
        draft.driver = DriverKind::Postgres;

        assert!(config(&draft, &info, None).is_err());

        draft.host = Some("db.example.com".into());
        let config = config(&draft, &info, None).unwrap();
        assert_eq!(
            config.endpoint,
            Endpoint::Tcp {
                dial_host: "db.example.com".into(),
                dial_port: 5432,
                server_name: "db.example.com".into()
            }
        );
    }
}
