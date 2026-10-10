//! Owning installed View qualification over the public Gateway and official MCP Tasks.
use super::*;
use rmcp::model::{
    CancelTaskParams, DetailedTask, GetTaskParams, ServerNotification, SubscriptionFilter, Task,
    TaskStatus,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_testing_support::installed::restart::{
    DeploymentRestart, DrainProfile, SelectedDrainTarget,
};
use veoveo_types::ResourceAddress;
use veoveo_types::{CanonicalTaskId, LocalToolName};
use veoveo_view_mcp::contract::{CloseViewRequest, CloseViewResult, Sha256Digest};

#[path = "installed_delivery.rs"]
mod delivery;

const VIEW_CONTAINER_ROLE: &str = "view-mcp";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FixtureInput {
    schema_version: FixtureVersion,
    deployment: String,
    pod: String,
    container: String,
    namespace_uid: uuid::Uuid,
    deployment_uid: uuid::Uuid,
    deployment_resource_version: String,
    pod_uid: uuid::Uuid,
    pod_resource_version: String,
    catalog_config_map: String,
    catalog_config_map_uid: uuid::Uuid,
    image_release: PathBuf,
}
#[derive(Deserialize)]
enum FixtureVersion {
    #[serde(rename = "veoveo.ai/view-installed-fixture/v1")]
    V1,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pod {
    metadata: Metadata,
    spec: PodSpec,
}
#[derive(Deserialize)]
struct Metadata {
    uid: uuid::Uuid,
    namespace: Option<String>,
    #[serde(rename = "resourceVersion")]
    resource_version: String,
}
#[derive(Deserialize)]
struct PodSpec {
    containers: Vec<Container>,
    volumes: Vec<Volume>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Container {
    name: String,
    image: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    command: Vec<String>,
    volume_mounts: Vec<VolumeMount>,
    resources: Resources,
}
#[derive(Deserialize)]
struct Resources {
    requests: BTreeMap<String, String>,
    limits: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct IdentityResource {
    metadata: Metadata,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VolumeMount {
    name: String,
    mount_path: String,
    #[serde(default)]
    read_only: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    name: String,
    config_map: Option<ConfigMapReference>,
}
#[derive(Deserialize)]
struct ConfigMapReference {
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigMap {
    metadata: Metadata,
    immutable: Option<bool>,
    #[serde(default)]
    data: BTreeMap<String, String>,
    #[serde(default)]
    binary_data: BTreeMap<String, String>,
}

async fn kube(target: &InstalledTarget, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = tokio::process::Command::new("kubectl");
    command
        .args([
            "--context",
            &target.target.kubernetes.context,
            "--namespace",
            &target.target.kubernetes.namespace,
            "--request-timeout=10s",
        ])
        .args(args)
        .kill_on_drop(true);
    let result = veoveo_testing_support::output_async(command, Duration::from_secs(15))
        .await
        .context("installed View Kubernetes observation")?;
    ensure!(
        result.status.success(),
        "installed View Kubernetes {} failed with {}; stderr excluded",
        args[0],
        result.status
    );
    ensure!(
        result.stdout.len() <= 2 * 1024 * 1024,
        "installed View observation exceeded two MiB"
    );
    Ok(result.stdout)
}
fn input(path: &Path) -> Result<FixtureInput> {
    let bytes = fs::read(path).context("read explicit installed View fixture declaration")?;
    ensure!(
        bytes.len() <= 65536,
        "installed View fixture declaration exceeds 64 KiB"
    );
    let mut admitted: FixtureInput = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow!(
            "invalid installed View fixture declaration; use veoveo.ai/view-installed-fixture/v1"
        )
    })?;
    admitted.validate()?;
    if admitted.image_release.is_relative() {
        admitted.image_release = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&admitted.image_release);
    }
    Ok(admitted)
}
impl FixtureInput {
    fn validate(&self) -> Result<()> {
        for name in [
            &self.deployment,
            &self.pod,
            &self.container,
            &self.catalog_config_map,
        ] {
            ensure!(
                valid_fixture_name(name),
                "installed View fixture names must be lowercase Kubernetes DNS labels"
            );
        }
        ensure!(
            self.container == VIEW_CONTAINER_ROLE,
            "installed View requires the chart's view-mcp Rust service container"
        );
        ensure!(
            [
                self.namespace_uid,
                self.deployment_uid,
                self.pod_uid,
                self.catalog_config_map_uid
            ]
            .iter()
            .all(|uid| !uid.is_nil()),
            "installed View fixture identities must be nonzero"
        );
        ensure!(
            [
                &self.deployment_resource_version,
                &self.pod_resource_version
            ]
            .iter()
            .all(|rv| !rv.is_empty() && rv.len() <= 1024 && !rv.chars().any(char::is_control)),
            "installed View fixture requires nonempty opaque resourceVersions"
        );
        ensure!(
            !self.image_release.as_os_str().is_empty(),
            "installed View fixture requires a qualified image release file"
        );
        Ok(())
    }
}
fn valid_fixture_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric()
}
fn require_catalog_bytes(map: &ConfigMap, canonical: &Path) -> Result<()> {
    for file in ["layers.json", "tileset.json"] {
        let expected = fs::read(canonical.join(file))?;
        ensure!(
            map.data
                .get(file)
                .is_some_and(|value| value.as_bytes() == expected),
            "installed View catalog/tileset does not match the maintained local fixture; Google-only catalogs are unsupported here"
        );
    }
    let triangle = map
        .binary_data
        .get("triangle.glb")
        .context("fixture ConfigMap has no binary triangle")?;
    let bytes = STANDARD
        .decode(triangle)
        .map_err(|_| anyhow!("fixture triangle is not admitted base64"))?;
    ensure!(
        bytes == fs::read(canonical.join("triangle.glb"))?,
        "installed triangle bytes differ from maintained fixture"
    );
    Ok(())
}
async fn connect_contexts(contexts: &[(&str, &str)]) -> Result<Vec<SmokeMcpClient>> {
    let mut clients = Vec::new();
    for (url, token) in contexts {
        match tokio::time::timeout(Duration::from_secs(15), connect_mcp_client(url, token))
            .await
            .context("installed View client admission exceeded fifteen seconds")
            .and_then(|result| result)
        {
            Ok(client) => clients.push(client),
            Err(error) => {
                // Each admitted SDK session is stopped even if a later independent connection fails.
                let _ = tokio::time::timeout(Duration::from_secs(10), async {
                    for client in clients {
                        let _ = client.cancel().await;
                    }
                })
                .await;
                return Err(error).context("installed View independent client admission failed");
            }
        }
    }
    Ok(clients)
}

fn require_catalog_command(command: &[String], args: &[String]) -> Result<()> {
    let command_arguments = match command.split_first() {
        None => &[][..],
        Some((executable, rest)) => {
            ensure!(
                executable == "/usr/local/bin/view-mcp",
                "installed View requires its qualified image entrypoint; wrappers and alternate commands are unsupported"
            );
            rest
        }
    };
    let effective: Vec<&str> = command_arguments
        .iter()
        .chain(args)
        .map(String::as_str)
        .collect();
    ensure!(
        !effective.contains(&"--"),
        "installed View does not admit an end-of-options command override"
    );
    let mut catalogs = Vec::new();
    for (index, arg) in effective.iter().enumerate() {
        if *arg == "--layer-catalog" {
            catalogs.push(
                effective
                    .get(index + 1)
                    .copied()
                    .context("layer catalog startup argument has no path")?,
            );
        } else if let Some(path) = arg.strip_prefix("--layer-catalog=") {
            catalogs.push(path);
        }
    }
    ensure!(
        catalogs.is_empty() || catalogs == ["/etc/veoveo/view/layers.json"],
        "installed View must load its admitted local catalog at /etc/veoveo/view/layers.json"
    );
    Ok(())
}
fn cleanup_views<'a>(
    replacement_completed: bool,
    composition: &SceneComposition,
    known: Option<&ViewRecord>,
    current: &'a [ViewRecord],
) -> Result<Vec<&'a ViewRecord>> {
    ensure!(
        current.len() <= 1,
        "View create reconciliation exceeded its one-dispatch resource bound"
    );
    ensure!(
        current
            .iter()
            .all(|view| view.composition_id() == composition.composition_id()
                && view.composition_digest_sha256() == composition.composition_digest_sha256()
                && view.scene_layer() == composition.base_layer()),
        "View cleanup refuses records outside its admitted fixture composition"
    );
    if replacement_completed {
        ensure!(
            current.is_empty(),
            "admitted View replacement still exposes an old owned View"
        );
        return Ok(Vec::new());
    }
    ensure!(
        !current.is_empty(),
        "View create cleanup remains unresolved without admitted replacement or a discovered owned View"
    );
    if let Some(known) = known {
        ensure!(
            current[0].view_id() == known.view_id(),
            "View create cleanup discovered a different identity"
        );
    }
    Ok(current.iter().collect())
}
async fn require_fixture(
    target: &InstalledTarget,
    input: &FixtureInput,
    canonical: &Path,
) -> Result<()> {
    let _ = &input.schema_version;
    let namespace: IdentityResource = serde_json::from_slice(
        &kube(
            target,
            &[
                "get",
                "namespace",
                &target.target.kubernetes.namespace,
                "-o",
                "json",
            ],
        )
        .await?,
    )
    .map_err(|_| anyhow!("invalid selected View namespace response"))?;
    let deployment: IdentityResource = serde_json::from_slice(
        &kube(
            target,
            &["get", "deployment", &input.deployment, "-o", "json"],
        )
        .await?,
    )
    .map_err(|_| anyhow!("invalid selected View Deployment response"))?;
    ensure!(
        namespace.metadata.uid == input.namespace_uid
            && deployment.metadata.uid == input.deployment_uid
            && deployment.metadata.resource_version == input.deployment_resource_version,
        "installed View namespace/Deployment identity differs from declared fixture"
    );
    let release_bytes =
        fs::read(&input.image_release).context("read qualified View image release")?;
    ensure!(
        release_bytes.len() <= 4 * 1024 * 1024,
        "qualified View release exceeds four MiB"
    );
    let release: veoveo_deploy_contract::ImageReleaseEvidence =
        serde_json::from_slice(&release_bytes)
            .map_err(|_| anyhow!("invalid qualified View image release"))?;
    release.validate()?;
    let image = release
        .images
        .iter()
        .find(|image| image.name == "view-mcp")
        .context("qualified image release has no view-mcp")?;
    let expected_image = format!("{}@{}", image.repository, image.digest);
    let pod: Pod =
        serde_json::from_slice(&kube(target, &["get", "pod", &input.pod, "-o", "json"]).await?)
            .map_err(|_| anyhow!("invalid selected View Pod response"))?;
    ensure!(
        pod.metadata.uid == input.pod_uid
            && pod.metadata.resource_version == input.pod_resource_version
            && pod.metadata.namespace.as_deref() == Some(&target.target.kubernetes.namespace),
        "selected View Pod differs from fixture declaration"
    );
    let container = pod
        .spec
        .containers
        .iter()
        .find(|container| container.name == input.container)
        .context("fixture server container absent")?;
    for quantities in [&container.resources.requests, &container.resources.limits] {
        ensure!(
            quantities
                .get("nvidia.com/gpu")
                .and_then(|value| value.parse::<u32>().ok())
                .is_some_and(|count| count > 0),
            "installed View requires positive NVIDIA GPU requests and limits before public calls"
        );
    }
    let startup = kube(
        target,
        &[
            "logs",
            &input.pod,
            "--container",
            &input.container,
            "--limit-bytes=65536",
        ],
    )
    .await?;
    readiness::admit_startup_logs(
        std::str::from_utf8(&startup).context("selected View startup logs are not UTF-8")?,
    )?;
    ensure!(
        container.image == expected_image,
        "selected View container differs from qualified immutable image release"
    );
    require_catalog_command(&container.command, &container.args)?;
    let map: ConfigMap = serde_json::from_slice(
        &kube(
            target,
            &["get", "configmap", &input.catalog_config_map, "-o", "json"],
        )
        .await?,
    )
    .map_err(|_| anyhow!("invalid View fixture ConfigMap response"))?;
    ensure!(
        map.metadata.uid == input.catalog_config_map_uid
            && map.metadata.namespace.as_deref() == Some(&target.target.kubernetes.namespace)
            && map.immutable == Some(true),
        "View fixture ConfigMap identity or immutable state differs"
    );
    for directory in ["/etc/veoveo/view", "/fixtures"] {
        ensure!(
            container
                .volume_mounts
                .iter()
                .any(|mount| mount.mount_path == directory
                    && mount.read_only
                    && pod
                        .spec
                        .volumes
                        .iter()
                        .any(|volume| volume.name == mount.name
                            && volume
                                .config_map
                                .as_ref()
                                .is_some_and(|map| map.name == input.catalog_config_map))),
            "View catalog and triangle must use the declared immutable read-only fixture ConfigMap"
        );
    }
    require_catalog_bytes(&map, canonical)?;
    // Prove the selected process sees the same mounted bytes, not merely matching API data.
    for (file, remote) in [
        ("layers.json", "/etc/veoveo/view/layers.json"),
        ("tileset.json", "/fixtures/tileset.json"),
        ("triangle.glb", "/fixtures/triangle.glb"),
    ] {
        let bytes = kube(
            target,
            &[
                "exec",
                &input.pod,
                "--container",
                &input.container,
                "--",
                "cat",
                remote,
            ],
        )
        .await?;
        ensure!(
            Sha256Digest::parse(hex::encode(Sha256::digest(&bytes)))?
                == Sha256Digest::parse(hex::encode(Sha256::digest(fs::read(
                    canonical.join(file)
                )?)))?,
            "selected process fixture bytes differ"
        );
    }
    Ok(())
}
fn require_identity(input: &FixtureInput, selected: &SelectedDrainTarget) -> Result<()> {
    let identity = selected.identity();
    ensure!(
        identity.namespace_uid() == input.namespace_uid
            && identity.deployment_uid() == input.deployment_uid
            && identity.pod_uid() == input.pod_uid
            && identity.deployment_resource_version() == input.deployment_resource_version
            && identity.pod_resource_version() == input.pod_resource_version,
        "installed View lifecycle identities changed; admit a fresh fixture before effects"
    );
    Ok(())
}
fn tool_name(name: &str) -> Result<GatewayToolName> {
    Ok(GatewayToolName::from_parts(
        &ServerSlug::parse("view")?,
        &LocalToolName::parse(name)?,
    )?)
}
async fn tool<T: DeserializeOwned>(
    client: &SmokeMcpClient,
    name: &str,
    request: impl Serialize,
) -> Result<T> {
    let name = tool_name(name)?;
    let params = CallToolRequestParams::new(name.to_string())
        .with_arguments(serde_json::from_value(serde_json::to_value(request)?)?);
    let response = client
        .call_tool(params)
        .await
        .with_context(|| format!("installed View tool {name}"))?;
    ensure!(
        response.is_error != Some(true),
        "installed View tool {name} returned an error"
    );
    serde_json::from_value(
        response
            .structured_content
            .context("View tool omitted structured response")?,
    )
    .map_err(|_| anyhow!("View response failed owner admission"))
}
async fn baseline(
    client: &SmokeMcpClient,
    task: &CanonicalTaskId,
    created: &Task,
    expected: &DetailedTask,
) -> Result<rmcp::model::CallToolResult> {
    let filter = SubscriptionFilter::builder()
        .task_ids([task.to_string()])
        .build();
    let mut stream = client.listen(filter.clone()).await?;
    ensure!(
        stream.acknowledged() == &filter,
        "View Task subscription changed its filter"
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        // SDK Subscription::next validates the exact subscription ID and
        // acknowledged filter before returning either Task or resource updates.
        match stream
            .next()
            .await?
            .context("View Task baseline stream ended")?
        {
            ServerNotification::TaskStatusNotification(update) => {
                delivery::completed(created, expected, &update.params.task, task)?;
                let current = client
                    .get_task(GetTaskParams::new(task.to_string()))
                    .await?;
                delivery::completed(created, &update.params.task, &current.task, task)
            }
            _ => bail!("View Task baseline contained an unexpected notification"),
        }
    })
    .await
    .context("independent View Task baseline exceeded fifteen seconds")?
}
async fn denied(client: &SmokeMcpClient, task: &CanonicalTaskId) -> Result<()> {
    require_isolation_rejection(
        client.get_task(GetTaskParams::new(task.to_string())).await,
        "unknown task id",
    )?;
    require_isolation_rejection(
        client
            .cancel_task(CancelTaskParams::new(task.to_string()))
            .await,
        "unknown task id",
    )?;
    let filter = SubscriptionFilter::builder()
        .task_ids([task.to_string()])
        .resource_subscriptions([veoveo_view_mcp::contract::ViewResource::Views
            .to_uri()?
            .to_string()])
        .build();
    let mut stream = client.listen(filter.clone()).await?;
    ensure!(
        stream.acknowledged() == &filter,
        "foreign View Task listener changed its acknowledged filter"
    );
    // SDK routing rejects missing/foreign subscription IDs and updates outside
    // the acknowledged filter. The permitted Views baseline anchors processing.

    tokio::time::timeout(Duration::from_secs(15), async {
        match stream
            .next()
            .await?
            .context("foreign listener ended before permitted Views collection baseline")?
        {
            ServerNotification::ResourceUpdatedNotification(update)
                if update.params.uri == "view://views" =>
            {
                Ok::<_, anyhow::Error>(())
            }
            ServerNotification::TaskStatusNotification(_) => {
                bail!("foreign listener received an owner Task")
            }
            _ => bail!("foreign View listener baseline is unexpected"),
        }
    })
    .await
    .context("foreign View listener has no permitted baseline anchor")??;
    match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
        Err(_) => Ok(()),
        Ok(Ok(Some(ServerNotification::TaskStatusNotification(_)))) => {
            bail!("foreign listener received an owner Task")
        }
        _ => bail!("foreign listener ended or produced an unexpected observation"),
    }
}

pub(super) async fn run(installation: &Path, fixture: &Path, evidence: &Path) -> Result<()> {
    let target = InstalledTarget::load(installation)?;
    let input = input(fixture)?;
    use std::io::{Seek, SeekFrom, Write};
    use std::os::unix::fs::OpenOptionsExt;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(evidence)
        .context("installed View evidence already exists or cannot be created")?;
    serde_json::to_writer_pretty(
        &mut output,
        &json!({"schemaVersion":"veoveo.ai/view-installed-evidence/v1","qualified":false,"phase":"fixtureAdmission","restartDispatched":false}),
    )?;
    output.flush()?;
    output.sync_data()?;
    let comparison = target
        .operator
        .comparison_context
        .clone()
        .context("installed View requires an explicitly declared comparison WorkContext")?;
    let administrator = target.administrator()?;
    ensure!(
        administrator.principal != target.operator.principal,
        "installed View requires a distinct administrator principal"
    );
    let temporary = tempfile::tempdir()?;
    write_local_fixture(temporary.path())?;
    // All private fixture checks precede OAuth and public domain requests.
    require_fixture(&target, &input, temporary.path()).await?;
    let owner_token = target.token().await?;
    let observer_token = target.token().await?;
    let comparison_token = target.operator.token_for_context(&comparison).await?;
    let administrator_token = administrator.token().await?;
    let owner_url = target.public_url(&["mcp", target.operator.profile.as_str()])?;
    let administrator_url = target.public_url(&["mcp", administrator.profile.as_str()])?;
    let clients = connect_contexts(&[
        (owner_url.as_str(), &owner_token),
        (owner_url.as_str(), &observer_token),
        (owner_url.as_str(), &comparison_token),
        (administrator_url.as_str(), &administrator_token),
    ])
    .await
    .map_err(|error| {
        anyhow!(bounded_redacted(
            &format!("{error:#}"),
            &[
                &owner_token,
                &observer_token,
                &comparison_token,
                &administrator_token
            ]
        ))
    })?;
    let [writer, observer, different_context, different_principal]: [SmokeMcpClient; 4] = clients
        .try_into()
        .map_err(|_| anyhow!("installed View independent client count differs"))?;
    let mut selected_identity = None;
    let mut created_view = None;
    let mut create_dispatched = None;
    let mut replacement_completed = false;
    let outcome = tokio::time::timeout(Duration::from_secs(240),async {
        let restart = DeploymentRestart::new(&target.target,&input.deployment,VIEW_CONTAINER_ROLE,observer.peer().clone(),veoveo_view_mcp::contract::ViewResource::Contract.to_uri()?)?;
        let selected = restart.select_drain_target(&input.pod,DrainProfile::nvidia(&input.container,Duration::from_secs(30))?).await?;
        require_identity(&input,&selected)?;
        selected_identity = Some(selected.identity());

        let startup = kube(&target,&["logs",&input.pod,"--container",&input.container,"--limit-bytes=65536"]).await?;
        let adapter = readiness::admit_startup_logs(std::str::from_utf8(&startup).context("View startup logs are not UTF-8")?)?;
        let current_views: Vec<ViewRecord> = serde_json::from_value(read_view_resource(&writer,veoveo_view_mcp::contract::ViewResource::Views.to_uri()?.as_ref()).await?)
            .map_err(|_| anyhow!("installed View collection failed owner admission"))?;
        ensure!(current_views.is_empty(), "installed View requires an isolated caller WorkContext with no existing Views");
        let composition:SceneComposition = tool(&writer,"create_scene_composition",composition_request(LOCAL_LAYER,false)?).await?;
        create_dispatched = Some(composition.clone());
        output.set_len(0)?;output.seek(SeekFrom::Start(0))?;
        serde_json::to_writer_pretty(&mut output,&json!({"schemaVersion":"veoveo.ai/view-installed-evidence/v1","qualified":false,"phase":"viewCreateDispatchIntent","compositionId":composition.composition_id(),"restartDispatched":false}))?;
        output.flush()?;output.sync_data()?;
        let view:ViewRecord = tool(&writer,"create_view",CreateViewRequest { composition_id:composition.composition_id().clone(), camera:local_camera() }).await?;
        created_view = Some(view.clone());
        let mut request:CaptureFrameRequest = serde_json::from_value(capture_request(view.view_id().as_str(),view.revision(),false)?)?;
        request.policy.encoding = FrameEncoding::Jpeg;
        let created = call_tool_as_task(&writer,tool_name("capture_frame")?.as_str(),serde_json::to_value(request)?).await?;
        let task = CanonicalTaskId::parse(&created.task_id)?;
        let terminal = await_task_terminal_with_timeout(&writer,task.as_str(),Duration::from_secs(30)).await?;
        ensure!(terminal.status() == TaskStatus::Completed, "installed capture did not complete");
        let delivered_payload = baseline(&observer,&task,&created,&terminal).await?;
        denied(&different_context,&task).await?;
        denied(&different_principal,&task).await?;
        let payload = task_payload(&observer,task.as_str()).await?;
        delivery::payload_agreement(&delivered_payload,&payload)?;
        let frame = payload.structured_content.as_ref().context("capture has no frame metadata")?;
        let bytes = image_bytes(&payload,"image/jpeg")?;
        assert_local_frame(frame,&bytes,"image/jpeg")?;
        let before_logs = kube(&target,&["logs",&input.pod,"--container",&input.container,"--limit-bytes=65536"]).await?;
        readiness::assert_capture_completions(&adapter,1,std::str::from_utf8(&before_logs)?)?;
        output.set_len(0)?;output.seek(SeekFrom::Start(0))?;
        serde_json::to_writer_pretty(&mut output,&json!({"schemaVersion":"veoveo.ai/view-installed-evidence/v1","selected":selected_identity,"qualified":false,"phase":"restartDispatchIntent","taskId":task}))?;
        output.flush()?;output.sync_data()?;
        let drain = restart.restart_with_drain(&selected).await?;
        replacement_completed = true;
        // A fresh official client must recover the completed owner Task after Pod replacement.
        let recovered = connect_mcp_client(owner_url.as_str(),&observer_token).await?;
        let recovery = async {
            let recovered_delivery = baseline(&recovered,&task,&created,&terminal).await?;
            let recovered_payload = task_payload(&recovered,task.as_str()).await?;
            delivery::payload_agreement(&recovered_delivery,&recovered_payload)?;
            let recovered_bytes = image_bytes(&recovered_payload,"image/jpeg")?;
            ensure!(recovered_bytes == bytes,"completed View capture changed across Pod replacement");
            admit_captured_frame(recovered_payload.structured_content.as_ref().context("recovered capture metadata absent")?,&recovered_bytes,"image/jpeg")?;
            denied(&different_context,&task).await?;
            denied(&different_principal,&task).await?;
            Ok::<_,anyhow::Error>(())
        }.await;
        let stopped = recovered.cancel().await;
        recovery?; stopped?;
        Ok::<_,anyhow::Error>(json!({"selected":selected_identity,"drain":drain,"taskId":task,"captureSha256":hex::encode(Sha256::digest(&bytes)),"independentContextDelivery":true,"comparisonContextDenied":true,"differentPrincipalDenied":true,"completedTaskRecovery":true,"foreignListenObservationSeconds":1}))
    }).await.context("installed View lifecycle exceeded 240 seconds").and_then(|result|result);
    // Cleanup is outside the scenario deadline and runs after every admitted View response.
    let cleanup = tokio::time::timeout(Duration::from_secs(30), async {
        if let Some(composition) = create_dispatched.as_ref() {
            let cleanup_client = connect_mcp_client(owner_url.as_str(), &owner_token).await?;
            let removal = async {
                let views: Vec<ViewRecord> = serde_json::from_value(
                    read_view_resource(
                        &cleanup_client,
                        veoveo_view_mcp::contract::ViewResource::Views
                            .to_uri()?
                            .as_ref(),
                    )
                    .await?,
                )
                .map_err(|_| anyhow!("cleanup View collection failed owner admission"))?;
                for view in cleanup_views(
                    replacement_completed,
                    composition,
                    created_view.as_ref(),
                    &views,
                )? {
                    let closed: CloseViewResult = tool(
                        &cleanup_client,
                        "close_view",
                        CloseViewRequest {
                            view_id: view.view_id().clone(),
                            expected_revision: view.revision(),
                        },
                    )
                    .await?;
                    ensure!(
                        closed.closed && &closed.view_id == view.view_id(),
                        "installed owned View cleanup was not acknowledged for its identity"
                    );
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            let stopped = cleanup_client.cancel().await;
            removal?;
            stopped?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("installed owned View cleanup exceeded thirty seconds")
    .and_then(|r| r);
    let sessions = tokio::time::timeout(Duration::from_secs(10), async {
        let (a, b, c, d) = tokio::join!(
            writer.cancel(),
            observer.cancel(),
            different_context.cancel(),
            different_principal.cancel()
        );
        a?;
        b?;
        c?;
        d?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("installed View client cleanup exceeded ten seconds")
    .and_then(|r| r);
    let failure = outcome
        .as_ref()
        .err()
        .or_else(|| cleanup.as_ref().err())
        .or_else(|| sessions.as_ref().err())
        .map(|error| {
            bounded_redacted(
                &format!("{error:#}"),
                &[
                    &owner_token,
                    &observer_token,
                    &comparison_token,
                    &administrator_token,
                ],
            )
        });
    let cleanup_failure = cleanup.as_ref().err().map(|error| {
        bounded_redacted(
            &format!("{error:#}"),
            &[
                &owner_token,
                &observer_token,
                &comparison_token,
                &administrator_token,
            ],
        )
    });
    let report = json!({"schemaVersion":"veoveo.ai/view-installed-evidence/v1","selected":selected_identity,"result":outcome.as_ref().ok(),"failure":failure,"cleanupPassed":cleanup.is_ok()&&sessions.is_ok(),"cleanupFailure":cleanup_failure,"viewCreateDispatched":create_dispatched.is_some(),"replacementAdmitted":replacement_completed,"qualified":outcome.is_ok()&&cleanup.is_ok()&&sessions.is_ok()});
    output.set_len(0)?;
    output.seek(SeekFrom::Start(0))?;
    serde_json::to_writer_pretty(&mut output, &report)?;
    output.flush()?;
    output.sync_data()?;
    if let Some(failure) = failure {
        bail!("{failure}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effective_command_admission_refuses_catalog_override_and_wrappers() -> Result<()> {
        let executable = "/usr/local/bin/view-mcp";
        let catalog = "/etc/veoveo/view/layers.json";
        let values = |args: &[&str]| args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for (command, args) in [
            (vec![], vec![]),
            (values(&[executable]), vec![]),
            (values(&[executable, "--layer-catalog", catalog]), vec![]),
            (vec![], values(&["--layer-catalog", catalog])),
            (
                values(&[executable]),
                values(&["--layer-catalog=/etc/veoveo/view/layers.json"]),
            ),
        ] {
            require_catalog_command(&command, &args)?;
        }
        // Decode the actual Kubernetes command field; an args-only check missed this override.
        let container: Container = serde_json::from_value(
            json!({"name":"server","image":"qualified-image","command":[executable,"--layer-catalog","/other/catalog.json"],"args":[],"volumeMounts":[],"resources":{"requests":{},"limits":{}}}),
        )?;
        ensure!(require_catalog_command(&container.command, &container.args).is_err());
        for (command, args) in [
            (values(&["/bin/sh", "-c", executable]), vec![]),
            (values(&["view-mcp"]), vec![]),
            (
                values(&[executable, "--layer-catalog", catalog]),
                values(&["--layer-catalog", catalog]),
            ),
            (vec![], values(&["--layer-catalog"])),
            (
                values(&[executable, "--"]),
                values(&["--layer-catalog", catalog]),
            ),
        ] {
            ensure!(require_catalog_command(&command, &args).is_err());
        }
        Ok(())
    }
    fn cleanup_fixture() -> Result<(SceneComposition, ViewRecord)> {
        let now = Utc::now();
        let authority = serde_json::from_value(
            json!({"principalId":"operator","invocation":{"work_context":"operations","tenant":"tenant","membership":"custodian","policy_revision":"r1","output_policy":{"owner":{"kind":"principal","id":"operator"},"initial_grants":[],"classification":"internal","data_labels":[]},"provenance":{"mode":"delegated","initiator":"operator","delegation_id":"capture"}}}),
        )?;
        let composition =
            SceneComposition::new(composition_request(LOCAL_LAYER, false)?, authority, now)?;
        let view = ViewRecord::new(
            ViewId::parse("fixture-view")?,
            &composition,
            local_camera(),
            now,
        )?;
        Ok((composition, view))
    }
    #[test]
    fn cleanup_requires_replacement_for_absence_and_closes_admitted_pre_restart_view() -> Result<()>
    {
        let (composition, view) = cleanup_fixture()?;
        ensure!(cleanup_views(true, &composition, Some(&view), &[])?.is_empty());
        ensure!(cleanup_views(false, &composition, Some(&view), &[]).is_err());
        ensure!(
            cleanup_views(true, &composition, Some(&view), std::slice::from_ref(&view)).is_err()
        );
        let candidates = cleanup_views(
            false,
            &composition,
            Some(&view),
            std::slice::from_ref(&view),
        )?;
        ensure!(candidates.len() == 1 && candidates[0].view_id() == view.view_id());
        Ok(())
    }
    #[test]
    fn lost_create_reply_reconciles_only_bounded_owned_composition_and_keeps_absence_unknown()
    -> Result<()> {
        let (composition, view) = cleanup_fixture()?;
        let candidates = cleanup_views(false, &composition, None, std::slice::from_ref(&view))?;
        ensure!(candidates.len() == 1 && candidates[0].view_id() == view.view_id());
        ensure!(cleanup_views(false, &composition, None, &[]).is_err());
        ensure!(cleanup_views(false, &composition, None, &[view.clone(), view.clone()]).is_err());
        let mut request = composition_request(LOCAL_LAYER, false)?;
        request.base_layer = LayerId::parse("foreign-layer")?;
        let foreign = SceneComposition::new(request, composition.authority().clone(), Utc::now())?;
        let foreign_view = ViewRecord::new(
            ViewId::parse("foreign-view")?,
            &foreign,
            local_camera(),
            Utc::now(),
        )?;
        ensure!(cleanup_views(false, &composition, None, &[foreign_view]).is_err());
        // A drained old instance makes lost-reply absence definitive for its in-process catalog.
        ensure!(cleanup_views(true, &composition, None, &[])?.is_empty());
        Ok(())
    }
    fn declaration() -> Value {
        json!({"schemaVersion":"veoveo.ai/view-installed-fixture/v1","deployment":"view-mcp","pod":"view-mcp-current","container":"view-mcp","namespaceUid":uuid::Uuid::new_v4(),"deploymentUid":uuid::Uuid::new_v4(),"deploymentResourceVersion":"opaque-deployment","podUid":uuid::Uuid::new_v4(),"podResourceVersion":"opaque-pod","catalogConfigMap":"view-fixture","catalogConfigMapUid":uuid::Uuid::new_v4(),"imageRelease":"release.json"})
    }
    #[test]
    fn view_fixture_container_role_matches_chart_and_refuses_sidecars() -> Result<()> {
        let definitions = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../deploy/helm/veoveo/definitions/domain-services.yaml"
        ));
        let template = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../deploy/helm/veoveo/templates/domain-services.yaml"
        ));
        ensure!(definitions.contains(&format!("- name: {VIEW_CONTAINER_ROLE}\n")));
        ensure!(template.contains("app.kubernetes.io/component: {{ $service.name }}"));
        ensure!(template.contains("- name: {{ $service.name }}"));
        let mut declared = declaration();
        // Deployment identity is installation-supplied; the process role is fixed.
        declared["deployment"] = json!("selected-view-deployment");
        let input: FixtureInput = serde_json::from_value(declared.clone())?;
        input.validate()?;
        for role in ["view", "server", "view-worker", "cuopt-executor"] {
            declared["container"] = json!(role);
            let input: FixtureInput = serde_json::from_value(declared.clone())?;
            ensure!(
                input.validate().is_err(),
                "View fixture admitted another process role"
            );
        }
        Ok(())
    }
    #[test]
    fn fixture_declaration_admits_current_identity_and_refuses_unsafe_inputs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.json");
        fs::write(&path, serde_json::to_vec(&declaration())?)?;
        let admitted = input(&path)?;
        ensure!(admitted.image_release == dir.path().join("release.json"));
        for (field, value) in [
            ("pod", json!("--all-namespaces")),
            ("container", json!("server/other")),
            ("namespaceUid", json!(uuid::Uuid::nil())),
            ("podResourceVersion", json!("")),
            ("deploymentResourceVersion", json!("opaque\nrv")),
            ("schemaVersion", json!("unknown")),
            ("imageRelease", json!("")),
        ] {
            let mut invalid = declaration();
            invalid[field] = value;
            fs::write(&path, serde_json::to_vec(&invalid)?)?;
            ensure!(
                input(&path).is_err(),
                "unsafe fixture field {field} was admitted"
            );
        }
        let mut extra = declaration();
        extra["secretToken"] = json!("private sentinel");
        fs::write(&path, serde_json::to_vec(&extra)?)?;
        let error = input(&path)
            .err()
            .context("unexpected extra field admitted")?;
        ensure!(!format!("{error:#}").contains("private sentinel"));
        Ok(())
    }
    #[test]
    fn fixture_preparation_admits_only_maintained_local_catalog_and_triangle() -> Result<()> {
        let directory = tempfile::tempdir()?;
        export_view_fixture(directory.path())?;
        let mut map = ConfigMap {
            metadata: Metadata {
                uid: uuid::Uuid::new_v4(),
                namespace: Some("isolated".into()),
                resource_version: "opaque".into(),
            },
            immutable: Some(true),
            data: [
                (
                    "layers.json".into(),
                    fs::read_to_string(directory.path().join("layers.json"))?,
                ),
                (
                    "tileset.json".into(),
                    fs::read_to_string(directory.path().join("tileset.json"))?,
                ),
            ]
            .into_iter()
            .collect(),
            binary_data: [(
                "triangle.glb".into(),
                STANDARD.encode(fs::read(directory.path().join("triangle.glb"))?),
            )]
            .into_iter()
            .collect(),
        };
        require_catalog_bytes(&map, directory.path())?;
        let valid_catalog = map.data["layers.json"].clone();
        map.data.insert(
            "layers.json".into(),
            r#"{"layers":[{"layerId":"google-photorealistic","source":{"kind":"google"}}]}"#.into(),
        );
        ensure!(require_catalog_bytes(&map, directory.path()).is_err());
        map.data.insert("layers.json".into(), valid_catalog);
        map.binary_data.insert(
            "triangle.glb".into(),
            STANDARD.encode(b"different geometry"),
        );
        ensure!(require_catalog_bytes(&map, directory.path()).is_err());
        map.binary_data.remove("triangle.glb");
        ensure!(require_catalog_bytes(&map, directory.path()).is_err());
        Ok(())
    }
}
