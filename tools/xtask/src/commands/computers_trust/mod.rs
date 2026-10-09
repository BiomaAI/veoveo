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
fn authority(root: &Path, name: &str, worker_trust: bool) -> Result<Issuer<'static, KeyPair>> {
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
    if worker_trust {
        put(&root.join("worker").join(format!("{name}-ca.pem")), &ca)?;
    }
    Ok(Issuer::new(params, key))
}
struct Leaf<'a> {
    filename: &'a str,
    common_name: &'a str,
    purpose: ExtendedKeyUsagePurpose,
    directory: &'a str,
    sans: Vec<String>,
}
fn issue(root: &Path, issuer: &Issuer<'_, KeyPair>, leaf: Leaf<'_>) -> Result<()> {
    let mut params = CertificateParams::new(leaf.sans)?;
    dates(&mut params, 90);
    params
        .distinguished_name
        .push(DnType::CommonName, leaf.common_name);
    params.extended_key_usages = vec![leaf.purpose];
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    let key = KeyPair::generate()?;
    put(
        &root
            .join(leaf.directory)
            .join(format!("{}.pem", leaf.filename)),
        params.signed_by(&key, issuer)?.pem(),
    )?;
    put(
        &root
            .join(leaf.directory)
            .join(format!("{}-key.pem", leaf.filename)),
        key.serialize_pem(),
    )?;
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
    let provider = authority(output, "provider", true)?;
    let worker = authority(output, "worker-client", false)?;
    let supervisor = authority(output, "supervisor-client", false)?;
    let storage = authority(output, "storage", true)?;
    for (issuer, leaf) in [
        (
            &provider,
            Leaf {
                filename: "provider-server",
                common_name: "veoveo-computers-provider",
                purpose: ExtendedKeyUsagePurpose::ServerAuth,
                directory: "host",
                sans: vec![host.into(), "host.openshell.internal".into()],
            },
        ),
        (
            &worker,
            Leaf {
                filename: "provider-worker",
                common_name: "veoveo-computers-worker",
                purpose: ExtendedKeyUsagePurpose::ClientAuth,
                directory: "worker",
                sans: vec![],
            },
        ),
        (
            &supervisor,
            Leaf {
                filename: "guest",
                common_name: "veoveo-computer-supervisor",
                purpose: ExtendedKeyUsagePurpose::ClientAuth,
                directory: "host",
                sans: vec![],
            },
        ),
        (
            &storage,
            Leaf {
                filename: "storage-server",
                common_name: "veoveo-computers-provider",
                purpose: ExtendedKeyUsagePurpose::ServerAuth,
                directory: "host",
                sans: vec![host.into()],
            },
        ),
        (
            &storage,
            Leaf {
                filename: "storage-worker",
                common_name: "veoveo-computers-worker",
                purpose: ExtendedKeyUsagePurpose::ClientAuth,
                directory: "worker",
                sans: vec![],
            },
        ),
    ] {
        issue(output, issuer, leaf)?;
    }
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
        "Veoveo Computers fresh enrollment. Leaf certificates expire in 90 days.\nInstall host/ and worker/ as separate Secrets. Provider server, worker client and supervisor client have separate CA roots; storage has its own root. Keep operator/ offline; it contains CA keys.\nSet execution.activeKeyId and execution.keys[0].id to worker/command-key-id; reference /etc/veoveo/computers/trust/command-key.bin.\nKeep the complete bundle in installation-owned encrypted backup. Never rerun enrollment over an installed provider.\n",
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
        let roots = ["provider", "worker-client", "supervisor-client", "storage"].map(|name| {
            let bytes = fs::read(output.join(format!("host/{name}-ca.pem"))).unwrap();
            x509_parser::pem::parse_x509_pem(&bytes).unwrap().1.contents
        });
        let mut keys = std::collections::BTreeSet::new();
        for der in &roots {
            let (_, certificate) = x509_parser::parse_x509_certificate(der).unwrap();
            assert!(certificate.is_ca());
            certificate.verify_signature(None).unwrap();
            assert!(keys.insert(certificate.public_key().raw.to_vec()));
        }
        for (path, root_index) in [
            ("host/provider-server.pem", 0),
            ("worker/provider-worker.pem", 1),
            ("host/guest.pem", 2),
            ("host/storage-server.pem", 3),
            ("worker/storage-worker.pem", 3),
        ] {
            let bytes = fs::read(output.join(path)).unwrap();
            let pem = x509_parser::pem::parse_x509_pem(&bytes).unwrap().1;
            let certificate = pem.parse_x509().unwrap();
            for (index, der) in roots.iter().enumerate() {
                let (_, root) = x509_parser::parse_x509_certificate(der).unwrap();
                assert_eq!(
                    certificate
                        .verify_signature(Some(root.public_key()))
                        .is_ok(),
                    index == root_index,
                    "{path} crossed its declared CA role"
                );
            }
        }
        for name in ["provider", "worker-client", "supervisor-client", "storage"] {
            assert!(output.join(format!("operator/{name}-ca-key.pem")).exists());
            assert!(!output.join(format!("host/{name}-ca-key.pem")).exists());
            assert!(!output.join(format!("worker/{name}-ca-key.pem")).exists());
        }
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
