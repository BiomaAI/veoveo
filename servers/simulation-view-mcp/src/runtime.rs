use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use veoveo_simulation_pose::{
    POSE_INGRESS_CONTROL_SCHEMA, POSE_PROTOCOL_SCHEMA, PoseIngressBinding, PoseIngressLimits,
    PoseIngressReadiness, PoseProducerAuthorization, SessionId, entity_identity_table_digest,
};

use crate::contract::{CameraDefinition, CameraRecord, PoseSourceState, SimulationViewSession};

pub const RENDERER_PROFILE: &str = "veoveo.io/simulation-view-renderer/isaac-rtx/v1";

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
    live_view_id: &'a veoveo_mcp_contract::LiveViewId,
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
    pub live_view_id: veoveo_mcp_contract::LiveViewId,
    pub ready: bool,
    pub signal_port: u16,
    pub media_port: u16,
    pub last_pose_sequence: Option<u64>,
    pub last_frame_at: Option<DateTime<Utc>>,
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
                expires_at: source.expires_at,
            },
        };
        self.put(
            &self.pose_endpoint,
            &self.pose_control_token,
            &format!("v1/bindings/{}", session.session_id),
            &declaration,
        )
        .await?;
        if let Err(error) = self
            .put_renderer(
                &format!("v1/sessions/{}/pose-source", session.session_id),
                &declaration,
            )
            .await
        {
            let _ = self
                .delete(
                    &self.pose_endpoint,
                    &self.pose_control_token,
                    &format!("v1/bindings/{}", session.session_id),
                    true,
                )
                .await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn revoke_pose(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> anyhow::Result<()> {
        let path = format!("v1/bindings/{session_id}");
        let pose = self
            .delete(&self.pose_endpoint, &self.pose_control_token, &path, false)
            .await;
        let renderer = self
            .delete(
                &self.renderer_endpoint,
                &self.renderer_control_token,
                &format!("v1/sessions/{session_id}/pose-source"),
                true,
            )
            .await;
        pose.and(renderer)
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

    pub async fn open_stream(
        &self,
        stream: &veoveo_mcp_contract::LiveViewState,
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
        let status: RendererStreamStatus = self
            .put_renderer_json(
                &format!(
                    "v1/sessions/{}/streams/{}",
                    stream.session_id, stream.live_view_id
                ),
                &RendererStreamBinding {
                    session_id: &stream.session_id,
                    camera_id: &stream.camera_id,
                    live_view_id: &stream.live_view_id,
                    render_slot,
                    media_port: stream.endpoint.media_port,
                },
            )
            .await?;
        anyhow::ensure!(
            status.live_view_id == stream.live_view_id
                && status.ready
                && status.signal_port == expected_signal_port
                && status.media_port == expected_media_port,
            "renderer stream did not become ready on its admitted media slot"
        );
        Ok(status)
    }

    pub async fn close_stream(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        stream_id: &veoveo_mcp_contract::LiveViewId,
    ) -> anyhow::Result<()> {
        self.delete(
            &self.renderer_endpoint,
            &self.renderer_control_token,
            &format!("v1/sessions/{session_id}/streams/{stream_id}"),
            false,
        )
        .await
    }

    async fn renderer_readiness(&self) -> anyhow::Result<RendererReadiness> {
        self.get_json(&self.renderer_endpoint, "readyz").await
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
    use super::*;

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
}
