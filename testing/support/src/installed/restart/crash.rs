//! Observe one externally signalled container crash without issuing a mutation.
use super::{DeploymentRestart, DrainProfile, SelectedDrainIdentity, SelectedDrainTarget, drain};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Duration};
use uuid::Uuid;

/// Private fixture declarations are checked against Kubernetes before the watch arms.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrashTarget {
    pub deployment: String,
    pub pod: String,
    pub container: String,
    pub namespace_uid: Uuid,
    pub deployment_uid: Uuid,
    pub replica_set_uid: Uuid,
    pub pod_uid: Uuid,
    pub container_id: String,
    pub image_id: String,
    pub restart_count: u32,
}
impl CrashTarget {
    pub fn validate(&self) -> Result<()> {
        for name in [&self.deployment, &self.pod, &self.container] {
            drain::name(name)?;
        }
        ensure!(
            [
                self.namespace_uid,
                self.deployment_uid,
                self.replica_set_uid,
                self.pod_uid
            ]
            .iter()
            .all(|uid| !uid.is_nil()),
            "crash fixture requires nonzero resource UIDs"
        );
        for value in [&self.container_id, &self.image_id] {
            ensure!(
                !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_whitespace),
                "crash fixture runtime identity is absent or invalid"
            );
        }
        ensure!(
            self.restart_count < u32::MAX,
            "crash restart count cannot increment"
        );
        Ok(())
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashIdentity {
    pub selected: SelectedDrainIdentity,
    pub replica_set_uid: Uuid,
    pub image_id: String,
    pub armed_at: DateTime<Utc>,
}
/// Partial progress survives cancellation because the caller owns the watch.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReceipt {
    pub identity: CrashIdentity,
    pub exit_code: Option<i32>,
    pub reported_signal: Option<i32>,
    pub termination_reason: Option<String>,
    pub signal_available: bool,
    pub finished_at: Option<DateTime<Utc>>,
    pub replacement_container_id: Option<String>,
    pub restart_count: u32,
    pub ready: bool,
    pub final_fenced_at: Option<DateTime<Utc>>,
}
pub struct CrashWatch {
    driver: DeploymentRestart,
    watch: drain::PodWatch,
    observation: Option<CrashObservation>,
}
impl DeploymentRestart {
    /// Admit the current live target before the owner dispatches domain mutations.
    pub async fn admit_crash_target(&self, expected: &CrashTarget) -> Result<()> {
        self.select_crash_target(expected).await.map(|_| ())
    }

    async fn select_crash_target(&self, expected: &CrashTarget) -> Result<SelectedDrainTarget> {
        expected.validate()?;
        ensure!(
            expected.deployment == self.deployment,
            "crash target selects another Deployment"
        );
        let (selected, pod) = self
            .select_drain_snapshot(
                &expected.pod,
                DrainProfile::server(&expected.container, Duration::from_secs(1))?,
            )
            .await?;
        ensure!(
            selected.namespace_uid == expected.namespace_uid
                && selected.deployment_uid == expected.deployment_uid
                && selected.pod_uid == expected.pod_uid
                && selected.container_id == expected.container_id
                && selected.restart_count == expected.restart_count,
            "live crash target differs from admitted fixture"
        );
        CrashObservation::initial(selected.clone(), expected, drain::WatchEvent::Added(pod))?;
        Ok(selected)
    }

    pub async fn arm_crash_watch(
        &self,
        expected: &CrashTarget,
        owned: &mut Option<CrashWatch>,
    ) -> Result<()> {
        ensure!(
            owned.is_none(),
            "crash watch is already owned; do not rearm"
        );
        let selected = self.select_crash_target(expected).await?;
        let watch = drain::PodWatch::start(&self.context, &self.namespace, &selected.pod)?;
        *owned = Some(CrashWatch {
            driver: self.clone(),
            watch,
            observation: None,
        });
        let handle = owned
            .as_mut()
            .context("crash watch ownership was not retained")?;
        handle.admit_initial(selected, expected).await
    }
}
impl CrashWatch {
    async fn admit_initial(
        &mut self,
        selected: SelectedDrainTarget,
        expected: &CrashTarget,
    ) -> Result<()> {
        let initial = tokio::time::timeout(Duration::from_secs(10), self.watch.next())
            .await
            .context("crash watch initial admission exceeded ten seconds")??;
        self.observation = Some(CrashObservation::initial(selected, expected, initial)?);
        Ok(())
    }
    pub fn identity(&self) -> Result<CrashIdentity> {
        Ok(self
            .observation
            .as_ref()
            .context("crash watch initial admission is incomplete")?
            .receipt
            .identity
            .clone())
    }
    pub fn snapshot(&self) -> Option<CrashReceipt> {
        self.observation.as_ref().map(|state| state.receipt.clone())
    }
    /// Recheck the observed replacement after the owner's retained-state reads.
    pub async fn admit_recovered_target(&mut self) -> Result<()> {
        let observation = self
            .observation
            .as_ref()
            .context("crash watch is not admitted")?;
        let expected = observation.recovered_target()?;
        let selected = self.driver.select_crash_target(&expected).await?;
        ensure!(
            selected.generation == observation.selected.generation,
            "crash recovery template generation changed during readback"
        );
        self.observation
            .as_mut()
            .context("crash watch admission was lost")?
            .receipt
            .final_fenced_at = Some(Utc::now());
        Ok(())
    }

    pub async fn wait(&mut self, deadline: tokio::time::Instant) -> Result<CrashReceipt> {
        ensure!(
            deadline > tokio::time::Instant::now()
                && deadline <= tokio::time::Instant::now() + Duration::from_secs(300),
            "crash watch deadline must be within 300 seconds"
        );
        let observation = self
            .observation
            .as_mut()
            .context("crash watch is not admitted")?;
        tokio::time::timeout_at(deadline, async {
            loop {
                let event = self.watch.next().await?;
                if observation.observe(event)? {
                    let after = self.driver.deployment().await?;
                    after.require_ready()?;
                    ensure!(after.metadata.uid == observation.selected.deployment_uid.to_string()
                        && after.metadata.generation == observation.selected.generation
                        && after.spec.replicas == 1,
                        "crash recovery Deployment identity, template generation or replica count changed");
                    let namespace: drain::Namespace = serde_json::from_slice(&self.driver.command(
                        &["get", "namespace", &self.driver.namespace, "-o", "json"]).await?)
                        .map_err(|_| anyhow::anyhow!("invalid crash namespace fence response"))?;
                    ensure!(namespace.metadata.uid == observation.selected.namespace_uid
                        && namespace.metadata.deletion_timestamp.is_none(), "crash recovery namespace changed");
                    self.driver.require_public_route().await?;
                    return Ok(observation.receipt.clone());
                }
            }
        }).await.context("same-Pod crash recovery watch exceeded its deadline")?
    }
    pub async fn close(self) -> bool {
        self.watch.close().await
    }
}

struct CrashObservation {
    selected: SelectedDrainTarget,
    replica_set_uid: Uuid,
    versions: BTreeSet<String>,
    receipt: CrashReceipt,
}
impl CrashObservation {
    fn initial(
        selected: SelectedDrainTarget,
        expected: &CrashTarget,
        event: drain::WatchEvent,
    ) -> Result<Self> {
        let drain::WatchEvent::Added(pod) = event else {
            anyhow::bail!("crash watch has no initial selected Pod");
        };
        pod.admit(&selected.namespace, &selected.profile)?;
        let identity = CrashIdentity {
            selected: selected.identity(),
            replica_set_uid: expected.replica_set_uid,
            image_id: expected.image_id.clone(),
            armed_at: Utc::now(),
        };
        let observation = Self {
            versions: BTreeSet::from([pod.metadata.resource_version.clone()]),
            replica_set_uid: expected.replica_set_uid,
            receipt: CrashReceipt {
                identity,
                exit_code: None,
                reported_signal: None,
                termination_reason: None,
                signal_available: false,
                finished_at: None,
                replacement_container_id: None,
                restart_count: selected.restart_count,
                ready: false,
                final_fenced_at: None,
            },
            selected,
        };
        observation.identity(&pod)?;
        let status = pod.selected_status(&observation.selected.profile)?;
        ensure!(
            status.container_id == expected.container_id
                && status.image_id == expected.image_id
                && status.restart_count == expected.restart_count,
            "crash watch initial container differs"
        );
        Ok(observation)
    }
    fn recovered_target(&self) -> Result<CrashTarget> {
        ensure!(
            self.receipt.ready
                && self.receipt.exit_code == Some(137)
                && self.receipt.restart_count == self.selected.restart_count + 1,
            "crash replacement has not been observed ready"
        );
        Ok(CrashTarget {
            deployment: self.selected.deployment.clone(),
            pod: self.selected.pod.clone(),
            container: self.selected.profile.container.clone(),
            namespace_uid: self.selected.namespace_uid,
            deployment_uid: self.selected.deployment_uid,
            replica_set_uid: self.replica_set_uid,
            pod_uid: self.selected.pod_uid,
            container_id: self
                .receipt
                .replacement_container_id
                .clone()
                .context("crash replacement identity absent")?,
            image_id: self.receipt.identity.image_id.clone(),
            restart_count: self.receipt.restart_count,
        })
    }

    fn identity(&self, pod: &drain::Pod) -> Result<()> {
        ensure!(
            pod.metadata.uid == self.selected.pod_uid
                && pod.metadata.name == self.selected.pod
                && pod.metadata.namespace.as_deref() == Some(&self.selected.namespace)
                && pod.metadata.deletion_timestamp.is_none(),
            "crash watch observed a replaced or deleting Pod"
        );
        ensure!(
            pod.metadata
                .owner_references
                .iter()
                .any(|owner| owner.kind == "ReplicaSet"
                    && owner.controller == Some(true)
                    && owner.uid == self.replica_set_uid),
            "crash watch ReplicaSet identity changed"
        );
        drain::version(&pod.metadata.resource_version)
    }
    fn observe(&mut self, event: drain::WatchEvent) -> Result<bool> {
        let drain::WatchEvent::Modified(pod) = event else {
            anyhow::bail!("crash watch gap, deletion or unexpected event");
        };
        self.identity(&pod)?;
        ensure!(
            self.versions.insert(pod.metadata.resource_version.clone()),
            "crash watch repeated a resource version"
        );
        let mut matching = pod
            .status
            .container_statuses
            .iter()
            .filter(|s| s.name == self.selected.profile.container);
        let status = matching
            .next()
            .context("crash watch selected container is absent")?;
        ensure!(
            matching.next().is_none(),
            "crash watch container status is ambiguous"
        );
        ensure!(
            status.image_id.is_empty() || status.image_id == self.receipt.identity.image_id,
            "crash watch image identity changed"
        );
        ensure!(
            status.restart_count >= self.selected.restart_count
                && status.restart_count <= self.selected.restart_count + 1,
            "crash watch observed an unexpected restart count"
        );
        for (terminal, current) in [
            (&status.state.terminated, true),
            (&status.last_state.terminated, false),
        ] {
            if let Some(terminal) = terminal {
                let same = terminal.container_id.as_deref() == Some(&self.selected.container_id)
                    || (current && status.container_id == self.selected.container_id);
                if same {
                    self.receipt.exit_code = Some(terminal.exit_code);
                    self.receipt.reported_signal = terminal.signal;
                    self.receipt.termination_reason = terminal.reason.clone();
                    self.receipt.signal_available = terminal.signal.is_some_and(|s| s != 0);
                    self.receipt.finished_at = Some(terminal.finished_at);
                    ensure!(
                        terminal.exit_code == 137
                            && terminal.signal.is_none_or(|s| s == 0 || s == 9)
                            && terminal
                                .reason
                                .as_deref()
                                .is_none_or(|reason| reason == "Error"),
                        "selected old container did not report SIGKILL-compatible termination"
                    );
                    ensure!(
                        terminal.finished_at
                            >= chrono::SubsecRound::trunc_subsecs(
                                self.receipt.identity.armed_at,
                                0
                            ),
                        "selected termination precedes the armed watch"
                    );
                }
            }
        }
        self.receipt.restart_count = status.restart_count;
        if status.ready
            && status.state.running.is_some()
            && status.container_id != self.selected.container_id
        {
            ensure!(
                !status.container_id.is_empty()
                    && status.image_id == self.receipt.identity.image_id
                    && status.restart_count == self.selected.restart_count + 1,
                "replacement container identity or count is invalid"
            );
            ensure!(
                self.receipt.exit_code == Some(137),
                "replacement became ready without observed old-container termination"
            );
            ensure!(
                pod.status
                    .conditions
                    .iter()
                    .any(|c| c.r#type == "Ready" && c.status == "True"),
                "replacement container is ready but Pod is not Ready"
            );
            self.receipt.replacement_container_id = Some(status.container_id.clone());
            self.receipt.ready = true;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn target() -> CrashTarget {
        CrashTarget {
            deployment: "service".into(),
            pod: "service-pod".into(),
            container: "server".into(),
            namespace_uid: Uuid::from_u128(1),
            deployment_uid: Uuid::from_u128(2),
            replica_set_uid: Uuid::from_u128(4),
            pod_uid: Uuid::from_u128(3),
            container_id: "containerd://old".into(),
            image_id: "sha256:fixture".into(),
            restart_count: 2,
        }
    }
    fn selected() -> SelectedDrainTarget {
        let target = target();
        SelectedDrainTarget {
            namespace: "fixture".into(),
            namespace_uid: target.namespace_uid,
            deployment: target.deployment,
            deployment_uid: target.deployment_uid,
            deployment_version: "1".into(),
            generation: 1,
            pod: target.pod,
            pod_uid: target.pod_uid,
            pod_version: "2".into(),
            container_id: target.container_id,
            restart_count: target.restart_count,
            profile: DrainProfile::server("server", Duration::from_secs(1)).unwrap(),
            grace: 30,
            annotations: BTreeMap::new(),
        }
    }
    fn pod() -> serde_json::Value {
        let target = target();
        json!({"metadata":{"name":target.pod,"namespace":"fixture","uid":target.pod_uid,"resourceVersion":"2",
            "ownerReferences":[{"kind":"ReplicaSet","controller":true,"name":"service-rs","uid":target.replica_set_uid}]},
            "spec":{"terminationGracePeriodSeconds":30,"containers":[{"name":"server","resources":{}}]},
            "status":{"conditions":[{"type":"Ready","status":"True"}],"containerStatuses":[{"name":"server",
                "containerID":target.container_id,"imageID":target.image_id,"restartCount":2,"ready":true,
                "state":{"running":{"startedAt":"2026-10-01T00:00:00Z"}}}]}})
    }
    fn observation() -> Result<CrashObservation> {
        CrashObservation::initial(
            selected(),
            &target(),
            drain::WatchEvent::Added(serde_json::from_value(pod())?),
        )
    }
    fn replacement(signal: Option<i32>, reason: &str) -> serde_json::Value {
        let mut value = pod();
        value["metadata"]["resourceVersion"] = json!("3");
        let status = &mut value["status"]["containerStatuses"][0];
        status["containerID"] = json!("containerd://new");
        status["restartCount"] = json!(3);
        status["lastState"] = json!({"terminated":{"exitCode":137,"signal":signal,"reason":reason,
            "containerID":"containerd://old","finishedAt":Utc::now()}});
        value
    }
    #[test]
    fn same_pod_crash_requires_old_instance_exit_and_records_signal_availability() -> Result<()> {
        for signal in [Some(9), Some(0), None] {
            let mut observation = observation()?;
            assert!(
                observation.observe(drain::WatchEvent::Modified(serde_json::from_value(
                    replacement(signal, "Error")
                )?))?
            );
            assert!(observation.receipt.ready);
            assert_eq!(observation.receipt.exit_code, Some(137));
            assert_eq!(observation.receipt.reported_signal, signal);
            assert_eq!(observation.receipt.signal_available, signal == Some(9));
            // Cleanup journals decode this observation without creating an
            // admitted target or conferring a process mutation capability.
            let wire = serde_json::to_value(&observation.receipt)?;
            let retained: CrashReceipt = serde_json::from_value(wire.clone())?;
            assert_eq!(serde_json::to_value(retained)?, wire);
        }
        Ok(())
    }
    #[test]
    fn final_fence_refuses_extra_restart_instance_image_or_pod_drift() -> Result<()> {
        let mut observation = CrashObservation::initial(
            selected(),
            &target(),
            drain::WatchEvent::Added(serde_json::from_value(pod())?),
        )?;
        assert!(observation.recovered_target().is_err());
        assert!(
            observation.observe(drain::WatchEvent::Modified(serde_json::from_value(
                replacement(Some(9), "Error")
            )?))?
        );
        let expected = observation.recovered_target()?;
        let mut selected = selected();
        selected.container_id = expected.container_id.clone();
        selected.restart_count = expected.restart_count;
        assert!(
            CrashObservation::initial(
                selected.clone(),
                &expected,
                drain::WatchEvent::Added(serde_json::from_value(replacement(Some(9), "Error"))?)
            )
            .is_ok()
        );
        for field in ["count", "instance", "image", "pod"] {
            let mut live = replacement(Some(9), "Error");
            match field {
                "count" => {
                    live["status"]["containerStatuses"][0]["restartCount"] =
                        json!(expected.restart_count + 1)
                }
                "instance" => {
                    live["status"]["containerStatuses"][0]["containerID"] =
                        json!("containerd://third")
                }
                "image" => {
                    live["status"]["containerStatuses"][0]["imageID"] = json!("sha256:other")
                }
                _ => live["metadata"]["uid"] = json!(Uuid::from_u128(99)),
            }
            assert!(
                CrashObservation::initial(
                    selected.clone(),
                    &expected,
                    drain::WatchEvent::Added(serde_json::from_value(live)?)
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn admission_refuses_wrong_initial_identity_image_owner_or_readiness() -> Result<()> {
        for field in ["uid", "image", "owner", "ready"] {
            let mut value = pod();
            match field {
                "uid" => value["metadata"]["uid"] = json!(Uuid::from_u128(99)),
                "image" => {
                    value["status"]["containerStatuses"][0]["imageID"] = json!("sha256:other")
                }
                "owner" => {
                    value["metadata"]["ownerReferences"][0]["uid"] = json!(Uuid::from_u128(99))
                }
                _ => value["status"]["containerStatuses"][0]["ready"] = json!(false),
            }
            assert!(
                CrashObservation::initial(
                    selected(),
                    &target(),
                    drain::WatchEvent::Added(serde_json::from_value(value)?)
                )
                .is_err()
            );
        }
        Ok(())
    }
    #[test]
    fn oom_wrong_signal_image_replacement_gap_or_skipped_restart_cannot_qualify() -> Result<()> {
        for case in [
            "oom",
            "signal",
            "image",
            "pod",
            "count",
            "missing_exit",
            "wrong_old",
            "exit",
        ] {
            let mut value = replacement(Some(9), "Error");
            match case {
                "oom" => {
                    value["status"]["containerStatuses"][0]["lastState"]["terminated"]["reason"] =
                        json!("OOMKilled")
                }
                "signal" => {
                    value["status"]["containerStatuses"][0]["lastState"]["terminated"]["signal"] =
                        json!(15)
                }
                "image" => {
                    value["status"]["containerStatuses"][0]["imageID"] = json!("sha256:other")
                }
                "pod" => value["metadata"]["uid"] = json!(Uuid::from_u128(99)),
                "count" => value["status"]["containerStatuses"][0]["restartCount"] = json!(4),
                "missing_exit" => value["status"]["containerStatuses"][0]["lastState"] = json!({}),
                "wrong_old" => {
                    value["status"]["containerStatuses"][0]["lastState"]["terminated"]["containerID"] =
                        json!("containerd://unrelated")
                }
                _ => {
                    value["status"]["containerStatuses"][0]["lastState"]["terminated"]["exitCode"] =
                        json!(0)
                }
            }
            assert!(
                observation()?
                    .observe(drain::WatchEvent::Modified(serde_json::from_value(value)?))
                    .is_err(),
                "{case}"
            );
        }
        assert!(
            observation()?
                .observe(drain::WatchEvent::Error(serde::de::IgnoredAny))
                .is_err()
        );
        assert!(
            observation()?
                .observe(drain::WatchEvent::Deleted(serde_json::from_value(pod())?))
                .is_err()
        );
        Ok(())
    }
    #[tokio::test]
    async fn cancelled_initial_admission_retains_pending_watch_for_awaited_cleanup() -> Result<()> {
        use rmcp::{ClientServiceExt, ServiceExt};
        let (server_io, client_io) = tokio::io::duplex(8192);
        struct Source;
        impl rmcp::ServerHandler for Source {}
        let server = tokio::spawn(async move { Source.serve(server_io).await });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        let server = server.await??;
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("unwanted-late-effect");
        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", "sleep 1; touch \"$1\"", "fixture"])
            .arg(&marker)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let driver = DeploymentRestart {
            context: "fixture".into(),
            namespace: "fixture".into(),
            deployment: "service".into(),
            component: "service".into(),
            caller: client.peer().clone(),
            readiness_uri: veoveo_types::ResourceUri::new("fixture://contract")?,
        };
        let mut owned = Some(CrashWatch {
            driver,
            watch: drain::PodWatch::from_child(crate::spawn_async(command)?)?,
            observation: None,
        });
        let result = tokio::time::timeout(
            Duration::from_millis(50),
            owned
                .as_mut()
                .expect("caller retains watch")
                .admit_initial(selected(), &target()),
        )
        .await;
        assert!(result.is_err());
        assert!(owned.as_ref().unwrap().snapshot().is_none());
        assert!(owned.as_ref().unwrap().identity().is_err());
        assert!(owned.take().unwrap().close().await);
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(!marker.exists());
        tokio::time::timeout(Duration::from_secs(2), client.cancel()).await??;
        tokio::time::timeout(Duration::from_secs(2), server.cancel()).await??;
        Ok(())
    }
    #[tokio::test]
    async fn cancelled_wait_retains_known_termination_and_awaits_owned_watch_cleanup() -> Result<()>
    {
        let mut observation = observation()?;
        let mut value = replacement(Some(9), "Error");
        value["status"]["containerStatuses"][0]["ready"] = json!(false);
        let event = serde_json::to_string(&json!({"type":"MODIFIED","object":value}))?;
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("unwanted-late-effect");
        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", "printf '%s' \"$1\"; sleep 1; touch \"$2\"", "fixture"])
            .arg(event)
            .arg(&marker)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut watch = drain::PodWatch::from_child(crate::spawn_async(command)?)?;
        let outcome = tokio::time::timeout(Duration::from_millis(50), async {
            loop {
                if observation.observe(watch.next().await?)? {
                    return Ok::<(), anyhow::Error>(());
                }
            }
        })
        .await;
        assert!(outcome.is_err());
        assert_eq!(observation.receipt.exit_code, Some(137));
        assert!(!observation.receipt.ready);
        assert!(watch.close().await);
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(!marker.exists());
        Ok(())
    }
}
