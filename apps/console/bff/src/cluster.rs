use std::{collections::BTreeMap, fs, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use reqwest::{
    Certificate, StatusCode, Url,
    header::{AUTHORIZATION, HeaderValue},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{AppState, api::authorize_cluster_inventory};

const TOKEN_PATH: &str = "/var/run/secrets/veoveo-console/token";
const CA_PATH: &str = "/var/run/secrets/veoveo-console/ca.crt";

#[derive(Clone)]
pub(crate) struct KubernetesClient {
    http: reqwest::Client,
    api: Url,
    namespace: String,
}

impl KubernetesClient {
    pub(crate) fn from_env() -> Result<Option<Self>> {
        let enabled = std::env::var("VEOVEO_CLUSTER_INSPECTION_ENABLED")
            .unwrap_or_else(|_| "false".to_owned());
        match enabled.as_str() {
            "false" => return Ok(None),
            "true" => {}
            _ => bail!("VEOVEO_CLUSTER_INSPECTION_ENABLED must be true or false"),
        }
        let namespace = std::env::var("VEOVEO_KUBERNETES_NAMESPACE")
            .context("VEOVEO_KUBERNETES_NAMESPACE is required for cluster inspection")?;
        if namespace.is_empty()
            || !namespace
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            bail!("VEOVEO_KUBERNETES_NAMESPACE must be a DNS label");
        }
        let host = std::env::var("KUBERNETES_SERVICE_HOST")
            .context("KUBERNETES_SERVICE_HOST is required for cluster inspection")?;
        let port = std::env::var("KUBERNETES_SERVICE_PORT_HTTPS")
            .or_else(|_| std::env::var("KUBERNETES_SERVICE_PORT"))
            .context("KUBERNETES_SERVICE_PORT is required for cluster inspection")?;
        let api = Url::parse(&format!("https://{host}:{port}/"))
            .context("building Kubernetes API URL")?;
        let token = fs::read_to_string(TOKEN_PATH)
            .with_context(|| format!("reading Kubernetes token {TOKEN_PATH}"))?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.trim()))
            .context("building Kubernetes authorization header")?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, authorization);
        let certificate = Certificate::from_pem(
            &fs::read(CA_PATH).with_context(|| format!("reading Kubernetes CA {CA_PATH}"))?,
        )
        .context("decoding Kubernetes CA")?;
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .add_root_certificate(certificate)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .context("building Kubernetes API client")?;
        Ok(Some(Self {
            http,
            api,
            namespace,
        }))
    }

    async fn list<T: DeserializeOwned>(&self, group: &str, resource: &str) -> Result<Vec<T>> {
        let path = if group == "core" {
            format!("api/v1/namespaces/{}/{resource}", self.namespace)
        } else {
            format!("apis/{group}/namespaces/{}/{resource}", self.namespace)
        };
        let response = self
            .http
            .get(self.api.join(&path)?)
            .send()
            .await
            .with_context(|| format!("requesting Kubernetes {resource}"))?
            .error_for_status()
            .with_context(|| format!("Kubernetes rejected {resource} inventory"))?;
        Ok(response.json::<KubernetesList<T>>().await?.items)
    }

    async fn snapshot(&self) -> Result<ClusterSnapshot> {
        let (
            deployments,
            stateful_sets,
            jobs,
            pods,
            services,
            claims,
            ingresses,
            network_policies,
            disruption_budgets,
            config_maps,
        ) = tokio::try_join!(
            self.list::<Deployment>("apps/v1", "deployments"),
            self.list::<StatefulSet>("apps/v1", "statefulsets"),
            self.list::<Job>("batch/v1", "jobs"),
            self.list::<Pod>("core", "pods"),
            self.list::<Service>("core", "services"),
            self.list::<PersistentVolumeClaim>("core", "persistentvolumeclaims"),
            self.list::<Ingress>("networking.k8s.io/v1", "ingresses"),
            self.list::<NamedResource>("networking.k8s.io/v1", "networkpolicies"),
            self.list::<NamedResource>("policy/v1", "poddisruptionbudgets"),
            self.list::<NamedResource>("core", "configmaps"),
        )?;

        let mut workloads = deployments
            .into_iter()
            .map(ClusterWorkload::from)
            .chain(stateful_sets.into_iter().map(ClusterWorkload::from))
            .chain(jobs.into_iter().map(ClusterWorkload::from))
            .collect::<Vec<_>>();
        workloads.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(ClusterSnapshot {
            orchestrator: "Kubernetes",
            namespace: self.namespace.clone(),
            generated_at: Utc::now(),
            workloads,
            pods: pods.into_iter().map(ClusterPod::from).collect(),
            services: services.into_iter().map(ClusterService::from).collect(),
            storage: claims.into_iter().map(ClusterStorage::from).collect(),
            ingresses: ingresses.into_iter().map(ClusterIngress::from).collect(),
            network_policies: names(network_policies),
            disruption_budgets: names(disruption_budgets),
            config_maps: names(config_maps),
        })
    }
}

pub(crate) async fn snapshot(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let headers = match authorize_cluster_inventory(&state, &request_headers).await {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let Some(cluster) = &state.cluster else {
        return (headers, StatusCode::SERVICE_UNAVAILABLE).into_response();
    };
    match cluster.snapshot().await {
        Ok(snapshot) => (headers, Json(snapshot)).into_response(),
        Err(error) => {
            tracing::error!(%error, "Kubernetes cluster inventory failed");
            (headers, StatusCode::BAD_GATEWAY).into_response()
        }
    }
}

fn names(mut resources: Vec<NamedResource>) -> Vec<String> {
    let mut names = resources
        .drain(..)
        .map(|resource| resource.metadata.name)
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[derive(Deserialize)]
struct KubernetesList<T> {
    items: Vec<T>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    name: String,
    #[serde(default)]
    creation_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct NamedResource {
    metadata: Metadata,
}

#[derive(Deserialize)]
struct Deployment {
    metadata: Metadata,
    spec: ReplicaSpec,
    #[serde(default)]
    status: ReplicaStatus,
}

#[derive(Deserialize)]
struct StatefulSet {
    metadata: Metadata,
    spec: ReplicaSpec,
    #[serde(default)]
    status: ReplicaStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaSpec {
    #[serde(default)]
    replicas: u32,
    template: PodTemplate,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaStatus {
    #[serde(default)]
    ready_replicas: u32,
    #[serde(default)]
    available_replicas: u32,
}

#[derive(Deserialize)]
struct PodTemplate {
    spec: PodSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    metadata: Metadata,
    spec: JobSpec,
    #[serde(default)]
    status: JobStatus,
}

#[derive(Deserialize)]
struct JobSpec {
    template: PodTemplate,
    #[serde(default = "one")]
    completions: u32,
}

#[derive(Default, Deserialize)]
struct JobStatus {
    #[serde(default)]
    succeeded: u32,
    #[serde(default)]
    active: u32,
}

const fn one() -> u32 {
    1
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodSpec {
    #[serde(default)]
    node_name: Option<String>,
    containers: Vec<ContainerSpec>,
}

#[derive(Clone, Deserialize)]
struct ContainerSpec {
    image: String,
}

#[derive(Deserialize)]
struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    #[serde(default)]
    status: PodStatus,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodStatus {
    #[serde(default)]
    phase: String,
    #[serde(default)]
    container_statuses: Vec<ContainerStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerStatus {
    ready: bool,
    restart_count: u32,
}

#[derive(Deserialize)]
struct Service {
    metadata: Metadata,
    spec: ServiceSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceSpec {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    cluster_ip: Option<String>,
    #[serde(default)]
    ports: Vec<ServicePort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServicePort {
    #[serde(default)]
    name: Option<String>,
    port: u16,
    #[serde(default)]
    node_port: Option<u16>,
}

#[derive(Deserialize)]
struct PersistentVolumeClaim {
    metadata: Metadata,
    spec: PersistentVolumeClaimSpec,
    #[serde(default)]
    status: PersistentVolumeClaimStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistentVolumeClaimSpec {
    #[serde(default)]
    access_modes: Vec<String>,
    #[serde(default)]
    storage_class_name: Option<String>,
    resources: ResourceRequirements,
}

#[derive(Default, Deserialize)]
struct ResourceRequirements {
    #[serde(default)]
    requests: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
struct PersistentVolumeClaimStatus {
    #[serde(default)]
    phase: String,
    #[serde(default)]
    capacity: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Ingress {
    metadata: Metadata,
    spec: IngressSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngressSpec {
    #[serde(default)]
    ingress_class_name: Option<String>,
    #[serde(default)]
    rules: Vec<IngressRule>,
}

#[derive(Deserialize)]
struct IngressRule {
    #[serde(default)]
    host: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterSnapshot {
    orchestrator: &'static str,
    namespace: String,
    generated_at: DateTime<Utc>,
    workloads: Vec<ClusterWorkload>,
    pods: Vec<ClusterPod>,
    services: Vec<ClusterService>,
    storage: Vec<ClusterStorage>,
    ingresses: Vec<ClusterIngress>,
    network_policies: Vec<String>,
    disruption_budgets: Vec<String>,
    config_maps: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterWorkload {
    name: String,
    kind: &'static str,
    desired: u32,
    ready: u32,
    available: u32,
    images: Vec<String>,
    created_at: Option<DateTime<Utc>>,
}

impl From<Deployment> for ClusterWorkload {
    fn from(value: Deployment) -> Self {
        Self {
            name: value.metadata.name,
            kind: "Deployment",
            desired: value.spec.replicas,
            ready: value.status.ready_replicas,
            available: value.status.available_replicas,
            images: value
                .spec
                .template
                .spec
                .containers
                .into_iter()
                .map(|item| item.image)
                .collect(),
            created_at: value.metadata.creation_timestamp,
        }
    }
}

impl From<StatefulSet> for ClusterWorkload {
    fn from(value: StatefulSet) -> Self {
        Self {
            name: value.metadata.name,
            kind: "StatefulSet",
            desired: value.spec.replicas,
            ready: value.status.ready_replicas,
            available: value.status.ready_replicas,
            images: value
                .spec
                .template
                .spec
                .containers
                .into_iter()
                .map(|item| item.image)
                .collect(),
            created_at: value.metadata.creation_timestamp,
        }
    }
}

impl From<Job> for ClusterWorkload {
    fn from(value: Job) -> Self {
        Self {
            name: value.metadata.name,
            kind: "Job",
            desired: value.spec.completions,
            ready: value.status.succeeded,
            available: value.status.active + value.status.succeeded,
            images: value
                .spec
                .template
                .spec
                .containers
                .into_iter()
                .map(|item| item.image)
                .collect(),
            created_at: value.metadata.creation_timestamp,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterPod {
    name: String,
    component: Option<String>,
    phase: String,
    ready: u32,
    containers: u32,
    restarts: u32,
    node: Option<String>,
    images: Vec<String>,
}

impl From<Pod> for ClusterPod {
    fn from(value: Pod) -> Self {
        let statuses = value.status.container_statuses;
        Self {
            name: value.metadata.name,
            component: value
                .metadata
                .labels
                .get("app.kubernetes.io/component")
                .cloned(),
            phase: value.status.phase,
            ready: statuses.iter().filter(|item| item.ready).count() as u32,
            containers: value.spec.containers.len() as u32,
            restarts: statuses.iter().map(|item| item.restart_count).sum(),
            node: value.spec.node_name,
            images: value
                .spec
                .containers
                .into_iter()
                .map(|item| item.image)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterService {
    name: String,
    kind: String,
    cluster_ip: Option<String>,
    ports: Vec<String>,
}

impl From<Service> for ClusterService {
    fn from(value: Service) -> Self {
        Self {
            name: value.metadata.name,
            kind: value.spec.kind,
            cluster_ip: value.spec.cluster_ip,
            ports: value
                .spec
                .ports
                .into_iter()
                .map(|port| {
                    let label = port.name.unwrap_or_else(|| "tcp".to_owned());
                    match port.node_port {
                        Some(node_port) => format!("{label}:{} → {node_port}", port.port),
                        None => format!("{label}:{}", port.port),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterStorage {
    name: String,
    phase: String,
    requested: Option<String>,
    capacity: Option<String>,
    storage_class: Option<String>,
    access_modes: Vec<String>,
}

impl From<PersistentVolumeClaim> for ClusterStorage {
    fn from(value: PersistentVolumeClaim) -> Self {
        Self {
            name: value.metadata.name,
            phase: value.status.phase,
            requested: value.spec.resources.requests.get("storage").cloned(),
            capacity: value.status.capacity.get("storage").cloned(),
            storage_class: value.spec.storage_class_name,
            access_modes: value.spec.access_modes,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClusterIngress {
    name: String,
    class_name: Option<String>,
    hosts: Vec<String>,
}

impl From<Ingress> for ClusterIngress {
    fn from(value: Ingress) -> Self {
        Self {
            name: value.metadata.name,
            class_name: value.spec.ingress_class_name,
            hosts: value
                .spec
                .rules
                .into_iter()
                .filter_map(|rule| rule.host)
                .collect(),
        }
    }
}
