pub use self::cli::TEdgeCertCli;

mod cli;
mod create;
mod create_csr;
mod error;
mod remove;
mod renew;
mod show;
mod upload;

pub use self::cli::*;
pub use self::create::*;
pub use self::error::*;

#[cfg(test)]
mod test_helpers {
    use camino::Utf8PathBuf;
    use std::path::Path;
    use tempfile::TempDir;
    use x509_parser::der_parser::asn1_rs::FromDer;
    use x509_parser::nom::AsBytes;

    pub fn temp_file_path(dir: &TempDir, filename: &str) -> Utf8PathBuf {
        dir.path().join(filename).try_into().unwrap()
    }
    pub fn parse_pem_file(path: impl AsRef<Path>) -> pem::Pem {
        let content = std::fs::read(path).expect("fail to read {path}");
        pem::parse(content).expect("Reading PEM block failed")
    }

    pub fn parse_x509_file(path: impl AsRef<Path>) -> x509_parser::pem::Pem {
        let content = std::fs::read(path).expect("fail to read {path}");

        x509_parser::pem::Pem::iter_from_buffer(&content)
            .next()
            .unwrap()
            .expect("Reading PEM block failed")
    }

    pub fn get_subject_from_csr(content: Vec<u8>) -> String {
        x509_parser::certification_request::X509CertificationRequest::from_der(content.as_bytes())
            .unwrap()
            .1
            .certification_request_info
            .subject
            .to_string()
    }
}
