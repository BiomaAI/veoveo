//! Explicit local or shared GPU prerequisite; observations never provision it.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Selection {
    pub namespace: String,
    pub namespace_uid: uuid::Uuid,
    pub deployment: String,
    pub deployment_uid: uuid::Uuid,
    pub service: String,
    pub service_uid: uuid::Uuid,
    pub port: u16,
    pub runtime: QualifiedEmbeddingRuntime,
}
impl Selection {
    pub fn admit(&self) -> Result<()> {
        for value in [&self.namespace, &self.deployment, &self.service] {
            name(value)?;
        }
        ensure!(
            !self.namespace_uid.is_nil()
                && !self.deployment_uid.is_nil()
                && !self.service_uid.is_nil()
                && self.port == 8000,
            "invalid selected GPU runtime identity or port"
        );
        ensure!(
            !["computer-host", "agent-kernel"].contains(&self.deployment.as_str()),
            "forbidden embedding workload role"
        );
        self.runtime
            .qualification_for(self.runtime.profile().id())?;
        Ok(())
    }
    fn endpoint(&self) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse("http://embedding.invalid/")?;
        url.set_host(Some(&format!(
            "{}.{}.svc.cluster.local",
            self.service, self.namespace
        )))?;
        url.set_port(Some(self.port))
            .map_err(|_| anyhow::anyhow!("invalid embedding service port"))?;
        Ok(url)
    }
    pub fn admit_endpoint(&self, spec: &PodSpec, caller_namespace: &str) -> Result<()> {
        let containers = spec
            .containers
            .iter()
            .filter(|c| c.name == "knowledge-mcp")
            .collect::<Vec<_>>();
        ensure!(containers.len() == 1, "Knowledge container role absent");
        let env = containers[0]
            .env
            .iter()
            .filter(|e| e.name == "VEOVEO_EMBEDDING_ENDPOINT")
            .collect::<Vec<_>>();
        ensure!(
            env.len() == 1 && env[0].value_from.is_none(),
            "Knowledge embedding endpoint must be an explicit single origin"
        );
        let value = env[0]
            .value
            .as_ref()
            .context("Knowledge embedding endpoint absent")?;
        let endpoint = reqwest::Url::parse(value)
            .map_err(|_| anyhow::anyhow!("Knowledge embedding endpoint invalid"))?;
        let mut local = self.endpoint()?;
        local.set_host(Some(&self.service))?;
        ensure!(
            (endpoint == self.endpoint()?
                || (caller_namespace == self.namespace && endpoint == local))
                && endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.query().is_none()
                && endpoint.fragment().is_none()
                && endpoint.path() == "/",
            "Knowledge embedding endpoint differs from selected Service FQDN"
        );
        Ok(())
    }
    fn admit_namespace(&self, metadata: &Metadata) -> Result<()> {
        ensure!(
            metadata.name == self.namespace
                && metadata.uid == self.namespace_uid
                && metadata.deletion_timestamp.is_none(),
            "embedding namespace identity changed"
        );
        Ok(())
    }
    fn admit_deployment(&self, deployment: &Deployment) -> Result<()> {
        ensure!(
            deployment
                .metadata
                .labels
                .get("app.kubernetes.io/component")
                .map(String::as_str)
                == Some("embedding")
                && deployment.metadata.name == self.deployment
                && deployment.metadata.uid == self.deployment_uid
                && deployment.metadata.deletion_timestamp.is_none()
                && deployment
                    .spec
                    .selector
                    .match_labels
                    .get("app.kubernetes.io/component")
                    .map(String::as_str)
                    == Some("embedding"),
            "selected embedding Deployment differs or has forbidden role"
        );
        ensure!(
            !deployment.spec.template.spec.containers.iter().any(|c| [
                "computer-host",
                "agent-kernel"
            ]
            .contains(&c.name.as_str())),
            "forbidden container in embedding workload"
        );
        super::admit_embedding(
            deployment,
            &self.namespace,
            self.runtime.profile().contents().runtime_image.hex(),
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServicePort {
    port: u16,
    target_port: TargetPort,
    protocol: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum TargetPort {
    Number(u16),
    Name(String),
}
#[derive(Deserialize)]
struct ServiceSpec {
    selector: BTreeMap<String, String>,
    ports: Vec<ServicePort>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "externalName")]
    external_name: Option<String>,
}
#[derive(Deserialize)]
struct Service {
    metadata: Metadata,
    spec: ServiceSpec,
}
#[derive(Deserialize)]
struct List<T> {
    items: Vec<T>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    kind: String,
    namespace: String,
    name: String,
    uid: uuid::Uuid,
}
#[derive(Deserialize)]
struct Conditions {
    ready: Option<bool>,
    terminating: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    target_ref: Option<Reference>,
    conditions: Conditions,
    addresses: Vec<std::net::IpAddr>,
}
#[derive(Deserialize)]
struct EndpointPort {
    port: Option<u16>,
    protocol: Option<String>,
}
#[derive(Deserialize)]
struct Slice {
    metadata: Metadata,
    endpoints: Vec<Endpoint>,
    ports: Vec<EndpointPort>,
}

pub(super) fn admit_caller_namespace(input: &Input, metadata: &Metadata) -> Result<()> {
    ensure!(
        metadata.name == input.namespace
            && metadata.uid == input.namespace_uid
            && metadata.deletion_timestamp.is_none(),
        "Knowledge namespace identity differs"
    );
    if input.namespace != input.embedding.namespace {
        ensure!(
            metadata.labels.get("veoveo.ai/embedding-consumer")
                == Some(&input.namespace_uid.to_string()),
            "shared embedding consumer namespace label absent"
        );
    }
    Ok(())
}
fn admit_service(selection: &Selection, deployment: &Deployment, service: &Service) -> Result<()> {
    ensure!(
        service.metadata.name == selection.service
            && service.metadata.namespace == selection.namespace
            && service.metadata.uid == selection.service_uid
            && service.metadata.deletion_timestamp.is_none()
            && service.spec.external_name.is_none()
            && service
                .spec
                .kind
                .as_deref()
                .is_none_or(|kind| kind == "ClusterIP"),
        "selected embedding Service identity or type differs"
    );
    ensure!(
        !service.spec.selector.is_empty()
            && service.spec.selector == deployment.spec.selector.match_labels,
        "embedding Service selector differs from selected Deployment"
    );
    let target_matches = match &service
        .spec
        .ports
        .first()
        .context("embedding Service port absent")?
        .target_port
    {
        TargetPort::Number(port) => *port == selection.port,
        TargetPort::Name(name) => {
            let ports = deployment
                .spec
                .template
                .spec
                .containers
                .iter()
                .filter(|c| c.name == "embedding")
                .flat_map(|c| &c.ports)
                .filter(|p| p.name.as_ref() == Some(name))
                .collect::<Vec<_>>();
            ports.len() == 1
                && ports[0].container_port == selection.port
                && ports[0].protocol.as_deref().is_none_or(|p| p == "TCP")
        }
    };
    ensure!(
        service.spec.ports.len() == 1
            && service.spec.ports[0].port == selection.port
            && service.spec.ports[0]
                .protocol
                .as_deref()
                .is_none_or(|p| p == "TCP")
            && target_matches,
        "embedding Service port differs"
    );
    Ok(())
}
fn admit_slices<'a>(selection: &Selection, slices: &'a List<Slice>) -> Result<&'a Endpoint> {
    ensure!(
        !slices.items.is_empty() && slices.items.len() <= 16,
        "embedding EndpointSlice inventory missing or oversized"
    );
    let mut selected = None;
    for slice in &slices.items {
        ensure!(
            slice.metadata.namespace == selection.namespace
                && slice.metadata.deletion_timestamp.is_none()
                && slice.metadata.labels.get("kubernetes.io/service-name")
                    == Some(&selection.service)
                && slice
                    .metadata
                    .owner_references
                    .iter()
                    .any(|o| o.kind == "Service"
                        && o.name == selection.service
                        && o.uid == selection.service_uid
                        && o.controller == Some(true)),
            "embedding EndpointSlice has foreign Service owner"
        );
        ensure!(
            slice.ports.len() == 1
                && slice.ports[0].port == Some(selection.port)
                && slice.ports[0].protocol.as_deref() == Some("TCP"),
            "embedding EndpointSlice port differs"
        );
        for endpoint in &slice.endpoints {
            ensure!(
                selected.is_none()
                    && endpoint.conditions.ready == Some(true)
                    && endpoint.conditions.terminating != Some(true)
                    && !endpoint.addresses.is_empty(),
                "embedding route is not one ready selected GPU Pod"
            );
            let target = endpoint
                .target_ref
                .as_ref()
                .context("embedding endpoint Pod reference absent")?;
            ensure!(
                target.kind == "Pod"
                    && target.namespace == selection.namespace
                    && !target.uid.is_nil(),
                "embedding endpoint has foreign target"
            );
            name(&target.name)?;
            selected = Some(endpoint);
        }
    }
    selected.context("embedding endpoint absent")
}
pub(super) async fn observe(input: &Input) -> Result<()> {
    let selected = &input.embedding;
    let namespace: Object = get(
        &input.context,
        &selected.namespace,
        "namespace",
        &selected.namespace,
    )
    .await?;
    selected.admit_namespace(&namespace.metadata)?;
    let deployment: Deployment = get(
        &input.context,
        &selected.namespace,
        "deployment",
        &selected.deployment,
    )
    .await?;
    selected.admit_deployment(&deployment)?;
    let service: Service = get(
        &input.context,
        &selected.namespace,
        "service",
        &selected.service,
    )
    .await?;
    admit_service(selected, &deployment, &service)?;
    let mut command = command(&input.context, &selected.namespace);
    command.args([
        "get",
        "endpointslices",
        "-l",
        &format!("kubernetes.io/service-name={}", selected.service),
        "-o",
        "json",
    ]);
    let output = veoveo_testing_support::output_async(command, Duration::from_secs(10))
        .await
        .map_err(|_| anyhow::anyhow!("embedding endpoint inventory failed"))?;
    ensure!(
        output.status.success() && output.stdout.len() <= 1024 * 1024,
        "embedding endpoint inventory unavailable or oversized"
    );
    let slices: List<Slice> = serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("embedding endpoint inventory invalid"))?;
    let endpoint = admit_slices(selected, &slices)?;
    let target = endpoint
        .target_ref
        .as_ref()
        .context("embedding endpoint target absent")?;
    let pod: Pod = get(&input.context, &selected.namespace, "pod", &target.name).await?;
    ensure!(
        pod.metadata.uid == target.uid
            && pod.metadata.namespace == selected.namespace
            && pod.metadata.deletion_timestamp.is_none()
            && deployment
                .spec
                .selector
                .match_labels
                .iter()
                .all(|(k, v)| pod.metadata.labels.get(k) == Some(v)),
        "embedding endpoint Pod identity/selector differs"
    );
    let status = pod.status.as_ref().context("embedding Pod status absent")?;
    ensure!(
        endpoint
            .addresses
            .iter()
            .all(|ip| Some(*ip) == status.pod_ip),
        "embedding EndpointSlice address differs from selected Pod"
    );
    ensure!(
        status
            .conditions
            .iter()
            .any(|c| c.kind == PodConditionKind::Ready && c.status == PodConditionStatus::True)
            && status
                .container_statuses
                .iter()
                .any(|c| c.name == "embedding"
                    && c.ready
                    && !c.container_id.is_empty()
                    && !c.image_id.is_empty()),
        "embedding Pod is not ready"
    );
    let projected = Deployment {
        metadata: deployment.metadata.clone(),
        spec: DeploymentSpec {
            replicas: 1,
            selector: Selector {
                match_labels: deployment.spec.selector.match_labels.clone(),
            },
            template: Template { spec: pod.spec },
        },
    };
    selected.admit_deployment(&projected)?;
    let owner = pod
        .metadata
        .owner_references
        .iter()
        .filter(|o| o.kind == "ReplicaSet" && o.controller == Some(true))
        .collect::<Vec<_>>();
    ensure!(owner.len() == 1, "embedding Pod ReplicaSet owner absent");
    let rs: Object = get(
        &input.context,
        &selected.namespace,
        "replicaset",
        &owner[0].name,
    )
    .await?;
    ensure!(
        rs.metadata.uid == owner[0].uid
            && rs.metadata.namespace == selected.namespace
            && rs.metadata.deletion_timestamp.is_none()
            && rs
                .metadata
                .owner_references
                .iter()
                .any(|o| o.kind == "Deployment"
                    && o.name == selected.deployment
                    && o.uid == selected.deployment_uid
                    && o.controller == Some(true)),
        "embedding Pod does not belong to selected Deployment"
    );
    Ok(())
}

#[test]
fn cold_start_explicit_embedding_route_rejects_shadow_and_foreign_identity() -> Result<()> {
    let selection = Selection {
        namespace: "gpu".into(),
        namespace_uid: uuid::Uuid::from_u128(10),
        deployment: "embedding".into(),
        deployment_uid: uuid::Uuid::from_u128(11),
        service: "embedding".into(),
        service_uid: uuid::Uuid::from_u128(12),
        port: 8000,
        runtime: crate::indexing::SyntheticEmbeddings::new().runtime,
    };
    selection.admit()?;
    let mut namespace: Metadata = serde_json::from_value(
        serde_json::json!({"name":"gpu","uid":selection.namespace_uid,"resourceVersion":"1"}),
    )?;
    selection.admit_namespace(&namespace)?;
    namespace.uid = uuid::Uuid::from_u128(99);
    ensure!(selection.admit_namespace(&namespace).is_err());
    namespace.uid = selection.namespace_uid;
    namespace.name = "foreign".into();
    ensure!(selection.admit_namespace(&namespace).is_err());
    let mut knowledge = control_pod(false, 0).spec;
    knowledge.containers[0].env[0].value = Some(selection.endpoint()?.to_string());
    selection.admit_endpoint(&knowledge, "isolated")?;
    for endpoint in [
        "http://embedding:8000",
        "http://embedding.other.svc.cluster.local:8000",
        "http://private@embedding.gpu.svc.cluster.local:8000",
        "http://embedding.gpu.svc.cluster.local:8000/v1",
        "http://embedding.gpu.svc.cluster.local:8000/?token=private",
    ] {
        knowledge.containers[0].env[0].value = Some(endpoint.into());
        ensure!(
            selection.admit_endpoint(&knowledge, "isolated").is_err(),
            "foreign or nonorigin embedding endpoint admitted"
        );
    }
    knowledge.containers[0].env[0].value = Some("http://embedding:8000".into());
    selection.admit_endpoint(&knowledge, "gpu")?;
    ensure!(selection.admit_endpoint(&knowledge, "isolated").is_err());
    let labels = serde_json::json!({"app.kubernetes.io/component":"embedding","app.kubernetes.io/instance":"fixture"});
    let digest = selection.runtime.profile().contents().runtime_image.hex();
    let deployment_wire = serde_json::json!({"metadata":{"name":"embedding","namespace":"gpu","uid":selection.deployment_uid,"resourceVersion":"1","labels":labels},"spec":{"replicas":1,"selector":{"matchLabels":labels},"template":{"spec":{"containers":[{"name":"embedding","ports":[{"name":"http","containerPort":8000,"protocol":"TCP"}],"image":format!("registry.invalid/embedding@sha256:{digest}"),"resources":{"requests":{"nvidia.com/gpu":"1"},"limits":{"nvidia.com/gpu":"1"}}}]}}}});
    let deployment: Deployment = serde_json::from_value(deployment_wire.clone())?;
    selection.admit_deployment(&deployment)?;
    for (pointer, value) in [
        ("/metadata/namespace", serde_json::json!("foreign")),
        (
            "/metadata/uid",
            serde_json::json!(uuid::Uuid::from_u128(99)),
        ),
        (
            "/metadata/labels/app.kubernetes.io~1component",
            serde_json::json!("computer-host"),
        ),
        (
            "/spec/template/spec/containers/0/name",
            serde_json::json!("agent-kernel"),
        ),
        (
            "/spec/template/spec/containers/0/image",
            serde_json::json!(format!(
                "registry.invalid/embedding@sha256:{}",
                "b".repeat(64)
            )),
        ),
    ] {
        let mut changed = deployment_wire.clone();
        *changed
            .pointer_mut(pointer)
            .context("native deployment field absent")? = value;
        ensure!(
            selection
                .admit_deployment(&serde_json::from_value(changed)?)
                .is_err()
        );
    }
    let service_wire = serde_json::json!({"metadata":{"name":"embedding","namespace":"gpu","uid":selection.service_uid,"resourceVersion":"2"},"spec":{"type":"ClusterIP","selector":labels,"ports":[{"port":8000,"targetPort":"http","protocol":"TCP"}]}});
    let service: Service = serde_json::from_value(service_wire.clone())?;
    admit_service(&selection, &deployment, &service)?;
    for (pointer, value) in [
        (
            "/metadata/uid",
            serde_json::json!(uuid::Uuid::from_u128(99)),
        ),
        (
            "/spec/selector/app.kubernetes.io~1component",
            serde_json::json!("computer-host"),
        ),
        ("/spec/ports/0/targetPort", serde_json::json!(9000)),
    ] {
        let mut changed = service_wire.clone();
        *changed
            .pointer_mut(pointer)
            .context("native service field absent")? = value;
        ensure!(admit_service(&selection, &deployment, &serde_json::from_value(changed)?).is_err());
    }
    let slices_wire = serde_json::json!({"items":[{"metadata":{"name":"gpu-slice","namespace":"gpu","uid":uuid::Uuid::from_u128(13),"resourceVersion":"3","labels":{"kubernetes.io/service-name":"embedding"},"ownerReferences":[{"kind":"Service","name":"embedding","uid":selection.service_uid,"controller":true}]},"ports":[{"port":8000,"protocol":"TCP"}],"endpoints":[{"addresses":["10.0.0.1"],"conditions":{"ready":true,"terminating":false},"targetRef":{"kind":"Pod","namespace":"gpu","name":"embedding-pod","uid":uuid::Uuid::from_u128(14)}}]}]});
    let slices: List<Slice> = serde_json::from_value(slices_wire.clone())?;
    ensure!(
        admit_slices(&selection, &slices)?
            .target_ref
            .as_ref()
            .context("native target")?
            .name
            == "embedding-pod"
    );
    for (pointer, value) in [
        (
            "/items/0/metadata/ownerReferences/0/uid",
            serde_json::json!(uuid::Uuid::from_u128(99)),
        ),
        (
            "/items/0/endpoints/0/conditions/ready",
            serde_json::json!(false),
        ),
        (
            "/items/0/endpoints/0/targetRef/namespace",
            serde_json::json!("foreign"),
        ),
        (
            "/items/0/endpoints/0/targetRef/kind",
            serde_json::json!("Service"),
        ),
    ] {
        let mut changed = slices_wire.clone();
        *changed
            .pointer_mut(pointer)
            .context("native endpoint field absent")? = value;
        ensure!(admit_slices(&selection, &serde_json::from_value(changed)?).is_err());
    }
    Ok(())
}
