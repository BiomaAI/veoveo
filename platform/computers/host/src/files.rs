use anyhow::{Context, Result, ensure};
use nix::{fcntl::OFlag, unistd::geteuid};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// Internal files never follow a final symlink.
pub fn read(path: &Path) -> Result<Vec<u8>> {
    read_handle(open_input(path, false)?)
}
/// Operator-owned ConfigMap/Secret projections follow Kubernetes revision links.
pub fn projected_input(path: &Path) -> Result<Vec<u8>> {
    read_handle(open_input(path, true)?)
}
fn open_input(path: &Path, projected: bool) -> Result<File> {
    let flags = if projected {
        OFlag::O_NONBLOCK
    } else {
        OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK
    };
    OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits())
        .open(path)
        .context("open bounded host input")
}
fn read_handle(file: File) -> Result<Vec<u8>> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == geteuid().as_raw()
            && metadata.mode() & 0o022 == 0
            && metadata.len() <= 65536,
        "host input must be a private regular file"
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 65536,
        "host input exceeds its bound"
    );
    Ok(bytes)
}
pub fn directory(path: &Path) -> Result<()> {
    child_directory(path, true)
}
pub fn owned_directory(path: &Path) -> Result<()> {
    child_directory(path, false)
}
fn child_directory(path: &Path, private: bool) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == 0
            && if private {
                metadata.mode() & 0o777 == 0o700
            } else {
                metadata.mode() & 0o022 == 0
            }
            && fs::canonicalize(path)? == path,
        "host directory must have its required owner and permissions: {}",
        path.display()
    );
    Ok(())
}
pub fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(OFlag::O_NOFOLLOW.bits())
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o777 == 0o600,
        "invalid host lock"
    );
    file.try_lock()
        .context("compute host data is already owned")?;
    Ok(file)
}
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(OFlag::O_NOFOLLOW.bits())
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    File::open(path.parent().context("host file parent")?)?.sync_all()?;
    Ok(())
}
fn projected_handle(root: &Path, name: &str) -> Result<File> {
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(root.join(name))?;
    ensure!(
        path.starts_with(&root),
        "projected host input escaped its root"
    );
    open_input(&path, true)
}
#[cfg(test)]
fn projected_file(root: &Path, name: &str) -> Result<Vec<u8>> {
    read_handle(projected_handle(root, name)?)
}
fn projected_trust(root: &Path, name: &str) -> Result<Vec<u8>> {
    let file =
        projected_handle(root, name).with_context(|| format!("read host trust input {name}"))?;
    if name.ends_with("-key.pem") {
        ensure!(
            file.metadata()?.mode() & 0o077 == 0,
            "host private key {name} must exclude group/world access"
        );
    }
    read_handle(file).with_context(|| format!("read host trust input {name}"))
}
fn certificate(bytes: &[u8]) -> Result<Vec<u8>> {
    let (remaining, pem) = x509_parser::pem::parse_x509_pem(bytes)
        .map_err(|_| anyhow::anyhow!("invalid PEM certificate"))?;
    ensure!(
        pem.label == "CERTIFICATE" && remaining.iter().all(u8::is_ascii_whitespace),
        "one complete PEM certificate required"
    );
    let (remaining, _) = x509_parser::parse_x509_certificate(&pem.contents)
        .map_err(|_| anyhow::anyhow!("invalid X.509 certificate"))?;
    ensure!(remaining.is_empty(), "trailing certificate data");
    Ok(pem.contents)
}

/// Admit independent provider/server, worker/client, supervisor/client and storage roots.
fn client_roots(
    server: &[u8],
    worker: &[u8],
    supervisor: &[u8],
    storage: &[u8],
) -> Result<Vec<u8>> {
    let mut identities = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    for (name, bytes) in [
        ("provider-ca.pem", server),
        ("worker-client-ca.pem", worker),
        ("supervisor-client-ca.pem", supervisor),
        ("storage-ca.pem", storage),
    ] {
        let der = certificate(bytes).with_context(|| format!("validate host root {name}"))?;
        let (_, cert) = x509_parser::parse_x509_certificate(&der)
            .map_err(|_| anyhow::anyhow!("invalid host root {name}"))?;
        ensure!(
            cert.is_ca() && cert.subject() == cert.issuer() && cert.validity().is_valid(),
            "host root {name} must be a valid self-signed CA"
        );
        cert.verify_signature(None)
            .map_err(|_| anyhow::anyhow!("invalid host root signature {name}"))?;
        ensure!(
            identities.insert(der.clone()) && keys.insert(cert.public_key().raw.to_vec()),
            "provider server, worker client, supervisor client and storage require independent trust roots"
        );
    }
    let mut bundle = worker.to_vec();
    bundle.push(b'\n');
    bundle.extend_from_slice(supervisor);
    ensure!(
        bundle.len() <= 65536,
        "provider client root bundle exceeds its bound"
    );
    Ok(bundle)
}
enum LeafRole {
    ProviderServer,
    SupervisorClient,
}
fn signed_certificate(bytes: &[u8], root: &[u8], role: LeafRole) -> Result<()> {
    let leaf = certificate(bytes)?;
    let root = certificate(root)?;
    let (_, leaf) = x509_parser::parse_x509_certificate(&leaf)
        .map_err(|_| anyhow::anyhow!("invalid host leaf certificate"))?;
    let (_, root) = x509_parser::parse_x509_certificate(&root)
        .map_err(|_| anyhow::anyhow!("invalid host leaf issuer"))?;
    ensure!(
        !leaf.is_ca() && leaf.issuer() == root.subject() && leaf.validity().is_valid(),
        "host leaf certificate must belong to its declared trust role"
    );
    let purpose = leaf
        .extended_key_usage()
        .map_err(|_| anyhow::anyhow!("invalid host certificate purpose"))?
        .context("host leaf certificate requires an extended key usage")?;
    ensure!(
        match role {
            LeafRole::SupervisorClient => purpose.value.client_auth && !purpose.value.server_auth,
            LeafRole::ProviderServer => purpose.value.server_auth && !purpose.value.client_auth,
        },
        "host certificate has another trust role"
    );
    leaf.verify_signature(Some(root.public_key()))
        .map_err(|_| anyhow::anyhow!("host certificate has another trust issuer"))?;
    Ok(())
}

pub struct IssuerCa(PathBuf);
impl IssuerCa {
    pub fn path(&self) -> &Path {
        &self.0
    }
}
pub(super) fn issuer_ca(source: &Path, target: &Path) -> Result<Option<IssuerCa>> {
    let source = fs::canonicalize(source).context("resolve host trust root")?;
    match fs::symlink_metadata(source.join("issuer-ca.pem")) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("inspect issuer-ca.pem"),
    }
    let bytes = projected_trust(&source, "issuer-ca.pem")?;
    ensure!(
        bytes
            .trim_ascii_start()
            .starts_with(b"-----BEGIN CERTIFICATE-----"),
        "issuer-ca.pem requires one complete CA certificate"
    );
    let der = certificate(&bytes).context("validate issuer-ca.pem")?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der)
        .map_err(|_| anyhow::anyhow!("invalid issuer CA certificate"))?;
    ensure!(
        cert.is_ca() && cert.subject() == cert.issuer() && cert.validity().is_valid(),
        "issuer-ca.pem must be a valid self-signed CA"
    );
    cert.verify_signature(None)
        .map_err(|_| anyhow::anyhow!("invalid issuer CA signature"))?;
    let path = target.join("issuer-ca.pem");
    write(&path, &bytes)?;
    Ok(Some(IssuerCa(path)))
}

pub fn trust() -> Result<Option<IssuerCa>> {
    let target = Path::new(super::config::RUN).join("trust");
    directory(&target)?;
    for name in [
        "provider-ca.pem",
        "worker-client-ca.pem",
        "supervisor-client-ca.pem",
        "provider-server.pem",
        "provider-server-key.pem",
        "guest.pem",
        "guest-key.pem",
        "jwt-key.pem",
        "jwt-public.pem",
        "jwt-kid",
        "storage-ca.pem",
        "storage-server.pem",
        "storage-server-key.pem",
    ] {
        let bytes = projected_trust(Path::new(super::config::TRUST), name)?;
        write(&target.join(name), &bytes)?;
    }
    let server = read(&target.join("provider-ca.pem"))?;
    let worker = read(&target.join("worker-client-ca.pem"))?;
    let supervisor = read(&target.join("supervisor-client-ca.pem"))?;
    let storage = read(&target.join("storage-ca.pem"))?;
    let bundle = client_roots(&server, &worker, &supervisor, &storage)?;
    signed_certificate(
        &read(&target.join("provider-server.pem"))?,
        &server,
        LeafRole::ProviderServer,
    )
    .context("validate provider-server.pem")?;
    signed_certificate(
        &read(&target.join("guest.pem"))?,
        &supervisor,
        LeafRole::SupervisorClient,
    )
    .context("validate guest.pem")?;
    write(&target.join("provider-client-ca.pem"), &bundle)?;
    issuer_ca(Path::new(super::config::TRUST), &target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn projected_inputs_are_bounded_and_cannot_escape_the_mounted_root() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("projection");
        fs::create_dir(&source).unwrap();
        let data = source.join("revision");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("key"), b"test-only-key").unwrap();
        fs::set_permissions(data.join("key"), fs::Permissions::from_mode(0o600)).unwrap();
        symlink("revision/key", source.join("key")).unwrap();
        assert_eq!(projected_file(&source, "key").unwrap(), b"test-only-key");
        fs::write(temp.path().join("outside"), b"foreign").unwrap();
        symlink("../outside", source.join("escape")).unwrap();
        assert!(projected_file(&source, "escape").is_err());
        fs::write(data.join("key"), vec![0; 65537]).unwrap();
        assert!(projected_file(&source, "key").is_err());
    }
    fn root(name: &str) -> (String, rcgen::Issuer<'static, rcgen::KeyPair>) {
        let mut params = rcgen::CertificateParams::new(vec![]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, name);
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
        let key = rcgen::KeyPair::generate().unwrap();
        let pem = params.self_signed(&key).unwrap().pem();
        (pem, rcgen::Issuer::new(params, key))
    }
    fn leaf(
        issuer: &rcgen::Issuer<'_, rcgen::KeyPair>,
        purpose: rcgen::ExtendedKeyUsagePurpose,
    ) -> String {
        let mut params = rcgen::CertificateParams::new(vec![]).unwrap();
        params.extended_key_usages = vec![purpose];
        params
            .signed_by(&rcgen::KeyPair::generate().unwrap(), issuer)
            .unwrap()
            .pem()
    }
    #[test]
    fn optional_issuer_ca_captures_projection_and_ignores_stale_copy_after_removal() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(issuer_ca(&source, &target).unwrap().is_none());
        let (old, _) = root("old issuer");
        let (new, _) = root("new issuer");
        for (revision, pem) in [("old", &old), ("new", &new)] {
            fs::create_dir(source.join(revision)).unwrap();
            fs::write(source.join(revision).join("issuer-ca.pem"), pem).unwrap();
            fs::set_permissions(
                source.join(revision).join("issuer-ca.pem"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }
        symlink("old", source.join("..data")).unwrap();
        symlink("..data/issuer-ca.pem", source.join("issuer-ca.pem")).unwrap();
        let admitted = issuer_ca(&source, &target).unwrap().unwrap();
        symlink("new", source.join("..next")).unwrap();
        fs::rename(source.join("..next"), source.join("..data")).unwrap();
        assert_eq!(read(admitted.path()).unwrap(), old.as_bytes());
        let current = issuer_ca(&source, &target).unwrap().unwrap();
        assert_eq!(read(current.path()).unwrap(), new.as_bytes());
        fs::remove_file(source.join("issuer-ca.pem")).unwrap();
        assert!(issuer_ca(&source, &target).unwrap().is_none());
        assert!(current.path().exists());
    }
    #[test]
    fn optional_issuer_ca_rejects_invalid_present_entries() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let (ca, issuer) = root("issuer");
        let leaf = leaf(&issuer, rcgen::ExtendedKeyUsagePurpose::ServerAuth);
        let key = rcgen::KeyPair::generate().unwrap().serialize_pem();
        for bytes in [
            Vec::new(),
            b"garbage".to_vec(),
            key.into_bytes(),
            leaf.into_bytes(),
            format!("{ca}garbage").into_bytes(),
            format!("garbage\n{ca}").into_bytes(),
            format!("{ca}{ca}").into_bytes(),
            vec![0; 65537],
        ] {
            let path = source.path().join("issuer-ca.pem");
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(issuer_ca(source.path(), target.path()).is_err());
        }
        let path = source.path().join("issuer-ca.pem");
        fs::write(&path, &ca).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(issuer_ca(source.path(), target.path()).is_err());
        fs::remove_file(&path).unwrap();
        symlink("absent", &path).unwrap();
        assert!(issuer_ca(source.path(), target.path()).is_err());
        fs::remove_file(&path).unwrap();
        let outside = target.path().join("outside.pem");
        fs::write(&outside, ca).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(issuer_ca(source.path(), target.path()).is_err());
    }
    #[test]
    fn provider_listener_admits_only_independent_client_roots_and_correct_leaf_roles() {
        let (server, server_issuer) = root("server");
        let (worker, _) = root("worker");
        let (supervisor, supervisor_issuer) = root("supervisor");
        let (storage, _) = root("storage");
        let bundle = client_roots(
            server.as_bytes(),
            worker.as_bytes(),
            supervisor.as_bytes(),
            storage.as_bytes(),
        )
        .unwrap();
        let roots = x509_parser::pem::Pem::iter_from_buffer(&bundle)
            .map(|pem| pem.unwrap().contents)
            .collect::<Vec<_>>();
        assert_eq!(
            roots,
            vec![
                certificate(worker.as_bytes()).unwrap(),
                certificate(supervisor.as_bytes()).unwrap()
            ]
        );
        let server_leaf = leaf(&server_issuer, rcgen::ExtendedKeyUsagePurpose::ServerAuth);
        let guest_leaf = leaf(
            &supervisor_issuer,
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        );
        signed_certificate(
            server_leaf.as_bytes(),
            server.as_bytes(),
            LeafRole::ProviderServer,
        )
        .unwrap();
        signed_certificate(
            guest_leaf.as_bytes(),
            supervisor.as_bytes(),
            LeafRole::SupervisorClient,
        )
        .unwrap();
        assert!(
            signed_certificate(
                guest_leaf.as_bytes(),
                server.as_bytes(),
                LeafRole::SupervisorClient
            )
            .is_err()
        );
        let (_, other_supervisor) = root("supervisor");
        let foreign_guest = leaf(
            &other_supervisor,
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        );
        assert!(
            signed_certificate(
                foreign_guest.as_bytes(),
                supervisor.as_bytes(),
                LeafRole::SupervisorClient
            )
            .is_err()
        );
        assert!(
            signed_certificate(
                server_leaf.as_bytes(),
                server.as_bytes(),
                LeafRole::SupervisorClient
            )
            .is_err()
        );
        let reformatted = worker.replace('\n', "\r\n");
        assert_ne!(worker, reformatted);
        assert!(
            client_roots(
                server.as_bytes(),
                worker.as_bytes(),
                reformatted.as_bytes(),
                storage.as_bytes()
            )
            .is_err()
        );
        for invalid in [
            "",
            "garbage",
            "-----BEGIN CERTIFICATE-----\nbroken\n-----END CERTIFICATE-----\n",
        ] {
            assert!(
                client_roots(
                    server.as_bytes(),
                    invalid.as_bytes(),
                    supervisor.as_bytes(),
                    storage.as_bytes()
                )
                .is_err()
            );
        }
        let repeated = format!("{worker}{worker}");
        assert!(
            client_roots(
                server.as_bytes(),
                repeated.as_bytes(),
                supervisor.as_bytes(),
                storage.as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn projection_rotation_keeps_opened_revision_and_internal_reads_refuse_links() {
        let temp = tempfile::tempdir().unwrap();
        for (revision, bytes) in [("old", b"old"), ("new", b"new")] {
            fs::create_dir(temp.path().join(revision)).unwrap();
            fs::write(temp.path().join(revision).join("host.json"), bytes).unwrap();
            fs::set_permissions(
                temp.path().join(revision).join("host.json"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }
        symlink("old", temp.path().join("..data")).unwrap();
        symlink("..data/host.json", temp.path().join("host.json")).unwrap();
        let old = open_input(&temp.path().join("host.json"), true).unwrap();
        symlink("new", temp.path().join("..next")).unwrap();
        fs::rename(temp.path().join("..next"), temp.path().join("..data")).unwrap();
        assert_eq!(read_handle(old).unwrap(), b"old");
        assert_eq!(
            projected_input(&temp.path().join("host.json")).unwrap(),
            b"new"
        );
        assert!(read(&temp.path().join("host.json")).is_err());
        assert!(lock(&temp.path().join("host.json")).is_err());
        let key = temp.path().join("new/guest-key.pem");
        fs::write(&key, b"test-key").unwrap();
        symlink("..data/guest-key.pem", temp.path().join("guest-key.pem")).unwrap();
        assert!(projected_trust(temp.path(), "guest-key.pem").is_err());
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            projected_trust(temp.path(), "guest-key.pem").unwrap(),
            b"test-key"
        );
        fs::write(&key, []).unwrap();
        assert!(projected_trust(temp.path(), "guest-key.pem").is_err());
        fs::write(&key, vec![0; 65537]).unwrap();
        assert!(projected_trust(temp.path(), "guest-key.pem").is_err());
    }
}
