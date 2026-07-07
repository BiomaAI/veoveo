//! HTTP client for the shared artifact plane.
//!
//! Domain servers (media, timeseries, optimization, duckdb) depend on this thin
//! crate — not on the heavy `artifact-service` crate — to reach the plane. It
//! implements the contract's [`ArtifactPlane`] over the service's internal HTTP
//! surface, forwarding the caller's gateway-signed bearer on every request.

use base64::Engine;
use veoveo_mcp_contract::access::{AccessDecision, AccessLevel, ArtifactSha256, Grant, Subject};
use veoveo_mcp_contract::storage::{ArtifactMetadata, ArtifactObject};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, GrantList, PlaneCaller, PutArtifactRequest, PutGrantRequest,
};

/// A plane client bound to one artifact-service base URL.
#[derive(Clone)]
pub struct HttpArtifactPlane {
    base_url: String,
    http: reqwest::Client,
}

impl HttpArtifactPlane {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn with_client(base_url: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

fn transport(e: impl std::fmt::Display) -> ArtifactPlaneError {
    ArtifactPlaneError::Transport(e.to_string())
}

/// Map an HTTP status onto a plane error, recovering the precise
/// [`AccessDecision`] from the `x-artifact-decision` header when present so the
/// reason chain survives the hop.
fn error_for_status(status: reqwest::StatusCode, decision_header: Option<&str>, body: String) -> ArtifactPlaneError {
    match status {
        reqwest::StatusCode::NOT_FOUND => ArtifactPlaneError::NotFound,
        reqwest::StatusCode::UNAUTHORIZED => ArtifactPlaneError::Unauthenticated,
        reqwest::StatusCode::BAD_REQUEST => ArtifactPlaneError::InvalidRequest(body),
        reqwest::StatusCode::CONFLICT => ArtifactPlaneError::Conflict(body),
        reqwest::StatusCode::FORBIDDEN => {
            let decision = decision_header
                .and_then(|h| serde_json::from_str::<AccessDecision>(h).ok())
                .unwrap_or(AccessDecision::DenyNeedToKnow);
            ArtifactPlaneError::Denied(decision)
        }
        _ => ArtifactPlaneError::Transport(format!("{status}: {body}")),
    }
}

fn decision_header(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("x-artifact-decision")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn metadata_header(response: &reqwest::Response) -> Result<ArtifactMetadata, ArtifactPlaneError> {
    let raw = response
        .headers()
        .get("x-artifact-metadata")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ArtifactPlaneError::Transport("missing x-artifact-metadata".into()))?;
    let json = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(transport)?;
    serde_json::from_slice(&json).map_err(transport)
}

impl HttpArtifactPlane {
    async fn read_object(&self, response: reqwest::Response) -> Result<ArtifactObject, ArtifactPlaneError> {
        let metadata = metadata_header(&response)?;
        let bytes = response.bytes().await.map_err(transport)?.to_vec();
        Ok(ArtifactObject { metadata, bytes })
    }
}

impl ArtifactPlane for HttpArtifactPlane {
    async fn put(
        &self,
        caller: &PlaneCaller,
        request: PutArtifactRequest,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let put_header = serde_json::to_string(&request).map_err(transport)?;
        let response = self
            .http
            .post(self.url("/artifacts"))
            .bearer_auth(&caller.bearer_token)
            .header("x-artifact-put", put_header)
            .body(bytes)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            let dh = decision_header(&response);
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(error_for_status(status, dh.as_deref(), body))
        }
    }

    async fn get(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        level: AccessLevel,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let level = match level {
            AccessLevel::Read => "read",
            AccessLevel::Write => "write",
            AccessLevel::Admin => "admin",
        };
        let url = reqwest::Url::parse_with_params(
            &self.url(&format!("/artifacts/{sha}")),
            &[("level", level)],
        )
        .map_err(transport)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            self.read_object(response).await
        } else {
            let dh = decision_header(&response);
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(error_for_status(status, dh.as_deref(), body))
        }
    }

    async fn head(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url(&format!("/artifacts/{sha}/meta")))
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            let dh = decision_header(&response);
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(error_for_status(status, dh.as_deref(), body))
        }
    }

    async fn resolve(
        &self,
        caller: &PlaneCaller,
        uri: &str,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let url = reqwest::Url::parse_with_params(&self.url("/resolve"), &[("uri", uri)])
            .map_err(transport)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            self.read_object(response).await
        } else {
            let dh = decision_header(&response);
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(error_for_status(status, dh.as_deref(), body))
        }
    }

    async fn grant(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        subject: Subject,
        level: AccessLevel,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifacts/{sha}/grants")))
            .bearer_auth(&caller.bearer_token)
            .json(&PutGrantRequest { subject, level })
            .send()
            .await
            .map_err(transport)?;
        expect_no_content(response).await
    }

    async fn revoke(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        subject: &Subject,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .delete(self.url(&format!("/artifacts/{sha}/grants")))
            .bearer_auth(&caller.bearer_token)
            .json(subject)
            .send()
            .await
            .map_err(transport)?;
        expect_no_content(response).await
    }

    async fn list_grants(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> Result<Vec<Grant>, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url(&format!("/artifacts/{sha}/grants")))
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            let list: GrantList = response.json().await.map_err(transport)?;
            Ok(list.grants)
        } else {
            let dh = decision_header(&response);
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(error_for_status(status, dh.as_deref(), body))
        }
    }
}

async fn expect_no_content(response: reqwest::Response) -> Result<(), ArtifactPlaneError> {
    if response.status().is_success() {
        Ok(())
    } else {
        let dh = decision_header(&response);
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(error_for_status(status, dh.as_deref(), body))
    }
}
