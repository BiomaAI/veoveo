use serde_json::{Value, json};
use std::{net::SocketAddr, path::PathBuf};
use uuid::Uuid;
use veoveo_computers_runtime::{
    DevelopmentTemplate, PERSISTENT_COMMAND, PersistentHome, parse_policy,
};

pub struct Files(pub PathBuf);
impl Files {
    pub fn new() -> Self {
        use rcgen::{
            BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
            KeyUsagePurpose,
        };
        let dir = std::env::temp_dir().join(format!("veoveo-computers-config-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let mut ca = CertificateParams::new(vec![]).unwrap();
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let key = KeyPair::generate().unwrap();
        std::fs::write(dir.join("ca.pem"), ca.self_signed(&key).unwrap().pem()).unwrap();
        let issuer = Issuer::new(ca, key);
        let mut client = CertificateParams::new(vec![]).unwrap();
        client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let key = KeyPair::generate().unwrap();
        std::fs::write(
            dir.join("cert.pem"),
            client.signed_by(&key, &issuer).unwrap().pem(),
        )
        .unwrap();
        std::fs::write(dir.join("key.pem"), key.serialize_pem()).unwrap();
        std::fs::write(dir.join("command.key"), [23; 32]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join("command.key"),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        Self(dir)
    }
    pub fn tls(&self) -> Value {
        json!({"endpoint":"localhost:1", "caFile":self.0.join("ca.pem"), "certificateFile":self.0.join("cert.pem"), "keyFile":self.0.join("key.pem")})
    }
    pub fn configured(&self, address: SocketAddr) -> Value {
        let policy = json!({"version":1,"filesystem":{"includeWorkdir":false, "readOnly":["/usr","/sandbox"],"readWrite":["/sandbox/persistent","/tmp"]}, "landlock":{"compatibility":"hard_requirement"},"process":{"runAsUser":"10001","runAsGroup":"10001"}});
        let image = format!("fixture.invalid/computer@sha256:{}", "a".repeat(64));
        let template = DevelopmentTemplate::new(
            image.clone(),
            2,
            2048,
            parse_policy(&policy).unwrap(),
            PERSISTENT_COMMAND.map(str::to_owned).into(),
            Some(PersistentHome::new(512, 32).unwrap()),
        )
        .unwrap();
        let mut config = unconfigured(address);
        config["capacity"] = json!({"kind":"openshell_docker","gateway":{"transport":self.tls(),"workspace":"computers"},"allocator":self.tls(),"limits":{"perOwner":1,"perTenant":4,"provider":4},"defaultTemplate":template.fingerprint(),"templates":[{"id":"development","fingerprint":template.fingerprint(),"image":image,"cpus":2,"memoryMib":2048,"homeCapacityMib":512,"temporaryMib":32,"policy":policy}]});
        config["capacity"]["execution"] = json!({
            "policy": {"maxGrants":2,"maximumLifetimeSeconds":3600,"maximumExecutionSeconds":60,"maximumOutputBytes":65536},
            "artifactEndpoint":"http://127.0.0.1:1", "activeKeyId":Uuid::from_u128(1),
            "keys":[{"id":Uuid::from_u128(1),"file":self.0.join("command.key")}],
            "templateFingerprints":[template.fingerprint()]
        });
        config["capacity"]["maintenanceTransitions"] = json!([]);
        config
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn unconfigured(address: SocketAddr) -> Value {
    json!({"schema":"veoveo.io/computers-service/v2","listen":address,"allowedHosts":[address.to_string()],"allowedOrigins":[format!("http://{address}")],"access":{"maxGrants":4,"absoluteSeconds":28800,"idleSeconds":1800},"providerInstanceId":Uuid::from_u128(100),"capacity":{"kind":"unconfigured"}})
}
