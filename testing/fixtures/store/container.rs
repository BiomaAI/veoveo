//! Owned Docker lifecycle for the native Store fixture.
use std::{fmt, path::PathBuf, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;

const IMAGE: &str =
    "surrealdb/surrealdb@sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20";

#[derive(Clone)]
pub(super) struct Docker {
    pub program: PathBuf,
    pub creation_timeout: Duration,
    pub command_timeout: Duration,
    pub creation_settlement_timeout: Duration,
}

impl Default for Docker {
    fn default() -> Self {
        Self {
            program: "docker".into(),
            creation_timeout: Duration::from_secs(90),
            command_timeout: Duration::from_secs(30),
            creation_settlement_timeout: Duration::from_secs(120),
        }
    }
}

#[derive(Debug)]
pub(super) struct Failure {
    pub stage: &'static str,
    container: String,
    reason: String,
    dispatched: bool,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Store fixture {}: {}: {}",
            self.container, self.stage, self.reason
        )
    }
}

impl std::error::Error for Failure {}

pub(super) struct Container {
    name: String,
    docker: Docker,
    creation: Creation,
}

enum Creation {
    Pending,
    Confirmed,
    NotDispatched,
}

impl Container {
    #[allow(
        dead_code,
        reason = "Only write-cost measurements inspect retained storage"
    )]
    pub async fn storage_bytes(&self) -> Result<u64, Failure> {
        let mut inspect = self.command();
        inspect.args(["inspect", "--size", "--format", "{{.SizeRw}}", &self.name]);
        let value = self
            .run(
                inspect,
                "measurement writable-layer size",
                self.docker.command_timeout,
            )
            .await?;
        std::str::from_utf8(&value)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .ok_or_else(|| self.failure("measurement writable-layer size", "expected a byte count"))
    }

    #[allow(
        dead_code,
        reason = "Only Linux write-cost measurements inspect host I/O"
    )]
    pub async fn local_process_id(&self) -> Result<u32, Failure> {
        let mut context = self.command();
        context.args([
            "context",
            "inspect",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ]);
        let endpoint = self
            .run(
                context,
                "measurement Docker context",
                self.docker.command_timeout,
            )
            .await?;
        if !endpoint.starts_with(b"unix://")
            || std::env::var("DOCKER_HOST").is_ok_and(|host| !host.starts_with("unix://"))
        {
            return Err(self.failure("measurement Docker context", "requires a local Unix socket"));
        }
        let mut inspect = self.command();
        inspect.args(["inspect", "--format", "{{.State.Pid}}", &self.name]);
        let value = self
            .run(
                inspect,
                "measurement process identity",
                self.docker.command_timeout,
            )
            .await?;
        std::str::from_utf8(&value)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 0)
            .ok_or_else(|| {
                self.failure("measurement process identity", "expected a running process")
            })
    }

    pub async fn start(
        docker: Docker,
        storage: &'static str,
        password: &str,
    ) -> Result<(Self, String), Failure> {
        // Own cleanup before dispatch. Failed or cancelled creation still drops this guard.
        let mut container = Self {
            name: format!("veoveo-native-store-test-{}", Uuid::now_v7().simple()),
            docker,
            creation: Creation::Pending,
        };
        let mut create = container.command();
        create
            .args([
                "create",
                "--rm",
                "--pull",
                "never",
                "--name",
                &container.name,
                "--runtime",
                "runc",
                "--memory",
                "2g",
                "--cpus",
                "2",
                "--pids-limit",
                "256",
                "--publish",
                "127.0.0.1::8000",
                "--env",
                "SURREAL_USER",
                "--env",
                "SURREAL_PASS",
                "--env",
                "SURREAL_ROCKSDB_BLOCK_CACHE_SIZE=67108864",
                "--env",
                "SURREAL_ROCKSDB_WRITE_BUFFER_SIZE=16777216",
                "--env",
                "SURREAL_ROCKSDB_MAX_WRITE_BUFFER_NUMBER=2",
                IMAGE,
                "start",
                "--log",
                "error",
                storage,
            ])
            .env("SURREAL_USER", "fixture_admin")
            .env("SURREAL_PASS", password);
        if let Err(error) = container
            .run(
                create,
                "container creation",
                container.docker.creation_timeout,
            )
            .await
        {
            if !error.dispatched {
                container.creation = Creation::NotDispatched;
            }
            return Err(error);
        }
        container.creation = Creation::Confirmed;
        let mut start = container.command();
        start.args(["start", &container.name]);
        container
            .run(start, "container startup", container.docker.command_timeout)
            .await?;
        let mut port = container.command();
        port.args(["port", &container.name, "8000/tcp"]);
        let output = container
            .run(port, "published port", container.docker.command_timeout)
            .await?;
        let port = std::str::from_utf8(&output)
            .ok()
            .and_then(|value| value.trim().strip_prefix("127.0.0.1:"))
            .and_then(|value| value.parse::<std::num::NonZeroU16>().ok())
            .ok_or_else(|| container.failure("published port", "expected one loopback TCP port"))?;
        Ok((container, format!("ws://127.0.0.1:{port}")))
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.docker.program);
        command
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command
    }

    fn failure(&self, stage: &'static str, reason: impl Into<String>) -> Failure {
        Failure {
            stage,
            container: self.name.clone(),
            reason: reason.into(),
            dispatched: true,
        }
    }

    async fn run(
        &self,
        mut command: Command,
        stage: &'static str,
        limit: Duration,
    ) -> Result<Vec<u8>, Failure> {
        // Never print command arguments, environment, or child output: all can contain secrets.
        let mut child = command.stdout(Stdio::piped()).spawn().map_err(|error| {
            let mut failure = self.failure(stage, format!("subprocess I/O {:?}", error.kind()));
            failure.dispatched = false;
            failure
        })?;
        let status = match tokio::time::timeout(limit, child.wait()).await {
            Ok(result) => result.map_err(|error| {
                self.failure(stage, format!("subprocess I/O {:?}", error.kind()))
            })?,
            Err(_) => {
                // Wait for SIGKILL/reaping while the cleanup runtime is still alive.
                let reaped = matches!(
                    tokio::time::timeout(Duration::from_secs(2), child.kill()).await,
                    Ok(Ok(()))
                );
                return Err(self.failure(
                    stage,
                    format!("deadline exceeded after {limit:?}; child reaped: {reaped}"),
                ));
            }
        };
        if !status.success() {
            return Err(self.failure(stage, format!("subprocess exit {:?}", status.code())));
        }
        let mut output = Vec::new();
        let mut stdout = child
            .stdout
            .take()
            .expect("fixture stdout is piped")
            .take(4097);
        tokio::time::timeout(Duration::from_secs(2), stdout.read_to_end(&mut output))
            .await
            .map_err(|_| self.failure(stage, "stdout deadline exceeded"))?
            .map_err(|error| self.failure(stage, format!("stdout I/O {:?}", error.kind())))?;
        if output.len() > 4096 {
            return Err(self.failure(stage, "stdout exceeded 4096 bytes"));
        }
        Ok(output)
    }

    async fn cleanup(&self) -> Result<(), Failure> {
        // Killing the CLI does not cancel an accepted daemon request. Wait for the
        // allocated name before removing it; an early `rm --force` reports success
        // for a missing name and can otherwise leave a late-created container behind.
        if matches!(self.creation, Creation::NotDispatched) {
            return Ok(());
        }
        if matches!(self.creation, Creation::Pending) {
            let deadline = tokio::time::Instant::now() + self.docker.creation_settlement_timeout;
            loop {
                let remaining = deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .filter(|duration| !duration.is_zero())
                    .ok_or_else(|| {
                        self.failure(
                            "creation settlement",
                            "outcome remains unknown; cleanup is unconfirmed",
                        )
                    })?;
                let mut inspect = self.command();
                inspect.args([
                    "inspect",
                    "--type",
                    "container",
                    "--format",
                    "{{.Id}}",
                    &self.name,
                ]);
                if self
                    .run(
                        inspect,
                        "creation settlement",
                        remaining.min(self.docker.command_timeout),
                    )
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100).min(remaining)).await;
            }
        }
        let mut command = self.command();
        command.args(["rm", "--force", "--volumes", &self.name]);
        self.run(
            command,
            "container cleanup",
            self.docker.command_timeout.min(Duration::from_secs(10)),
        )
        .await?;
        Ok(())
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        // Drop also runs on a current-thread test runtime. A separate thread avoids
        // nesting a runtime and completes owned cleanup before the test can finish.
        let result = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| {
                            self.failure("cleanup runtime", format!("I/O {:?}", error.kind()))
                        })?;
                    runtime.block_on(self.cleanup())
                })
                .join()
        });
        let failure = match result {
            Ok(Ok(_)) => return,
            Ok(Err(error)) => format!("{error}; inspect this fixture name before further tests"),
            Err(_) => format!("Store fixture {}: cleanup worker panicked", self.name),
        };
        if std::thread::panicking() {
            eprintln!("{failure}");
        } else {
            panic!("{failure}");
        }
    }
}
