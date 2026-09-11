//! Fresh enrollment only. Rotation of an installed provider is a separate maintenance action.
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};

fn directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}
fn put(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes.as_ref())?;
    file.sync_all()?;
    Ok(())
}
fn dates(params: &mut CertificateParams, days: i64) {
    let start = (Utc::now() - chrono::Duration::days(1)).date_naive();
    let end = (Utc::now() + chrono::Duration::days(days)).date_naive();
    params.not_before = rcgen::date_time_ymd(start.year(), start.month() as u8, start.day() as u8);
    params.not_after = rcgen::date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
}
fn authority(root: &Path, name: &str, host: &str, provider: bool) -> Result<()> {
    let mut params = CertificateParams::new(vec![])?;
    dates(&mut params, 3650);
    params
        .distinguished_name
        .push(DnType::CommonName, format!("Veoveo Computers {name} CA"));
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let key = KeyPair::generate()?;
    let ca = params.self_signed(&key)?.pem();
    put(
        &root.join("operator").join(format!("{name}-ca-key.pem")),
        key.serialize_pem(),
    )?;
    put(&root.join("operator").join(format!("{name}-ca.pem")), &ca)?;
    put(&root.join("host").join(format!("{name}-ca.pem")), &ca)?;
    put(&root.join("worker").join(format!("{name}-ca.pem")), &ca)?;
    let issuer = Issuer::new(params, key);
    for (role, cn, purpose, directory) in [
        (
            "server",
            "veoveo-computers-provider",
            ExtendedKeyUsagePurpose::ServerAuth,
            "host",
        ),
        (
            "worker",
            "veoveo-computers-worker",
            ExtendedKeyUsagePurpose::ClientAuth,
            "worker",
        ),
        (
            "guest",
            "veoveo-computer-supervisor",
            ExtendedKeyUsagePurpose::ClientAuth,
            "host",
        ),
    ] {
        if role == "guest" && !provider {
            continue;
        }
        let sans = if role == "server" {
            let mut names = vec![host.into()];
            if provider {
                names.push("host.openshell.internal".into());
            }
            names
        } else {
            vec![]
        };
        let mut params = CertificateParams::new(sans)?;
        dates(&mut params, 90);
        params.distinguished_name.push(DnType::CommonName, cn);
        params.extended_key_usages = vec![purpose];
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        let key = KeyPair::generate()?;
        let filename = if role == "guest" {
            "guest".into()
        } else {
            format!("{name}-{role}")
        };
        put(
            &root.join(directory).join(format!("{filename}.pem")),
            params.signed_by(&key, &issuer)?.pem(),
        )?;
        put(
            &root.join(directory).join(format!("{filename}-key.pem")),
            key.serialize_pem(),
        )?;
    }
    Ok(())
}
pub(crate) fn create(output: &Path, host: &str) -> Result<()> {
    ensure!(
        host.len() <= 253
            && host.contains('.')
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            }),
        "host-name must be an explicit private DNS name"
    );
    directory(output).context("trust output must be a new directory with an existing parent")?;
    for name in ["host", "worker", "operator"] {
        directory(&output.join(name))?;
    }
    authority(output, "provider", host, true)?;
    authority(output, "storage", host, false)?;
    let mut command_key = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut command_key)?;
    put(&output.join("worker/command-key.bin"), command_key)?;
    put(
        &output.join("worker/command-key-id"),
        uuid::Uuid::now_v7().to_string(),
    )?;
    let jwt = KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
    put(&output.join("host/jwt-key.pem"), jwt.serialize_pem())?;
    put(&output.join("host/jwt-public.pem"), jwt.public_key_pem())?;
    put(
        &output.join("host/jwt-kid"),
        uuid::Uuid::now_v7().to_string(),
    )?;
    put(
        &output.join("README.txt"),
        "Veoveo Computers fresh enrollment. Leaf certificates expire in 90 days.\nInstall host/ and worker/ as separate Secrets. Keep operator/ offline; it contains CA keys.\nSet execution.activeKeyId and execution.keys[0].id to worker/command-key-id; reference /etc/veoveo/computers/trust/command-key.bin.\nKeep the complete bundle in installation-owned encrypted backup. Never rerun enrollment over an installed provider.\n",
    )?;
    println!(
        "Created private Computers trust bundle; certificates expire in 90 days. Keep operator CA keys outside Kubernetes."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn fresh_enrollment_separates_authorities_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("trust");
        create(&output, "computer-host.test.svc.cluster.local").unwrap();
        assert!(create(&output, "computer-host.test.svc.cluster.local").is_err());
        assert!(!output.join("host/provider-worker-key.pem").exists());
        assert!(!output.join("worker/guest-key.pem").exists());
        assert!(!output.join("host/provider-ca-key.pem").exists());
        assert!(!output.join("host/command-key.bin").exists());
        assert_eq!(
            fs::read(output.join("worker/command-key.bin"))
                .unwrap()
                .len(),
            32
        );
        assert!(
            uuid::Uuid::parse_str(
                &fs::read_to_string(output.join("worker/command-key-id")).unwrap()
            )
            .is_ok()
        );
        assert_ne!(
            fs::read(output.join("host/provider-ca.pem")).unwrap(),
            fs::read(output.join("host/storage-ca.pem")).unwrap()
        );
        for directory in ["host", "worker", "operator"] {
            assert_eq!(
                fs::metadata(output.join(directory))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            for entry in fs::read_dir(output.join(directory)).unwrap() {
                assert_eq!(
                    entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        assert!(create(&root.path().join("invalid"), "host/elsewhere").is_err());
        assert!(!root.path().join("invalid").exists());
    }
}
