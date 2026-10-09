use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn put(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    fs::write(root.join(name), bytes).unwrap();
    fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o600)).unwrap();
}
fn authority(root: &Path, leaves: &[&str]) {
    fs::create_dir(root).unwrap();
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    let mut ca = CertificateParams::new(vec![]).unwrap();
    ca.distinguished_name.push(
        DnType::CommonName,
        root.file_name().unwrap().to_str().unwrap(),
    );
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let key = KeyPair::generate().unwrap();
    put(root, "ca.pem", ca.self_signed(&key).unwrap().pem());
    let issuer = Issuer::new(ca, key);
    for (name, cn, purpose, sans) in [
        (
            "server",
            "veoveo-computers-provider",
            ExtendedKeyUsagePurpose::ServerAuth,
            vec![
                "localhost".into(),
                "127.0.0.1".into(),
                "host.openshell.internal".into(),
            ],
        ),
        (
            "client",
            "veoveo-computers-worker",
            ExtendedKeyUsagePurpose::ClientAuth,
            vec![],
        ),
        (
            "guest",
            "veoveo-computer-supervisor",
            ExtendedKeyUsagePurpose::ClientAuth,
            vec![],
        ),
    ] {
        if !leaves.contains(&name) {
            continue;
        }
        let mut params = CertificateParams::new(sans).unwrap();
        params.distinguished_name.push(DnType::CommonName, cn);
        params.extended_key_usages = vec![purpose];
        let key = KeyPair::generate().unwrap();
        put(
            root,
            &format!("{name}.pem"),
            params.signed_by(&key, &issuer).unwrap().pem(),
        );
        put(root, &format!("{name}-key.pem"), key.serialize_pem());
    }
}
pub fn create(root: &Path) {
    authority(&root.join("provider"), &["server"]);
    authority(&root.join("worker"), &["client"]);
    authority(&root.join("supervisor"), &["guest"]);
    authority(&root.join("storage"), &["server", "client"]);
    // Worker keeps server trust and its own client identity; Host receives only its root.
    for name in ["client.pem", "client-key.pem"] {
        put(
            &root.join("provider"),
            name,
            fs::read(root.join("worker").join(name)).unwrap(),
        );
    }
    for name in ["guest.pem", "guest-key.pem"] {
        put(
            &root.join("provider"),
            name,
            fs::read(root.join("supervisor").join(name)).unwrap(),
        );
    }
    let trust = root.join("trust");
    fs::create_dir(&trust).unwrap();
    for (prefix, directory) in [("provider", "provider"), ("storage", "storage")] {
        for name in ["ca.pem", "server.pem", "server-key.pem"] {
            put(
                &trust,
                &format!("{prefix}-{name}"),
                fs::read(root.join(directory).join(name)).unwrap(),
            );
        }
    }
    for (name, directory) in [
        ("worker-client-ca.pem", "worker"),
        ("supervisor-client-ca.pem", "supervisor"),
    ] {
        put(
            &trust,
            name,
            fs::read(root.join(directory).join("ca.pem")).unwrap(),
        );
    }
    for name in ["guest.pem", "guest-key.pem"] {
        put(
            &trust,
            name,
            fs::read(root.join("supervisor").join(name)).unwrap(),
        );
    }
    let key = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    put(&trust, "jwt-key.pem", key.serialize_pem());
    put(&trust, "jwt-public.pem", key.public_key_pem());
    put(&trust, "jwt-kid", uuid::Uuid::now_v7().to_string());
}
