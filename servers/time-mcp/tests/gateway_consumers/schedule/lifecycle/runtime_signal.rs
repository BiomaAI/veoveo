//! Explicit operator admission for one ancestor-namespace containerd signal.
use super::*;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::{process::Command, time::Instant};
use veoveo_deploy_contract::InstallationTarget;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeSignal {
    node_name: String,
    node_container_id: NodeContainerId,
    node_host_pid: NonZeroU32,
    node_host_start_ticks: NonZeroU64,
    task_pid: NonZeroU32,
    host_pid: NonZeroU32,
    host_start_ticks: NonZeroU64,
}
pub struct RuntimeContainerProfile;
impl veoveo_types::IdProfile for RuntimeContainerProfile {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> = veoveo_types::IdProfileSpec::hex(
        veoveo_types::HexGrammar {
            length: 64,
            case: veoveo_types::HexCase::Lower,
            nonzero: true,
        },
        |value, _, _| {
            veoveo_types::IdentifierError::new(
                value,
                "expected a nonzero64hex runtime container identity",
            )
        },
    );
}
#[veoveo_types::id(hex(RuntimeContainerProfile))]
struct NodeContainerId(String);
#[veoveo_types::id(hex(RuntimeContainerProfile))]
struct ContainerdTaskId(String);
impl RuntimeSignal {
    pub(super) fn validate(&self, target: &CrashTarget) -> Result<()> {
        ensure!(
            !self.node_name.is_empty()
                && self.node_name.len() <= 253
                && self.node_name.bytes().all(|b| b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'-' | b'.')),
            "invalid runtime node name"
        );
        task_id(target)?;
        ensure!(
            self.host_pid != self.node_host_pid,
            "runtime ancestor must be a distinct process"
        );
        Ok(())
    }
}
fn task_id(target: &CrashTarget) -> Result<ContainerdTaskId> {
    let value = target
        .container_id
        .strip_prefix("containerd://")
        .context("runtime signal requires selected containerd identity")?;
    Ok(ContainerdTaskId::parse(value)?)
}
pub(super) struct Prepared {
    config: RuntimeSignal,
    target: CrashTarget,
    task: ContainerdTaskId,
    context: String,
    namespace: String,
    dispatched: AtomicBool,
}
#[derive(Deserialize)]
struct Placement {
    metadata: PlacementMetadata,
    spec: PlacementSpec,
}
#[derive(Deserialize)]
struct PlacementMetadata {
    uid: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlacementSpec {
    node_name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Node {
    id: String,
    state: NodeState,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NodeState {
    pid: u32,
    running: bool,
}

async fn output(mut command: Command, end: Instant) -> Result<Vec<u8>> {
    let remaining = end
        .checked_duration_since(Instant::now())
        .filter(|v| !v.is_zero())
        .context("Time signal original deadline expired")?;
    command.kill_on_drop(true);
    let result =
        veoveo_testing_support::output_async(command, remaining.min(Duration::from_secs(10)))
            .await?;
    ensure!(
        Instant::now() < end,
        "Time signal command completed after original deadline"
    );
    ensure!(
        result.status.success(),
        "Time runtime command failed with {}; outcome unresolved",
        result.status
    );
    Ok(result.stdout)
}
impl Prepared {
    pub(super) async fn admit(
        config: RuntimeSignal,
        target: &CrashTarget,
        installation: &InstallationTarget,
        end: Instant,
    ) -> Result<Self> {
        config.validate(target)?;
        let prepared = Self {
            config,
            target: target.clone(),
            task: task_id(target)?,
            context: installation.kubernetes.context.clone(),
            namespace: installation.kubernetes.namespace.clone(),
            dispatched: AtomicBool::new(false),
        };
        prepared.verify(end).await?;
        Ok(prepared)
    }
    pub(super) fn identity(&self) -> RuntimeSignal {
        self.config.clone()
    }
    pub(super) async fn verify(&self, end: Instant) -> Result<()> {
        let mut command = Command::new("kubectl");
        command.args([
            "--context",
            &self.context,
            "--namespace",
            &self.namespace,
            "get",
            "pod",
            &self.target.pod,
            "-o",
            "json",
        ]);
        let pod: Placement = serde_json::from_slice(&output(command, end).await?)
            .map_err(|_| anyhow::anyhow!("invalid Time runtime Pod placement"))?;
        ensure!(
            pod.metadata.uid == self.target.pod_uid && pod.spec.node_name == self.config.node_name,
            "Time runtime Pod/node placement changed"
        );
        let mut command = Command::new("docker");
        command.args([
            "inspect",
            "--format",
            r#"{"Id":{{json .Id}},"State":{"Pid":{{json .State.Pid}},"Running":{{json .State.Running}}}}"#,
            &self.config.node_container_id.0,
        ]);
        let node: Node = serde_json::from_slice(&output(command, end).await?)
            .map_err(|_| anyhow::anyhow!("invalid Time runtime node identity"))?;
        ensure!(
            node.id == self.config.node_container_id.0
                && node.state.running
                && node.state.pid == self.config.node_host_pid.get(),
            "Time runtime node container changed"
        );
        let mut command = Command::new("docker");
        command.args([
            "exec",
            &self.config.node_container_id.0,
            "ctr",
            "-n",
            "k8s.io",
            "tasks",
            "list",
        ]);
        admit_task_list(
            &output(command, end).await?,
            &self.task,
            self.config.task_pid,
        )?;
        self.verify_processes(end).await?;
        ensure!(Instant::now() < end, "Time runtime process fence expired");
        Ok(())
    }
    async fn verify_processes(&self, end: Instant) -> Result<()> {
        let c = &self.config;
        require_start(
            &format!("/proc/{}/stat", c.node_host_pid),
            c.node_host_start_ticks,
        )?;
        require_start(&format!("/proc/{}/stat", c.host_pid), c.host_start_ticks)?;
        // Docker already supplies the admitted ancestor privilege. Do not
        // require root-only host traversal through /proc/<node>/root.
        let stat_path = format!("/proc/{}/stat", c.task_pid);
        let status_path = format!("/proc/{}/status", c.task_pid);
        let mut command = Command::new("docker");
        command.args(["exec", &c.node_container_id.0, "cat", &stat_path]);
        let stat = output(command, end).await?;
        require_start_value(std::str::from_utf8(&stat)?, c.host_start_ticks)?;
        require_nspid(
            &fs::read_to_string(format!("/proc/{}/status", c.host_pid))?,
            &[c.host_pid.get(), c.task_pid.get(), 1],
        )?;
        let mut command = Command::new("docker");
        command.args(["exec", &c.node_container_id.0, "cat", &status_path]);
        let status = output(command, end).await?;
        require_nspid(std::str::from_utf8(&status)?, &[c.task_pid.get(), 1])?;
        Ok(())
    }
    fn claim_dispatch(&self, end: Instant) -> Result<()> {
        ensure!(
            Instant::now() < end,
            "Time signal original deadline expired"
        );
        ensure!(
            !self.dispatched.swap(true, Ordering::SeqCst),
            "Time signal already dispatched or unresolved; never resend"
        );
        Ok(())
    }
    fn command(&self) -> Command {
        let mut command = Command::new("docker");
        command.args([
            "exec",
            &self.config.node_container_id.0,
            "ctr",
            "-n",
            "k8s.io",
            "tasks",
            "kill",
            "--signal",
            "SIGKILL",
            &self.task.0,
        ]);
        command
    }
    pub(super) async fn dispatch(&self, end: Instant) -> Result<()> {
        self.claim_dispatch(end)?;
        owner::check_effect()?;
        output(self.command(), end).await.map(|_| ())
    }
}
fn require_start(path: &str, expected: NonZeroU64) -> Result<()> {
    let value = fs::read_to_string(path)?;
    require_start_value(&value, expected)
}
fn require_start_value(value: &str, expected: NonZeroU64) -> Result<()> {
    let tail = value
        .rsplit_once(") ")
        .context("invalid runtime process stat")?
        .1;
    let ticks: u64 = tail
        .split_whitespace()
        .nth(19)
        .context("runtime process start absent")?
        .parse()?;
    ensure!(
        ticks == expected.get(),
        "Time runtime process start changed"
    );
    Ok(())
}
fn require_nspid(value: &str, expected: &[u32]) -> Result<()> {
    let line = value
        .lines()
        .find_map(|s| s.strip_prefix("NSpid:"))
        .context("runtime process namespace identity absent")?;
    let ids = line
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        ids == expected,
        "Time runtime process namespace identity changed"
    );
    Ok(())
}
fn admit_task_list(bytes: &[u8], task: &ContainerdTaskId, pid: NonZeroU32) -> Result<()> {
    let text = std::str::from_utf8(bytes)?;
    let mut lines = text.lines();
    ensure!(
        lines
            .next()
            .is_some_and(|s| s.split_whitespace().eq(["TASK", "PID", "STATUS"])),
        "unsupported containerd task list profile"
    );
    let rows = lines
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let selected = rows
        .iter()
        .filter(|r| r.first() == Some(&task.0.as_str()))
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1
            && selected[0].len() == 3
            && selected[0][1].parse::<u32>()? == pid.get()
            && selected[0][2] == "RUNNING",
        "Time containerd task identity changed"
    );
    Ok(())
}

/// The supplied read is the actual public caller GET, not a retained journal fact.
pub(super) async fn dispatch_after_current(
    id: &CanonicalTaskId,
    created: &Task,
    end: Instant,
    read: impl std::future::Future<Output = Result<DetailedTask>>,
    mut retain: impl FnMut(&DetailedTask, bool) -> Result<()>,
    dispatch: impl std::future::Future<Output = Result<()>>,
) -> Result<()> {
    ensure!(
        Instant::now() < end,
        "Time signal original deadline expired"
    );
    let current = tokio::time::timeout_at(end, read)
        .await
        .context("Time final Working read deadline")??;
    retain(&current, false)?;
    ensure!(
        Instant::now() < end,
        "Time final Working read completed after original deadline"
    );
    working(id, created, &current)?;
    retain(&current, true)?;
    ensure!(
        Instant::now() < end,
        "Time signal intent exceeded original deadline"
    );
    tokio::time::timeout_at(end, dispatch)
        .await
        .context("Time signal dispatch outcome unresolved")??;
    ensure!(
        Instant::now() < end,
        "Time signal dispatch completed after original deadline; outcome unresolved"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Result<RuntimeSignal> {
        Ok(serde_json::from_value(serde_json::json!({
            "nodeName":"node-fixture","nodeContainerId":"a".repeat(64),
            "nodeHostPid":10,"nodeHostStartTicks":20,"taskPid":30,
            "hostPid":40,"hostStartTicks":50
        }))?)
    }
    fn prepared() -> Result<Prepared> {
        let mut target = super::super::tests::target()?.target;
        target.container_id = format!("containerd://{}", "b".repeat(64));
        Ok(Prepared {
            config: config()?,
            task: task_id(&target)?,
            target,
            context: "fixture".into(),
            namespace: "fixture".into(),
            dispatched: AtomicBool::new(false),
        })
    }
    #[test]
    fn runtime_profile_fences_exact_container_task_and_namespace_process() -> Result<()> {
        let p = prepared()?;
        p.config.validate(&p.target)?;
        assert!(serde_json::from_value::<RuntimeSignal>(serde_json::json!({"nodeName":"n","nodeContainerId":"short","nodeHostPid":1,"nodeHostStartTicks":1,"taskPid":2,"hostPid":3,"hostStartTicks":1})).is_err());
        let list = format!("TASK PID STATUS\n{} 30 RUNNING\n", p.task.0);
        admit_task_list(list.as_bytes(), &p.task, p.config.task_pid)?;
        for list in [
            format!("TASK PID STATUS\n{} 31 RUNNING\n", p.task.0),
            format!("TASK PID STATUS\n{} 30 STOPPED\n", p.task.0),
            format!(
                "TASK PID STATUS\n{} 30 RUNNING\n{} 30 RUNNING\n",
                p.task.0, p.task.0
            ),
        ] {
            assert!(admit_task_list(list.as_bytes(), &p.task, p.config.task_pid).is_err());
        }
        require_nspid("NSpid:\t40 30 1\n", &[40, 30, 1])?;
        assert!(require_nspid("NSpid:\t40 31 1\n", &[40, 30, 1]).is_err());
        let root = tempfile::tempdir()?;
        let path = root.path().join("stat");
        let mut fields = vec!["0"; 20];
        fields[19] = "50";
        fs::write(&path, format!("40 (time mcp) {}", fields.join(" ")))?;
        require_start(path.to_str().unwrap(), p.config.host_start_ticks)?;
        assert!(require_start(path.to_str().unwrap(), NonZeroU64::new(51).unwrap()).is_err());
        let args = p
            .command()
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "exec",
                &"a".repeat(64),
                "ctr",
                "-n",
                "k8s.io",
                "tasks",
                "kill",
                "--signal",
                "SIGKILL",
                &"b".repeat(64)
            ]
        );
        Ok(())
    }
    #[tokio::test]
    async fn fresh_working_read_precedes_persisted_once_only_signal() -> Result<()> {
        let p = prepared()?;
        let id = CanonicalTaskId::parse("time.fixture-original")?;
        let created = Task::new(
            id.to_string(),
            TaskStatus::Working,
            "2026-10-10T00:00:00Z",
            "2026-10-10T00:00:00Z",
        );
        let current = DetailedTask::new(created.clone(), TaskPayload::Working);
        let log = std::sync::Mutex::new(Vec::new());
        let input = super::super::super::tests::fixture()?;
        let mut journal = Journal::new(&input);
        let root = tempfile::tempdir()?;
        let path = root.path().join("signal-receipt.jsonl");
        let mut file = open_receipt(&path)?;
        let end = Instant::now() + Duration::from_secs(1);
        let result = dispatch_after_current(
            &id,
            &created,
            end,
            async {
                log.lock().unwrap().push("fresh_read");
                Ok(current)
            },
            |current, intent| {
                super::super::retain_signal_checkpoint(&mut journal, &mut file, current, intent)?;
                log.lock().unwrap().push(if intent {
                    "persist_intent"
                } else {
                    "persist_observation"
                });
                Ok(())
            },
            async {
                p.claim_dispatch(end)?;
                log.lock().unwrap().push("signal");
                anyhow::bail!("uncertain command completion")
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "fresh_read",
                "persist_observation",
                "persist_intent",
                "signal"
            ]
        );
        assert!(p.claim_dispatch(end).is_err());
        let retained: serde_json::Value =
            serde_json::from_str(fs::read_to_string(&path)?.lines().last().unwrap())?;
        assert_eq!(retained["lifecycle"]["signalDispatchIntent"], true);
        // A reconstructed command owner must not bypass the persisted journal
        // intent after ambiguous completion of the original dispatch.
        let second = prepared()?;
        let current = DetailedTask::new(created.clone(), TaskPayload::Working);
        let retried = dispatch_after_current(
            &id,
            &created,
            end,
            async { Ok(current) },
            |current, intent| {
                super::super::retain_signal_checkpoint(&mut journal, &mut file, current, intent)
            },
            async { second.claim_dispatch(end) },
        )
        .await;
        assert!(retried.is_err());
        assert!(!second.dispatched.load(Ordering::SeqCst));

        for mismatch in 0..4 {
            let p = prepared()?;
            let mut task = created.clone();
            if mismatch == 0 {
                task.status = TaskStatus::Completed;
            }
            if mismatch == 1 {
                task.task_id = "time.foreign".into();
            }
            if mismatch == 2 {
                task.created_at = "2026-10-10T00:00:01Z".into();
            }
            let current = DetailedTask::new(
                task,
                if mismatch == 0 {
                    TaskPayload::Cancelled
                } else {
                    TaskPayload::Working
                },
            );
            let end = if mismatch == 3 {
                Instant::now() - Duration::from_millis(1)
            } else {
                end
            };
            let result = dispatch_after_current(
                &id,
                &created,
                end,
                async { Ok(current) },
                |_, _| Ok(()),
                async { p.claim_dispatch(end) },
            )
            .await;
            assert!(result.is_err());
            assert!(!p.dispatched.load(Ordering::SeqCst));
        }
        Ok(())
    }
    #[tokio::test(flavor = "current_thread")]
    async fn expired_late_working_read_cannot_dispatch() -> Result<()> {
        let p = prepared()?;
        let id = CanonicalTaskId::parse("time.fixture-original")?;
        let created = Task::new(
            id.to_string(),
            TaskStatus::Working,
            "2026-10-10T00:00:00Z",
            "2026-10-10T00:00:00Z",
        );
        let current = DetailedTask::new(created.clone(), TaskPayload::Working);
        let end = Instant::now() + Duration::from_millis(1);
        let result = dispatch_after_current(
            &id,
            &created,
            end,
            async {
                std::thread::sleep(Duration::from_millis(20));
                Ok(current)
            },
            |_, _| Ok(()),
            async { p.claim_dispatch(end) },
        )
        .await;
        assert!(result.is_err());
        assert!(!p.dispatched.load(Ordering::SeqCst));
        Ok(())
    }
}
