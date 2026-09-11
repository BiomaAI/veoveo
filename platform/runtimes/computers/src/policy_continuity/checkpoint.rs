//! Closed private checkpoint encoding; never plaintext persistence or authority.
use super::{Bound, FAILURE, ReplacementPolicy, admitted_config};
use crate::{
    Binding, DevelopmentTemplate, GATEWAY_VERSION, Observation, OpenShellRuntime, Phase, Result,
    maintenance_protocol::PolicyCheckpoint, models::identifier,
};
use prost::Message;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

// The provider config is already bounded by the 1 MiB gRPC decoding limit.
// Leave bounded space for the owning envelope's fixed identity fields.
pub(super) const MAX_CHECKPOINT_BYTES: usize = 1024 * 1024 + 4096;

impl ReplacementPolicy {
    /// Sensitive plaintext for immediate encryption by the owning journal.
    /// Never store these bytes in Tasks, logs, artifacts or public resources.
    /// Zeroizing protects the returned buffer, not every provider-owned string.
    pub fn checkpoint(&self) -> Result<Zeroizing<Vec<u8>>> {
        let message = PolicyCheckpoint {
            version: 1,
            gateway_version: GATEWAY_VERSION.into(),
            provider_instance_id: self.installation_provider_id.as_bytes().to_vec(),
            computer_id: self.source.computer_id().as_bytes().to_vec(),
            source_instance_id: self
                .source
                .replacement_instance_id()
                .unwrap_or(self.source.computer_id())
                .as_bytes()
                .to_vec(),
            template_fingerprint: self.source.template_fingerprint().into(),
            resource_id: self.provider_id.clone(),
            process_id: self.process_id.clone(),
            config: Some(self.config.clone()),
        };
        if message.encoded_len() > MAX_CHECKPOINT_BYTES {
            return Err(FAILURE);
        }
        Ok(Zeroizing::new(message.encode_to_vec()))
    }

    pub(super) fn update_fingerprint(&mut self) -> Result<()> {
        let mut hash = Sha256::new();
        hash.update(b"veoveo-private-policy-checkpoint-v1\0");
        hash.update(&*self.checkpoint()?);
        self.fingerprint = hex::encode(hash.finalize());
        Ok(())
    }
}

impl OpenShellRuntime {
    /// Decode bytes authenticated by the maintenance journal's encryption key.
    /// `source` and `expected` must come from that operation's immutable binding.
    /// No provider read is needed: the source may already have been retired.
    /// Decoding confers neither writer-fence proof nor permission to restore.
    pub fn recover_replacement_policy(
        &self,
        bytes: &[u8],
        source: &Binding,
        template: &DevelopmentTemplate,
        expected: &Observation,
    ) -> Result<ReplacementPolicy> {
        if bytes.is_empty() || bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(FAILURE);
        }
        let message = PolicyCheckpoint::decode(bytes).map_err(|_| FAILURE)?;
        // Persist one encoding: reject unknown fields, duplicate map/message
        // fields, alternate order, or other silently normalized input.
        if Zeroizing::new(message.encode_to_vec()).as_slice() != bytes
            || message.version != 1
            || message.gateway_version != GATEWAY_VERSION
            || Uuid::from_slice(&message.provider_instance_id).ok()
                != Some(self.provider_instance_id)
            || Uuid::from_slice(&message.computer_id).ok() != Some(source.computer_id())
            || Uuid::from_slice(&message.source_instance_id).ok()
                != Some(
                    source
                        .replacement_instance_id()
                        .unwrap_or(source.computer_id()),
                )
            || message.template_fingerprint != source.template_fingerprint()
            || template.fingerprint() != source.template_fingerprint()
            || template.persistent_home().is_none()
            || message.resource_id != expected.sandbox_id
            || message.process_id != expected.main_process_instance_id
            || !identifier(&message.resource_id)
            || !identifier(&message.process_id)
            || !matches!(expected.phase, Phase::Ready | Phase::Stopped)
        {
            return Err(FAILURE);
        }
        let config = message.config.ok_or(FAILURE)?;
        if config.version == 0 || config.encoded_len() > 1024 * 1024 {
            return Err(FAILURE);
        }
        let bound = Bound {
            provider_id: message.resource_id.clone(),
            process: message.process_id.clone(),
            resource_version: 0, // Private replay validates the captured config, not a live CAS.
            policy_version: config.version,
            phase: expected.phase,
        };
        admitted_config(
            &config,
            &bound,
            &self.workspace,
            template,
            source.computer_id(),
        )?;
        let mut snapshot = ReplacementPolicy {
            source: source.clone(),
            installation_provider_id: self.provider_instance_id,
            provider_id: message.resource_id,
            process_id: message.process_id,
            config,
            fingerprint: String::new(),
        };
        snapshot.update_fingerprint()?;
        Ok(snapshot)
    }
}
