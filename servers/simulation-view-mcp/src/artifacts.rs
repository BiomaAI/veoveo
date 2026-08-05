use std::{collections::BTreeSet, sync::Arc, time::Duration};

use anyhow::{Context, ensure};
use reqwest::{
    Body, Client, Url,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{PlaneCaller, parse_artifact_plane_uri};

use crate::contract::{GovernedArtifact, SceneDeclaration, SimulationViewError, VisualAssetFormat};

const MATERIALIZATION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub(crate) struct SceneArtifactMaterializer {
    artifact_plane: HttpArtifactPlane,
    artifact_service_endpoint: Url,
    renderer_endpoint: Url,
    renderer_control_token: Arc<str>,
    http: Client,
}

impl SceneArtifactMaterializer {
    pub(crate) fn new(
        artifact_service_endpoint: &str,
        renderer_endpoint: &str,
        renderer_control_token: &str,
    ) -> anyhow::Result<Arc<Self>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let artifact_service_endpoint =
            internal_endpoint(artifact_service_endpoint, "artifact service")?;
        let renderer_endpoint = internal_endpoint(renderer_endpoint, "renderer")?;
        ensure!(
            (32..=512).contains(&renderer_control_token.len())
                && !renderer_control_token.chars().any(char::is_whitespace),
            "renderer control token must contain 32 to 512 non-whitespace characters"
        );
        let http = Client::builder()
            .timeout(MATERIALIZATION_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let artifact_http = Client::builder()
            .timeout(MATERIALIZATION_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Arc::new(Self {
            artifact_plane: HttpArtifactPlane::with_client(
                artifact_service_endpoint.as_str(),
                artifact_http,
            ),
            artifact_service_endpoint,
            renderer_endpoint,
            renderer_control_token: Arc::from(renderer_control_token),
            http,
        }))
    }

    pub(crate) async fn ready(&self) -> bool {
        let endpoint = match self.artifact_service_endpoint.join("readyz") {
            Ok(endpoint) => endpoint,
            Err(_) => return false,
        };
        tokio::time::timeout(Duration::from_secs(2), self.http.get(endpoint).send())
            .await
            .is_ok_and(|response| response.is_ok_and(|response| response.status().is_success()))
    }

    pub(crate) async fn materialize(
        &self,
        caller: &PlaneCaller,
        scene: &SceneDeclaration,
    ) -> anyhow::Result<()> {
        let mut materialized = BTreeSet::new();
        for artifact in std::iter::once(&scene.body.environment).chain(
            scene
                .body
                .prototypes
                .iter()
                .map(|prototype| &prototype.asset),
        ) {
            let suffix = renderer_suffix(artifact.format)?;
            let key = (artifact.digest.as_str().to_owned(), suffix);
            if materialized.insert(key) {
                self.materialize_artifact(caller, artifact, suffix).await?;
            }
        }
        Ok(())
    }

    async fn materialize_artifact(
        &self,
        caller: &PlaneCaller,
        artifact: &GovernedArtifact,
        suffix: &'static str,
    ) -> anyhow::Result<()> {
        artifact
            .validate()
            .map_err(|error| anyhow::anyhow!(error))?;
        let artifact_id = parse_artifact_plane_uri(&artifact.artifact_uri)
            .ok_or_else(|| anyhow::anyhow!("scene artifact URI is not canonical"))?;
        let download = self
            .artifact_plane
            .download(caller, &artifact.artifact_uri)
            .await
            .context("authorizing governed scene artifact")?;
        ensure!(
            download.metadata.artifact_id == artifact_id
                && download.metadata.artifact_uri == artifact_id.plane_uri(),
            "artifact service returned a different governed occurrence"
        );
        ensure!(
            download.metadata.byte_len == artifact.byte_length,
            "artifact metadata byte length does not match the scene declaration"
        );
        if let Some(content_length) = download.response.content_length() {
            ensure!(
                content_length == artifact.byte_length,
                "artifact download byte length does not match the scene declaration"
            );
        }

        let hexadecimal = &artifact.digest.as_str()[7..];
        let endpoint = self
            .renderer_endpoint
            .join(&format!("v1/artifacts/sha256/{hexadecimal}.{suffix}"))?;
        let response = self
            .http
            .put(endpoint)
            .bearer_auth(&self.renderer_control_token)
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_LENGTH, artifact.byte_length)
            .body(Body::wrap_stream(download.response.bytes_stream()))
            .send()
            .await
            .context("streaming governed scene artifact to the renderer")?;
        response
            .error_for_status()
            .context("renderer rejected governed scene artifact")?;
        Ok(())
    }
}

fn renderer_suffix(format: VisualAssetFormat) -> Result<&'static str, SimulationViewError> {
    match format {
        VisualAssetFormat::Usd => Ok("usd"),
        VisualAssetFormat::Usdz => Ok("usdz"),
        VisualAssetFormat::Glb => Ok("glb"),
        VisualAssetFormat::Gltf => Ok("gltf"),
        VisualAssetFormat::Ktx2 | VisualAssetFormat::Png | VisualAssetFormat::Jpeg => {
            Err(SimulationViewError::InvalidArtifact)
        }
    }
}

fn internal_endpoint(value: &str, label: &str) -> anyhow::Result<Url> {
    let mut endpoint = Url::parse(value)?;
    ensure!(
        endpoint.scheme() == "http"
            && endpoint.host_str().is_some()
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.query().is_none()
            && endpoint.fragment().is_none(),
        "{label} endpoint must be a credential-free internal HTTP URL"
    );
    if !endpoint.path().ends_with('/') {
        endpoint.set_path(&format!("{}/", endpoint.path()));
    }
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{Arc, Mutex},
    };

    use axum::{
        Router,
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::{get, put},
    };
    use chrono::{TimeDelta, Utc};
    use sha2::{Digest, Sha256};
    use veoveo_mcp_contract::{
        AccessSubject, ArtifactId, ArtifactMetadata, ComplianceMetadata, DataLabelId,
        GatewayInternalIdentity, GatewayProfileId, InvocationAuthority, InvocationProvenance,
        JwtId, PolicyVersion, Principal, PrincipalId, PrincipalKind, ServerSlug, TenantId,
        TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel,
        WorkContextOutputPolicy,
    };
    use veoveo_simulation_pose::Sha256Digest;

    use super::*;

    #[derive(Clone)]
    struct ArtifactFixture {
        metadata: ArtifactMetadata,
        bytes: Bytes,
    }

    type ReceivedIngest = Arc<Mutex<Option<(String, Bytes)>>>;

    async fn metadata(State(fixture): State<ArtifactFixture>) -> impl IntoResponse {
        axum::Json(fixture.metadata)
    }

    async fn download(
        State(fixture): State<ArtifactFixture>,
        headers: HeaderMap,
    ) -> impl IntoResponse {
        if headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some("Bearer signed-simulation-view-token")
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        fixture.bytes.into_response()
    }

    async fn ingest(
        Path(filename): Path<String>,
        State(received): State<ReceivedIngest>,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl IntoResponse {
        let expected_authorization = format!("Bearer {}", "r".repeat(32));
        if headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some(expected_authorization.as_str())
        {
            return StatusCode::UNAUTHORIZED;
        }
        *received.lock().unwrap() = Some((filename, body));
        StatusCode::NO_CONTENT
    }

    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    fn caller() -> PlaneCaller {
        let now = Utc::now();
        let principal_id = PrincipalId::new("fixture-operator").unwrap();
        let tenant = TenantId::new("fixture").unwrap();
        let actor = Principal {
            id: principal_id.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.test").unwrap(),
            subject: TokenSubject::new("fixture-operator").unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(now),
        };
        PlaneCaller {
            bearer_token: "signed-simulation-view-token".to_owned(),
            memberships: BTreeSet::new(),
            identity: GatewayInternalIdentity {
                issuer: TokenIssuer::new("veoveo-internal").unwrap(),
                profile: GatewayProfileId::new("simulation").unwrap(),
                server: ServerSlug::new("simulation-view").unwrap(),
                actor,
                authority: InvocationAuthority {
                    work_context: WorkContextId::new("fixture").unwrap(),
                    tenant,
                    membership: WorkContextMembershipLevel::Contributor,
                    policy_revision: PolicyVersion::new("fixture-1").unwrap(),
                    output_policy: WorkContextOutputPolicy {
                        owner: AccessSubject::Principal(principal_id.clone()),
                        initial_grants: Vec::new(),
                        classification: None,
                        data_labels: BTreeSet::new(),
                    },
                    provenance: InvocationProvenance::Direct {
                        initiator: principal_id,
                    },
                },
                jwt_id: JwtId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
                issued_at: now,
                not_before: now,
                expires_at: now + TimeDelta::minutes(5),
            },
        }
    }

    #[tokio::test]
    async fn governed_artifact_streams_to_exact_renderer_digest_path() {
        let bytes = Bytes::from_static(b"#usda 1.0\n\ndef Xform \"Root\" {}\n");
        let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        let artifact_id = ArtifactId::new();
        let fixture = ArtifactFixture {
            metadata: ArtifactMetadata {
                artifact_id,
                byte_len: bytes.len() as u64,
                mime_type: Some("model/vnd.usd".to_owned()),
                filename: Some("fixture.usd".to_owned()),
                artifact_uri: artifact_id.plane_uri(),
                download_url: None,
                created_at: Utc::now(),
                release_state: Default::default(),
                compliance: ComplianceMetadata::default(),
                metadata: serde_json::Value::Null,
            },
            bytes: bytes.clone(),
        };
        let artifact_router = Router::new()
            .route("/artifacts/{id}/meta", get(metadata))
            .route("/artifacts/{id}/download", get(download))
            .with_state(fixture);
        let (artifact_endpoint, artifact_task) = serve(artifact_router).await;

        let received = Arc::new(Mutex::new(None));
        let renderer_router = Router::new()
            .route("/v1/artifacts/sha256/{filename}", put(ingest))
            .with_state(received.clone());
        let (renderer_endpoint, renderer_task) = serve(renderer_router).await;
        let materializer =
            SceneArtifactMaterializer::new(&artifact_endpoint, &renderer_endpoint, &"r".repeat(32))
                .unwrap();
        let artifact = GovernedArtifact {
            artifact_uri: artifact_id.plane_uri(),
            digest: digest.clone(),
            format: VisualAssetFormat::Usd,
            byte_length: bytes.len() as u64,
        };

        materializer
            .materialize_artifact(&caller(), &artifact, "usd")
            .await
            .unwrap();
        let (filename, ingested) = received.lock().unwrap().clone().unwrap();
        assert_eq!(filename, format!("{}.usd", &digest.as_str()[7..]));
        assert_eq!(ingested, bytes);

        artifact_task.abort();
        renderer_task.abort();
    }
}
