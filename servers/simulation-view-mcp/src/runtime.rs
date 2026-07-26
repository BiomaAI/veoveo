use serde::{Deserialize, Serialize};
use url::Url;
use veoveo_simulation_pose::POSE_PROTOCOL_SCHEMA;

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
            && self.hardware_accelerated
            && self.nvidia
            && self.render_product_ready
            && self.nvenc_ready
            && self.visible_non_stale_frame
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseIngressReadiness {
    pub ready: bool,
    pub protocol_schema: String,
    pub mutually_authenticated: bool,
}

impl PoseIngressReadiness {
    pub fn is_ready(&self) -> bool {
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

#[derive(Debug, Clone)]
pub struct RuntimeClients {
    client: reqwest::Client,
    renderer_endpoint: Url,
    pose_endpoint: Url,
}

impl RuntimeClients {
    pub fn new(renderer_endpoint: &str, pose_endpoint: &str) -> anyhow::Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            renderer_endpoint: internal_http_endpoint(renderer_endpoint, "renderer")?,
            pose_endpoint: internal_http_endpoint(pose_endpoint, "pose ingress")?,
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
        let response = self.client.get(endpoint).send().await?.error_for_status()?;
        Ok(response.json().await?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_readiness_fails_closed() {
        let mut readiness = RendererReadiness {
            ready: true,
            profile: "isaac-rtx".to_owned(),
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
        assert!(RuntimeClients::new("http://renderer:8810", "http://pose:8811").is_ok());
        assert!(RuntimeClients::new("https://renderer:8810", "http://pose:8811").is_err());
        assert!(RuntimeClients::new("http://user@renderer:8810", "http://pose:8811").is_err());
    }
}
