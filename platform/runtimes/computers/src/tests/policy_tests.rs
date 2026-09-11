use super::*;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

type Reply<T> = std::result::Result<Response<T>, Status>;
const PROVENANCE: &str = "veoveo.io/replacement-policy";
const SECRET: &str = "PRIVATE-FIXTURE-SETTING";

fn rule(name: &str, host: &str) -> policy::NetworkPolicyRule {
    policy::NetworkPolicyRule {
        name: name.into(),
        endpoints: vec![policy::NetworkEndpoint {
            host: host.into(),
            port: 443,
            ports: vec![443],
            protocol: "tcp".into(),
            tls: "passthrough".into(),
            ..Default::default()
        }],
        binaries: vec![policy::NetworkBinary {
            path: "/opt/fixture/bin/tool".into(),
            ..Default::default()
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
    let status = value.status.as_mut().unwrap();
    status.current_policy_version = version;
    status.main_process_instance_id = format!("main-{id}");
    value
}
fn config_for(sandbox: &api::Sandbox) -> policy::GetSandboxConfigResponse {
    policy::GetSandboxConfigResponse {
        policy: sandbox.spec.as_ref().unwrap().policy.clone(),
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
    Warning,
    ChangedProcess,
    ChangedSettings,
    Unloaded,
    OldRestarted,
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
}
impl Fixture {
    fn new() -> Self {
        let (old, new) = bindings();
        let source = sandbox_for(&old, "sandbox-old", 3);
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
        assert_eq!(request.workspace, "computers");
        Ok(Response::new(api::SandboxResponse {
            sandbox: Some(self.by_name(&request.name).clone()),
        }))
    }
    pub(super) fn config(
        &mut self,
        request: policy::GetSandboxConfigRequest,
    ) -> Reply<policy::GetSandboxConfigResponse> {
        self.calls.push("config");
        let config = if request.sandbox_id == "sandbox-old" {
            &self.old_config
        } else {
            assert_eq!(request.sandbox_id, "sandbox-new");
            &self.new_config
        };
        Ok(Response::new(config.clone()))
    }
    pub(super) fn loaded(
        &mut self,
        request: api::GetSandboxPolicyStatusRequest,
    ) -> Reply<api::GetSandboxPolicyStatusResponse> {
        self.calls.push("loaded");
        assert_eq!(request.workspace, "computers");
        assert!(!request.global);
        let old = self.by_name(&request.name).metadata.as_ref().unwrap().id == "sandbox-old";
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
        assert_eq!(request.id, "sandbox-new");
        assert!(request.follow_status);
        assert!(!request.follow_logs);
        assert!(!request.stop_on_terminal);
        let (sender, receiver) = mpsc::channel(8);
        sender
            .try_send(Ok(api::SandboxStreamEvent {
                payload: Some(api::sandbox_stream_event::Payload::Sandbox(
                    self.target.clone(),
                )),
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
        assert_eq!(request.name, self.target.metadata.as_ref().unwrap().name);
        assert_eq!(request.workspace, "computers");
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
            Fault::ChangedSettings => {
                self.new_config.settings.clear();
            }
            Fault::OldRestarted => {
                self.source.status.as_mut().unwrap().phase = Phase::Ready as i32;
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
    running
        .runtime
        .restore_replacement_policy(snapshot, &bindings().1, &profile(), Uuid::from_u128(100))
        .await
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
    assert_eq!(result, restore(&running, &snapshot).await.unwrap());
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
    for fault in 0..9 {
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
            _ => unreachable!(),
        }
        rejected(&invalid.encode_to_vec(), &bindings().0, &expected);
    }
    let mut different_provider = running.runtime.clone();
    different_provider.provider_instance_id = Uuid::now_v7();
    assert!(
        different_provider
            .recover_replacement_policy(&bytes, &bindings().0, &profile(), &expected)
            .is_err()
    );
    assert!(
        different_provider
            .restore_replacement_policy(&snapshot, &bindings().1, &profile(), Uuid::now_v7())
            .await
            .is_err()
    );
    edit(&running, |f| {
        f.source.status.as_mut().unwrap().main_process_instance_id = "later-source-run".into();
    });
    assert!(restore(&running, &snapshot).await.is_err());
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
        restore(&running, &snapshot).await.unwrap().policy_version,
        2
    );
    edit(&running, |f| assert_eq!((f.updates, f.watches), (1, 1)));
}

#[tokio::test]
async fn unconfirmed_watch_load_process_configuration_or_old_stop_fails_closed() {
    for fault in [
        Fault::Warning,
        Fault::ChangedProcess,
        Fault::ChangedSettings,
        Fault::Unloaded,
        Fault::OldRestarted,
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
async fn running_original_or_changed_source_never_mutates_the_candidate() {
    for change in 0..3 {
        let running = fixture().await;
        let snapshot = captured(&running).await;
        edit(&running, |f| match change {
            0 => f.source.status.as_mut().unwrap().phase = Phase::Ready as i32,
            1 => f.old_config.config_revision += 1,
            _ => f.source.metadata.as_mut().unwrap().id = "changed-source".into(),
        });
        assert!(restore(&running, &snapshot).await.is_err());
        edit(&running, |f| assert_eq!((f.updates, f.watches), (0, 0)));
    }
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
                .restore_replacement_policy(&snapshot, &binding, &profile(), operation)
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
                    endpoint.enforcement = "audit".into();
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
