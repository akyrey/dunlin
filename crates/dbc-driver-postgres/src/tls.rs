//! rustls client configuration for each TLS mode (SECURITY.md#tls).
//!
//! `prefer` and `require` encrypt without verifying the certificate, as libpq does. `verify-ca`
//! checks the chain but not the name; `verify-full` checks both, against the *real* server name
//! (not the tunnel's loopback address, DECISIONS D-004).

use std::path::Path;
use std::sync::Arc;

use dbc_core::driver::{TlsConfig, TlsMode};
use dbc_core::{DbError, DbErrorCode};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};

/// `None` for `disable`.
pub(crate) fn client_config(tls: &TlsConfig) -> Result<Option<ClientConfig>, DbError> {
    if tls.mode == TlsMode::Disable {
        return Ok(None);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier: Arc<dyn ServerCertVerifier> = match tls.mode {
        TlsMode::VerifyFull => webpki_verifier(tls, &provider)?,
        TlsMode::VerifyCa => Arc::new(IgnoreName {
            inner: webpki_verifier(tls, &provider)?,
        }),
        _ => Arc::new(NoVerification {
            provider: Arc::clone(&provider),
        }),
    };
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(tls_error)?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    let config = match (&tls.client_cert_file, &tls.client_key_file) {
        (Some(cert), Some(key)) => builder
            .with_client_auth_cert(certificates(cert)?, private_key(key)?)
            .map_err(tls_error)?,
        (None, None) => builder.with_no_client_auth(),
        _ => {
            return Err(DbError::new(
                DbErrorCode::Tls,
                "A client certificate needs both a certificate and a key file",
            ));
        }
    };
    Ok(Some(config))
}

fn webpki_verifier(
    tls: &TlsConfig,
    provider: &Arc<CryptoProvider>,
) -> Result<Arc<WebPkiServerVerifier>, DbError> {
    let mut roots = RootCertStore::empty();
    // Unreadable platform certificates are skipped, as browsers do.
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    if let Some(ca) = &tls.ca_file {
        for certificate in certificates(ca)? {
            roots.add(certificate).map_err(tls_error)?;
        }
    }
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(provider))
        .build()
        .map_err(|e| DbError::new(DbErrorCode::Tls, format!("Cannot verify certificates: {e}")))
}

fn certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, DbError> {
    let certificates = CertificateDer::pem_file_iter(path)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|e| file_error(path, &e))?;
    if certificates.is_empty() {
        return Err(DbError::new(
            DbErrorCode::Tls,
            format!("No certificate found in {}", path.display()),
        ));
    }
    Ok(certificates)
}

fn private_key(path: &Path) -> Result<PrivateKeyDer<'static>, DbError> {
    PrivateKeyDer::from_pem_file(path).map_err(|e| file_error(path, &e))
}

fn file_error(path: &Path, error: &dyn std::fmt::Display) -> DbError {
    DbError::new(
        DbErrorCode::Tls,
        format!("Cannot read {}: {error}", path.display()),
    )
}

fn tls_error(error: rustls::Error) -> DbError {
    DbError::new(DbErrorCode::Tls, error.to_string())
}

/// `verify-ca`: a trusted chain is required, the name is not checked.
#[derive(Debug)]
struct IgnoreName {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for IgnoreName {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self
            .inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// `prefer` / `require`: encrypted, certificate not checked (the form warns about this).
/// Handshake signatures are still verified, so the session key belongs to the presented cert.
#[derive(Debug)]
struct NoVerification {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn disable_needs_no_configuration() {
        assert!(client_config(&TlsConfig::default()).unwrap().is_none());
    }

    #[test]
    fn every_encrypting_mode_builds_a_configuration() {
        for mode in [
            TlsMode::Prefer,
            TlsMode::Require,
            TlsMode::VerifyCa,
            TlsMode::VerifyFull,
        ] {
            let tls = TlsConfig {
                mode,
                ..TlsConfig::default()
            };

            assert!(client_config(&tls).unwrap().is_some(), "{mode:?}");
        }
    }

    #[test]
    fn missing_ca_file_is_a_tls_error() {
        let tls = TlsConfig {
            mode: TlsMode::VerifyFull,
            ca_file: Some(PathBuf::from("/nonexistent/ca.pem")),
            ..TlsConfig::default()
        };

        let error = client_config(&tls).err().unwrap();

        assert_eq!(error.code, DbErrorCode::Tls);
        assert!(error.message.contains("/nonexistent/ca.pem"));
    }

    #[test]
    fn a_certificate_without_a_key_is_rejected() {
        let tls = TlsConfig {
            mode: TlsMode::Require,
            client_cert_file: Some(PathBuf::from("/tmp/cert.pem")),
            ..TlsConfig::default()
        };

        assert_eq!(
            client_config(&tls).err().map(|e| e.code),
            Some(DbErrorCode::Tls)
        );
    }
}
