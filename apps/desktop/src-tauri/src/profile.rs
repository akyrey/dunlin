//! Turns a connection profile into a driver `ConnectionConfig`, validating it against the
//! driver's declared endpoint and options.

use std::path::PathBuf;

use dbc_core::driver::{ConnectionConfig, DriverInfo, Endpoint, EndpointKind, TlsConfig};
use dbc_store::ConnectionDraft;

use crate::error::AppError;

pub fn validate(draft: &ConnectionDraft, info: &DriverInfo) -> Result<(), AppError> {
    endpoint(draft, info).map(|_| ())?;
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

pub fn config(draft: &ConnectionDraft, info: &DriverInfo) -> Result<ConnectionConfig, AppError> {
    validate(draft, info)?;
    Ok(ConnectionConfig {
        endpoint: endpoint(draft, info)?,
        database: non_empty(draft.database.as_deref()),
        user: non_empty(draft.user.as_deref()),
        password: None,
        tls: TlsConfig::default(),
        read_only: draft.read_only,
        options: draft.options.clone(),
    })
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
        }
    }

    #[test]
    fn file_driver_needs_a_path() {
        let error = config(&draft(Some("  ")), &SqliteDriver.info())
            .err()
            .unwrap();

        assert_eq!(error.code, "invalid_argument");
    }

    #[test]
    fn file_config_carries_path_and_read_only() {
        let config = config(&draft(Some("/tmp/a.db")), &SqliteDriver.info()).unwrap();

        assert_eq!(
            config.endpoint,
            Endpoint::File {
                path: "/tmp/a.db".into()
            }
        );
        assert!(config.read_only);
    }

    #[test]
    fn unknown_options_are_rejected() {
        let mut draft = draft(Some("/tmp/a.db"));
        draft.options.insert("bogus".into(), "1".into());

        let error = validate(&draft, &SqliteDriver.info()).err().unwrap();

        assert!(error.message.contains("bogus"));
    }

    #[test]
    fn tcp_endpoint_requires_host_and_port() {
        let mut info = SqliteDriver.info();
        info.endpoint = EndpointKind::Tcp;
        let mut draft = draft(None);
        draft.host = Some("db.example.com".into());

        assert!(config(&draft, &info).is_err());

        draft.port = Some(5432);
        let config = config(&draft, &info).unwrap();
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
