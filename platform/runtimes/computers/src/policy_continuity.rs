//! Installation-only policy continuity for an externally quiesced replacement.
//! A stopped-state read is not a distributed fence. The installation must drain
//! controllers/provider work and hold exclusive maintenance authority throughout.
use crate::{
    Binding, DevelopmentTemplate, Observation, OpenShellRuntime, Phase, Result, RuntimeFailure,
    canonical,
    client::request,
    models::valid_fingerprint,
    protocol::{sandbox::v1 as policy, v1 as api},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Duration};
use uuid::Uuid;

const FAILURE: RuntimeFailure = RuntimeFailure::PolicyContinuity;
const PROVENANCE: &str = "veoveo.io/replacement-policy";
const DEADLINE: Duration = Duration::from_secs(75);

/// Private in-memory provider snapshot. Deliberately has no Debug or Serialize:
/// effective settings may be sensitive. A durable operation stores only its
/// fingerprint and recaptures from the retained original instance on recovery.
pub struct ReplacementPolicy {
    source: Binding,
    provider_id: String,
    config: policy::GetSandboxConfigResponse,
    fingerprint: String,
}
impl ReplacementPolicy {
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// Internal completion evidence, not a provider or operator wire model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRestoration {
    pub policy_version: u32,
    pub policy_hash: String,
}

#[derive(Eq, PartialEq)]
struct Bound {
    provider_id: String,
    process: String,
    resource_version: u64,
    policy_version: u32,
    phase: Phase,
}

fn checked(
    sandbox: api::Sandbox,
    binding: &Binding,
    workspace: &str,
    template: &DevelopmentTemplate,
) -> Result<Bound> {
    let observed =
        Observation::checked(sandbox.clone(), binding, workspace).map_err(|_| FAILURE)?;
    if template.fingerprint() != binding.template_fingerprint()
        || template.persistent_home().is_none()
        || sandbox.spec.as_ref()
            != Some(&template.spec(binding.computer_id()).map_err(|_| FAILURE)?)
        || !matches!(observed.phase, Phase::Ready | Phase::Stopped)
        || (observed.phase == Phase::Ready && observed.main_process_instance_id.is_empty())
    {
        return Err(FAILURE);
    }
    let metadata = sandbox.metadata.ok_or(FAILURE)?;
    let status = sandbox.status.ok_or(FAILURE)?;
    if metadata.resource_version == 0 || status.current_policy_version == 0 {
        return Err(FAILURE);
    }
    Ok(Bound {
        provider_id: observed.sandbox_id,
        process: observed.main_process_instance_id,
        resource_version: metadata.resource_version,
        policy_version: status.current_policy_version,
        phase: observed.phase,
    })
}

fn static_admitted(base: &policy::SandboxPolicy, value: &policy::SandboxPolicy) -> bool {
    let (Some(base_fs), Some(fs)) = (&base.filesystem, &value.filesystem) else {
        return false;
    };
    let base_ro: BTreeSet<_> = base_fs.read_only.iter().map(String::as_str).collect();
    let ro: BTreeSet<_> = fs.read_only.iter().map(String::as_str).collect();
    // Existing pinned proxy-mode enrichment; no additional writable paths.
    let proxy_ro = [
        "/usr",
        "/lib",
        "/etc",
        "/app",
        "/var/log",
        "/proc",
        "/dev/urandom",
    ];
    base.version == value.version
        && base.landlock == value.landlock
        && base.process == value.process
        && base.network_middlewares == value.network_middlewares
        && base_fs.include_workdir == fs.include_workdir
        && base_fs.read_write.iter().collect::<BTreeSet<_>>()
            == fs.read_write.iter().collect::<BTreeSet<_>>()
        && base_ro.is_subset(&ro)
        && ro.difference(&base_ro).all(|p| proxy_ro.contains(p))
}

fn admitted_config(
    config: &policy::GetSandboxConfigResponse,
    bound: &Bound,
    workspace: &str,
    template: &DevelopmentTemplate,
    computer: Uuid,
) -> Result<()> {
    let spec = template.spec(computer).map_err(|_| FAILURE)?;
    let base = spec.policy.as_ref().ok_or(FAILURE)?;
    let effective = config.policy.as_ref().ok_or(FAILURE)?;
    if config.workspace != workspace || config.policy_source != policy::PolicySource::Sandbox as i32
        || config.global_policy_version != 0 || config.version != bound.policy_version
        || !valid_fingerprint(&config.policy_hash) || !static_admitted(base, effective)
        || !template.admits_network_policy(effective)
        // Creation starts with the immutable baseline. It must never temporarily
        // restore a permission that the original instance had removed/restricted.
        || base.network_policies.iter().any(|(key, rule)| effective.network_policies.get(key) != Some(rule))
        || effective.network_policies.len() > base.network_policies.len() + 64
    {
        return Err(FAILURE);
    }
    for (name, extra) in &effective.network_policies {
        if base.network_policies.contains_key(name) {
            continue;
        }
        if extra.endpoints.is_empty()
            || extra.binaries.is_empty()
            || &extra.name != name
            || extra.endpoints.iter().any(|endpoint| {
                endpoint.host.is_empty()
                    || endpoint.host.contains('*')
                    || effective
                        .network_policies
                        .iter()
                        .any(|(other_name, other)| {
                            other_name != name
                                && other.endpoints.iter().any(|other_endpoint| {
                                    other_endpoint.host.is_empty()
                                        || other_endpoint.host.contains('*')
                                        || other_endpoint.host.eq_ignore_ascii_case(&endpoint.host)
                                })
                        })
            })
        {
            // The provider folds overlapping AddRule endpoints into other rules.
            // Never let replay of a grant change the policy's binary/host pairing.
            return Err(FAILURE);
        }
    }
    Ok(())
}

fn other_config_equal(
    left: &policy::GetSandboxConfigResponse,
    right: &policy::GetSandboxConfigResponse,
) -> bool {
    let mut normalized = right.clone();
    normalized.policy = left.policy.clone();
    normalized.version = left.version;
    normalized.policy_hash = left.policy_hash.clone();
    normalized.config_revision = left.config_revision;
    &normalized == left
}

impl OpenShellRuntime {
    async fn policy_bound(
        &self,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> Result<Bound> {
        let sandbox = self
            .client
            .clone()
            .get_sandbox(request(
                api::GetSandboxRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                },
                10,
            ))
            .await
            .map_err(|_| FAILURE)?
            .into_inner()
            .sandbox
            .ok_or(FAILURE)?;
        checked(sandbox, binding, &self.workspace, template)
    }

    async fn policy_config(&self, id: &str) -> Result<policy::GetSandboxConfigResponse> {
        self.client
            .clone()
            .get_sandbox_config(request(
                policy::GetSandboxConfigRequest {
                    sandbox_id: id.into(),
                },
                10,
            ))
            .await
            .map(|reply| reply.into_inner())
            .map_err(|_| FAILURE)
    }

    async fn policy_loaded(
        &self,
        binding: &Binding,
        version: u32,
        hash: &str,
    ) -> Result<api::SandboxPolicyRevision> {
        let status = self
            .client
            .clone()
            .get_sandbox_policy_status(request(
                api::GetSandboxPolicyStatusRequest {
                    name: binding.name(),
                    workspace: self.workspace.clone(),
                    version,
                    global: false,
                },
                10,
            ))
            .await
            .map_err(|_| FAILURE)?
            .into_inner();
        let revision = status.revision.ok_or(FAILURE)?;
        if status.active_version != version
            || revision.version != version
            || revision.policy_hash != hash
            || revision.status != api::PolicyStatus::Loaded as i32
        {
            return Err(FAILURE);
        }
        Ok(revision)
    }

    /// Capture exact already-applied additive grants without exposing their data.
    /// This does not confer authority or make a concurrent snapshot a lock.
    pub async fn capture_replacement_policy(
        &self,
        source: &Binding,
        template: &DevelopmentTemplate,
    ) -> Result<ReplacementPolicy> {
        tokio::time::timeout(DEADLINE, async {
            let before = self.policy_bound(source, template).await?;
            let config = self.policy_config(&before.provider_id).await?;
            admitted_config(
                &config,
                &before,
                &self.workspace,
                template,
                source.computer_id(),
            )?;
            self.policy_loaded(source, config.version, &config.policy_hash)
                .await?;
            if self.policy_config(&before.provider_id).await? != config
                || self.policy_bound(source, template).await? != before
            {
                return Err(FAILURE);
            }
            let mut hash = Sha256::new();
            hash.update(b"veoveo-replacement-policy-v1\0");
            hash.update(source.computer_id().as_bytes());
            hash.update(
                source
                    .replacement_instance_id()
                    .unwrap_or(Uuid::nil())
                    .as_bytes(),
            );
            hash.update(source.template_fingerprint().as_bytes());
            hash.update((before.provider_id.len() as u64).to_be_bytes());
            hash.update(before.provider_id.as_bytes());
            hash.update(canonical::encode(
                &config,
                ".openshell.sandbox.v1.GetSandboxConfigResponse",
            ));
            Ok(ReplacementPolicy {
                source: source.clone(),
                provider_id: before.provider_id,
                config,
                fingerprint: hex::encode(hash.finalize()),
            })
        })
        .await
        .map_err(|_| FAILURE)?
    }

    async fn original_stopped(
        &self,
        snapshot: &ReplacementPolicy,
        template: &DevelopmentTemplate,
    ) -> Result<()> {
        let before = self.policy_bound(&snapshot.source, template).await?;
        if before.phase != Phase::Stopped
            || before.provider_id != snapshot.provider_id
            || self.policy_config(&snapshot.provider_id).await? != snapshot.config
            || self.policy_bound(&snapshot.source, template).await? != before
        {
            return Err(FAILURE);
        }
        Ok(())
    }

    /// Replay only the captured additive grants onto a new ready instance.
    /// The installation must already hold an external quiescence fence; this
    /// method does not stop/start/create either instance or transfer domain state.
    /// An unconfirmed result requires reconciliation of the same candidate.
    pub async fn restore_replacement_policy(
        &self,
        snapshot: &ReplacementPolicy,
        target: &Binding,
        template: &DevelopmentTemplate,
        operation: Uuid,
    ) -> Result<PolicyRestoration> {
        tokio::time::timeout(
            DEADLINE,
            self.restore_policy(snapshot, target, template, operation),
        )
        .await
        .map_err(|_| FAILURE)?
    }

    async fn restore_policy(
        &self,
        snapshot: &ReplacementPolicy,
        target: &Binding,
        template: &DevelopmentTemplate,
        operation: Uuid,
    ) -> Result<PolicyRestoration> {
        if operation.is_nil()
            || target.replacement_instance_id().is_none()
            || target == &snapshot.source
            || target.computer_id() != snapshot.source.computer_id()
            || target.template_fingerprint() != snapshot.source.template_fingerprint()
        {
            return Err(FAILURE);
        }
        self.original_stopped(snapshot, template).await?;
        let bound = self.policy_bound(target, template).await?;
        if bound.phase != Phase::Ready || bound.provider_id == snapshot.provider_id {
            return Err(FAILURE);
        }
        let before = self.policy_config(&bound.provider_id).await?;
        admitted_config(
            &before,
            &bound,
            &self.workspace,
            template,
            target.computer_id(),
        )?;
        if !other_config_equal(&snapshot.config, &before) {
            return Err(FAILURE);
        }
        self.policy_loaded(target, before.version, &before.policy_hash)
            .await?;
        let spec = template.spec(target.computer_id()).map_err(|_| FAILURE)?;
        let base = spec.policy.as_ref().ok_or(FAILURE)?;
        let original = snapshot.config.policy.as_ref().ok_or(FAILURE)?;
        let mut expected = before.policy.as_ref().ok_or(FAILURE)?.clone();
        let changed = expected.network_policies != original.network_policies;
        if changed && expected.network_policies != base.network_policies {
            return Err(FAILURE);
        }
        expected
            .network_policies
            .clone_from(&original.network_policies);
        let (version, hash) = if changed {
            let mut watch = self
                .client
                .clone()
                .watch_sandbox(request(
                    api::WatchSandboxRequest {
                        id: bound.provider_id.clone(),
                        follow_status: true,
                        stop_on_terminal: false,
                        ..Default::default()
                    },
                    65,
                ))
                .await
                .map_err(|_| FAILURE)?
                .into_inner();
            let initial = watch.message().await.map_err(|_| FAILURE)?.ok_or(FAILURE)?;
            let Some(api::sandbox_stream_event::Payload::Sandbox(initial)) = initial.payload else {
                return Err(FAILURE);
            };
            if checked(initial, target, &self.workspace, template)? != bound {
                return Err(FAILURE);
            }
            let operations = original
                .network_policies
                .iter()
                .filter(|(name, _)| !base.network_policies.contains_key(*name))
                .map(|(name, rule)| api::PolicyMergeOperation {
                    operation: Some(api::policy_merge_operation::Operation::AddRule(
                        api::AddNetworkRule {
                            rule_name: name.clone(),
                            rule: Some(rule.clone()),
                        },
                    )),
                })
                .collect();
            let update = self
                .client
                .clone()
                .update_config(request(
                    api::UpdateConfigRequest {
                        name: target.name(),
                        workspace: self.workspace.clone(),
                        expected_resource_version: bound.resource_version,
                        annotations: [(PROVENANCE.into(), operation.to_string())].into(),
                        merge_operations: operations,
                        ..Default::default()
                    },
                    15,
                ))
                .await
                .map_err(|_| FAILURE)?
                .into_inner();
            if update.version <= before.version
                || !valid_fingerprint(&update.policy_hash)
                || update.annotations.get(PROVENANCE) != Some(&operation.to_string())
            {
                return Err(FAILURE);
            }
            let mut confirmed = false;
            for _ in 0..128 {
                let event = watch.message().await.map_err(|_| FAILURE)?.ok_or(FAILURE)?;
                match event.payload {
                    Some(api::sandbox_stream_event::Payload::Sandbox(sandbox)) => {
                        let seen = checked(sandbox, target, &self.workspace, template)?;
                        if seen.provider_id != bound.provider_id
                            || seen.process != bound.process
                            || seen.phase != Phase::Ready
                            || seen.policy_version > update.version
                        {
                            return Err(FAILURE);
                        }
                        if seen.policy_version == update.version {
                            confirmed = true;
                            break;
                        }
                    }
                    Some(api::sandbox_stream_event::Payload::Warning(_)) | None => {
                        return Err(FAILURE);
                    }
                    _ => {}
                }
            }
            if !confirmed {
                return Err(FAILURE);
            }
            (update.version, update.policy_hash)
        } else {
            (before.version, before.policy_hash.clone())
        };
        let revision = self.policy_loaded(target, version, &hash).await?;
        if changed && revision.provenance.get(PROVENANCE) != Some(&operation.to_string()) {
            return Err(FAILURE);
        }
        let after = self.policy_config(&bound.provider_id).await?;
        if after.version != version
            || after.policy_hash != hash
            || after.policy.as_ref() != Some(&expected)
            || !other_config_equal(&before, &after)
        {
            return Err(FAILURE);
        }
        let current = self.policy_bound(target, template).await?;
        if current.provider_id != bound.provider_id
            || current.process != bound.process
            || current.phase != Phase::Ready
            || current.policy_version != version
        {
            return Err(FAILURE);
        }
        self.original_stopped(snapshot, template).await?;
        Ok(PolicyRestoration {
            policy_version: version,
            policy_hash: hash,
        })
    }
}
