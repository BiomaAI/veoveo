//! Ops attests the active catalog; native reads fence its selected public inputs.
use super::*;
use std::collections::BTreeMap;
use veoveo_deploy_contract::InstallationTarget;
use veoveo_mcp_contract::GatewayControlPlane;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Routing {
    pub active_control_plane: PathBuf,
    pub active_control_plane_sha256: Sha256Digest,
    pub gateway_deployment: String,
    pub gateway_deployment_uid: uuid::Uuid,
    pub gateway_deployment_resource_version: String,
    pub gateway_pod: String,
    pub gateway_pod_uid: uuid::Uuid,
    pub gateway_pod_resource_version: String,
    pub gateway_container: String,
    pub gateway_container_id: String,
    pub gateway_image_id: String,
    pub config_map: String,
    pub config_map_uid: uuid::Uuid,
    pub config_map_resource_version: String,
    pub config_map_data_sha256: Sha256Digest,
    pub config_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    uid: uuid::Uuid,
    resource_version: String,
    deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    owner_references: Vec<Owner>,
}
#[derive(Deserialize)]
struct Owner {
    kind: String,
    uid: uuid::Uuid,
    name: String,
    controller: Option<bool>,
}
#[derive(Deserialize)]
struct Object {
    metadata: Metadata,
    #[serde(default)]
    spec: serde_json::Value,
    #[serde(default)]
    status: serde_json::Value,
    #[serde(default)]
    data: BTreeMap<String, String>,
}
fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
impl Routing {
    pub fn admit(
        &self,
        target: &veoveo_deploy_contract::InstallationTarget,
        setup: &HandoffFixture,
    ) -> Result<()> {
        ensure!(
            target
                .expected_deployments
                .contains(&self.gateway_deployment),
            "Gateway routing Deployment is not declared"
        );
        for name in [
            &self.gateway_deployment,
            &self.gateway_pod,
            &self.gateway_container,
            &self.config_map,
        ] {
            ensure!(
                !name.is_empty()
                    && name.len() <= 253
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-._".contains(&c)),
                "invalid Gateway routing resource name"
            );
        }
        ensure!(
            [
                self.gateway_deployment_uid,
                self.gateway_pod_uid,
                self.config_map_uid
            ]
            .iter()
            .all(|uid| !uid.is_nil())
                && [
                    &self.gateway_deployment_resource_version,
                    &self.gateway_pod_resource_version,
                    &self.config_map_resource_version,
                    &self.gateway_container_id,
                    &self.gateway_image_id
                ]
                .iter()
                .all(|v| !v.is_empty()),
            "Gateway routing identities absent"
        );
        ensure!(
            !self.config_key.is_empty()
                && self
                    .config_key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-._".contains(&c)),
            "invalid public configuration key"
        );
        let metadata = fs::symlink_metadata(&self.active_control_plane)?;
        ensure!(
            self.active_control_plane.is_absolute()
                && metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.permissions().mode() & 0o077 == 0
                && metadata.len() <= 4 * 1024 * 1024,
            "active public catalog copy must be private and bounded"
        );
        let bytes = fs::read(&self.active_control_plane)?;
        ensure!(
            sha(&bytes) == self.active_control_plane_sha256,
            "active catalog copy digest differs"
        );
        admit_catalog(
            &bytes,
            &target.kubernetes.namespace,
            &setup.service,
            setup.service_port,
        )
    }
    pub async fn live(&self, target: &InstallationTarget, setup: &HandoffFixture) -> Result<()> {
        let deployment = read(target, "deployment", &self.gateway_deployment).await?;
        identity(
            &deployment,
            self.gateway_deployment_uid,
            &self.gateway_deployment_resource_version,
        )?;
        let pod = read(target, "pod", &self.gateway_pod).await?;
        identity(
            &pod,
            self.gateway_pod_uid,
            &self.gateway_pod_resource_version,
        )?;
        let owners = pod
            .metadata
            .owner_references
            .iter()
            .filter(|owner| owner.kind == "ReplicaSet" && owner.controller == Some(true))
            .collect::<Vec<_>>();
        ensure!(
            owners.len() == 1,
            "Gateway Pod requires one controlling ReplicaSet"
        );
        let replica_set = read(target, "replicaset", &owners[0].name).await?;
        ensure!(
            replica_set.metadata.uid == owners[0].uid
                && replica_set
                    .metadata
                    .owner_references
                    .iter()
                    .any(|owner| owner.kind == "Deployment"
                        && owner.controller == Some(true)
                        && owner.uid == self.gateway_deployment_uid),
            "Gateway Pod belongs to another Deployment"
        );
        let statuses = pod
            .status
            .get("containerStatuses")
            .and_then(|v| v.as_array())
            .context("Gateway statuses absent")?;
        ensure!(
            statuses
                .iter()
                .any(|value| value.get("name").and_then(|v| v.as_str())
                    == Some(&self.gateway_container)
                    && value.get("containerID").and_then(|v| v.as_str())
                        == Some(&self.gateway_container_id)
                    && value.get("imageID").and_then(|v| v.as_str())
                        == Some(&self.gateway_image_id)
                    && value.get("ready").and_then(|v| v.as_bool()) == Some(true)
                    && value.pointer("/state/running").is_some()),
            "Gateway process changed or not Ready"
        );
        // Verify the selected public ConfigMap is actually mounted by this process.
        let containers = pod
            .spec
            .get("containers")
            .and_then(|v| v.as_array())
            .context("Gateway containers absent")?;
        let container = containers
            .iter()
            .find(|value| {
                value.get("name").and_then(|v| v.as_str()) == Some(&self.gateway_container)
            })
            .context("Gateway selected container absent")?;
        let expected_path = format!("/etc/veoveo/gateway/{}", self.config_key);
        ensure!(
            container
                .get("args")
                .and_then(|v| v.as_array())
                .is_some_and(|args| args
                    .windows(2)
                    .any(|pair| pair[0].as_str() == Some("--expected-control-plane")
                        && pair[1].as_str() == Some(&expected_path))),
            "Gateway expected catalog argument differs"
        );
        let mount = container
            .get("volumeMounts")
            .and_then(|v| v.as_array())
            .and_then(|mounts| {
                mounts.iter().find(|value| {
                    value.get("mountPath").and_then(|v| v.as_str()) == Some("/etc/veoveo/gateway")
                })
            })
            .context("Gateway public catalog mount absent")?;
        let volume_name = mount
            .get("name")
            .and_then(|v| v.as_str())
            .context("Gateway public catalog volume absent")?;
        let volume = pod
            .spec
            .get("volumes")
            .and_then(|v| v.as_array())
            .and_then(|volumes| {
                volumes
                    .iter()
                    .find(|value| value.get("name").and_then(|v| v.as_str()) == Some(volume_name))
            })
            .context("Gateway public catalog volume binding absent")?;
        ensure!(
            volume.pointer("/configMap/name").and_then(|v| v.as_str()) == Some(&self.config_map)
                && volume.pointer("/configMap/items").is_none_or(|items| items
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item
                        .get("key")
                        .and_then(|v| v.as_str())
                        == Some(&self.config_key)
                        && item.get("path").and_then(|v| v.as_str()) == Some(&self.config_key)))),
            "Gateway mounted catalog ConfigMap/key differs"
        );
        let config = read(target, "configmap", &self.config_map).await?;
        identity(
            &config,
            self.config_map_uid,
            &self.config_map_resource_version,
        )?;
        ensure!(
            sha(&serde_json::to_vec(&config.data)?) == self.config_map_data_sha256,
            "Gateway public ConfigMap data changed"
        );
        let bytes = config
            .data
            .get(&self.config_key)
            .context("Gateway public catalog key absent")?
            .as_bytes();
        ensure!(
            sha(bytes) == self.active_control_plane_sha256,
            "mounted public catalog differs from Ops active-catalog attestation"
        );
        admit_catalog(
            bytes,
            &target.kubernetes.namespace,
            &setup.service,
            setup.service_port,
        )
    }
}
fn identity(object: &Object, uid: uuid::Uuid, version: &str) -> Result<()> {
    ensure!(
        object.metadata.uid == uid
            && object.metadata.resource_version == version
            && object.metadata.deletion_timestamp.is_none(),
        "Gateway routing resource identity/version changed"
    );
    Ok(())
}
async fn read(target: &InstallationTarget, kind: &str, name: &str) -> Result<Object> {
    let mut command = tokio::process::Command::new("kubectl");
    command
        .args([
            "--context",
            &target.kubernetes.context,
            "--namespace",
            &target.kubernetes.namespace,
            "get",
            kind,
            name,
            "-o",
            "json",
        ])
        .kill_on_drop(true);
    let output = veoveo_testing_support::output_async(command, Duration::from_secs(15))
        .await
        .map_err(|_| anyhow::anyhow!("Gateway routing read failed"))?;
    ensure!(
        output.status.success() && output.stdout.len() <= 4 * 1024 * 1024,
        "Gateway routing read failed or exceeded limit"
    );
    serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("Gateway routing observation malformed"))
}

fn admit_catalog(bytes: &[u8], namespace: &str, service: &str, port: u16) -> Result<()> {
    // Use the same canonical decoder as GatewayCatalog::load_json. Runtime
    // activation is attested by Ops; a local candidate file cannot establish it.
    let catalog: GatewayControlPlane = serde_json::from_slice(bytes)?;
    let mut servers = catalog
        .servers
        .iter()
        .filter(|server| server.slug.as_str() == "time");
    let time = servers.next().context("active Time binding absent")?;
    ensure!(servers.next().is_none(), "active Time binding repeated");
    let url = url::Url::parse(time.upstream.url.as_str())?;
    let host = url.host_str().context("active Time upstream host absent")?;
    let mut admitted_hosts = [
        service.to_owned(),
        format!("{}.{}", service, namespace),
        format!("{}.{}.svc", service, namespace),
        format!("{}.{}.svc.cluster.local", service, namespace),
    ];
    admitted_hosts.sort();
    ensure!(
        url.scheme() == "http"
            && admitted_hosts.iter().any(|value| value == host)
            && url.port_or_known_default() == Some(port)
            && url.path() == "/time/mcp"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && time.mount_path.as_str() == "/time"
            && time.mcp_path.as_str() == "/time/mcp"
            && time.uri_scheme.as_str() == "time",
        "active Time upstream does not bind selected Service/port/mount"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_routing_rejects_foreign_service_port_and_mount() -> Result<()> {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/bioma/gateway.json"
        ));
        admit_catalog(bytes, "fixture", "time-mcp", 8800)?;
        for wrong in [
            "http://foreign:8800/time/mcp",
            "http://time-mcp:8801/time/mcp",
            "http://time-mcp:8800/other/mcp",
        ] {
            let mut catalog: GatewayControlPlane = serde_json::from_slice(bytes)?;
            let time = catalog
                .servers
                .iter_mut()
                .find(|server| server.slug.as_str() == "time")
                .unwrap();
            time.upstream.url = wrong.parse()?;
            ensure!(
                admit_catalog(&serde_json::to_vec(&catalog)?, "fixture", "time-mcp", 8800).is_err()
            );
        }
        Ok(())
    }
}
