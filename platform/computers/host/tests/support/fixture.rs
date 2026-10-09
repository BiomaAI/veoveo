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
    WorkerOAuthConfig, WorkerOAuthFields, WorkerTokenAuthentication,
};
#[path = "../../../../runtimes/computers/tests/support/worker_issuer.rs"]
mod worker_issuer;
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
async fn issuer_bridge_address() -> Result<std::net::Ipv4Addr> {
    #[derive(serde::Deserialize)]
    struct BridgeAddress {
        #[serde(rename = "Gateway")]
        gateway: Option<std::net::Ipv4Addr>,
    }
    let body = checked(host().args([
        "network",
        "inspect",
        "bridge",
        "--format",
        "{{json .IPAM.Config}}",
    ]))
    .await?;
    let addresses: Vec<BridgeAddress> =
        serde_json::from_str(&body).context("decode inspected fixture bridge")?;
    ensure!(
        addresses.len() == 1,
        "one inspected IPv4 fixture bridge required"
    );
    let address = addresses[0]
        .gateway
        .context("inspected fixture bridge gateway absent")?;
    ensure!(
        address.is_private() && !address.is_loopback(),
        "private fixture bridge gateway required"
    );
    Ok(address)
}
#[path = "../../src/images.rs"]
pub(crate) mod images;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// One explicitly selected development registry reachable by the private Host.
struct PullRegistry {
    url: reqwest::Url,
    authority: String,
}
impl PullRegistry {
    fn parse(authority: &str) -> Result<Self> {
        ensure!(
            !authority.is_empty()
                && authority.len() <= 253
                && authority
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-:".contains(&b)),
            "Host pull registry requires one DNS/IPv4 authority"
        );
        let url = reqwest::Url::parse(&format!("http://{authority}/"))
            .map_err(|_| anyhow::anyhow!("invalid Host pull registry"))?;
        ensure!(
            url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && url.origin().ascii_serialization() == format!("http://{authority}"),
            "invalid Host pull registry authority"
        );
        Ok(Self {
            url,
            authority: authority.to_owned(),
        })
    }
    fn authority(&self) -> &str {
        &self.authority
    }
}

struct MappedImage {
    local: String,
    pull: String,
    manifest: reqwest::Url,
    digest: veoveo_types::Sha256Digest,
    local_authority: String,
}
impl MappedImage {
    fn new(local: String, registry: &PullRegistry) -> Result<Self> {
        let (repository, digest) = local
            .split_once("@sha256:")
            .context("Host fixture requires an immutable image reference")?;
        let digest = veoveo_types::Sha256Digest::from_hex(digest)?;
        let (authority, name) = repository
            .split_once('/')
            .context("explicit local image registry required")?;
        let local_registry = PullRegistry::parse(authority)?;
        ensure!(
            !name.is_empty()
                && repository.len() <= 512
                && name.split('/').all(|part| !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part.bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b"._-".contains(&b))),
            "invalid Host fixture image repository"
        );
        let local_authority = local_registry.authority().to_owned();
        let mut pull = local_registry.url;
        pull.set_host(registry.url.host_str())?;
        pull.set_port(registry.url.port())
            .map_err(|_| anyhow::anyhow!("invalid pull registry port"))?;
        {
            let mut path = pull
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("image path unavailable"))?;
            path.clear().extend(name.split('/'));
        }
        // The image reference is serialized at the Docker boundary. URL owns the
        // authority replacement and repository component encoding.
        let pull = format!(
            "{}@sha256:{}",
            pull.as_str()
                .strip_prefix("http://")
                .context("image reference serialization failed")?,
            digest.hex()
        );
        let mut manifest = registry.url.clone();
        manifest
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("manifest path unavailable"))?
            .clear()
            .push("v2")
            .extend(name.split('/'))
            .push("manifests")
            .push(&format!("sha256:{}", digest.hex()));
        Ok(Self {
            local,
            pull,
            manifest,
            digest,
            local_authority,
        })
    }
    async fn admit_remote(
        &self,
        client: &reqwest::Client,
        id: &veoveo_types::Sha256Digest,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut response = client.get(self.manifest.clone())
                .header(reqwest::header::ACCEPT, "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json")
                .send().await.map_err(|_| anyhow::anyhow!("Host pull manifest transport failed"))?;
            ensure!(response.status() == reqwest::StatusCode::OK, "Host pull manifest unavailable");
            let header = response.headers().get("Docker-Content-Digest")
                .and_then(|v| v.to_str().ok()).map(str::to_owned);
            ensure!(response.content_length().is_none_or(|n| n <= MAX_MANIFEST_BYTES as u64), "Host pull manifest exceeds 64 KiB");
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| anyhow::anyhow!("Host pull manifest read failed"))? {
                ensure!(chunk.len() <= MAX_MANIFEST_BYTES - bytes.len(), "Host pull manifest exceeds 64 KiB");
                bytes.extend_from_slice(&chunk);
            }
            self.admit_response(response.status(), header.as_deref(), &bytes, id)
        }).await.context("Host pull manifest admission deadline")?
    }
    fn admit_response(
        &self,
        status: reqwest::StatusCode,
        header: Option<&str>,
        bytes: &[u8],
        id: &veoveo_types::Sha256Digest,
    ) -> Result<()> {
        ensure!(
            status == reqwest::StatusCode::OK,
            "Host pull manifest unavailable"
        );
        self.admit_manifest(header, bytes, id)
    }
    fn admit_manifest(
        &self,
        header: Option<&str>,
        bytes: &[u8],
        id: &veoveo_types::Sha256Digest,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES,
            "Host pull manifest exceeds 64 KiB"
        );
        ensure!(
            header == Some(format!("sha256:{}", self.digest.hex()).as_str())
                && veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into())
                    == self.digest,
            "Host pull manifest identity mismatch"
        );
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Manifest {
            schema_version: u32,
            media_type: String,
            config: Descriptor,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Descriptor {
            media_type: String,
            digest: veoveo_types::Sha256Digest,
        }
        let manifest: Manifest = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("invalid Host pull manifest"))?;
        ensure!(
            manifest.schema_version == 2
                && matches!(
                    manifest.media_type.as_str(),
                    "application/vnd.oci.image.manifest.v1+json"
                        | "application/vnd.docker.distribution.manifest.v2+json"
                )
                && matches!(
                    manifest.config.media_type.as_str(),
                    "application/vnd.oci.image.config.v1+json"
                        | "application/vnd.docker.container.image.v1+json"
                )
                && &manifest.config.digest == id,
            "Host pull manifest must match the admitted runnable image"
        );
        Ok(())
    }
}

fn manifest_client() -> Result<reqwest::Client> {
    worker_issuer::initialize_tls();
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()?)
}
/// Admitted local images and their private-Host pull references. Construction
/// completes all reads before any fixture directory or container is created.
pub struct FixtureImages {
    registry: PullRegistry,
    template: MappedImage,
    supervisor: MappedImage,
    template_id: veoveo_types::Sha256Digest,
    supervisor_id: veoveo_types::Sha256Digest,
}
impl FixtureImages {
    pub fn template_image(&self) -> &str {
        &self.template.pull
    }
    pub async fn admit() -> Result<Self> {
        let client = manifest_client()?;
        let registry = PullRegistry::parse(
            &std::env::var("VEOVEO_COMPUTERS_HOST_PULL_REGISTRY")
                .context("explicit bridge-reachable Host pull registry required")?,
        )?;
        let template = MappedImage::new(
            std::env::var("VEOVEO_COMPUTERS_HOST_TEST_IMAGE")?,
            &registry,
        )?;
        let supervisor = MappedImage::new(
            std::env::var("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR_IMAGE")?,
            &registry,
        )?;
        ensure!(
            template.local_authority == supervisor.local_authority,
            "Host fixture local images require one registry"
        );
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct LocalImage {
            id: veoveo_types::Sha256Digest,
            repo_digests: Vec<String>,
        }
        let inspected = checked(host().args([
            "image",
            "inspect",
            &template.local,
            "--format",
            "{{json .}}",
        ]))
        .await?;
        ensure!(
            inspected.len() <= MAX_MANIFEST_BYTES,
            "local template inspection exceeds 64 KiB"
        );
        let local: LocalImage = serde_json::from_str(&inspected)
            .map_err(|_| anyhow::anyhow!("invalid local template metadata"))?;
        ensure!(
            local.repo_digests.contains(&template.local),
            "template digest is not locally admitted"
        );
        let inspected = checked(host().args([
            "image",
            "inspect",
            &supervisor.local,
            "--format",
            "{{json .}}",
        ]))
        .await?;
        let admitted = images::decode(inspected.as_bytes())?;
        let supervisor_id = admitted.admit(&supervisor.local, &supervisor.local_authority)?;
        template.admit_remote(&client, &local.id).await?;
        supervisor.admit_remote(&client, &supervisor_id).await?;
        Ok(Self {
            registry,
            template,
            supervisor,
            template_id: local.id,
            supervisor_id,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HostReplacementProfile {
    Restart,
    ImageUpgrade,
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
    authentication: worker_issuer::TestIssuer,
    profile: HostReplacementProfile,
}
impl Fixture {
    pub async fn start(
        template: &DevelopmentTemplate,
        selected: &FixtureImages,
        profile: HostReplacementProfile,
    ) -> Result<Self> {
        let computer_image = selected.template_image();
        ensure!(
            template.image() == computer_image,
            "Host template must use the admitted pull image"
        );
        let image =
            std::env::var("VEOVEO_COMPUTERS_HOST_IMAGE").context("candidate compute host image")?;
        let image =
            checked(host().args(["image", "inspect", &image, "--format", "{{.Id}}"])).await?;
        ensure!(
            image.starts_with("sha256:") && image.len() == 71,
            "immutable host image identity"
        );
        let replacement_image = match profile {
            HostReplacementProfile::Restart => image.clone(),
            HostReplacementProfile::ImageUpgrade => {
                let replacement = std::env::var("VEOVEO_COMPUTERS_HOST_REPLACEMENT_IMAGE")
                    .context("replacement compute host image")?;
                checked(host().args(["image", "inspect", &replacement, "--format", "{{.Id}}"]))
                    .await?
            }
        };
        ensure!(
            replacement_image.starts_with("sha256:") && replacement_image.len() == 71,
            "immutable replacement host image identity"
        );
        ensure!(
            (profile == HostReplacementProfile::Restart && replacement_image == image)
                || (profile == HostReplacementProfile::ImageUpgrade && replacement_image != image),
            "Host replacement image must match the explicit restart or image-upgrade profile"
        );
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
            local_template_image: &'a str,
            local_supervisor_image: &'a str,
            template_image_id: &'a veoveo_types::Sha256Digest,
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
                supervisor_image: &selected.supervisor.pull,
                supervisor_image_id: &selected.supervisor_id,
                local_template_image: &selected.template.local,
                local_supervisor_image: &selected.supervisor.local,
                template_image_id: &selected.template_id,
                template_fingerprint: template.fingerprint(),
                home_capacity_bytes: 536870912,
            })?,
        )?;
        super::trust::create(&dir);
        let issuer_address = issuer_bridge_address().await?;
        let authentication =
            worker_issuer::TestIssuer::start_on(&dir.join("issuer"), issuer_address).await;
        fs::copy(dir.join("issuer/ca.pem"), dir.join("trust/issuer-ca.pem"))?;
        fs::set_permissions(
            dir.join("trust/issuer-ca.pem"),
            fs::Permissions::from_mode(0o644),
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
            authentication,
            profile,
        };
        for name in ["data", "config"] {
            fs::create_dir(fixture.dir.join(name))?;
        }
        let authority = selected.registry.authority();
        let supervisor_image = &selected.supervisor.pull;
        let authentication = &fixture.authentication.config;
        let config = serde_json::json!({
            "schema": "veoveo.ai/computer-host/v1", "providerId": provider,
            "namespace": "host-qualification", "defaultImage": computer_image,
            "supervisorImage": supervisor_image, "images": [computer_image, supervisor_image],
            "providerAuthentication": {"issuer": authentication.issuer(), "audience": authentication.audience(),
                "rolesClaim": "roles", "adminRole": "openshell-admin", "userRole": "openshell-user", "jwksTtlSecs": 300},
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
            "--env",
            "SSL_CERT_FILE=/etc/veoveo/computers/host-trust/issuer-ca.pem",
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
            self.authentication.config.clone(),
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
    pub async fn assert_provider_security(&self) -> Result<()> {
        use worker_issuer::WorkerTokenProbe;
        let authorized = self
            .authentication
            .probe_token(WorkerTokenProbe::Authorized);
        let wrong_signature = self
            .authentication
            .probe_token(WorkerTokenProbe::WrongSignature);
        let wrong_issuer = self
            .authentication
            .probe_token(WorkerTokenProbe::WrongIssuer);
        let wrong_audience = self
            .authentication
            .probe_token(WorkerTokenProbe::WrongAudience);
        let expired = self.authentication.probe_token(WorkerTokenProbe::Expired);
        let unauthorized_roles = self
            .authentication
            .probe_token(WorkerTokenProbe::UnauthorizedRoles);
        super::guest_authority::assert_provider_security(
            &self.dir.join("provider"),
            &self.endpoint,
            super::guest_authority::ProviderProbeTokens {
                authorized: &authorized,
                wrong_signature: &wrong_signature,
                wrong_issuer: &wrong_issuer,
                wrong_audience: &wrong_audience,
                expired: &expired,
                unauthorized_roles: &unauthorized_roles,
            },
        )
        .await;
        Ok(())
    }
    /// Inspect the selected template without a volume or provider process.
    pub async fn assert_template_home_absent(&self, images: &FixtureImages) -> Result<()> {
        let name = format!("veoveo-template-probe-{}", Uuid::now_v7().simple());
        let result = checked(host().args([
            "exec",
            &self.name,
            "docker",
            "--host",
            "unix:///run/veoveo-computers/docker.sock",
            "run",
            "--rm",
            "--pull",
            "never",
            "--name",
            &name,
            "--network",
            "none",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--entrypoint",
            "/bin/sh",
            images.template_image(),
            "-c",
            "test ! -e /sandbox/persistent && test ! -L /sandbox/persistent",
        ]))
        .await;
        if result.is_err() {
            let _ = checked(host().args([
                "exec",
                &self.name,
                "docker",
                "--host",
                "unix:///run/veoveo-computers/docker.sock",
                "rm",
                "--force",
                &name,
            ]))
            .await;
        }
        result.context("selected template must leave retained target absent")?;
        Ok(())
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
            profile: &'a str,
            source_image_id: &'a str,
            target_image_id: &'a str,
            elapsed_millis: u128,
        }
        let evidence = Upgrade {
            profile: match self.profile {
                HostReplacementProfile::Restart => "container_restart",
                HostReplacementProfile::ImageUpgrade => "image_upgrade",
            },
            source_image_id: &source,
            target_image_id: &self.image,
            elapsed_millis: started.elapsed().as_millis(),
        };
        fs::write(
            self.dir.join(match self.profile {
                HostReplacementProfile::Restart => "host-restart.json",
                HostReplacementProfile::ImageUpgrade => "host-upgrade.json",
            }),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
        eprintln!(
            "Native Host {}: {} ms",
            evidence.profile,
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
        self.authentication.shutdown().await;
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
mod registry_admission_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn native_manifest_client_initializes_tls_before_network_or_fixture_effects() {
        manifest_client().expect("native manifest client constructs in a fresh process");
    }
    fn selected(media: &str) -> (MappedImage, Vec<u8>, veoveo_types::Sha256Digest) {
        let id = veoveo_types::Sha256Digest::from_hex("b".repeat(64)).unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2, "mediaType": media,
            "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": id},
            "layers": []
        }))
        .unwrap();
        let digest = veoveo_types::Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        let registry = PullRegistry::parse("172.17.0.2:5000").unwrap();
        let image = MappedImage::new(
            format!("127.0.0.1:5001/team/computer@sha256:{}", digest.hex()),
            &registry,
        )
        .unwrap();
        (image, bytes, id)
    }
    #[test]
    fn pull_mapping_preserves_repository_and_manifest_and_refuses_uncontrolled_addresses() {
        let (image, _, _) = selected("application/vnd.oci.image.manifest.v1+json");
        assert_eq!(
            image.pull,
            image.local.replacen("127.0.0.1:5001", "172.17.0.2:5000", 1)
        );
        assert_eq!(image.manifest.host_str(), Some("172.17.0.2"));
        assert_eq!(image.manifest.port(), Some(5000));
        assert_eq!(
            image.manifest.path(),
            format!("/v2/team/computer/manifests/sha256:{}", image.digest.hex())
        );
        for bad in [
            "http://172.17.0.2:5000",
            "user@172.17.0.2:5000",
            "172.17.0.2:5000/?token=secret",
            "172.17.0.2:5000#secret",
            "[::1]:5000",
            "127.1:5000",
        ] {
            assert!(PullRegistry::parse(bad).is_err());
        }
        let registry = PullRegistry::parse("172.17.0.2:5000").unwrap();
        for bad in [
            "computer:latest".to_owned(),
            "127.0.0.1:5001/team/../computer@sha256:".to_owned() + &"a".repeat(64),
            "127.0.0.1:5001/team/computer?secret@sha256:".to_owned() + &"a".repeat(64),
        ] {
            assert!(MappedImage::new(bad, &registry).is_err());
        }
    }
    #[test]
    fn pull_manifest_requires_actual_bytes_runnable_profile_and_local_config_identity() {
        let (image, bytes, id) = selected("application/vnd.oci.image.manifest.v1+json");
        let header = format!("sha256:{}", image.digest.hex());
        image
            .admit_response(reqwest::StatusCode::OK, Some(&header), &bytes, &id)
            .unwrap();
        let mut changed = bytes.clone();
        changed.push(b' ');
        assert!(
            image
                .admit_response(reqwest::StatusCode::OK, Some(&header), &changed, &id)
                .is_err()
        );
        assert!(
            image
                .admit_response(reqwest::StatusCode::OK, None, &bytes, &id)
                .is_err()
        );
        assert!(
            image
                .admit_response(reqwest::StatusCode::NOT_FOUND, Some(&header), &bytes, &id)
                .is_err()
        );
        assert!(
            image
                .admit_response(reqwest::StatusCode::FOUND, Some(&header), &bytes, &id)
                .is_err()
        );
        let foreign = veoveo_types::Sha256Digest::from_hex("c".repeat(64)).unwrap();
        assert!(
            image
                .admit_response(reqwest::StatusCode::OK, Some(&header), &bytes, &foreign)
                .is_err()
        );
        let (index, bytes, id) = selected("application/vnd.oci.image.index.v1+json");
        assert!(
            index
                .admit_manifest(Some(&format!("sha256:{}", index.digest.hex())), &bytes, &id)
                .is_err()
        );
        assert!(
            image
                .admit_manifest(Some(&header), &vec![b' '; MAX_MANIFEST_BYTES + 1], &id)
                .is_err()
        );
    }
}
