use anyhow::{Context, Result, ensure};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command as SyncCommand, Stdio},
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;
use veoveo_computers_runtime::{
    AllocationConfig, DevelopmentTemplate, GatewayConfig, HomeAllocator, OpenShellRuntime,
};
const HOST: &str = "unix:///var/run/docker.sock";
const TEST: &str = "composite_host_replaces_its_namespace_and_retains_the_computer";
pub fn host() -> Command {
    let mut command = Command::new("docker");
    command.args(["--host", HOST]).kill_on_drop(true);
    command
}
pub async fn checked(command: &mut Command) -> Result<String> {
    let output = tokio::time::timeout(Duration::from_secs(55), command.output())
        .await
        .context("owned fixture command deadline")??;
    ensure!(
        output.status.success(),
        "fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn bridge() -> (String, String) {
    (
        fs::read_to_string("/sys/class/net/docker0/ifindex").unwrap(),
        fs::read_to_string("/sys/class/net/docker0/address").unwrap(),
    )
}
pub struct Fixture {
    pub dir: PathBuf,
    pub provider: Uuid,
    pub endpoint: String,
    storage_endpoint: String,
    image: String,
    replacement_image: String,
    name: String,
    bridge: (String, String),
    generation: u8,
    finished: bool,
}
impl Fixture {
    pub async fn start(template: &DevelopmentTemplate, computer_image: &str) -> Result<Self> {
        let image =
            std::env::var("VEOVEO_COMPUTERS_HOST_IMAGE").context("candidate compute host image")?;
        let image =
            checked(host().args(["image", "inspect", &image, "--format", "{{.Id}}"])).await?;
        ensure!(
            image.starts_with("sha256:") && image.len() == 71,
            "immutable host image identity"
        );
        let replacement = std::env::var("VEOVEO_COMPUTERS_HOST_REPLACEMENT_IMAGE")
            .context("replacement compute host image")?;
        let replacement_image =
            checked(host().args(["image", "inspect", &replacement, "--format", "{{.Id}}"])).await?;
        ensure!(
            replacement_image.starts_with("sha256:") && replacement_image.len() == 71,
            "immutable replacement host image identity"
        );
        ensure!(
            replacement_image != image,
            "host upgrade must change image identity"
        );
        let provider = Uuid::now_v7();
        let name = format!("veoveo-host-probe-{}", provider.simple());
        let dir = PathBuf::from(
            std::env::var_os("VEOVEO_COMPUTERS_NATIVE_OUTPUT").context("owned diagnostic root")?,
        )
        .join(&name);
        fs::create_dir(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Inputs<'a> {
            source_host_image_id: &'a str,
            target_host_image_id: &'a str,
            template_image: &'a str,
            template_fingerprint: String,
            home_capacity_bytes: u64,
        }
        // Preserve the public input tuple after owned trust/data cleanup. The
        // image's registry must be reachable from the private bridge namespace;
        // the host's loopback publication port is not a bridge endpoint.
        fs::write(
            dir.join("inputs.json"),
            serde_json::to_vec_pretty(&Inputs {
                source_host_image_id: &image,
                target_host_image_id: &replacement_image,
                template_image: computer_image,
                template_fingerprint: template.fingerprint(),
                home_capacity_bytes: 536870912,
            })?,
        )?;
        let mut fixture = Self {
            dir,
            provider,
            endpoint: String::new(),
            storage_endpoint: String::new(),
            image,
            replacement_image,
            name,
            bridge: bridge(),
            generation: 0,
            finished: false,
        };
        super::trust::create(&fixture.dir);
        for name in ["data", "config"] {
            fs::create_dir(fixture.dir.join(name))?;
        }
        let authority = computer_image
            .split_once('/')
            .context("candidate registry")?
            .0;
        let config = serde_json::json!({
            "schema": "veoveo.io/computer-host/v1", "providerId": provider,
            "namespace": "host-qualification", "defaultImage": computer_image, "images": [computer_image],
            "templates": [{"fingerprint": template.fingerprint(), "capacityBytes": 536870912}],
            "reserveBytes": 536870912,
            "registry": {"authority": authority, "transport": "development_http"},
            "bridgeAddress": "172.30.0.1", "networkPool": "172.31.0.0"
        });
        fs::write(
            fixture.dir.join("config/host.json"),
            serde_json::to_vec(&config)?,
        )?;
        fs::set_permissions(
            fixture.dir.join("config/host.json"),
            fs::Permissions::from_mode(0o644),
        )?;
        checked(host().args([
            "run",
            "--rm",
            "--network",
            "none",
            "--mount",
            &format!("type=bind,source={},target=/fixture", fixture.dir.display()),
            "--entrypoint",
            "/bin/chown",
            &fixture.image,
            "-R",
            "0:0",
            "/fixture/config",
            "/fixture/trust",
        ]))
        .await?;
        fixture.launch().await?;
        println!("Composite host diagnostics: {}", fixture.dir.display());
        Ok(fixture)
    }
    async fn launch(&mut self) -> Result<()> {
        let mut command = host();
        command.args([
            "create",
            "--pull",
            "never",
            "--name",
            &self.name,
            "--privileged",
            "--network",
            "bridge",
            "--memory",
            "6g",
            "--cpus",
            "4",
            "--pids-limit",
            "1024",
            "--tmpfs",
            "/tmp:rw,exec,mode=1777",
            "--tmpfs",
            "/run:rw,exec,mode=755",
            "--publish",
            "127.0.0.1::8805",
            "--publish",
            "127.0.0.1::8806",
        ]);
        for (name, path, options) in [
            ("data", "/var/lib/veoveo-computers", ""),
            ("config", "/etc/veoveo/computers/host-config", ",readonly"),
            ("trust", "/etc/veoveo/computers/host-trust", ",readonly"),
        ] {
            command.arg("--mount").arg(format!(
                "type=bind,source={},target={path}{options}",
                self.dir.join(name).display()
            ));
        }
        command.arg("--mount").arg(format!(
            "type=bind,source={},target=/probe,readonly",
            std::env::current_exe()?.display()
        ));
        command.arg(&self.image);
        checked(&mut command).await?;
        ensure!(
            checked(host().args([
                "inspect",
                "--format",
                "{{.HostConfig.NetworkMode}}",
                &self.name
            ]))
            .await?
                == "bridge",
            "private network required"
        );
        checked(host().args(["start", &self.name])).await?;
        self.endpoint = self.port("8805").await?;
        self.storage_endpoint = self.port("8806").await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let mut command = host();
            command.args([
                "exec",
                &self.name,
                "/usr/local/bin/veoveo-computer-host",
                "health",
            ]);
            if checked(&mut command).await.is_ok() {
                break;
            }
            ensure!(
                checked(host().args(["inspect", "--format", "{{.State.Running}}", &self.name]))
                    .await?
                    == "true",
                "compute host exited; declared inputs and diagnostics at {}",
                self.dir.display()
            );
            ensure!(
                tokio::time::Instant::now() < deadline,
                "compute host readiness deadline"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure!(
            bridge() == self.bridge,
            "compute host changed installation bridge"
        );
        Ok(())
    }
    async fn port(&self, port: &str) -> Result<String> {
        let address = checked(host().args(["port", &self.name, port])).await?;
        ensure!(
            address.starts_with("127.0.0.1:")
                && address.parse::<std::net::SocketAddr>()?.port() != 0,
            "loopback-only test endpoint"
        );
        Ok(address)
    }
    pub async fn engine(&self) -> Result<String> {
        checked(host().args([
            "exec",
            &self.name,
            "docker",
            "--host",
            "unix:///run/veoveo-computers/docker.sock",
            "info",
            "--format",
            "{{.ID}}",
        ]))
        .await
    }
    pub async fn runtime(&self) -> Result<OpenShellRuntime> {
        self.runtime_in("default").await
    }
    pub async fn runtime_in(&self, workspace: &str) -> Result<OpenShellRuntime> {
        let dir = self.dir.join("provider");
        let config = GatewayConfig::new(
            self.provider,
            self.endpoint.clone(),
            workspace.into(),
            dir.join("ca.pem"),
            dir.join("client.pem"),
            dir.join("client-key.pem"),
        )?;
        Ok(OpenShellRuntime::connect(config).await?)
    }
    pub async fn allocator(&self, template: &DevelopmentTemplate) -> Result<HomeAllocator> {
        let dir = self.dir.join("storage");
        let config = AllocationConfig::new(
            self.storage_endpoint.clone(),
            dir.join("ca.pem"),
            dir.join("client.pem"),
            dir.join("client-key.pem"),
        )?;
        Ok(HomeAllocator::new(config, self.provider, template.fingerprint(), 536870912).await?)
    }
    pub async fn fault(&self, mode: &str, computer: Uuid) -> Result<()> {
        checked(host().args([
            "exec",
            "--env",
            &format!("VEOVEO_HOST_PROBE_FAULT={mode}"),
            "--env",
            &format!("VEOVEO_HOST_PROBE_COMPUTER={computer}"),
            &self.name,
            "/probe",
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
        ]))
        .await?;
        Ok(())
    }
    fn logs(&self) {
        if let Ok(diagnostics) = SyncCommand::new("timeout")
            .args([
                "5",
                "docker",
                "--host",
                HOST,
                "exec",
                &self.name,
                "find",
                "/var/lib/veoveo-computers/state/retained/homes",
                "-maxdepth",
                "3",
                "-ls",
            ])
            .output()
        {
            let _ = fs::write(
                self.dir.join(format!("storage-{}.log", self.generation)),
                [diagnostics.stdout, diagnostics.stderr].concat(),
            );
        }
        if let Ok(logs) = SyncCommand::new("timeout")
            .args(["5", "docker", "--host", HOST, "logs", &self.name])
            .output()
        {
            let _ = fs::write(
                self.dir.join(format!("host-{}.log", self.generation)),
                [logs.stdout, logs.stderr].concat(),
            );
        }
    }
    pub async fn replace(&mut self) -> Result<()> {
        let source = self.image.clone();
        let started = std::time::Instant::now();
        checked(host().args(["stop", "--time", "40", &self.name])).await?;
        self.logs();
        checked(host().args(["rm", &self.name])).await?;
        self.generation += 1;
        self.image = self.replacement_image.clone();
        self.launch().await?;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Upgrade<'a> {
            source_image_id: &'a str,
            target_image_id: &'a str,
            elapsed_millis: u128,
        }
        let evidence = Upgrade {
            source_image_id: &source,
            target_image_id: &self.image,
            elapsed_millis: started.elapsed().as_millis(),
        };
        fs::write(
            self.dir.join("host-upgrade.json"),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
        eprintln!(
            "Native host image upgrade: {} ms",
            started.elapsed().as_millis()
        );
        Ok(())
    }
    fn cleanup(&self) -> bool {
        self.logs();
        let removed = SyncCommand::new("timeout")
            .args(["45", "docker", "--host", HOST, "rm", "--force", &self.name])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    || String::from_utf8_lossy(&output.stderr).contains("No such container")
            });
        if !removed {
            return false;
        }
        let output = SyncCommand::new("timeout")
            .args([
                "45",
                "docker",
                "--host",
                HOST,
                "run",
                "--rm",
                "--privileged",
                "--network",
                "none",
                "--mount",
            ])
            .arg(format!(
                "type=bind,source={},target=/fixture",
                self.dir.display()
            ))
            .arg("--mount")
            .arg(format!(
                "type=bind,source={},target=/probe,readonly",
                std::env::current_exe().unwrap().display()
            ))
            .args([
                "--env",
                "VEOVEO_HOST_PROBE_CLEANUP=1",
                "--entrypoint",
                "/probe",
                &self.image,
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
            ])
            .stdin(Stdio::null())
            .output();
        if let Ok(output) = output {
            let _ = fs::write(
                self.dir.join("cleanup.log"),
                [output.stdout, output.stderr].concat(),
            );
            output.status.success() && bridge() == self.bridge
        } else {
            false
        }
    }
    pub async fn finish(mut self) -> Result<()> {
        checked(host().args(["stop", "--time", "40", &self.name])).await?;
        ensure!(
            self.cleanup(),
            "owned compute host cleanup failed; data preserved for recovery"
        );
        self.finished = true;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.finished && !self.cleanup() {
            eprintln!(
                "compute host cleanup requires recovery at {}",
                self.dir.display()
            );
        }
    }
}
