//! Governed, bounded MCP resource reads for model turns.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rig::tool::{Tool, ToolContext, rmcp::McpClientError};
use rmcp::{
    ServiceError,
    model::{
        ErrorCode, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
        ResourceContents,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::connection::ConnectionEpoch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceReadLimits {
    pub max_contents: usize,
    pub item_bytes: usize,
    pub response_bytes: usize,
    pub cumulative_episode_bytes: usize,
    pub reads_per_episode: u32,
    pub reads_per_family: u32,
    pub max_families_per_episode: u32,
    pub wall_time: Duration,
    pub max_pagination_depth: u32,
}

impl Default for ResourceReadLimits {
    fn default() -> Self {
        Self {
            max_contents: 8,
            item_bytes: 64 * 1024,
            response_bytes: 128 * 1024,
            cumulative_episode_bytes: 512 * 1024,
            reads_per_episode: 16,
            reads_per_family: 8,
            max_families_per_episode: 4,
            wall_time: Duration::from_secs(10),
            max_pagination_depth: 4,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GovernedResourceUri {
    raw: String,
    family: String,
    page_collection: Option<String>,
}

impl GovernedResourceUri {
    fn parse(value: &str) -> Result<Self, SafeCorrectionDiagnostic> {
        let parsed = url::Url::parse(value)
            .ok()
            .filter(|uri| uri.host_str().is_some());
        let Some(parsed) = parsed else {
            return Err(SafeCorrectionDiagnostic::invalid_uri(value));
        };
        let prohibited = matches!(
            parsed.scheme(),
            "data" | "file" | "ftp" | "http" | "https" | "javascript" | "ui" | "ws" | "wss"
        );
        if prohibited
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
            || parsed.query_pairs().any(|(name, _)| {
                matches!(
                    name.to_ascii_lowercase().replace('-', "_").as_str(),
                    "access_token"
                        | "api_key"
                        | "authorization"
                        | "credential"
                        | "password"
                        | "secret"
                        | "signature"
                        | "token"
                )
            })
        {
            return Err(SafeCorrectionDiagnostic::invalid_uri(value));
        }
        let page_collection = parsed
            .query_pairs()
            .any(|(name, _)| name == "cursor")
            .then(|| {
                let mut collection = parsed.clone();
                collection.set_query(None);
                collection.to_string()
            });
        Ok(Self {
            raw: value.to_owned(),
            family: parsed.scheme().to_owned(),
            page_collection,
        })
    }

    fn family(&self) -> &str {
        &self.family
    }

    fn page_collection(&self) -> Option<&str> {
        self.page_collection.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeCorrectionCode {
    InvalidUri,
    InvalidResource,
    BudgetExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeCorrectionKind {
    Input,
    Budget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceBudgetDimension {
    ReadCount,
    FamilyReadCount,
    FamilyCount,
    Bytes,
    WallTime,
    PageDepth,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RemainingResourceBudget {
    pub reads: u32,
    pub family_reads: u32,
    pub families: u32,
    pub bytes: usize,
    pub wall_time_ms: u64,
    pub page_depth: u32,
    pub permitted_actions: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SafeCorrectionDiagnostic {
    pub code: SafeCorrectionCode,
    pub kind: SafeCorrectionKind,
    pub requested_uri: Option<String>,
    pub guidance: &'static str,
    pub automatic_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhausted: Option<ResourceBudgetDimension>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<RemainingResourceBudget>,
}

impl SafeCorrectionDiagnostic {
    fn invalid_uri(value: &str) -> Self {
        Self {
            code: SafeCorrectionCode::InvalidUri,
            kind: SafeCorrectionKind::Input,
            requested_uri: safe_requested_uri(value),
            guidance: "Use an authorized non-browser resource URI without credentials or a fragment.",
            automatic_retry: false,
            exhausted: None,
            remaining: None,
        }
    }

    fn invalid_resource(uri: &str) -> Self {
        Self {
            code: SafeCorrectionCode::InvalidResource,
            kind: SafeCorrectionKind::Input,
            requested_uri: safe_requested_uri(uri),
            guidance: "The resource is unavailable in the current profile. Use an authorized resource link or correct the URI.",
            automatic_retry: false,
            exhausted: None,
            remaining: None,
        }
    }

    fn budget(
        uri: &GovernedResourceUri,
        exhausted: ResourceBudgetDimension,
        remaining: RemainingResourceBudget,
    ) -> Self {
        Self {
            code: SafeCorrectionCode::BudgetExhausted,
            kind: SafeCorrectionKind::Budget,
            requested_uri: Some(uri.raw.clone()),
            guidance: "The episode resource budget is exhausted. Continue with existing context or end the episode.",
            automatic_retry: false,
            exhausted: Some(exhausted),
            remaining: Some(remaining),
        }
    }
}

fn safe_requested_uri(value: &str) -> Option<String> {
    let mut parsed = url::Url::parse(value).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    let value = parsed.to_string();
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Some(value[..end].to_owned())
}

#[derive(Clone, Debug, Default)]
struct ResourceReadLedgerState {
    reads: u32,
    bytes: usize,
    wall_time: Duration,
    family_reads: BTreeMap<String, u32>,
    page_reads: BTreeMap<String, u32>,
}

#[derive(Clone, Debug)]
pub struct ResourceReadLedger {
    limits: ResourceReadLimits,
    state: Arc<Mutex<ResourceReadLedgerState>>,
}

impl ResourceReadLedger {
    pub fn new(limits: ResourceReadLimits) -> Self {
        Self {
            limits,
            state: Arc::new(Mutex::new(ResourceReadLedgerState::default())),
        }
    }

    fn admit(&self, uri: &GovernedResourceUri) -> Result<Duration, SafeCorrectionDiagnostic> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let family_reads = state.family_reads.get(uri.family()).copied().unwrap_or(0);
        if state.reads >= self.limits.reads_per_episode {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::ReadCount,
                self.remaining_for(&state, uri.family()),
            ));
        }
        if family_reads >= self.limits.reads_per_family {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::FamilyReadCount,
                self.remaining_for(&state, uri.family()),
            ));
        }
        if !state.family_reads.contains_key(uri.family())
            && state.family_reads.len() >= self.limits.max_families_per_episode as usize
        {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::FamilyCount,
                self.remaining_for(&state, uri.family()),
            ));
        }
        if state.bytes >= self.limits.cumulative_episode_bytes {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::Bytes,
                self.remaining_for(&state, uri.family()),
            ));
        }
        if state.wall_time >= self.limits.wall_time {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::WallTime,
                self.remaining_for(&state, uri.family()),
            ));
        }
        if let Some(collection) = uri.page_collection()
            && state.page_reads.get(collection).copied().unwrap_or(0)
                >= self.limits.max_pagination_depth
        {
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::PageDepth,
                self.remaining_for(&state, uri.family()),
            ));
        }
        state.reads += 1;
        *state.family_reads.entry(uri.family.clone()).or_default() += 1;
        if let Some(collection) = uri.page_collection() {
            *state.page_reads.entry(collection.to_owned()).or_default() += 1;
        }
        Ok(self.limits.wall_time.saturating_sub(state.wall_time))
    }

    fn commit_bytes(
        &self,
        uri: &GovernedResourceUri,
        bytes: usize,
    ) -> Result<(), SafeCorrectionDiagnostic> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(total) = state.bytes.checked_add(bytes) else {
            state.bytes = self.limits.cumulative_episode_bytes;
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::Bytes,
                self.remaining_for(&state, uri.family()),
            ));
        };
        if total > self.limits.cumulative_episode_bytes {
            state.bytes = self.limits.cumulative_episode_bytes;
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::Bytes,
                self.remaining_for(&state, uri.family()),
            ));
        }
        state.bytes = total;
        Ok(())
    }

    fn commit_elapsed(
        &self,
        uri: &GovernedResourceUri,
        elapsed: Duration,
    ) -> Result<(), SafeCorrectionDiagnostic> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(total) = state.wall_time.checked_add(elapsed) else {
            state.wall_time = self.limits.wall_time;
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::WallTime,
                self.remaining_for(&state, uri.family()),
            ));
        };
        if total > self.limits.wall_time {
            state.wall_time = self.limits.wall_time;
            return Err(SafeCorrectionDiagnostic::budget(
                uri,
                ResourceBudgetDimension::WallTime,
                self.remaining_for(&state, uri.family()),
            ));
        }
        state.wall_time = total;
        Ok(())
    }

    fn remaining_for(
        &self,
        state: &ResourceReadLedgerState,
        family: &str,
    ) -> RemainingResourceBudget {
        let family_page_depth = state
            .page_reads
            .iter()
            .filter(|(collection, _)| collection.starts_with(&format!("{family}://")))
            .map(|(_, depth)| *depth)
            .max()
            .unwrap_or(0);
        let reads = self.limits.reads_per_episode.saturating_sub(state.reads);
        let family_reads = self
            .limits
            .reads_per_family
            .saturating_sub(state.family_reads.get(family).copied().unwrap_or(0));
        let families = self
            .limits
            .max_families_per_episode
            .saturating_sub(state.family_reads.len() as u32);
        let bytes = self
            .limits
            .cumulative_episode_bytes
            .saturating_sub(state.bytes);
        let wall_time_ms = self
            .limits
            .wall_time
            .saturating_sub(state.wall_time)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let page_depth = self
            .limits
            .max_pagination_depth
            .saturating_sub(family_page_depth);
        let global_read_budget = reads > 0 && bytes > 0 && wall_time_ms > 0;
        let mut permitted_actions = Vec::with_capacity(4);
        if global_read_budget && family_reads > 0 && page_depth > 0 {
            permitted_actions.push("read_same_family");
        }
        if global_read_budget && families > 0 {
            permitted_actions.push("read_other_family");
        }
        permitted_actions.extend(["use_existing_context", "end_episode"]);
        RemainingResourceBudget {
            reads,
            family_reads,
            families,
            bytes,
            wall_time_ms,
            page_depth,
            permitted_actions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceTextContent {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedContents {
    contents: Vec<ResourceTextContent>,
    response_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceReadFailure {
    #[error("Resource content is not admitted for model context.")]
    ProhibitedContent,
    #[error("Resource content exceeds the admitted response bounds.")]
    ResponseLimit,
    #[error("The resource service is unavailable.")]
    Unavailable,
    #[error("The resource request is not authorized.")]
    Authorization,
    #[error("The resource request timed out.")]
    Timeout,
    #[error("The resource transport failed.")]
    Transport,
    #[error("The resource request failed.")]
    Internal,
}

fn validate_contents(
    result: ReadResourceResult,
    limits: &ResourceReadLimits,
) -> Result<ValidatedContents, ResourceReadFailure> {
    if result.contents.is_empty() || result.contents.len() > limits.max_contents {
        return Err(ResourceReadFailure::ResponseLimit);
    }
    let mut response_bytes = 0_usize;
    let mut contents = Vec::with_capacity(result.contents.len());
    for content in result.contents {
        let ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } = content
        else {
            return Err(ResourceReadFailure::ProhibitedContent);
        };
        GovernedResourceUri::parse(&uri).map_err(|_| ResourceReadFailure::ProhibitedContent)?;
        let mime_type = mime_type.unwrap_or_else(|| "text/plain".to_owned());
        let base_mime = mime_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let admitted_mime = (base_mime.starts_with("text/")
            && base_mime != "text/html"
            && base_mime != "text/event-stream")
            || base_mime == "application/json"
            || base_mime.ends_with("+json");
        if !admitted_mime {
            return Err(ResourceReadFailure::ProhibitedContent);
        }
        let item_bytes = text.len();
        if item_bytes > limits.item_bytes {
            return Err(ResourceReadFailure::ResponseLimit);
        }
        response_bytes = response_bytes
            .checked_add(item_bytes)
            .ok_or(ResourceReadFailure::ResponseLimit)?;
        if response_bytes > limits.response_bytes {
            return Err(ResourceReadFailure::ResponseLimit);
        }
        contents.push(ResourceTextContent {
            uri,
            mime_type,
            text,
        });
    }
    Ok(ValidatedContents {
        contents,
        response_bytes,
    })
}

enum ResourceReadErrorAction {
    Correction(SafeCorrectionDiagnostic),
    Failure(ResourceReadFailure),
}

fn map_read_error(uri: &str, error: McpClientError) -> ResourceReadErrorAction {
    match error {
        McpClientError::ResourceRead(ServiceError::McpError(error))
            if error.code == ErrorCode::INVALID_PARAMS =>
        {
            ResourceReadErrorAction::Correction(SafeCorrectionDiagnostic::invalid_resource(uri))
        }
        McpClientError::Unavailable => {
            ResourceReadErrorAction::Failure(ResourceReadFailure::Unavailable)
        }
        McpClientError::ResourceRead(ServiceError::McpError(error))
            if error.code == ErrorCode::INVALID_REQUEST =>
        {
            ResourceReadErrorAction::Failure(ResourceReadFailure::Authorization)
        }
        McpClientError::ResourceRead(ServiceError::Timeout { .. }) => {
            ResourceReadErrorAction::Failure(ResourceReadFailure::Timeout)
        }
        McpClientError::ResourceRead(
            ServiceError::TransportClosed | ServiceError::TransportSend(_),
        ) => ResourceReadErrorAction::Failure(ResourceReadFailure::Transport),
        _ => ResourceReadErrorAction::Failure(ResourceReadFailure::Internal),
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResourceReadArgs {
    pub uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResourceReadOutput {
    Complete {
        uri: String,
        contents: Vec<ResourceTextContent>,
        response_bytes: usize,
    },
    CorrectionRequired {
        diagnostic: SafeCorrectionDiagnostic,
    },
}

pub struct ResourceReadTool {
    epoch: watch::Receiver<ConnectionEpoch>,
    limits: ResourceReadLimits,
}

impl ResourceReadTool {
    pub fn new(epoch: watch::Receiver<ConnectionEpoch>, limits: ResourceReadLimits) -> Self {
        Self { epoch, limits }
    }
}

impl Tool for ResourceReadTool {
    const NAME: &'static str = "resource_read";
    type Error = ResourceReadFailure;
    type Args = ResourceReadArgs;
    type Output = ResourceReadOutput;

    fn description(&self) -> String {
        "Read one authorized MCP resource URI into bounded text context. Correctable URI errors return guidance; do not retry unchanged input.".to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "uri": {
                    "type": "string",
                    "description": "An authorized non-browser resource URI from a tool result or resource listing."
                }
            },
            "required": ["uri"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let uri = match GovernedResourceUri::parse(&args.uri) {
            Ok(uri) => uri,
            Err(diagnostic) => {
                return Ok(ResourceReadOutput::CorrectionRequired { diagnostic });
            }
        };
        let ledger = context
            .get::<ResourceReadLedger>()
            .cloned()
            .ok_or(ResourceReadFailure::Internal)?;
        let remaining_time = match ledger.admit(&uri) {
            Ok(remaining) => remaining,
            Err(diagnostic) => {
                return Ok(ResourceReadOutput::CorrectionRequired { diagnostic });
            }
        };
        let request = self
            .epoch
            .borrow()
            .request
            .clone()
            .ok_or(ResourceReadFailure::Unavailable)?;
        let started = Instant::now();
        let response = tokio::time::timeout(
            remaining_time,
            request.read_resource(ReadResourceRequestParams::new(uri.raw.clone())),
        )
        .await;
        if let Err(diagnostic) = ledger.commit_elapsed(&uri, started.elapsed()) {
            return Ok(ResourceReadOutput::CorrectionRequired { diagnostic });
        }
        let response = response.map_err(|_| ResourceReadFailure::Timeout)?;
        let response = match response {
            Ok(response) => response,
            Err(error) => match map_read_error(&uri.raw, error) {
                ResourceReadErrorAction::Correction(diagnostic) => {
                    return Ok(ResourceReadOutput::CorrectionRequired { diagnostic });
                }
                ResourceReadErrorAction::Failure(error) => return Err(error),
            },
        };
        let ReadResourceResponse::Complete(result) = response else {
            return Err(ResourceReadFailure::Internal);
        };
        let validated = validate_contents(result, &self.limits)?;
        if let Err(diagnostic) = ledger.commit_bytes(&uri, validated.response_bytes) {
            return Ok(ResourceReadOutput::CorrectionRequired { diagnostic });
        }
        Ok(ResourceReadOutput::Complete {
            uri: uri.raw,
            contents: validated.contents,
            response_bytes: validated.response_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use rig::tool::{
        Tool,
        rmcp::{McpClientConfig, McpClientError, McpClientHandler},
        server::ToolServer,
    };
    use rmcp::{
        RoleServer, ServerHandler, ServiceError, ServiceExt,
        model::{
            CacheScope, ErrorData, Implementation, ListToolsResult, PaginatedRequestParams,
            ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
            ResourceContents, ServerCapabilities, ServerInfo,
        },
        service::RequestContext,
    };

    use super::*;

    #[test]
    fn uri_admission_rejects_browser_network_and_credential_surfaces() {
        for uri in [
            "ui://map/admin.html",
            "file:///etc/passwd",
            "https://example.test/data",
            "map://user:password@layers/private",
            "map://layers/private#fragment",
            "map://layers/private?access_token=PRIVATE-QUERY-CANARY",
        ] {
            assert!(
                GovernedResourceUri::parse(uri).is_err(),
                "{uri} must not enter model context"
            );
        }
        assert_eq!(
            GovernedResourceUri::parse("map://layers/current")
                .unwrap()
                .family(),
            "map"
        );
        let diagnostic =
            GovernedResourceUri::parse("map://layers/private?access_token=PRIVATE-QUERY-CANARY")
                .unwrap_err();
        let visible = serde_json::to_string(&diagnostic).unwrap();
        assert!(!visible.contains("PRIVATE-QUERY-CANARY"));
    }

    #[test]
    fn content_validation_accepts_bounded_text_and_rejects_html_or_blobs() {
        let limits = ResourceReadLimits::default();
        let accepted = validate_contents(
            ReadResourceResult::new(vec![
                ResourceContents::text(r#"{"name":"current"}"#, "map://layers/current")
                    .with_mime_type("application/json"),
            ]),
            &limits,
        )
        .unwrap();
        assert_eq!(accepted.response_bytes, 18);

        let html = ReadResourceResult::new(vec![
            ResourceContents::text("<script>secret()</script>", "map://layers/current")
                .with_mime_type("text/html"),
        ]);
        assert!(matches!(
            validate_contents(html, &limits),
            Err(ResourceReadFailure::ProhibitedContent)
        ));

        let blob = ReadResourceResult::new(vec![ResourceContents::blob(
            "U0VDUkVU",
            "map://layers/current",
        )]);
        assert!(matches!(
            validate_contents(blob, &limits),
            Err(ResourceReadFailure::ProhibitedContent)
        ));
    }

    #[test]
    fn invalid_params_maps_to_bounded_guidance_without_upstream_text() {
        let secret = "PRIVATE-UPSTREAM-CANARY";
        let outcome = map_read_error(
            "map://layers/missing",
            McpClientError::ResourceRead(ServiceError::McpError(ErrorData::invalid_params(
                format!("unknown resource; database said {secret}"),
                Some(serde_json::json!({"query": secret})),
            ))),
        );
        let ResourceReadErrorAction::Correction(diagnostic) = outcome else {
            panic!("Invalid Params must remain correctable");
        };
        let visible = serde_json::to_string(&diagnostic).unwrap();
        assert!(visible.contains("map://layers/missing"));
        assert!(!visible.contains(secret));
        assert!(!diagnostic.automatic_retry);
    }

    #[test]
    fn protected_failures_remain_generic_and_hide_upstream_text() {
        let secret = "PRIVATE-AUTHORIZATION-CANARY";
        let outcome = map_read_error(
            "map://layers/private",
            McpClientError::ResourceRead(ServiceError::McpError(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("denied bearer {secret}"),
                Some(serde_json::json!({"credential": secret})),
            ))),
        );
        let ResourceReadErrorAction::Failure(failure) = outcome else {
            panic!("authorization failure must not become model correction content");
        };
        assert!(matches!(failure, ResourceReadFailure::Authorization));
        assert!(!failure.to_string().contains(secret));
    }

    #[test]
    fn ledger_rejects_each_exhausted_dimension_independently() {
        fn limits() -> ResourceReadLimits {
            ResourceReadLimits {
                reads_per_episode: 10,
                reads_per_family: 10,
                max_families_per_episode: 10,
                cumulative_episode_bytes: 100,
                wall_time: Duration::from_secs(10),
                max_pagination_depth: 10,
                ..ResourceReadLimits::default()
            }
        }

        let map = GovernedResourceUri::parse("map://layers/current").unwrap();
        let time = GovernedResourceUri::parse("time://authority/current").unwrap();

        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            reads_per_episode: 1,
            ..limits()
        });
        ledger.admit(&map).unwrap();
        assert_eq!(
            ledger.admit(&map).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::ReadCount)
        );
        let diagnostic = ledger.admit(&map).unwrap_err();
        let actions = &diagnostic.remaining.unwrap().permitted_actions;
        assert_eq!(actions, &["use_existing_context", "end_episode"]);

        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            reads_per_family: 1,
            ..limits()
        });
        ledger.admit(&map).unwrap();
        assert_eq!(
            ledger.admit(&map).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::FamilyReadCount)
        );

        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            max_families_per_episode: 1,
            ..limits()
        });
        ledger.admit(&map).unwrap();
        assert_eq!(
            ledger.admit(&time).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::FamilyCount)
        );

        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            cumulative_episode_bytes: 8,
            ..limits()
        });
        ledger.admit(&map).unwrap();
        ledger.commit_bytes(&map, 8).unwrap();
        assert_eq!(
            ledger.admit(&map).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::Bytes)
        );

        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            wall_time: Duration::from_millis(8),
            ..limits()
        });
        ledger.admit(&map).unwrap();
        ledger
            .commit_elapsed(&map, Duration::from_millis(8))
            .unwrap();
        assert_eq!(
            ledger.admit(&map).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::WallTime)
        );

        let first_page =
            GovernedResourceUri::parse("optimization://solutions?cursor=first").unwrap();
        let second_page =
            GovernedResourceUri::parse("optimization://solutions?cursor=second").unwrap();
        let ledger = ResourceReadLedger::new(ResourceReadLimits {
            max_pagination_depth: 1,
            ..limits()
        });
        ledger.admit(&first_page).unwrap();
        assert_eq!(
            ledger.admit(&second_page).unwrap_err().exhausted,
            Some(ResourceBudgetDimension::PageDepth)
        );
    }

    #[derive(Clone)]
    struct CorrectableResourceServer;

    impl ServerHandler for CorrectableResourceServer {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(
                ServerCapabilities::builder()
                    .enable_tools()
                    .enable_resources()
                    .build(),
            )
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::new("resource-test", "0.1.0"))
        }

        async fn list_tools(
            &self,
            _: Option<PaginatedRequestParams>,
            _: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, ErrorData> {
            Ok(ListToolsResult::with_all_items(Vec::new())
                .with_ttl_ms(0)
                .with_cache_scope(CacheScope::Private))
        }

        async fn read_resource(
            &self,
            request: ReadResourceRequestParams,
            _: RequestContext<RoleServer>,
        ) -> Result<ReadResourceResponse, ErrorData> {
            if request.uri != "map://layers/current" {
                return Err(ErrorData::invalid_params(
                    "unknown resource with PRIVATE-UPSTREAM-CANARY",
                    None,
                ));
            }
            Ok(ReadResourceResult::new(vec![
                ResourceContents::text("current", request.uri).with_mime_type("text/plain"),
            ])
            .into())
        }
    }

    #[tokio::test]
    async fn invalid_uri_can_be_corrected_in_the_same_tool_context() {
        let (client_to_server, server_from_client) = tokio::io::duplex(8192);
        let (server_to_client, client_from_server) = tokio::io::duplex(8192);
        let server_task = tokio::spawn(async move {
            let running = CorrectableResourceServer
                .serve((server_from_client, server_to_client))
                .await
                .expect("server start");
            let _ = running.waiting().await;
        });
        let guard = McpClientHandler::new(
            McpClientConfig::new(Implementation::new("kernel-test", "0.1.0")),
            ToolServer::new().run(),
        )
        .connect((client_from_server, client_to_server))
        .await
        .expect("client connect");
        let (_epoch_tx, epoch_rx) = watch::channel(ConnectionEpoch {
            epoch: 1,
            resolver: Some(guard.deferred_resolver()),
            request: Some(guard.request_handle()),
        });
        let limits = ResourceReadLimits::default();
        let tool = ResourceReadTool::new(epoch_rx, limits.clone());
        let mut context = ToolContext::new();
        context.insert(ResourceReadLedger::new(limits));

        let first = tool
            .call(
                &mut context,
                ResourceReadArgs {
                    uri: "map://layers/missing".to_owned(),
                },
            )
            .await
            .expect("correctable result");
        let ResourceReadOutput::CorrectionRequired { diagnostic } = first else {
            panic!("wrong URI must be correctable");
        };
        assert_eq!(diagnostic.code, SafeCorrectionCode::InvalidResource);
        assert_eq!(
            diagnostic.requested_uri.as_deref(),
            Some("map://layers/missing")
        );

        let second = tool
            .call(
                &mut context,
                ResourceReadArgs {
                    uri: "map://layers/current".to_owned(),
                },
            )
            .await
            .expect("corrected result");
        let ResourceReadOutput::Complete { contents, .. } = second else {
            panic!("corrected URI must complete in the same context");
        };
        assert_eq!(contents[0].text, "current");

        guard.cancel().await.expect("client close");
        server_task.await.expect("server task");
    }
}
