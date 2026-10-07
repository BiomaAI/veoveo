use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageInspection {
    id: veoveo_types::Sha256Digest,
    repo_digests: Vec<String>,
    config: ImageConfig,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageConfig {
    labels: std::collections::BTreeMap<String, String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfile {
    gateway_version: String,
    binaries: Vec<ProfileBinary>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBinary {
    name: BinaryName,
    source_tree: String,
    target: String,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum BinaryName {
    #[vocabulary(rename = "openshell")]
    Cli,
    #[vocabulary(rename = "openshell-gateway")]
    Gateway,
    #[vocabulary(rename = "openshell-driver-docker")]
    Driver,
    #[vocabulary(rename = "openshell-supervisor")]
    Supervisor,
    #[vocabulary(rename = "openshell-sandbox")]
    Sandbox,
}
fn admit_supervisor(reference: &str, authority: &str, image: &ImageInspection) -> Result<()> {
    let (repository, digest) = reference
        .split_once("@sha256:")
        .context("digest-pinned supervisor image required")?;
    veoveo_types::Sha256Digest::from_hex(digest)?;
    ensure!(
        repository
            .split_once('/')
            .is_some_and(|(registry, name)| registry == authority && !name.is_empty()),
        "supervisor image must use the Computer installation registry"
    );
    ensure!(
        image.repo_digests.iter().any(|value| value == reference),
        "supervisor image digest is not locally admitted"
    );
    let bytes = include_bytes!("../../../../runtimes/computers/provider-patches/manifest.json");
    let profile: ProviderProfile = serde_json::from_slice(bytes)?;
    let supervisor: Vec<_> = profile
        .binaries
        .iter()
        .filter(|binary| binary.name == BinaryName::Supervisor)
        .collect();
    let sandbox: Vec<_> = profile
        .binaries
        .iter()
        .filter(|binary| binary.name == BinaryName::Sandbox)
        .collect();
    ensure!(
        supervisor.len() == 1 && sandbox.len() == 1,
        "matched supervisor and sandbox source declarations required"
    );
    ensure!(
        supervisor[0].source_tree == sandbox[0].source_tree
            && supervisor[0].target == "x86_64-unknown-linux-gnu"
            && sandbox[0].target == "x86_64-unknown-linux-musl",
        "matched GNU supervisor and static musl sandbox profile required"
    );
    let digest = veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let labels = &image.config.labels;
    ensure!(
        profile.gateway_version == veoveo_computers_runtime::GATEWAY_VERSION
            && labels.get("ai.veoveo.provider.profile") == Some(&profile.gateway_version)
            && labels
                .get("ai.veoveo.provider.manifest-sha256")
                .is_some_and(|value| value == &digest.hex())
            && labels.get("ai.veoveo.provider.supervisor-source-tree")
                == Some(&supervisor[0].source_tree),
        "supervisor image differs from the compiled provider source profile"
    );
    Ok(())
}
pub struct Fixture {
    pub dir: PathBuf,
    pub provider: veoveo_computers_runtime::ProviderInstanceId,
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
        let authority = computer_image
            .split_once('/')
            .context("candidate registry")?
            .0;
        let supervisor_image = std::env::var("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR_IMAGE")
            .context("matched supervisor image required")?;
        let inspected = checked(host().args([
            "image",
            "inspect",
            &supervisor_image,
            "--format",
            "{{json .}}",
        ]))
        .await?;
        let supervisor: ImageInspection = serde_json::from_str(&inspected)?;
        admit_supervisor(&supervisor_image, authority, &supervisor)?;
        let provider = veoveo_computers_runtime::ProviderInstanceId::new();
        let name = format!("veoveo-host-probe-{}", provider.as_uuid().simple());
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
            supervisor_image: &'a str,
            supervisor_image_id: &'a veoveo_types::Sha256Digest,
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
                supervisor_image: &supervisor_image,
                supervisor_image_id: &supervisor.id,
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
            "schema": "veoveo.ai/computer-host/v1", "providerId": provider,
            "namespace": "host-qualification", "defaultImage": computer_image,
            "supervisorImage": supervisor_image, "images": [computer_image, supervisor_image],
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
        // Reproduce Kubernetes' node-wide cgroup view on the replacement. Its
        // bootstrap must scope this before upstream dind enables controllers.
        let cgroup_namespace = if self.generation == 0 {
            "private"
        } else {
            "host"
        };
        command.args([
            "create",
            "--pull",
            "never",
            "--name",
            &self.name,
            "--privileged",
            "--cgroupns",
            cgroup_namespace,
            "--network",
            "bridge",
            "--memory",
            "6g",
            "--cpus",
            "1",
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companion_image_rejects_foreign_digest_and_unmatched_source_before_allocation() {
        let bytes = include_bytes!("../../../../runtimes/computers/provider-patches/manifest.json");
        let profile: ProviderProfile = serde_json::from_slice(bytes).unwrap();
        let tree = profile
            .binaries
            .iter()
            .find(|binary| binary.name == BinaryName::Supervisor)
            .unwrap()
            .source_tree
            .clone();
        let digest = veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        let reference = format!("registry.internal/provider@sha256:{}", "a".repeat(64));
        let mut image = ImageInspection {
            id: veoveo_types::Sha256Digest::from_bytes([0; 32]),
            repo_digests: vec![reference.clone()],
            config: ImageConfig {
                labels: std::collections::BTreeMap::from([
                    ("ai.veoveo.provider.profile".into(), profile.gateway_version),
                    (
                        "ai.veoveo.provider.manifest-sha256".into(),
                        digest.hex().to_owned(),
                    ),
                    ("ai.veoveo.provider.supervisor-source-tree".into(), tree),
                ]),
            },
        };
        admit_supervisor(&reference, "registry.internal", &image).unwrap();
        assert!(admit_supervisor(&reference, "foreign.internal", &image).is_err());
        assert!(
            admit_supervisor(
                "registry.internal/provider:latest",
                "registry.internal",
                &image
            )
            .is_err()
        );
        image.repo_digests.clear();
        assert!(admit_supervisor(&reference, "registry.internal", &image).is_err());
        image.repo_digests.push(reference.clone());
        for name in [
            "ai.veoveo.provider.profile",
            "ai.veoveo.provider.manifest-sha256",
            "ai.veoveo.provider.supervisor-source-tree",
        ] {
            let previous = image
                .config
                .labels
                .insert(name.into(), "unpatched-or-foreign-source".into())
                .unwrap();
            assert!(admit_supervisor(&reference, "registry.internal", &image).is_err());
            image.config.labels.insert(name.into(), previous);
        }
    }
}
