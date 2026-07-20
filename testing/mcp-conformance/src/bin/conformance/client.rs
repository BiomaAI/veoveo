use super::*;

use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Serialize, de::DeserializeOwned};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, CANCEL_TASK_METHOD, CancelTaskParams, CreateTaskResult, DISCOVER_METHOD,
    DetailedTask, DiscoverParams, DiscoverResult, EXTENSION_ID, GET_TASK_METHOD, GetTaskParams,
    GetTaskResult, HEADER_MCP_METHOD, HEADER_MCP_NAME, HEADER_MCP_PROTOCOL_VERSION,
    PROTOCOL_VERSION, ProtocolTaskId, RequestMeta, ToolCallParams,
};

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Default)]
pub(super) struct CliHandler;

impl ClientHandler for CliHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-conformance", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
            "  [progress] {:.0}%{}",
            params.progress * 100.0 / params.total.unwrap_or(1.0),
            params
                .message
                .map(|m| format!(" — {m}"))
                .unwrap_or_default()
        );
    }

    async fn on_task_status(
        &self,
        params: TaskStatusNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
            "  [task {}] {:?}: {}",
            params.task.task_id,
            params.task.status,
            params.task.status_message.as_deref().unwrap_or("")
        );
    }

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!("  [resource updated] {}", params.uri);
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<rmcp::RoleClient>) {
        println!("  [resource list changed]");
    }
}

pub(super) type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

#[derive(Clone)]
pub(super) struct FinalTaskClient {
    http: reqwest::Client,
    endpoint: String,
    bearer_token: Option<String>,
    request_ids: Arc<AtomicU64>,
}

impl FinalTaskClient {
    pub(super) fn from_args(args: &Args) -> Result<Self> {
        let bearer_token = if let Some(token) = &args.bearer_token {
            Some(token.clone())
        } else if let Some(private_key_der_b64) = &args.internal_signing_key_der_b64 {
            Some(issue_internal_conformance_token(args, private_key_der_b64)?)
        } else {
            None
        };
        Ok(Self {
            http: reqwest::Client::new(),
            endpoint: args.url.clone(),
            bearer_token,
            request_ids: Arc::new(AtomicU64::new(1)),
        })
    }

    pub(super) async fn discover(&self) -> Result<DiscoverResult> {
        let result: DiscoverResult = self
            .request(
                DISCOVER_METHOD,
                None,
                &DiscoverParams {
                    meta: RequestMeta::new(),
                },
            )
            .await?;
        let extensions = result
            .capabilities
            .get("extensions")
            .and_then(Value::as_object);
        if result
            .supported_versions
            .iter()
            .all(|version| version != PROTOCOL_VERSION)
            || !extensions.is_some_and(|extensions| extensions.contains_key(EXTENSION_ID))
        {
            return Err(anyhow!(
                "server does not advertise the final MCP task extension"
            ));
        }
        Ok(result)
    }

    pub(super) async fn start_tool(&self, request: ToolCallParams) -> Result<CreateTaskResult> {
        self.discover().await?;
        self.request("tools/call", Some(&request.name), &request)
            .await
    }

    pub(super) async fn get(&self, task_id: ProtocolTaskId) -> Result<DetailedTask> {
        let result: GetTaskResult = self
            .request(
                GET_TASK_METHOD,
                Some(&task_id.to_string()),
                &GetTaskParams {
                    meta: task_meta(),
                    task_id,
                },
            )
            .await?;
        Ok(result.task)
    }

    pub(super) async fn cancel(&self, task_id: ProtocolTaskId) -> Result<AcknowledgeTaskResult> {
        self.request(
            CANCEL_TASK_METHOD,
            Some(&task_id.to_string()),
            &CancelTaskParams {
                meta: task_meta(),
                task_id,
            },
        )
        .await
    }

    async fn request<T, P>(&self, method: &str, name: Option<&str>, params: &P) -> Result<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let mut request = self
            .http
            .post(&self.endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream")
            .header(HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION)
            .header(HEADER_MCP_METHOD, method)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": self.request_ids.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            }));
        if let Some(name) = name {
            request = request.header(HEADER_MCP_NAME, name);
        }
        if let Some(token) = &self.bearer_token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = request.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        let envelope: RpcResponse<T> = serde_json::from_slice(&body).map_err(|error| {
            anyhow!(
                "decoding task extension response from {} (HTTP {status}): {error}",
                self.endpoint
            )
        })?;
        match (envelope.result, envelope.error) {
            (Some(result), None) if status.is_success() => Ok(result),
            (_, Some(error)) => Err(anyhow!(
                "task extension request `{method}` failed ({}): {}",
                error.code,
                error.message
            )),
            _ => Err(anyhow!(
                "task extension request `{method}` returned invalid HTTP {status} response"
            )),
        }
    }
}

fn task_meta() -> RequestMeta {
    RequestMeta::new().with_task_capability()
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<veoveo_mcp_task_extension::JsonRpcErrorData>,
}

pub(super) async fn connect(args: &Args) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = &args.bearer_token {
        config = config.auth_header(token.clone());
    } else if let Some(private_key_der_b64) = &args.internal_signing_key_der_b64 {
        config = config.auth_header(issue_internal_conformance_token(args, private_key_der_b64)?);
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    Ok(CliHandler.serve(transport).await?)
}

fn issue_internal_conformance_token(args: &Args, private_key_der_b64: &str) -> Result<String> {
    let private_key_der = BASE64_STANDARD.decode(private_key_der_b64.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(args.internal_signing_key_id.clone(), private_key_der)?,
    );
    let principal_issuer = TokenIssuer::new("https://conformance.veoveo.local")?;
    let principal_subject = TokenSubject::new(args.internal_principal_subject.clone())?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new(args.internal_tenant.clone())?),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: args
            .internal_scopes
            .iter()
            .map(|scope| ScopeName::new(scope.clone()))
            .collect::<Result<_, _>>()?,
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::new("conformance")?,
        tenant: TenantId::new(args.internal_tenant.clone())?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1")?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: Default::default(),
        },
        provenance: InvocationProvenance::Automated,
    };
    let token = issuer.issue(
        GatewayProfileId::new(args.internal_profile.clone())?,
        ServerSlug::new(args.internal_server.clone())?,
        principal,
        authority,
        Utc::now() + TimeDelta::minutes(30),
    )?;
    Ok(token.bearer_token)
}
