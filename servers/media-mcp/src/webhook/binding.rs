//! Private callback capabilities bind an authenticated provider body to one dispatch.
use hmac::{KeyInit, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use veoveo_mcp_contract::ArtifactWriteCapabilitySecret;
use veoveo_types::{ExtensionName, Sha256Digest, TaskId, TenantId};

type HmacSha256 = hmac::Hmac<Sha256>;

/// The full capability is used only in the submission URL and never formatted in diagnostics.
pub struct CallbackBinding(String);
impl std::fmt::Debug for CallbackBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallbackBinding([REDACTED])")
    }
}
impl CallbackBinding {
    pub fn derive(
        secret: &ArtifactWriteCapabilitySecret,
        task: TaskId,
        tenant: &TenantId,
        provider: &ExtensionName,
    ) -> Self {
        let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(b"veoveo.ai/media-callback-binding/v1\0");
        // Each Task admits one dispatch. Length prefixes keep each identity unambiguous.
        for field in [task.to_string(), tenant.to_string(), provider.to_string()] {
            mac.update(&(field.len() as u64).to_be_bytes());
            mac.update(field.as_bytes());
        }
        Self(hex::encode(mac.finalize().into_bytes()))
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
    pub fn digest(&self) -> Sha256Digest {
        digest(&self.0)
    }
    /// Check only the persisted digest; the expired private context is not needed.
    pub fn verify(persisted: &Sha256Digest, supplied: &str) -> bool {
        if supplied.len() != 64
            || !supplied
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return false;
        }
        bool::from(
            persisted
                .as_str()
                .as_bytes()
                .ct_eq(digest(supplied).as_str().as_bytes()),
        )
    }
}
fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(value.as_bytes()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_dispatch_binding_is_task_tenant_provider_scoped_and_context_independent() {
        let secret = ArtifactWriteCapabilitySecret::new("s".repeat(32)).unwrap();
        let task = TaskId::new();
        let tenant = TenantId::parse("fixture").unwrap();
        let provider = ExtensionName::parse("media").unwrap();
        let token = CallbackBinding::derive(&secret, task, &tenant, &provider);
        let saved = token.digest();
        assert!(CallbackBinding::verify(&saved, token.expose_secret()));
        assert!(!CallbackBinding::verify(
            &saved,
            CallbackBinding::derive(&secret, TaskId::new(), &tenant, &provider).expose_secret()
        ));
        assert!(!CallbackBinding::verify(
            &saved,
            CallbackBinding::derive(&secret, task, &TenantId::parse("other").unwrap(), &provider)
                .expose_secret()
        ));
        assert!(!CallbackBinding::verify(
            &saved,
            CallbackBinding::derive(
                &secret,
                task,
                &tenant,
                &ExtensionName::parse("other").unwrap()
            )
            .expose_secret()
        ));
        assert_eq!(format!("{token:?}"), "CallbackBinding([REDACTED])");
        assert!(!CallbackBinding::verify(&saved, ""));
    }
}
