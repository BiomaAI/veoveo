use super::*;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

type Reply<T> = std::result::Result<Response<T>, Status>;
const PROVENANCE: &str = "veoveo.ai/replacement-policy";
const SECRET: &str = "PRIVATE-FIXTURE-SETTING";

fn rule(name: &str, host: &str) -> policy::NetworkPolicyRule {
    policy::NetworkPolicyRule {
        name: name.into(),
        endpoints: vec![policy::NetworkEndpoint {
            host: host.into(),
            port: 443,
            ports: vec![443],
            protocol: "tcp".into(),
            tls: policy::NetworkTlsMode::Skip as i32,
            ..Default::default()
        }],
        binaries: vec![policy::NetworkBinary {
            path: "/opt/fixture/bin/tool".into(),
        }],
    }
}
fn profile() -> DevelopmentTemplate {
    let mut spec = template(true).spec(binding().computer_id()).unwrap();
    let mut policy = spec.policy.take().unwrap();
    policy
        .network_policies
        .insert("packages".into(), rule("packages", "packages.example.com"));
    DevelopmentTemplate::new(
        spec.template.unwrap().image,
        16,
        65536,
        policy,
        PERSISTENT_COMMAND.iter().map(|s| s.to_string()).collect(),
        Some(PersistentHome::new(32768, 1024).unwrap()),
    )
    .unwrap()
}
fn bindings() -> (Binding, Binding) {
    let source = Binding::new(binding().computer_id(), profile().fingerprint()).unwrap();
    let target = Binding::replacement(
        source.computer_id(),
        Uuid::from_u128(10),
        profile().fingerprint(),
    )
    .unwrap();
    (source, target)
}
fn sandbox_for(binding: &Binding, id: &str, version: u32) -> api::Sandbox {
    let mut value = sandbox(Phase::Ready);
    let meta = value.metadata.as_mut().unwrap();
    meta.id = id.into();
    meta.name = binding.name();
    meta.labels = binding.labels();
    meta.resource_version = 6;
    value.spec = Some(profile().bound_spec(binding).unwrap());
    value.spec.as_mut().unwrap().provider_attachment_epoch = if id == "sandbox-old" {
        "00000000-0000-4000-8000-000000000011"
    } else {
        "00000000-0000-4000-8000-000000000012"
    }
    .into();
    let status = value.status.as_mut().unwrap();
    status.configuration_admission = Some(api::SandboxConfigurationAdmission {
        instance_id: if id == "sandbox-old" {
            "00000000-0000-4000-8000-000000000021"
        } else {
            "00000000-0000-4000-8000-000000000022"
        }
        .into(),
        state: api::ConfigurationAdmissionState::Accepted as i32,
        ..Default::default()
    });
    status.current_policy_version = version;
    status.main_process_instance_id = format!("main-{id}");
    value
}
fn config_for(sandbox: &api::Sandbox) -> policy::GetSandboxConfigResponse {
    policy::GetSandboxConfigResponse {
        configuration_instance_id: sandbox
            .status
            .as_ref()
            .unwrap()
            .configuration_admission
            .as_ref()
            .unwrap()
            .instance_id
            .clone(),
        configuration_admitted: true,
        policy: sandbox.spec.as_ref().unwrap().policy.clone(),
        provider_attachment_epoch: sandbox
            .spec
            .as_ref()
            .unwrap()
            .provider_attachment_epoch
            .clone(),
        version: sandbox.status.as_ref().unwrap().current_policy_version,
        policy_hash: "a".repeat(64),
        config_revision: 10,
        workspace: "computers".into(),
        policy_source: policy::PolicySource::Sandbox as i32,
        policy_validation_failure_mode: "fail_closed".into(),
        provider_env_revision: 100,
        settings: [(
            "fixture.setting".into(),
            policy::EffectiveSetting {
                value: Some(policy::SettingValue {
                    value: Some(policy::setting_value::Value::StringValue(SECRET.into())),
                }),
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    LostReply,
    UncertainPending,
    Warning,
    ChangedProcess,
    ChangedEpoch,
    ChangedConfiguration,
    MissingConfigurationConfig,
    MalformedConfigurationConfig,
    ChangedConfigurationConfig,
    MissingConfigurationWatch,
    MalformedConfigurationWatch,
    ChangedConfigurationWatch,
    MissingConfigurationFinal,
    MalformedConfigurationFinal,
    ChangedConfigurationFinal,
    MissingConfigEpoch,
    MalformedConfigEpoch,
    ChangedConfigEpoch,
    MissingWatchEpoch,
    MalformedWatchEpoch,
    ChangedWatchEpoch,
    MissingFinalEpoch,
    MalformedFinalEpoch,
    ChangedFinalEpoch,
    ChangedSettings,
    Unloaded,
    ChangedRules,
}

pub(super) struct Fixture {
    source: api::Sandbox,
    target: api::Sandbox,
    old_config: policy::GetSandboxConfigResponse,
    new_config: policy::GetSandboxConfigResponse,
    annotations: BTreeMap<String, String>,
    sender: Option<mpsc::Sender<std::result::Result<api::SandboxStreamEvent, Status>>>,
    fault: Fault,
    updates: usize,
    watches: usize,
    calls: Vec<&'static str>,
    retired: bool,
}
impl Fixture {
    fn new() -> Self {
        let (old, new) = bindings();
        let mut source = sandbox_for(&old, "sandbox-old", 3);
        source.status.as_mut().unwrap().phase = Phase::Stopped as i32;
        let target = sandbox_for(&new, "sandbox-new", 1);
        let mut old_config = config_for(&source);
        old_config
            .policy
            .as_mut()
            .unwrap()
            .network_policies
            .insert("next-build".into(), rule("next-build", "next.example.com"));
        let new_config = config_for(&target);
        Self {
            source,
            target,
            old_config,
            new_config,
            annotations: BTreeMap::new(),
            sender: None,
            fault: Fault::None,
            updates: 0,
            watches: 0,
            calls: Vec::new(),
            retired: false,
        }
    }
    fn by_name(&self, name: &str) -> &api::Sandbox {
        if name == self.source.metadata.as_ref().unwrap().name {
            &self.source
        } else {
            assert_eq!(name, self.target.metadata.as_ref().unwrap().name);
            &self.target
        }
    }
    pub(super) fn get(&mut self, request: api::GetSandboxRequest) -> Reply<api::SandboxResponse> {
        self.calls.push("get");
        assert!(request.workspace_scope == crate::client::workspace_scope("computers"));
        if request.name == self.source.metadata.as_ref().unwrap().name && self.retired {
            panic!("restoration must not read the retired source");
        }
        let sandbox = self.by_name(&request.name).clone();
        if self.fault == Fault::ChangedEpoch {
            self.source.spec.as_mut().unwrap().provider_attachment_epoch =
                "00000000-0000-4000-8000-000000000013".into();
        }
        if self.fault == Fault::ChangedConfiguration {
            self.source
                .status
                .as_mut()
                .unwrap()
                .configuration_admission
                .as_mut()
                .unwrap()
                .instance_id = "00000000-0000-4000-8000-000000000023".into();
        }
        Ok(Response::new(api::SandboxResponse {
            sandbox: Some(sandbox),
            ..Default::default()
        }))
    }
    pub(super) fn config(
        &mut self,
        request: policy::GetSandboxConfigRequest,
    ) -> Reply<policy::GetSandboxConfigResponse> {
        self.calls.push("config");
        let config = if request.name == self.source.metadata.as_ref().unwrap().name {
            assert!(!self.retired, "restoration must not read retired settings");
            &self.old_config
        } else {
            assert_eq!(request.name, self.target.metadata.as_ref().unwrap().name);
            &self.new_config
        };
        Ok(Response::new(config.clone()))
    }
    pub(super) fn loaded(
        &mut self,
        request: api::GetSandboxPolicyStatusRequest,
    ) -> Reply<api::GetSandboxPolicyStatusResponse> {
        self.calls.push("loaded");
        assert!(request.workspace_scope == crate::client::workspace_scope("computers"));
        assert!(!request.global);
        let old = self.by_name(&request.sandbox).metadata.as_ref().unwrap().id == "sandbox-old";
        assert!(
            !old || !self.retired,
            "restoration must not query retired policy status"
        );
        let config = if old {
            &self.old_config
        } else {
            &self.new_config
        };
        assert_eq!(request.version, config.version);
        Ok(Response::new(api::GetSandboxPolicyStatusResponse {
            active_version: if !old && self.fault == Fault::Unloaded && self.updates > 0 {
                1
            } else {
                config.version
            },
            revision: Some(api::SandboxPolicyRevision {
                version: config.version,
                policy_hash: config.policy_hash.clone(),
                status: api::PolicyStatus::Loaded as i32,
                provenance: self.annotations.clone(),
                ..Default::default()
            }),
        }))
    }
    pub(super) fn watch(
        &mut self,
        request: api::WatchSandboxRequest,
    ) -> Reply<BoxStream<api::SandboxStreamEvent>> {
        self.calls.push("watch");
        self.watches += 1;
        assert_eq!(request.sandbox, self.target.metadata.as_ref().unwrap().name);
        assert!(request.follow_status);
        assert!(!request.follow_logs);
        assert!(!request.stop_on_terminal);
        let (sender, receiver) = mpsc::channel(8);
        sender
            .try_send(Ok(api::SandboxStreamEvent {
                payload: Some(api::sandbox_stream_event::Payload::Sandbox(
                    self.target.clone(),
                )),
                ..Default::default()
            }))
            .unwrap();
        self.sender = Some(sender);
        Ok(Response::new(Box::pin(stream::unfold(
            receiver,
            |mut receiver| async { receiver.recv().await.map(|event| (event, receiver)) },
        ))))
    }
    pub(super) fn update(
        &mut self,
        request: api::UpdateConfigRequest,
    ) -> Reply<api::UpdateConfigResponse> {
        self.calls.push("update");
        self.updates += 1;
        assert!(self.sender.is_some(), "subscribe before mutation");
        assert_eq!(request.sandbox, self.target.metadata.as_ref().unwrap().name);
        assert!(request.workspace_scope == crate::client::workspace_scope("computers"));
        assert_eq!(
            request.expected_resource_version,
            self.target.metadata.as_ref().unwrap().resource_version
        );
        assert!(!request.global);
        assert!(!request.delete_setting);
        assert!(request.setting_key.is_empty());
        assert!(request.setting_value.is_none());
        assert!(request.policy.is_none());
        assert_eq!(request.merge_operations.len(), 1);
        if self.fault == Fault::UncertainPending {
            return Err(Status::unavailable(SECRET));
        }
        for operation in request.merge_operations {
            let api::policy_merge_operation::Operation::AddRule(rule) =
                operation.operation.unwrap()
            else {
                panic!("additive grant only");
            };
            assert_eq!(rule.rule_name, "next-build");
            self.new_config
                .policy
                .as_mut()
                .unwrap()
                .network_policies
                .insert(rule.rule_name, rule.rule.unwrap());
        }
        self.annotations = request.annotations;
        assert_eq!(self.annotations.len(), 1);
        assert!(Uuid::parse_str(&self.annotations[PROVENANCE]).is_ok());
        self.new_config.version += 1;
        self.new_config.policy_hash = "b".repeat(64);
        self.new_config.config_revision += 1;
        self.target.metadata.as_mut().unwrap().resource_version += 1;
        self.target.status.as_mut().unwrap().current_policy_version = self.new_config.version;
        match self.fault {
            Fault::MissingConfigurationConfig => self.new_config.configuration_instance_id.clear(),
            Fault::MalformedConfigurationConfig => {
                self.new_config.configuration_instance_id = "malformed".into()
            }
            Fault::ChangedConfigurationConfig => {
                self.new_config.configuration_instance_id =
                    "00000000-0000-4000-8000-000000000023".into()
            }
            Fault::MissingConfigEpoch => self.new_config.provider_attachment_epoch.clear(),
            Fault::MalformedConfigEpoch => {
                self.new_config.provider_attachment_epoch = "malformed".into()
            }
            Fault::ChangedConfigEpoch => {
                self.new_config.provider_attachment_epoch =
                    "00000000-0000-4000-8000-000000000013".into()
            }
            Fault::ChangedSettings => {
                self.new_config.settings.clear();
            }
            Fault::ChangedRules => {
                self.new_config
                    .policy
                    .as_mut()
                    .unwrap()
                    .network_policies
                    .clear();
            }
            _ => {}
        }
        let mut sandbox = self.target.clone();
        if self.fault == Fault::ChangedProcess {
            sandbox.status.as_mut().unwrap().main_process_instance_id = "wrong-process".into();
        }
        match self.fault {
            Fault::MissingWatchEpoch => sandbox
                .spec
                .as_mut()
                .unwrap()
                .provider_attachment_epoch
                .clear(),
            Fault::MalformedWatchEpoch => {
                sandbox.spec.as_mut().unwrap().provider_attachment_epoch = "malformed".into()
            }
            Fault::ChangedWatchEpoch => {
                sandbox.spec.as_mut().unwrap().provider_attachment_epoch =
                    "00000000-0000-4000-8000-000000000013".into()
            }
            Fault::MissingFinalEpoch => self
                .target
                .spec
                .as_mut()
                .unwrap()
                .provider_attachment_epoch
                .clear(),
            Fault::MalformedFinalEpoch => {
                self.target.spec.as_mut().unwrap().provider_attachment_epoch = "malformed".into()
            }
            Fault::ChangedFinalEpoch => {
                self.target.spec.as_mut().unwrap().provider_attachment_epoch =
                    "00000000-0000-4000-8000-000000000013".into()
            }
            _ => {}
        }
        let configuration_fault = match self.fault {
            Fault::MissingConfigurationWatch | Fault::MissingConfigurationFinal => Some(""),
            Fault::MalformedConfigurationWatch | Fault::MalformedConfigurationFinal => {
                Some("malformed")
            }
            Fault::ChangedConfigurationWatch | Fault::ChangedConfigurationFinal => {
                Some("00000000-0000-4000-8000-000000000023")
            }
            _ => None,
        };
        if let Some(identity) = configuration_fault {
            let value = if matches!(
                self.fault,
                Fault::MissingConfigurationWatch
                    | Fault::MalformedConfigurationWatch
                    | Fault::ChangedConfigurationWatch
            ) {
                &mut sandbox
            } else {
                &mut self.target
            };
            value
                .status
                .as_mut()
                .unwrap()
                .configuration_admission
                .as_mut()
                .unwrap()
                .instance_id = identity.into();
        }
        let payload = if self.fault == Fault::Warning {
            api::sandbox_stream_event::Payload::Warning(api::SandboxStreamWarning {
                message: SECRET.into(),
            })
        } else {
            api::sandbox_stream_event::Payload::Sandbox(sandbox)
        };
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .try_send(Ok(api::SandboxStreamEvent {
                payload: Some(payload),
                ..Default::default()
            }));
        if self.fault == Fault::LostReply {
            return Err(Status::unavailable(SECRET));
        }
        Ok(Response::new(api::UpdateConfigResponse {
            version: self.new_config.version,
            policy_hash: self.new_config.policy_hash.clone(),
            annotations: self.annotations.clone(),
            ..Default::default()
        }))
    }
}
fn edit(running: &Running, change: impl FnOnce(&mut Fixture)) {
    change(
        running
            .fake
            .0
            .lock()
            .unwrap()
            .policy_fixture
            .as_mut()
            .unwrap(),
    );
}
async fn fixture() -> Running {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().policy_fixture = Some(Fixture::new());
    running
}
async fn captured(running: &Running) -> ReplacementPolicy {
    let snapshot = running
        .runtime
        .capture_replacement_policy(&bindings().0, &profile())
        .await
        .unwrap();
    edit(running, |f| {
        f.source.status.as_mut().unwrap().phase = Phase::Stopped as i32
    });
    snapshot
}
async fn restore(running: &Running, snapshot: &ReplacementPolicy) -> Result<PolicyRestoration> {
    edit(running, |f| f.retired = true);
    let handoff = fixture_handoff(running, bindings().1, Uuid::from_u128(100));
    running
        .runtime
        .restore_replacement_policy(snapshot, &handoff, &profile(), &profile())
        .await
}

async fn reconcile(running: &Running, snapshot: &ReplacementPolicy) -> Result<PolicyRestoration> {
    edit(running, |f| f.retired = true);
    running
        .runtime
        .reconcile_replacement_policy(
            snapshot,
            &fixture_handoff(running, bindings().1, Uuid::from_u128(100)),
            &profile(),
            &profile(),
            Duration::from_secs(10),
        )
        .await
}

fn fixture_handoff(running: &Running, target: Binding, operation: Uuid) -> RetainedHandoff {
    RetainedHandoff::fixture(
        running.runtime.provider_instance_id(),
        veoveo_types::TaskId::from_uuid(operation),
        bindings().0,
        target,
        "sandbox-old".into(),
    )
}

fn replacement_profile(change: u8) -> DevelopmentTemplate {
    let spec = profile().spec(bindings().0.computer_id()).unwrap();
    let mut policy = spec.policy.unwrap();
    if change == 4 {
        policy
            .filesystem
            .as_mut()
            .unwrap()
            .read_only
            .push("/opt/another".into());
    }
    DevelopmentTemplate::new(
        format!("registry.example/next@sha256:{}", "b".repeat(64)),
        if change == 1 { 8 } else { 16 },
        if change == 2 { 32768 } else { 65536 },
        policy,
        if change == 5 {
            PERSISTENT_BUILD_COMMAND
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            spec.command
        },
        Some(
            PersistentHome::new(
                if change == 3 { 16384 } else { 32768 },
                if change == 6 { 512 } else { 1024 },
            )
            .unwrap(),
        ),
    )
    .unwrap()
}

#[test]
fn retained_image_transition_preflight_rejects_resource_policy_command_or_storage_changes() {
    let source = profile();
    let target = replacement_profile(0);
    assert_ne!(source.fingerprint(), target.fingerprint());
    source.check_replacement_profile(&target).unwrap();
    target.check_replacement_profile(&source).unwrap();
    for change in 1..=6 {
        assert!(
            source
                .check_replacement_profile(&replacement_profile(change))
                .is_err()
        );
    }
    assert!(
        template(false)
            .check_replacement_profile(&template(false))
            .is_err()
    );
}

#[tokio::test]
async fn retired_source_policy_restores_onto_an_image_change_only_with_exact_handoff() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    let target_profile = replacement_profile(0);
    let target = Binding::replacement(
        bindings().0.computer_id(),
        Uuid::from_u128(10),
        target_profile.fingerprint(),
    )
    .unwrap();
    edit(&running, |f| {
        f.retired = true;
        f.target.metadata.as_mut().unwrap().labels = target.labels();
        let epoch = f
            .target
            .spec
            .as_ref()
            .unwrap()
            .provider_attachment_epoch
            .clone();
        f.target.spec = Some(target_profile.bound_spec(&target).unwrap());
        f.target.spec.as_mut().unwrap().provider_attachment_epoch = epoch;
        f.calls.clear();
    });
    for fault in 0..3 {
        let receipt = RetainedHandoff::fixture(
            if fault == 0 {
                crate::ProviderInstanceId::new()
            } else {
                running.runtime.provider_instance_id()
            },
            veoveo_types::TaskId::new(),
            if fault == 1 {
                target.clone()
            } else {
                bindings().0
            },
            target.clone(),
            if fault == 2 {
                "different-source".into()
            } else {
                "sandbox-old".into()
            },
        );
        assert!(
            running
                .runtime
                .restore_replacement_policy(&snapshot, &receipt, &profile(), &target_profile)
                .await
                .is_err()
        );
    }
    edit(&running, |f| assert!(f.calls.is_empty()));
    let receipt = fixture_handoff(&running, target, Uuid::now_v7());
    assert_eq!(
        running
            .runtime
            .restore_replacement_policy(&snapshot, &receipt, &profile(), &target_profile)
            .await
            .unwrap()
            .policy_version,
        2
    );
    edit(&running, |f| {
        assert!(f.new_config.policy == f.old_config.policy);
        assert!(f.new_config.settings == f.old_config.settings);
    });
}

#[tokio::test]
async fn additive_policy_restores_once_without_spec_settings_home_or_lifecycle_changes() {
    let running = fixture().await;
    let original_specs = {
        let state = running.fake.0.lock().unwrap();
        let fixture = state.policy_fixture.as_ref().unwrap();
        (fixture.source.spec.clone(), fixture.target.spec.clone())
    };
    let snapshot = captured(&running).await;
    assert_eq!(snapshot.fingerprint().len(), 64);
    let again = running
        .runtime
        .capture_replacement_policy(&bindings().0, &profile())
        .await
        .unwrap();
    assert_eq!(snapshot.fingerprint(), again.fingerprint());
    let result = restore(&running, &snapshot).await.unwrap();
    assert_eq!(result.policy_version, 2);
    assert_eq!(result, reconcile(&running, &snapshot).await.unwrap());
    edit(&running, |f| {
        assert_eq!((f.updates, f.watches), (1, 1));
        assert!(f.old_config.policy == f.new_config.policy);
        assert!(f.old_config.settings == f.new_config.settings);
        assert!(f.source.spec == original_specs.0);
        assert!(f.target.spec == original_specs.1);
    });
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}

#[tokio::test]
async fn already_matching_policy_is_read_only_and_needs_no_watch() {
    let running = fixture().await;
    edit(&running, |f| {
        f.old_config.policy = f.new_config.policy.clone()
    });
    let snapshot = captured(&running).await;
    assert_eq!(
        restore(&running, &snapshot).await.unwrap().policy_version,
        1
    );
    edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
}

#[tokio::test]
async fn private_checkpoint_recovers_without_source_reads_and_preserves_exact_policy() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    let fingerprint = snapshot.fingerprint().to_owned();
    let bytes = snapshot.checkpoint().unwrap();
    drop(snapshot);
    let expected = running.runtime.get(&bindings().0).await.unwrap().unwrap();
    edit(&running, |f| f.calls.clear());
    let snapshot = running
        .runtime
        .recover_replacement_policy(&bytes, &bindings().0, &profile(), &expected)
        .unwrap();
    assert_eq!(fingerprint, snapshot.fingerprint());
    assert!(snapshot.checkpoint().unwrap().as_slice() == bytes.as_slice());
    edit(&running, |f| assert!(f.calls.is_empty()));
    let result = restore(&running, &snapshot).await.unwrap();
    assert_eq!(result.policy_version, 2);
    edit(&running, |f| {
        assert!(f.new_config.policy == f.old_config.policy);
        assert!(f.new_config.settings == f.old_config.settings);
        assert_eq!(f.updates, 1);
    });
}

#[tokio::test]
async fn checkpoint_rejects_noncanonical_data_and_cross_provider_instance_process_rebinding() {
    use crate::maintenance_protocol::PolicyCheckpoint;
    use prost::Message;

    let running = fixture().await;
    let snapshot = captured(&running).await;
    let bytes = snapshot.checkpoint().unwrap();
    let expected = running.runtime.get(&bindings().0).await.unwrap().unwrap();
    let rejected = |bytes: &[u8], source: &Binding, expected: &Observation| {
        assert!(matches!(
            running
                .runtime
                .recover_replacement_policy(bytes, source, &profile(), expected),
            Err(RuntimeFailure::PolicyContinuity)
        ));
    };
    rejected(&[], &bindings().0, &expected);
    rejected(&vec![0; 1024 * 1024 + 4097], &bindings().0, &expected);
    rejected(&bytes[..bytes.len() - 1], &bindings().0, &expected);
    for suffix in [[0x08, 0x01], [0x50, 0x01]] {
        let mut invalid = bytes.to_vec();
        invalid.extend(suffix);
        rejected(&invalid, &bindings().0, &expected);
    }
    rejected(&bytes, &bindings().1, &expected);
    let mut wrong = expected.clone();
    wrong.main_process_instance_id = "changed-process".into();
    rejected(&bytes, &bindings().0, &wrong);
    wrong = expected.clone();
    wrong.sandbox_id = "changed-resource".into();
    rejected(&bytes, &bindings().0, &wrong);
    wrong = expected.clone();
    wrong.phase = Phase::Starting;
    rejected(&bytes, &bindings().0, &wrong);
    for fault in 0..16 {
        let mut invalid = PolicyCheckpoint::decode(bytes.as_slice()).unwrap();
        match fault {
            0 => invalid.version += 1,
            1 => invalid.gateway_version = "other-version".into(),
            2 => invalid.provider_instance_id = Uuid::now_v7().as_bytes().to_vec(),
            3 => invalid.source_instance_id = Uuid::now_v7().as_bytes().to_vec(),
            4 => invalid.template_fingerprint = "b".repeat(64),
            5 => invalid.config = None,
            6 => invalid.config.as_mut().unwrap().workspace = "different".into(),
            7 => invalid.config.as_mut().unwrap().version = 0,
            8 => invalid.config.as_mut().unwrap().global_policy_version = 1,
            9 => invalid.version = 1,
            10 => invalid
                .config
                .as_mut()
                .unwrap()
                .provider_attachment_epoch
                .clear(),
            11 => invalid.config.as_mut().unwrap().provider_attachment_epoch = "malformed".into(),
            12 => {
                invalid.config.as_mut().unwrap().provider_attachment_epoch =
                    "00000000-0000-7000-8000-000000000011".into()
            }
            13 => invalid
                .config
                .as_mut()
                .unwrap()
                .configuration_instance_id
                .clear(),
            14 => invalid.config.as_mut().unwrap().configuration_instance_id = "malformed".into(),
            15 => {
                invalid.config.as_mut().unwrap().configuration_instance_id =
                    "00000000-0000-7000-8000-000000000021".into()
            }
            _ => unreachable!(),
        }
        rejected(&invalid.encode_to_vec(), &bindings().0, &expected);
    }
    let mut different_provider = running.runtime.clone();
    different_provider.provider_instance_id = crate::ProviderInstanceId::new();
    assert!(
        different_provider
            .recover_replacement_policy(&bytes, &bindings().0, &profile(), &expected)
            .is_err()
    );
    assert!(
        different_provider
            .restore_replacement_policy(
                &snapshot,
                &fixture_handoff(&running, bindings().1, Uuid::now_v7()),
                &profile(),
                &profile()
            )
            .await
            .is_err()
    );
    edit(&running, |f| assert_eq!(f.updates, 0));
}

#[tokio::test]
async fn lost_mutation_reply_reconciles_the_same_loaded_policy_without_resubmission() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    edit(&running, |f| f.fault = Fault::LostReply);
    assert_eq!(
        restore(&running, &snapshot).await.unwrap_err(),
        RuntimeFailure::PolicyContinuity
    );
    edit(&running, |f| {
        assert_eq!(f.calls.last(), Some(&"update"));
        f.fault = Fault::None;
    });
    assert_eq!(
        reconcile(&running, &snapshot).await.unwrap().policy_version,
        2
    );
    edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
}

#[tokio::test]
async fn uncertain_pending_update_never_resubmits_from_an_unchanged_baseline() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    edit(&running, |f| f.fault = Fault::UncertainPending);
    assert!(restore(&running, &snapshot).await.is_err());
    for _ in 0..3 {
        assert!(reconcile(&running, &snapshot).await.is_err());
    }
    edit(&running, |f| {
        assert_eq!((f.updates, f.watches), (1, 1));
        assert_eq!(f.new_config.version, 1);
    });
}

#[tokio::test]
async fn matching_grants_require_the_original_maintenance_provenance() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    restore(&running, &snapshot).await.unwrap();
    for wrong in [None, Some(Uuid::now_v7().to_string())] {
        edit(&running, |f| {
            f.annotations.clear();
            if let Some(wrong) = wrong {
                f.annotations.insert(PROVENANCE.into(), wrong);
            }
        });
        assert!(reconcile(&running, &snapshot).await.is_err());
        assert!(restore(&running, &snapshot).await.is_err());
    }
    edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
}

#[tokio::test]
async fn reconciliation_with_no_budget_makes_no_provider_call() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    edit(&running, |f| f.calls.clear());
    assert!(
        running
            .runtime
            .reconcile_replacement_policy(
                &snapshot,
                &fixture_handoff(&running, bindings().1, Uuid::from_u128(100)),
                &profile(),
                &profile(),
                Duration::ZERO,
            )
            .await
            .is_err()
    );
    edit(&running, |f| assert!(f.calls.is_empty()));
}

#[tokio::test]
async fn unconfirmed_target_watch_load_process_or_configuration_fails_closed() {
    for fault in [
        Fault::Warning,
        Fault::ChangedProcess,
        Fault::ChangedSettings,
        Fault::Unloaded,
        Fault::ChangedRules,
    ] {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| f.fault = fault);
        let error = restore(&running, &snapshot).await.unwrap_err();
        assert_eq!(error, RuntimeFailure::PolicyContinuity);
        assert!(!format!("{error:?} {error}").contains(SECRET));
        edit(&running, |f| {
            assert_eq!((f.updates, f.watches), (1, 1));
            if matches!(fault, Fault::Warning | Fault::ChangedProcess) {
                assert_eq!(f.calls.last(), Some(&"update"));
            }
        });
    }
}

#[tokio::test]
async fn capture_requires_a_stopped_source_with_an_exact_run() {
    for phase in [Phase::Ready, Phase::Starting, Phase::Stopping] {
        let running = fixture().await;
        edit(&running, |f| {
            f.source.status.as_mut().unwrap().phase = phase as i32
        });
        assert!(
            running
                .runtime
                .capture_replacement_policy(&bindings().0, &profile())
                .await
                .is_err()
        );
        edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
    }
    let running = fixture().await;
    edit(&running, |f| {
        f.source
            .status
            .as_mut()
            .unwrap()
            .main_process_instance_id
            .clear()
    });
    assert!(
        running
            .runtime
            .capture_replacement_policy(&bindings().0, &profile())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wrong_instance_owner_template_or_operation_never_dispatches_a_write() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    let (source, target) = bindings();
    for (binding, operation) in [
        (source.clone(), Uuid::from_u128(100)),
        (target.clone(), Uuid::nil()),
        (
            Binding::replacement(
                Uuid::from_u128(3),
                Uuid::from_u128(10),
                profile().fingerprint(),
            )
            .unwrap(),
            Uuid::from_u128(100),
        ),
        (
            Binding::replacement(source.computer_id(), Uuid::from_u128(10), "a".repeat(64))
                .unwrap(),
            Uuid::from_u128(100),
        ),
    ] {
        assert!(
            running
                .runtime
                .restore_replacement_policy(
                    &snapshot,
                    &fixture_handoff(&running, binding, operation),
                    &profile(),
                    &profile()
                )
                .await
                .is_err()
        );
    }
    edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
}

#[tokio::test]
async fn changed_candidate_settings_or_existing_grants_are_not_overwritten() {
    for settings in [true, false] {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| {
            if settings {
                f.new_config.settings.clear();
            } else {
                f.new_config
                    .policy
                    .as_mut()
                    .unwrap()
                    .network_policies
                    .insert(
                        "unrelated".into(),
                        rule("unrelated", "elsewhere.example.com"),
                    );
            }
        });
        assert!(restore(&running, &snapshot).await.is_err());
        edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
    }
}

#[tokio::test]
async fn capture_rejects_static_changes_restrictions_global_policy_and_ambiguous_merges() {
    for change in 0..8 {
        let running = fixture().await;
        edit(&running, |f| {
            let policy = f.old_config.policy.as_mut().unwrap();
            match change {
                0 => policy
                    .filesystem
                    .as_mut()
                    .unwrap()
                    .read_write
                    .push("/".into()),
                1 => {
                    policy.network_policies.remove("packages");
                }
                2 => {
                    policy
                        .network_policies
                        .get_mut("next-build")
                        .unwrap()
                        .endpoints[0]
                        .host = "*.example.com".into()
                }
                3 => {
                    policy
                        .network_policies
                        .get_mut("next-build")
                        .unwrap()
                        .endpoints[0]
                        .host = "packages.example.com".into()
                }
                4 => policy.process.as_mut().unwrap().run_as_user = "0".into(),
                5 => f.old_config.policy_source = policy::PolicySource::Global as i32,
                6 => policy.network_policies.get_mut("next-build").unwrap().name = "other".into(),
                _ => {
                    let endpoint = &mut policy
                        .network_policies
                        .get_mut("next-build")
                        .unwrap()
                        .endpoints[0];
                    endpoint.protocol = "rest".into();
                    endpoint.enforcement = policy::NetworkEnforcementMode::Audit as i32;
                }
            }
        });
        assert!(
            running
                .runtime
                .capture_replacement_policy(&bindings().0, &profile())
                .await
                .is_err()
        );
        edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
    }
}

#[tokio::test]
async fn capture_admits_gateway_owned_attachment_epoch() {
    let running = fixture().await;
    assert!(
        running
            .runtime
            .capture_replacement_policy(&bindings().0, &profile())
            .await
            .is_ok(),
        "current gateway assigns an attachment epoch distinct from immutable template inputs"
    );
    edit(&running, |f| assert_eq!(f.updates, 0));
}

#[tokio::test]
async fn capture_refuses_absent_malformed_mismatched_or_changed_attachment_epoch() {
    for epoch in ["", "not-an-epoch", "00000000-0000-7000-8000-000000000011"] {
        let running = fixture().await;
        edit(&running, |f| {
            f.source.spec.as_mut().unwrap().provider_attachment_epoch = epoch.into()
        });
        assert!(
            running
                .runtime
                .capture_replacement_policy(&bindings().0, &profile())
                .await
                .is_err()
        );
        edit(&running, |f| assert_eq!(f.updates, 0));
    }
    let running = fixture().await;
    edit(&running, |f| {
        f.old_config.provider_attachment_epoch = "00000000-0000-4000-8000-000000000012".into()
    });
    assert!(
        running
            .runtime
            .capture_replacement_policy(&bindings().0, &profile())
            .await
            .is_err()
    );
    edit(&running, |f| assert_eq!(f.updates, 0));
    let running = fixture().await;
    edit(&running, |f| f.fault = Fault::ChangedEpoch);
    assert!(
        running
            .runtime
            .capture_replacement_policy(&bindings().0, &profile())
            .await
            .is_err()
    );
    edit(&running, |f| assert_eq!(f.updates, 0));
}

#[tokio::test]
async fn replacement_epoch_mismatch_refuses_before_policy_mutation() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    edit(&running, |f| {
        f.target.spec.as_mut().unwrap().provider_attachment_epoch =
            "00000000-0000-4000-8000-000000000013".into();
    });
    assert!(restore(&running, &snapshot).await.is_err());
    edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
}

#[tokio::test]
async fn restoration_requires_original_epoch_in_config_watch_and_final_spec() {
    for fault in [
        Fault::MissingConfigEpoch,
        Fault::MalformedConfigEpoch,
        Fault::ChangedConfigEpoch,
        Fault::MissingWatchEpoch,
        Fault::MalformedWatchEpoch,
        Fault::ChangedWatchEpoch,
        Fault::MissingFinalEpoch,
        Fault::MalformedFinalEpoch,
        Fault::ChangedFinalEpoch,
    ] {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| f.fault = fault);
        assert!(
            restore(&running, &snapshot).await.is_err(),
            "changed target identity cannot settle restoration"
        );
        edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
    }
}

#[tokio::test]
async fn observation_only_reconciliation_refuses_epoch_drift_without_replay() {
    for fault in [
        Fault::MissingConfigEpoch,
        Fault::MalformedConfigEpoch,
        Fault::ChangedConfigEpoch,
        Fault::MissingFinalEpoch,
        Fault::MalformedFinalEpoch,
        Fault::ChangedFinalEpoch,
    ] {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| f.fault = fault);
        assert!(restore(&running, &snapshot).await.is_err());
        for _ in 0..2 {
            assert!(reconcile(&running, &snapshot).await.is_err());
        }
        edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
    }
}

#[tokio::test]
async fn replacement_admits_independent_provider_configuration_instances() {
    let running = fixture().await;
    let snapshot = captured(&running).await;
    edit(&running, |f| {
        assert_ne!(
            f.old_config.configuration_instance_id,
            f.new_config.configuration_instance_id
        )
    });
    restore(&running, &snapshot).await.unwrap();
    reconcile(&running, &snapshot).await.unwrap();
    edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
}

#[tokio::test]
async fn configuration_identity_is_required_and_correlated_during_capture() {
    for identity in [
        "",
        "malformed",
        "00000000-0000-7000-8000-000000000021",
        "00000000-0000-4000-8000-000000000023",
    ] {
        let running = fixture().await;
        edit(&running, |f| {
            f.old_config.configuration_instance_id = identity.into()
        });
        assert!(
            running
                .runtime
                .capture_replacement_policy(&bindings().0, &profile())
                .await
                .is_err()
        );
        edit(&running, |f| assert_eq!(f.updates, 0));
    }
    for identity in [None, Some(""), Some("malformed")] {
        let running = fixture().await;
        edit(&running, |f| {
            if let Some(identity) = identity {
                f.source
                    .status
                    .as_mut()
                    .unwrap()
                    .configuration_admission
                    .as_mut()
                    .unwrap()
                    .instance_id = identity.into();
            } else {
                f.source.status.as_mut().unwrap().configuration_admission = None;
            }
        });
        assert!(
            running
                .runtime
                .capture_replacement_policy(&bindings().0, &profile())
                .await
                .is_err()
        );
    }
    let running = fixture().await;
    edit(&running, |f| f.fault = Fault::ChangedConfiguration);
    assert!(
        running
            .runtime
            .capture_replacement_policy(&bindings().0, &profile())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn restoration_configuration_identity_cannot_drift_after_dispatch() {
    for fault in [
        Fault::MissingConfigurationConfig,
        Fault::MalformedConfigurationConfig,
        Fault::ChangedConfigurationConfig,
        Fault::MissingConfigurationWatch,
        Fault::MalformedConfigurationWatch,
        Fault::ChangedConfigurationWatch,
        Fault::MissingConfigurationFinal,
        Fault::MalformedConfigurationFinal,
        Fault::ChangedConfigurationFinal,
    ] {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| f.fault = fault);
        assert!(restore(&running, &snapshot).await.is_err());
        edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
        if !matches!(
            fault,
            Fault::MissingConfigurationWatch
                | Fault::MalformedConfigurationWatch
                | Fault::ChangedConfigurationWatch
        ) {
            for _ in 0..2 {
                assert!(reconcile(&running, &snapshot).await.is_err());
            }
            edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
        }
    }
}
