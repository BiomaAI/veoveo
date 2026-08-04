use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use veoveo_simulation_pose::{
    POSE_INGRESS_CONTROL_SCHEMA, POSE_PROTOCOL_SCHEMA, PoseIngressBinding, PoseIngressLimits,
    PoseIngressReadiness, PoseIngressStatus, PoseProducerAuthorization, SessionId,
    entity_identity_table_digest,
};

use crate::contract::{
    CameraDefinition, CameraRecord, GeospatialLayerHealth, PoseInterpolationStatus,
    PoseSourceState, SimulationViewSession,
};

pub const RENDERER_PROFILE: &str = "veoveo.io/simulation-view-renderer/isaac-rtx/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererFailureCode {
    RequiredExtensionMissing,
    CesiumMdlAssetsMissing,
    CesiumMaterialSearchPathMissing,
    CesiumMaterialAllowlistMissing,
    CesiumTangentFrameMissing,
    LdrColorPipelineFailed,
    DiagnosticLightIsolationFailed,
    RendererInitializationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererFailure {
    pub code: RendererFailureCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererReadiness {
    pub ready: bool,
    pub profile: String,
    pub hardware_accelerated: bool,
    pub nvidia: bool,
    pub render_product_ready: bool,
    pub nvenc_ready: bool,
    pub visible_non_stale_frame: bool,
    pub streamed_world_ready: bool,
    pub cesium_mdl_ready: bool,
    pub cesium_tangent_frames_ready: bool,
    pub governed_lighting_ready: bool,
    pub color_pipeline_ready: bool,
    pub failure: Option<RendererFailure>,
}

impl RendererReadiness {
    pub fn is_ready(&self) -> bool {
        self.ready
            && self.profile == RENDERER_PROFILE
            && self.hardware_accelerated
            && self.nvidia
            && self.render_product_ready
            && self.nvenc_ready
            && self.visible_non_stale_frame
            && self.streamed_world_ready
            && self.cesium_mdl_ready
            && self.cesium_tangent_frames_ready
            && self.governed_lighting_ready
            && self.color_pipeline_ready
            && self.failure.is_none()
    }
}

trait PoseReadinessExt {
    fn is_ready(&self) -> bool;
}

impl PoseReadinessExt for PoseIngressReadiness {
    fn is_ready(&self) -> bool {
        self.ready && self.mutually_authenticated && self.protocol_schema == POSE_PROTOCOL_SCHEMA
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationViewReadiness {
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<RendererReadiness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose_ingress: Option<PoseIngressReadiness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererSessionBinding<'a> {
    session_id: &'a veoveo_mcp_contract::LiveSessionId,
    epoch_id: &'a veoveo_simulation_pose::EpochId,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererCameraBinding<'a> {
    session_id: &'a veoveo_mcp_contract::LiveSessionId,
    camera_id: &'a veoveo_mcp_contract::LiveCameraId,
    revision: u64,
    render_slot: u16,
    definition: &'a CameraDefinition,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererStreamBinding<'a> {
    session_id: &'a veoveo_mcp_contract::LiveSessionId,
    camera_id: &'a veoveo_mcp_contract::LiveCameraId,
    stream_product_id: &'a veoveo_mcp_contract::LiveStreamProductId,
    render_slot: u16,
    media_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererCameraStatus {
    pub camera_id: veoveo_mcp_contract::LiveCameraId,
    pub ready: bool,
    pub last_pose_sequence: Option<u64>,
    pub last_frame_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererStreamStatus {
    pub stream_product_id: veoveo_mcp_contract::LiveStreamProductId,
    pub ready: bool,
    pub signal_port: u16,
    pub media_port: u16,
    pub last_pose_sequence: Option<u64>,
    pub last_frame_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererInventory {
    pub generation: uuid::Uuid,
    pub camera_ids: Vec<veoveo_mcp_contract::LiveCameraId>,
    pub stream_product_ids: Vec<veoveo_mcp_contract::LiveStreamProductId>,
}

#[derive(Clone)]
pub struct RuntimeClients {
    client: reqwest::Client,
    renderer_endpoint: Url,
    pose_endpoint: Url,
    renderer_signaling_port_base: u16,
    public_media_port_base: u16,
    renderer_control_token: Arc<str>,
    pose_control_token: Arc<str>,
}

impl RuntimeClients {
    pub fn new(
        renderer_endpoint: &str,
        pose_endpoint: &str,
        renderer_control_token: &str,
        pose_control_token: &str,
        renderer_signaling_url: &str,
        public_media_port_base: u16,
    ) -> anyhow::Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        validate_control_token(renderer_control_token)?;
        validate_control_token(pose_control_token)?;
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            renderer_endpoint: internal_http_endpoint(renderer_endpoint, "renderer")?,
            pose_endpoint: internal_http_endpoint(pose_endpoint, "pose ingress")?,
            renderer_signaling_port_base: internal_ws_port(renderer_signaling_url)?,
            public_media_port_base,
            renderer_control_token: Arc::from(renderer_control_token),
            pose_control_token: Arc::from(pose_control_token),
        })
    }

    pub async fn readiness(&self) -> SimulationViewReadiness {
        let (renderer, pose_ingress) =
            tokio::join!(self.renderer_readiness(), self.pose_readiness());
        match (renderer, pose_ingress) {
            (Ok(renderer), Ok(pose_ingress)) => SimulationViewReadiness {
                ready: renderer.is_ready() && pose_ingress.is_ready(),
                renderer: Some(renderer),
                pose_ingress: Some(pose_ingress),
                error: None,
            },
            (renderer, pose_ingress) => SimulationViewReadiness {
                ready: false,
                renderer: renderer.ok(),
                pose_ingress: pose_ingress.ok(),
                error: Some("renderer or pose-ingress readiness request failed".to_owned()),
            },
        }
    }

    pub async fn create_session(&self, session: &SimulationViewSession) -> anyhow::Result<()> {
        self.put_renderer(
            &format!("v1/sessions/{}", session.session_id),
            &RendererSessionBinding {
                session_id: &session.session_id,
                epoch_id: &session.epoch_id,
            },
        )
        .await
    }

    pub async fn bind_scene(&self, session: &SimulationViewSession) -> anyhow::Result<()> {
        self.put_renderer(
            &format!("v1/sessions/{}/scene", session.session_id),
            session
                .scene
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("session scene is missing"))?,
        )
        .await
    }

    pub async fn close_session(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> anyhow::Result<()> {
        let _ = self
            .delete(
                &self.pose_endpoint,
                &self.pose_control_token,
                &format!("v1/bindings/{session_id}"),
                true,
            )
            .await;
        self.delete(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            &format!("v1/sessions/{session_id}"),
            false,
        )
        .await
    }

    pub async fn bind_pose(
        &self,
        session: &SimulationViewSession,
        source: &PoseSourceState,
    ) -> anyhow::Result<()> {
        let scene = session
            .scene
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session scene is missing"))?;
        let mut entity_ids = scene
            .body
            .entities
            .iter()
            .map(|entity| entity.entity_id.clone())
            .collect::<Vec<_>>();
        entity_ids.sort();
        let declaration = PoseIngressBinding {
            schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
            session_id: SessionId::new(session.session_id.as_str())?,
            epoch_id: session.epoch_id.clone(),
            frame_revision: scene.body.frame_revision.clone(),
            entity_table_revision: 1,
            entity_table_digest: entity_identity_table_digest(1, &entity_ids),
            limits: PoseIngressLimits {
                maximum_entities: u32::try_from(entity_ids.len())?,
                maximum_message_bytes: 4 * 1024 * 1024,
                maximum_cadence_hz: 120,
                stale_after_ms: scene.body.quality.maximum_pose_age_ms,
            },
            producer: PoseProducerAuthorization {
                producer_id: source.producer_id.to_string(),
                spiffe_id: source.spiffe_id.clone(),
                authorization_revision: source.authorization_revision,
                expires_at: source.expires_at,
                revoked: source.revoked,
            },
        };
        self.put(
            &self.pose_endpoint,
            &self.pose_control_token,
            &format!("v1/bindings/{}", session.session_id),
            &declaration,
        )
        .await?;
        self.put_renderer(
            &format!("v1/sessions/{}/pose-source", session.session_id),
            &declaration,
        )
        .await
    }

    pub async fn revoke_pose(
        &self,
        session: &SimulationViewSession,
        source: &PoseSourceState,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(source.revoked, "pose authorization is not revoked");
        self.bind_pose(session, source).await
    }

    pub async fn pose_status(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> anyhow::Result<PoseIngressStatus> {
        let status: PoseIngressStatus = self
            .get_json_authenticated(
                &self.pose_endpoint,
                &self.pose_control_token,
                &format!("v1/bindings/{session_id}"),
            )
            .await?;
        anyhow::ensure!(
            status.schema_version == POSE_INGRESS_CONTROL_SCHEMA
                && status.session_id.as_str() == session_id.as_str(),
            "pose ingress returned status for a different session"
        );
        Ok(status)
    }

    pub async fn interpolation_status(
        &self,
        session: &SimulationViewSession,
    ) -> anyhow::Result<PoseInterpolationStatus> {
        let status: PoseInterpolationStatus = self
            .get_json_authenticated(
                &self.renderer_endpoint,
                &self.renderer_control_token,
                &format!("v1/sessions/{}/pose-source", session.session_id),
            )
            .await?;
        let expected = session
            .scene
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session scene is missing"))?
            .body
            .quality
            .interpolation;
        anyhow::ensure!(
            status.policy == expected,
            "renderer returned interpolation status for a different policy"
        );
        Ok(status)
    }

    pub async fn layer_status(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> anyhow::Result<Option<GeospatialLayerHealth>> {
        let endpoint = self
            .renderer_endpoint
            .join(&format!("v1/sessions/{session_id}/layer"))?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.client
                .get(endpoint)
                .bearer_auth(&*self.renderer_control_token)
                .send(),
        )
        .await??;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json().await?))
    }

    pub async fn upsert_camera(
        &self,
        camera: &CameraRecord,
        render_slot: u16,
    ) -> anyhow::Result<RendererCameraStatus> {
        let status: RendererCameraStatus = self
            .put_renderer_json(
                &format!(
                    "v1/sessions/{}/cameras/{}",
                    camera.session_id, camera.camera_id
                ),
                &RendererCameraBinding {
                    session_id: &camera.session_id,
                    camera_id: &camera.camera_id,
                    revision: camera.revision,
                    render_slot,
                    definition: &camera.definition,
                },
            )
            .await?;
        anyhow::ensure!(
            status.camera_id == camera.camera_id,
            "renderer returned status for a different camera"
        );
        Ok(status)
    }

    pub async fn camera_status(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        camera_id: &veoveo_mcp_contract::LiveCameraId,
    ) -> anyhow::Result<RendererCameraStatus> {
        let status: RendererCameraStatus = self
            .get_json_authenticated(
                &self.renderer_endpoint,
                &self.renderer_control_token,
                &format!("v1/sessions/{session_id}/cameras/{camera_id}"),
            )
            .await?;
        anyhow::ensure!(
            status.camera_id == *camera_id,
            "renderer returned status for a different camera"
        );
        Ok(status)
    }

    pub async fn close_camera(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        camera_id: &veoveo_mcp_contract::LiveCameraId,
    ) -> anyhow::Result<()> {
        self.delete(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            &format!("v1/sessions/{session_id}/cameras/{camera_id}"),
            false,
        )
        .await
    }

    pub async fn renderer_inventory(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> anyhow::Result<RendererInventory> {
        self.get_json_authenticated(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            &format!("v1/sessions/{session_id}/inventory"),
        )
        .await
    }

    pub async fn open_stream(
        &self,
        stream: &veoveo_mcp_contract::LiveViewState,
        stream_product_id: &veoveo_mcp_contract::LiveStreamProductId,
        render_slot: u16,
    ) -> anyhow::Result<RendererStreamStatus> {
        let expected_signal_port = self
            .renderer_signaling_port_base
            .checked_add(render_slot)
            .ok_or_else(|| anyhow::anyhow!("renderer signaling port range overflow"))?;
        let expected_media_port = self
            .public_media_port_base
            .checked_add(render_slot)
            .ok_or_else(|| anyhow::anyhow!("public media port range overflow"))?;
        anyhow::ensure!(
            stream.endpoint.media_port == expected_media_port,
            "live-view endpoint does not match its physical media slot"
        );
        let path = format!(
            "v1/sessions/{}/streams/{}",
            stream.session_id, stream_product_id
        );
        let binding = RendererStreamBinding {
            session_id: &stream.session_id,
            camera_id: &stream.camera_id,
            stream_product_id,
            render_slot,
            media_port: stream.endpoint.media_port,
        };
        let status: RendererStreamStatus = self.put_renderer_json(&path, &binding).await?;
        anyhow::ensure!(
            status.stream_product_id == *stream_product_id
                && status.signal_port == expected_signal_port
                && status.media_port == expected_media_port,
            "renderer stream did not remain on its admitted media slot"
        );
        anyhow::ensure!(
            status.ready,
            "renderer stream product is not ready on its admitted media slot"
        );
        Ok(status)
    }

    pub async fn close_stream_product(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        stream_product_id: &veoveo_mcp_contract::LiveStreamProductId,
    ) -> anyhow::Result<()> {
        self.delete(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            &format!("v1/sessions/{session_id}/streams/{stream_product_id}"),
            false,
        )
        .await
    }

    async fn renderer_readiness(&self) -> anyhow::Result<RendererReadiness> {
        let endpoint = self.renderer_endpoint.join("readyz")?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.client.get(endpoint).send(),
        )
        .await??;
        anyhow::ensure!(
            response.status().is_success()
                || response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "renderer readiness returned {}",
            response.status()
        );
        Ok(response.json().await?)
    }

    async fn pose_readiness(&self) -> anyhow::Result<PoseIngressReadiness> {
        self.get_json(&self.pose_endpoint, "readyz").await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        base: &Url,
        path: &str,
    ) -> anyhow::Result<T> {
        let endpoint = base.join(path)?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.client.get(endpoint).send(),
        )
        .await??
        .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn get_json_authenticated<T: serde::de::DeserializeOwned>(
        &self,
        base: &Url,
        token: &str,
        path: &str,
    ) -> anyhow::Result<T> {
        let endpoint = base.join(path)?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.client.get(endpoint).bearer_auth(token).send(),
        )
        .await??
        .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn put_renderer<T: Serialize + ?Sized>(
        &self,
        path: &str,
        value: &T,
    ) -> anyhow::Result<()> {
        self.put(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            path,
            value,
        )
        .await
    }

    async fn put_renderer_json<T: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        value: &T,
    ) -> anyhow::Result<R> {
        self.put_json(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            path,
            value,
        )
        .await
    }

    async fn put<T: Serialize + ?Sized>(
        &self,
        base: &Url,
        token: &str,
        path: &str,
        value: &T,
    ) -> anyhow::Result<()> {
        self.client
            .put(base.join(path)?)
            .bearer_auth(token)
            .json(value)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn put_json<T: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
        &self,
        base: &Url,
        token: &str,
        path: &str,
        value: &T,
    ) -> anyhow::Result<R> {
        let response = self
            .client
            .put(base.join(path)?)
            .bearer_auth(token)
            .json(value)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn delete(
        &self,
        base: &Url,
        token: &str,
        path: &str,
        allow_not_found: bool,
    ) -> anyhow::Result<()> {
        let response = self
            .client
            .delete(base.join(path)?)
            .bearer_auth(token)
            .send()
            .await?;
        if allow_not_found && response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        response.error_for_status()?;
        Ok(())
    }
}

fn internal_http_endpoint(value: &str, label: &str) -> anyhow::Result<Url> {
    let mut url = Url::parse(value)?;
    anyhow::ensure!(
        url.scheme() == "http"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "{label} endpoint must be a credential-free internal HTTP URL"
    );
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn internal_ws_port(value: &str) -> anyhow::Result<u16> {
    let url = Url::parse(value)?;
    anyhow::ensure!(
        url.scheme() == "ws"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_some()
            && url.query().is_none()
            && url.fragment().is_none(),
        "renderer signaling URL must be a credential-free internal ws URL with an explicit port"
    );
    Ok(url.port().expect("validated explicit port"))
}

fn validate_control_token(token: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        (32..=512).contains(&token.len()) && !token.chars().any(char::is_whitespace),
        "runtime control tokens must contain 32 to 512 non-whitespace characters"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{Json, Router, extract::State, routing::put};
    use serde_json::{Value, json};
    use veoveo_mcp_contract::{LiveCameraId, LiveSessionId, LiveStreamProductId};

    use super::*;

    #[derive(Clone)]
    struct StreamFixture {
        requests: Arc<AtomicUsize>,
        ready_after: usize,
        signal_port: u16,
        media_port: u16,
        stream_product_id: LiveStreamProductId,
    }

    async fn stream_fixture(
        State(fixture): State<StreamFixture>,
        Json(_body): Json<Value>,
    ) -> Json<Value> {
        let request = fixture.requests.fetch_add(1, Ordering::SeqCst) + 1;
        Json(json!({
            "streamProductId": fixture.stream_product_id,
            "ready": request >= fixture.ready_after,
            "signalPort": fixture.signal_port,
            "mediaPort": fixture.media_port,
            "lastPoseSequence": 42,
            "lastFrameAt": "2026-08-03T03:00:00Z"
        }))
    }

    async fn runtime_with_stream_fixture(
        fixture: StreamFixture,
    ) -> (RuntimeClients, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/{*path}", put(stream_fixture))
            .with_state(fixture);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let token = "a".repeat(32);
        let runtime = RuntimeClients::new(
            &format!("http://{address}"),
            &format!("http://{address}"),
            &token,
            &token,
            "ws://renderer:49100",
            47998,
        )
        .unwrap();
        (runtime, server)
    }

    async fn await_fixture_stream(
        runtime: &RuntimeClients,
        stream_product_id: &LiveStreamProductId,
    ) -> anyhow::Result<RendererStreamStatus> {
        let session_id = LiveSessionId::new("session-1").unwrap();
        let camera_id = LiveCameraId::new("camera-1").unwrap();
        let binding = RendererStreamBinding {
            session_id: &session_id,
            camera_id: &camera_id,
            stream_product_id,
            render_slot: 0,
            media_port: 47998,
        };
        let status: RendererStreamStatus = runtime.put_renderer_json("v1/stream", &binding).await?;
        anyhow::ensure!(
            status.stream_product_id == *stream_product_id
                && status.signal_port == 49100
                && status.media_port == 47998,
            "renderer stream did not remain on its admitted media slot"
        );
        anyhow::ensure!(
            status.ready,
            "renderer stream product is not ready on its admitted media slot"
        );
        Ok(status)
    }

    #[test]
    fn renderer_readiness_fails_closed() {
        let mut readiness = RendererReadiness {
            ready: true,
            profile: RENDERER_PROFILE.to_owned(),
            hardware_accelerated: true,
            nvidia: true,
            render_product_ready: true,
            nvenc_ready: true,
            visible_non_stale_frame: true,
            streamed_world_ready: true,
            cesium_mdl_ready: true,
            cesium_tangent_frames_ready: true,
            governed_lighting_ready: true,
            color_pipeline_ready: true,
            failure: None,
        };
        assert!(readiness.is_ready());
        readiness.nvidia = false;
        assert!(!readiness.is_ready());
    }

    #[test]
    fn runtime_endpoints_are_private_http_only() {
        let token = "a".repeat(32);
        assert!(
            RuntimeClients::new(
                "http://renderer:8810",
                "http://pose:8811",
                &token,
                &token,
                "ws://renderer:49100",
                47998,
            )
            .is_ok()
        );
        assert!(
            RuntimeClients::new(
                "https://renderer:8810",
                "http://pose:8811",
                &token,
                &token,
                "ws://renderer:49100",
                47998,
            )
            .is_err()
        );
        assert!(
            RuntimeClients::new(
                "http://user@renderer:8810",
                "http://pose:8811",
                &token,
                &token,
                "ws://renderer:49100",
                47998,
            )
            .is_err()
        );
        assert!(
            RuntimeClients::new(
                "http://renderer:8810",
                "http://pose:8811",
                &token,
                &token,
                "ws://renderer",
                47998,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn stream_open_uses_one_reactive_renderer_request() {
        let stream_product_id = LiveStreamProductId::new("product-1").unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let fixture = StreamFixture {
            requests: requests.clone(),
            ready_after: 1,
            signal_port: 49100,
            media_port: 47998,
            stream_product_id: stream_product_id.clone(),
        };
        let (runtime, server) = runtime_with_stream_fixture(fixture).await;

        let status = await_fixture_stream(&runtime, &stream_product_id)
            .await
            .unwrap();

        assert!(status.ready);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn stream_open_rejects_renderer_slot_drift() {
        let stream_product_id = LiveStreamProductId::new("product-1").unwrap();
        let fixture = StreamFixture {
            requests: Arc::new(AtomicUsize::new(0)),
            ready_after: 1,
            signal_port: 49101,
            media_port: 47998,
            stream_product_id: stream_product_id.clone(),
        };
        let (runtime, server) = runtime_with_stream_fixture(fixture).await;

        let error = await_fixture_stream(&runtime, &stream_product_id)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("admitted media slot"));
        server.abort();
    }

    #[tokio::test]
    async fn stream_open_reports_not_ready_without_polling() {
        let stream_product_id = LiveStreamProductId::new("product-1").unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let fixture = StreamFixture {
            requests: requests.clone(),
            ready_after: usize::MAX,
            signal_port: 49100,
            media_port: 47998,
            stream_product_id: stream_product_id.clone(),
        };
        let (runtime, server) = runtime_with_stream_fixture(fixture).await;

        let error = await_fixture_stream(&runtime, &stream_product_id)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("is not ready"));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
    }
}
