//! Per-instance credentials. Private material stays in owned Kubernetes Secrets.
use anyhow::{Context, Result, ensure};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use rsa::{
    RsaPrivateKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
    traits::PublicKeyParts,
};
use std::collections::BTreeMap;
use veoveo_platform_store::{
    PlatformStore,
    agent_management::instances::{ManagedAgentClaim, ManagedAgentInstance, ManagedAgentPublicKey},
};

use crate::{
    kubernetes::{Kubernetes, Resource, owned},
    kubernetes_types::Secret,
    resources::metadata,
};

const PRIVATE_KEY: &str = "private-key-der-b64";
const KEY_ID: &str = "kid";

pub async fn ensure_credentials(
    kube: &Kubernetes,
    store: &PlatformStore,
    claim: &ManagedAgentClaim,
    instance: &ManagedAgentInstance,
) -> Result<()> {
    let secret = if let Some(secret) = kube
        .get::<Secret>(Resource::Secrets, &instance.resources.credential_secret)
        .await?
    {
        secret
    } else {
        ensure!(
            instance.public_key.is_none(),
            "registered private credential is missing; operator recovery is required"
        );
        let data = tokio::task::spawn_blocking(|| -> Result<BTreeMap<String, String>> {
            let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)?;
            let der = private.to_pkcs8_der()?;
            Ok(BTreeMap::from([
                (
                    PRIVATE_KEY.into(),
                    STANDARD.encode(STANDARD.encode(der.as_bytes())),
                ),
                (KEY_ID.into(), STANDARD.encode("managed-key")),
            ]))
        })
        .await??;
        store.renew_managed_agent_claim(claim).await?;
        let secret = Secret {
            api_version: "v1".into(),
            kind: "Secret".into(),
            metadata: metadata(instance, &instance.resources.credential_secret),
            immutable: true,
            secret_type: "Opaque".into(),
            data,
        };
        // Conflict or an uncertain response is recovered by reading this same
        // name next time. Never replace an existing key with the generated one.
        kube.create::<_, Secret>(Resource::Secrets, &secret).await?
    };
    owned(&secret.metadata, &instance.resources.workload)?;
    ensure!(
        secret.immutable && secret.secret_type == "Opaque",
        "credential Secret must be immutable and Opaque"
    );
    let public = public_key(&secret)?;
    store.register_managed_agent_key(claim, public).await?;
    Ok(())
}

pub fn public_key(secret: &Secret) -> Result<ManagedAgentPublicKey> {
    let encoded = STANDARD.decode(
        secret
            .data
            .get(PRIVATE_KEY)
            .context("credential key is absent")?,
    )?;
    let der = STANDARD.decode(encoded)?;
    let private =
        RsaPrivateKey::from_pkcs8_der(&der).context("invalid private credential encoding")?;
    ensure!(
        private.n().bits() >= 2048,
        "private credential is too small"
    );
    let kid = String::from_utf8(
        STANDARD.decode(
            secret
                .data
                .get(KEY_ID)
                .context("credential key id is absent")?,
        )?,
    )?;
    Ok(ManagedAgentPublicKey {
        kid,
        n: URL_SAFE_NO_PAD.encode(private.n().to_bytes_be()),
        e: URL_SAFE_NO_PAD.encode(private.e().to_bytes_be()),
    })
}
