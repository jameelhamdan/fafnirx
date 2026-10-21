//! Self-signed CA and server certificate, generated on first start. Clients
//! pin the CA (`ca.pem`).

use anyhow::{Context, Result};
use rcgen::{date_time_ymd, BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
use std::fs;
use std::path::Path;

pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn ensure(dir: &Path, hosts: &[String]) -> Result<Identity> {
    fs::create_dir_all(dir)?;
    let ca_pem = dir.join("ca.pem");
    let ca_key_path = dir.join("ca.key");
    let cert_path = dir.join("server.pem");
    let key_path = dir.join("server.key");
    let hosts_path = dir.join("hosts.txt");
    let hosts_line = hosts.join(",");

    let ca_key = if ca_key_path.is_file() {
        KeyPair::from_pem(&fs::read_to_string(&ca_key_path)?)?
    } else {
        KeyPair::generate()?
    };
    let mut ca_params = if ca_pem.is_file() {
        CertificateParams::from_ca_cert_pem(&fs::read_to_string(&ca_pem)?)?
    } else {
        let mut p = CertificateParams::new(Vec::<String>::new())?;
        p.distinguished_name.push(DnType::CommonName, "fafnirx CA");
        p.not_before = date_time_ymd(2024, 1, 1);
        p.not_after = date_time_ymd(2064, 1, 1);
        p
    };
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_cert = ca_params.self_signed(&ca_key)?;
    if !ca_pem.is_file() {
        fs::write(&ca_pem, ca_cert.pem())?;
        write_private(&ca_key_path, &ca_key.serialize_pem())?;
    }

    let unchanged = cert_path.is_file()
        && key_path.is_file()
        && fs::read_to_string(&hosts_path).map(|h| h == hosts_line).unwrap_or(false);
    if !unchanged {
        let server_key = KeyPair::generate()?;
        let mut params = CertificateParams::new(hosts.to_vec()).context("invalid TLS host")?;
        params.distinguished_name.push(DnType::CommonName, "fafnirx-server");
        // Strict verifiers (rustls, Python 3.13+) look the issuer up by key identifier.
        params.use_authority_key_identifier_extension = true;
        params.not_before = date_time_ymd(2024, 1, 1);
        params.not_after = date_time_ymd(2064, 1, 1);
        let cert = params.signed_by(&server_key, &ca_cert, &ca_key)?;
        fs::write(&cert_path, cert.pem())?;
        write_private(&key_path, &server_key.serialize_pem())?;
        fs::write(&hosts_path, &hosts_line)?;
    }

    Ok(Identity { cert_pem: fs::read_to_string(&cert_path)?, key_pem: fs::read_to_string(&key_path)? })
}

fn write_private(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_reuses_ca() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        ensure(dir.path(), &hosts).unwrap();
        let ca1 = fs::read_to_string(dir.path().join("ca.pem")).unwrap();
        // Changing the hosts re-issues the server certificate under the same CA.
        let hosts2 = vec!["localhost".to_string(), "10.0.0.5".to_string()];
        let id = ensure(dir.path(), &hosts2).unwrap();
        let ca2 = fs::read_to_string(dir.path().join("ca.pem")).unwrap();
        assert_eq!(ca1, ca2);
        assert!(id.cert_pem.contains("BEGIN CERTIFICATE"));
    }
}
