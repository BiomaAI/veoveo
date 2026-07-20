//! HTTP client for the shared artifact plane.
//!
//! Domain servers (media, timeseries, optimization, duckdb) depend on this thin
//! crate — not on the heavy `artifact-service` crate — to reach the plane. It
//! implements the contract's [`ArtifactPlane`] over the service's internal HTTP
//! surface. Synchronous operations forward the caller's gateway-signed bearer;
//! asynchronous writes use a separately issued, task-bound write capability.

use base64::Engine;
use veoveo_mcp_contract::access::{AccessDecision, AccessLevel, AccessSubject, ArtifactId, Grant};
use veoveo_mcp_contract::storage::{ArtifactMetadata, ArtifactObject};
use veoveo_mcp_contract::{
    ArtifactAccessRequest, ArtifactAccessRequestId, ArtifactAccessRequestPage, ArtifactPage,
    ArtifactPlane, ArtifactPlaneError, ArtifactReleaseState, ArtifactShareLink,
    ArtifactShareLinkId, ArtifactWriteCapabilitySecret, CreateArtifactAccessRequest,
    CreateArtifactShareLinkRequest, DecideArtifactAccessRequest, GrantList,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability, ListArtifactAccessRequests,
    ListArtifactsRequest, PlaneCaller, PutArtifactRequest, PutGrantRequest,
    RedeemArtifactWriteCapabilityRequest,
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

    /// Issue a task-bound capability while a live gateway identity is
    /// available. Only artifact-service can issue or redeem this secret.
    pub async fn issue_write_capability(
        &self,
        caller: &PlaneCaller,
        request: &IssueArtifactWriteCapabilityRequest,
    ) -> Result<IssuedArtifactWriteCapability, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url("/artifact-write-capabilities"))
            .bearer_auth(&caller.bearer_token)
            .json(request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    /// Redeem a task-bound capability for exactly the artifact put described
    /// by `request`. The capability cannot be used by any other artifact or
    /// identity operation.
    pub async fn redeem_write_capability(
        &self,
        secret: &ArtifactWriteCapabilitySecret,
        request: &RedeemArtifactWriteCapabilityRequest,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let redeem_header = serde_json::to_string(request).map_err(transport)?;
        let response = self
            .http
            .post(self.url(&format!(
                "/artifact-write-capabilities/{}/redeem",
                request.capability_id
            )))
            .bearer_auth(secret.expose_secret())
            .header("x-artifact-capability-redeem", redeem_header)
            .body(bytes)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }
}

fn transport(e: impl std::fmt::Display) -> ArtifactPlaneError {
    ArtifactPlaneError::Transport(e.to_string())
}

/// Map an HTTP status onto a plane error, recovering the precise
/// [`AccessDecision`] from the `x-artifact-decision` header when present so the
/// reason chain survives the hop.
fn error_for_status(
    status: reqwest::StatusCode,
    decision_header: Option<&str>,
    body: String,
) -> ArtifactPlaneError {
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

async fn response_error<T>(response: reqwest::Response) -> Result<T, ArtifactPlaneError> {
    let dh = decision_header(&response);
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(error_for_status(status, dh.as_deref(), body))
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
    async fn read_object(
        &self,
        response: reqwest::Response,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
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
        artifact_id: &ArtifactId,
        level: AccessLevel,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let level = match level {
            AccessLevel::Read => "read",
            AccessLevel::Write => "write",
            AccessLevel::Admin => "admin",
        };
        let url = reqwest::Url::parse_with_params(
            &self.url(&format!("/artifacts/{artifact_id}")),
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
        artifact_id: &ArtifactId,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url(&format!("/artifacts/{artifact_id}/meta")))
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

    async fn list(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactsRequest,
    ) -> Result<ArtifactPage, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url("/artifacts"))
            .bearer_auth(&caller.bearer_token)
            .query(&request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
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
        artifact_id: &ArtifactId,
        subject: AccessSubject,
        level: AccessLevel,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifacts/{artifact_id}/grants")))
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
        artifact_id: &ArtifactId,
        subject: &AccessSubject,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .delete(self.url(&format!("/artifacts/{artifact_id}/grants")))
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
        artifact_id: &ArtifactId,
    ) -> Result<Vec<Grant>, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url(&format!("/artifacts/{artifact_id}/grants")))
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

    async fn set_release_state(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        release_state: ArtifactReleaseState,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let response = self
            .http
            .put(self.url(&format!("/artifacts/{artifact_id}/release-state")))
            .bearer_auth(&caller.bearer_token)
            .json(&veoveo_mcp_contract::SetArtifactReleaseStateRequest { release_state })
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    async fn create_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactShareLinkRequest,
    ) -> Result<ArtifactShareLink, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifacts/{artifact_id}/share-links")))
            .bearer_auth(&caller.bearer_token)
            .json(&request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    async fn revoke_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        link_id: &ArtifactShareLinkId,
    ) -> Result<(), ArtifactPlaneError> {
        let response = self
            .http
            .delete(self.url(&format!("/artifacts/{artifact_id}/share-links/{link_id}")))
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        expect_no_content(response).await
    }

    async fn create_access_request(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactAccessRequest,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifacts/{artifact_id}/access-requests")))
            .bearer_auth(&caller.bearer_token)
            .json(&request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    async fn list_access_requests(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactAccessRequests,
    ) -> Result<ArtifactAccessRequestPage, ArtifactPlaneError> {
        let response = self
            .http
            .get(self.url("/artifact-access-requests"))
            .bearer_auth(&caller.bearer_token)
            .query(&request)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    async fn decide_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
        decision: DecideArtifactAccessRequest,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifact-access-requests/{request_id}/decision")))
            .bearer_auth(&caller.bearer_token)
            .json(&decision)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
        }
    }

    async fn cancel_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let response = self
            .http
            .post(self.url(&format!("/artifact-access-requests/{request_id}/cancel")))
            .bearer_auth(&caller.bearer_token)
            .send()
            .await
            .map_err(transport)?;
        if response.status().is_success() {
            response.json().await.map_err(transport)
        } else {
            response_error(response).await
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
