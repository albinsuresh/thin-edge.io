use super::create::certificate_cn;
use super::error::CertError;
use crate::cli::certificate::show::ShowCertCmd;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::override_public_key;
use crate::reuse_private_key;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use certificate::KeyCertPair;
use tedge_config::TEdgeConfig;

/// Renew the self-signed device certificate
pub struct RenewCertCmd {
    /// The path of the certificate to be updated
    pub cert_path: Utf8PathBuf,

    /// The path where the new certificate will be stored
    pub new_cert_path: Utf8PathBuf,

    /// The path of the private key to re-use
    pub key_path: Utf8PathBuf,

    /// CSR template
    pub csr_template: CsrTemplate,
}

#[async_trait::async_trait]
impl Command for RenewCertCmd {
    fn description(&self) -> String {
        "Renew the self-signed certificate of the device.".into()
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.renew_test_certificate(&self.csr_template).await?;
        eprintln!("Certificate renewed successfully");
        eprintln!("    For an un-interrupted service:");
        eprintln!("    => the new certificate has to be uploaded to the cloud");
        eprintln!("    => then the device reconnected\n");
        ShowCertCmd::show(&self.new_cert_path).await?;
        Ok(())
    }
}

impl RenewCertCmd {
    async fn renew_test_certificate(&self, config: &CsrTemplate) -> Result<(), CertError> {
        let cert_path = &self.cert_path;
        let new_cert_path = &self.new_cert_path;
        let key_path = &self.key_path;
        let id = certificate_cn(cert_path).await?;

        // Re-create the certificate from the key, with new validity
        let previous_key = reuse_private_key(key_path)
            .await
            .map_err(|e| CertError::IoError(e).key_context(key_path.clone()))?;
        let cert = KeyCertPair::new_selfsigned_certificate(config, &id, &previous_key)?;

        override_public_key(new_cert_path, cert.certificate_pem_string()?)
            .await
            .map_err(|err| err.cert_context(new_cert_path.clone()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::certificate::test_helpers::*;
    use crate::CreateCertCmd;
    use assert_matches::assert_matches;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::*;

    #[tokio::test]
    async fn validate_renew_certificate() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let new_cert_path = temp_file_path(&dir, "my-device-cert.pem.new");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let id = "my-device-id";
        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            csr_template: CsrTemplate::default(),
        };

        // First create both cert and key
        cmd.create_test_certificate(&CsrTemplate::default())
            .await
            .unwrap();

        // Keep the cert and key data for validation
        let first_key = std::fs::read_to_string(&key_path).unwrap();
        let first_pem = parse_x509_file(&cert_path);
        let first_x509_cert = first_pem.parse_x509().expect("X.509: decoding DER failed");

        // Wait 2 secs to get different timestamp for the certificate validity
        sleep(Duration::from_secs(2));

        // Renew the certificate from the existing key
        let cmd = RenewCertCmd {
            cert_path: cert_path.clone(),
            new_cert_path: new_cert_path.clone(),
            key_path: key_path.clone(),
            csr_template: CsrTemplate::default(),
        };
        cmd.renew_test_certificate(&CsrTemplate::default())
            .await
            .unwrap();

        // Get the cert and key data for validation
        let second_key = std::fs::read_to_string(&key_path).unwrap();
        let second_pem = parse_x509_file(&new_cert_path);
        let second_x509_cert = second_pem.parse_x509().expect("X.509: decoding DER failed");

        // The key must be unchanged
        assert_eq!(first_key, second_key);

        // The new cert must have newer validity than the first one
        assert!(
            second_x509_cert.validity.not_before.timestamp()
                > first_x509_cert.validity.not_before.timestamp()
        );

        // The renewed cert is issued by thin-edge
        assert_eq!(
            second_x509_cert.issuer().to_string(),
            "CN=my-device-id, O=Thin Edge, OU=Test Device"
        );
    }

    #[tokio::test]
    async fn renew_certificate_without_key() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let new_cert_path = temp_file_path(&dir, "my-device-cert.pem.new");
        let key_path = Utf8PathBuf::from("/non/existent/key/path");

        let cmd = RenewCertCmd {
            cert_path,
            new_cert_path,
            key_path,
            csr_template: CsrTemplate::default(),
        };

        let cert_error = cmd
            .renew_test_certificate(&CsrTemplate::default())
            .await
            .unwrap_err();
        assert_matches!(cert_error, CertError::CertificateNotFound { .. });
    }
}
