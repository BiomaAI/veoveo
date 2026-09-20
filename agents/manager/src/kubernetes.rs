//! Namespace-scoped Kubernetes requests and bounded native watch recovery.
use anyhow::{Context, Result, bail, ensure};
use futures::StreamExt;
use reqwest::{Client, Method, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use std::{path::PathBuf, time::Duration};
use tokio::sync::watch;

use crate::kubernetes_types::*;

pub const OWNER_LABEL: &str = "veoveo.ai/managed-agent";
pub const GENERATION: &str = "veoveo.ai/managed-generation";
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct ApiFailure(pub StatusCode);
impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Kubernetes request rejected with {}", self.0)
    }
}
impl std::error::Error for ApiFailure {}
pub fn retryable(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<ApiFailure>().is_some_and(|failure| {
            failure.0.is_server_error()
                || matches!(
                    failure.0,
                    StatusCode::CONFLICT | StatusCode::TOO_MANY_REQUESTS
                )
        }) || cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|failure| {
                failure.is_timeout() || failure.is_connect() || failure.is_body()
            })
    })
}

#[derive(Clone)]
pub struct Kubernetes {
    http: Client,
    endpoint: String,
    namespace: String,
    token_file: PathBuf,
}

#[derive(Clone, Copy)]
pub enum Resource {
    Secrets,
    Claims,
    ConfigMaps,
    Deployments,
    Pods,
}
impl Resource {
    fn path(self) -> (&'static str, &'static str) {
        match self {
            Self::Secrets => ("api/v1", "secrets"),
            Self::Claims => ("api/v1", "persistentvolumeclaims"),
            Self::ConfigMaps => ("api/v1", "configmaps"),
            Self::Deployments => ("apis/apps/v1", "deployments"),
            Self::Pods => ("api/v1", "pods"),
        }
    }
}

impl Kubernetes {
    pub fn in_cluster(namespace: String) -> Result<Self> {
        let root = PathBuf::from("/var/run/secrets/kubernetes.io/serviceaccount");
        let ca = reqwest::Certificate::from_pem(&std::fs::read(root.join("ca.crt"))?)?;
        let http = Client::builder()
            .add_root_certificate(ca)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            endpoint: "https://kubernetes.default.svc".into(),
            namespace,
            token_file: root.join("token"),
        })
    }

    fn url(&self, resource: Resource, name: Option<&str>) -> String {
        let (api, plural) = resource.path();
        let mut url = format!(
            "{}/{api}/namespaces/{}/{plural}",
            self.endpoint, self.namespace
        );
        if let Some(name) = name {
            url.push('/');
            url.push_str(name);
        }
        url
    }

    async fn request(
        &self,
        method: Method,
        resource: Resource,
        name: Option<&str>,
    ) -> Result<reqwest::RequestBuilder> {
        // Projected service-account tokens rotate while the manager is alive.
        let token = SecretString::from(tokio::fs::read_to_string(&self.token_file).await?);
        Ok(self
            .http
            .request(method, self.url(resource, name))
            .bearer_auth(token.expose_secret().trim()))
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        resource: Resource,
        name: &str,
    ) -> Result<Option<T>> {
        let response = self
            .request(Method::GET, resource, Some(name))
            .await?
            .send()
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(decode(response).await?))
    }

    pub async fn create<T: Serialize, R: DeserializeOwned>(
        &self,
        resource: Resource,
        body: &T,
    ) -> Result<R> {
        decode(
            self.request(Method::POST, resource, None)
                .await?
                .json(body)
                .send()
                .await?,
        )
        .await
    }

    pub async fn replace<T: Serialize, R: DeserializeOwned>(
        &self,
        resource: Resource,
        name: &str,
        body: &T,
    ) -> Result<R> {
        decode(
            self.request(Method::PUT, resource, Some(name))
                .await?
                .json(body)
                .send()
                .await?,
        )
        .await
    }

    pub async fn delete(&self, resource: Resource, metadata: &Metadata) -> Result<()> {
        let body = DeleteOptions {
            api_version: "v1",
            kind: "DeleteOptions",
            propagation_policy: "Foreground",
            preconditions: Preconditions {
                uid: metadata.uid.clone().context("owned resource has no UID")?,
                resource_version: metadata
                    .resource_version
                    .clone()
                    .context("owned resource has no resourceVersion")?,
            },
        };
        let response = self
            .request(Method::DELETE, resource, Some(&metadata.name))
            .await?
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(ApiFailure(response.status()).into());
        }
        Ok(())
    }

    pub async fn pods(&self, owner: Option<&str>) -> Result<ResourceList<Pod>> {
        let selector = owner.map_or_else(
            || OWNER_LABEL.to_owned(),
            |owner| format!("{OWNER_LABEL}={owner}"),
        );
        decode(
            self.request(Method::GET, Resource::Pods, None)
                .await?
                .query(&[("labelSelector", selector), ("limit", "200".into())])
                .send()
                .await?,
        )
        .await
    }

    /// Inventory establishes the resource version before watch. A 410 or watch
    /// loss triggers a fresh inventory and a reconciliation hint.
    pub async fn watch_pods(self, changed: watch::Sender<u64>) {
        loop {
            let result = async {
                let inventory = self.pods(None).await?;
                changed.send_modify(|version| *version = version.wrapping_add(1));
                self.watch_from(&inventory.metadata.resource_version, &changed)
                    .await
            }
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, "managed Pod watch restarting from inventory");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    async fn watch_from(&self, version: &str, changed: &watch::Sender<u64>) -> Result<()> {
        let response = self
            .request(Method::GET, Resource::Pods, None)
            .await?
            .query(&[
                ("watch", "true"),
                ("resourceVersion", version),
                ("allowWatchBookmarks", "true"),
                ("timeoutSeconds", "50"),
                ("labelSelector", OWNER_LABEL),
            ])
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "Kubernetes watch rejected with {}",
            response.status()
        );
        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        while let Some(bytes) = stream.next().await {
            pending.extend_from_slice(&bytes?);
            ensure!(
                pending.len() <= MAX_RESPONSE,
                "Kubernetes watch frame exceeds limit"
            );
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<_> = pending.drain(..=end).collect();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                match serde_json::from_slice::<WatchEvent>(&line)? {
                    WatchEvent::Added(pod)
                    | WatchEvent::Modified(pod)
                    | WatchEvent::Deleted(pod) => {
                        ensure!(
                            pod.metadata.resource_version.is_some(),
                            "Pod event lacks resource version"
                        );
                        changed.send_modify(|version| *version = version.wrapping_add(1));
                    }
                    WatchEvent::Bookmark(bookmark) => {
                        ensure!(
                            bookmark.metadata.resource_version.is_some(),
                            "watch bookmark lacks resource version"
                        );
                    }
                    WatchEvent::Error(status) => {
                        bail!("Kubernetes watch ended with {}", status.code)
                    }
                }
            }
        }
        Ok(())
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    // Never include Kubernetes error bodies: admission errors can echo Secret data.
    if !response.status().is_success() {
        return Err(ApiFailure(response.status()).into());
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(bytes) = stream.next().await {
        body.extend_from_slice(&bytes?);
        ensure!(
            body.len() <= MAX_RESPONSE,
            "Kubernetes response exceeds limit"
        );
    }
    serde_json::from_slice(&body).context("invalid Kubernetes resource response")
}

pub fn owned(metadata: &Metadata, owner: &str) -> Result<()> {
    ensure!(
        metadata
            .labels
            .get(OWNER_LABEL)
            .is_some_and(|label| label == owner),
        "resource belongs to another controller"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn rejected_secret_response_is_redacted_and_conflicts_are_recoverable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut connection, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let count = connection.read(&mut request).await.unwrap();
            assert!(count > 0);
            let body = "upstream echoed PRIVATE_CREDENTIAL_MUST_NOT_APPEAR";
            connection.write_all(format!("HTTP/1.1 409 Conflict\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let response = Client::new()
            .get(format!("http://{address}/secret"))
            .send()
            .await
            .unwrap();
        let error = decode::<ConfigMap>(response).await.unwrap_err();
        assert!(retryable(&error));
        assert!(!error.to_string().contains("PRIVATE_CREDENTIAL"));
        server.await.unwrap();
        assert!(!retryable(&ApiFailure(StatusCode::FORBIDDEN).into()));
        assert!(retryable(
            &ApiFailure(StatusCode::SERVICE_UNAVAILABLE).into()
        ));
    }

    #[test]
    fn cleanup_requires_exact_resource_ownership() {
        let mut metadata = Metadata::default();
        assert!(owned(&metadata, "agent-one").is_err());
        metadata
            .labels
            .insert(OWNER_LABEL.into(), "agent-other".into());
        assert!(owned(&metadata, "agent-one").is_err());
        metadata
            .labels
            .insert(OWNER_LABEL.into(), "agent-one".into());
        assert!(owned(&metadata, "agent-one").is_ok());
    }
}
