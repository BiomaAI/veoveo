//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: authorization discovery, resources (+templates),
//! completions, SEP-1319 tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use clap::{Parser, Subcommand};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header, encode,
    jwk::{Jwk, JwkSet},
};
use reqwest::header::WWW_AUTHENTICATE;
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        ArgumentInfo, CallToolRequestParams, CallToolResult, CancelTaskParams, ClientCapabilities,
        ClientInfo, ClientRequest, CompleteRequestParams, ContentBlock, GetPromptRequestParams,
        GetTaskParams, GetTaskPayloadParams, Implementation, ListTasksRequest, NumberOrString,
        ProgressNotificationParam, ProgressToken, ReadResourceRequestParams, Reference, Request,
        RequestParamsMeta, ResourceUpdatedNotificationParam, ServerResult, SubscribeRequestParams,
        TaskMetadata, TaskStatus, TaskStatusNotificationParam,
    },
    service::NotificationContext,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenIssuer, GatewayProfileId,
    InternalTokenSecret, Principal, PrincipalId, PrincipalKind, ProviderUris, ScopeName,
    ServerSlug, TenantId, TokenIssuer, TokenSubject,
};

#[derive(Parser, Debug)]
#[command(name = "conformance", about = "Veoveo MCP conformance client")]
struct Args {
    /// MCP endpoint of the server under test.
    #[arg(long, default_value = "http://localhost:8787/media/mcp", global = true)]
    url: String,
    /// URI scheme used by the server's Veoveo resources.
    #[arg(long, default_value = "media", global = true)]
    scheme: String,
    /// Bearer token sent to the MCP endpoint under test.
    #[arg(long, env = "MCP_BEARER_TOKEN", global = true, hide_env_values = true)]
    bearer_token: Option<String>,
    /// Internal gateway signing secret for direct hosted-server conformance.
    #[arg(
        long,
        env = "VEOVEO_INTERNAL_TOKEN_SECRET",
        global = true,
        hide_env_values = true,
        conflicts_with = "bearer_token"
    )]
    internal_token_secret: Option<String>,
    /// Server slug for direct hosted-server conformance.
    #[arg(long, default_value = "media", global = true)]
    internal_server: String,
    /// Gateway profile id embedded in direct hosted-server conformance assertions.
    #[arg(long, default_value = "default", global = true)]
    internal_profile: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Verify protected-resource metadata and unauthenticated Bearer challenge.
    AuthDiscovery {
        /// Protected-resource metadata URL. If omitted, inferred from /mcp/{profile}.
        #[arg(long)]
        metadata_url: Option<String>,
        /// Scope that must appear in metadata and the Bearer challenge.
        #[arg(long = "required-scope")]
        required_scopes: Vec<String>,
        /// MCP extension id that must appear in protected-resource metadata.
        #[arg(long = "required-extension")]
        required_extensions: Vec<String>,
        /// Authorization-server metadata URL to verify.
        #[arg(long)]
        authorization_server_metadata_url: Option<String>,
        /// Authorization-server JWKS URL to verify. Overrides metadata jwks_uri when set.
        #[arg(long)]
        authorization_server_jwks_url: Option<String>,
        /// JWKS key id that must appear in the authorization server JWKS.
        #[arg(long = "required-jwks-key-id")]
        required_jwks_key_ids: Vec<String>,
        /// OAuth grant type that must appear in authorization-server metadata.
        #[arg(long = "required-grant-type")]
        required_grant_types: Vec<String>,
        /// OAuth grant profile that must appear in authorization-server metadata.
        #[arg(long = "required-grant-profile")]
        required_grant_profiles: Vec<String>,
        /// Token endpoint auth method that must appear in authorization-server metadata.
        #[arg(long = "required-token-auth-method")]
        required_token_auth_methods: Vec<String>,
    },
    /// Print the deterministic conformance JWKS as JSON.
    GatewayJwks,
    /// Print the deterministic conformance private key as compact base64 DER.
    GatewayPrivateKeyDerB64,
    /// Print a private-key JWT client assertion signed by the conformance private key.
    GatewayClientAssertion {
        /// OAuth client id used as issuer and subject.
        #[arg(long, default_value = "veoveo-headless")]
        client_id: String,
        /// Token endpoint audience claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/oauth/default/token")]
        audience: String,
        /// JWT id claim.
        #[arg(long)]
        jwt_id: Option<String>,
        /// Token lifetime in minutes.
        #[arg(long, default_value_t = 5)]
        ttl_minutes: i64,
    },
    /// Exchange a private-key JWT client assertion for a gateway access token.
    GatewayTokenExchange {
        /// Gateway token endpoint URL.
        #[arg(long)]
        token_url: String,
        /// OAuth client id used as issuer and subject.
        #[arg(long, default_value = "veoveo-headless")]
        client_id: String,
        /// Client assertion audience claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/oauth/default/token")]
        audience: String,
        /// OAuth scope. Repeat for multiple scopes.
        #[arg(long = "scope")]
        scopes: Vec<String>,
        /// Client assertion JWT id claim.
        #[arg(long)]
        jwt_id: Option<String>,
        /// Client assertion lifetime in minutes.
        #[arg(long, default_value_t = 5)]
        ttl_minutes: i64,
    },
    /// Print an Enterprise-Managed Authorization ID-JAG signed by the conformance private key.
    GatewayIdJag {
        /// Enterprise IdP issuer claim.
        #[arg(long, default_value = "https://idp.example.com")]
        issuer: String,
        /// Resource Authorization Server issuer audience claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/oauth/default")]
        audience: String,
        /// MCP protected resource claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/mcp/default")]
        resource: String,
        /// Registered MCP client id.
        #[arg(long, default_value = "veoveo-browser")]
        client_id: String,
        /// Enterprise user subject claim.
        #[arg(long, default_value = "00u-smoke")]
        subject: String,
        /// ID-JAG scope. Repeat for multiple scopes.
        #[arg(long = "scope")]
        scopes: Vec<String>,
        /// Tenant claim.
        #[arg(long, default_value = "tenant-a")]
        tenant: String,
        /// Group claim. Repeat for multiple groups.
        #[arg(long = "group")]
        groups: Vec<String>,
        /// Role claim. Repeat for multiple roles.
        #[arg(long = "role")]
        roles: Vec<String>,
        /// Data-label claim. Repeat for multiple labels.
        #[arg(long = "data-label")]
        data_labels: Vec<String>,
        /// JWT id claim.
        #[arg(long)]
        jwt_id: Option<String>,
        /// ID-JAG lifetime in minutes.
        #[arg(long, default_value_t = 5)]
        ttl_minutes: i64,
    },
    /// Exchange an Enterprise-Managed Authorization ID-JAG for a gateway access token.
    GatewayIdJagTokenExchange {
        /// Gateway token endpoint URL.
        #[arg(long)]
        token_url: String,
        /// Enterprise IdP issuer claim.
        #[arg(long, default_value = "https://idp.example.com")]
        issuer: String,
        /// Resource Authorization Server issuer audience claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/oauth/default")]
        audience: String,
        /// MCP protected resource claim.
        #[arg(long, default_value = "https://veoveo.bioma.ai/mcp/default")]
        resource: String,
        /// Registered MCP client id.
        #[arg(long, default_value = "veoveo-browser")]
        client_id: String,
        /// Enterprise user subject claim.
        #[arg(long, default_value = "00u-smoke")]
        subject: String,
        /// Scope embedded in the ID-JAG. Repeat for multiple scopes.
        #[arg(long = "id-jag-scope")]
        id_jag_scopes: Vec<String>,
        /// Optional requested access-token scope. Repeat for multiple scopes.
        #[arg(long = "scope")]
        scopes: Vec<String>,
        /// Tenant claim.
        #[arg(long, default_value = "tenant-a")]
        tenant: String,
        /// Group claim. Repeat for multiple groups.
        #[arg(long = "group")]
        groups: Vec<String>,
        /// Role claim. Repeat for multiple roles.
        #[arg(long = "role")]
        roles: Vec<String>,
        /// Data-label claim. Repeat for multiple labels.
        #[arg(long = "data-label")]
        data_labels: Vec<String>,
        /// ID-JAG JWT id claim.
        #[arg(long)]
        jwt_id: Option<String>,
        /// ID-JAG lifetime in minutes.
        #[arg(long, default_value_t = 5)]
        ttl_minutes: i64,
    },
    /// Show server info, capabilities, instructions, and the tool list.
    Info,
    /// Read the model catalog resource, optionally filtering locally.
    Models {
        query: Option<String>,
        /// Filter by model type (e.g. image-to-image, text-to-video).
        #[arg(long)]
        r#type: Option<String>,
    },
    /// Autocomplete model ids via completion/complete on the model template.
    Complete { prefix: String },
    /// List prompt templates.
    Prompts,
    /// Read one JSON resource by URI.
    Resource { uri: String },
    /// Render one prompt template.
    Prompt {
        name: String,
        /// Prompt arguments as a JSON object.
        #[arg(long)]
        arguments: Option<String>,
    },
    /// List MCP tasks visible to the authenticated principal.
    Tasks,
    /// Read the full schema resource for one model.
    Schema { model_id: String },
    /// Read the live state of a prediction resource.
    Prediction { id: String },
    /// Read a task usage report.
    Usage { task_id: String },
    /// Read and save an artifact resource.
    Artifact {
        sha256: String,
        /// Where to save the artifact file.
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
    },
    /// Run a model as an MCP task and download its outputs.
    Run {
        model_id: String,
        /// Model input as a JSON object (see `schema <model_id>`).
        #[arg(long)]
        input: String,
        /// Where to save output files.
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
        /// Cancel the task right after submission (tests tasks/cancel).
        #[arg(long)]
        cancel: bool,
    },
}

#[derive(Debug, Deserialize)]
struct AuthDiscoveryMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
    bearer_methods_supported: Vec<String>,
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerDiscoveryMetadata {
    issuer: String,
    token_endpoint: String,
    #[serde(default)]
    jwks_uri: Option<String>,
    #[serde(default)]
    grant_types_supported: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default)]
    authorization_grant_profiles_supported: Vec<String>,
}

const CONFORMANCE_KEY_ID: &str = "test-key";
const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
// Public conformance keypair for deterministic local smoke tokens; never deployment material.
const CONFORMANCE_RSA_PRIVATE_KEY_DER_B64: &str = r#"
MIIEpAIBAAKCAQEAvCUS6tGS9/VE3pGzncb1rDsZt/V/LkPHl2QO9jDlaO/jAEdfPOtCSsSyv7dY
+nmY61GpXedIpqg6U7gcU/TcOVar0APPbKZ3OERrvrX9w5/oTJyqK42Lwybl9vmFApcRDIexmSQ8
HBdc1tQPqdkSCHS2csfZVxAQ64PLh48017Q+w8L1UuXYOxD8QdpQx2R1TD3bOiSeaZRs2Utww6rb
ex0/Gn6kkYJw3kr+rQgqmmmOoZuEi7p3qSg6KXvKf3hcfugKQlRIamdP8FOz/3sM2vf2jzUV9BUM
xtOF/yj2GzLmUYHxPtn+K46QDTcGpFyYN6gAPaiGBKkxxZDIaHgosQIDAQABAoIBAAl/bB7tRTht
+ePr8ker2m1PPvc/xgOzgX0BnLU+JuiXGowiLjs8q5graZQeyPe9AXSYpt6CDVN3cNlW1RxCY0ck
OlBqDtOu7BwLrS4/kO/KD9+lNXx1HOn1Odzvv/CPaHmL1JH057Fp1wKTyjYiaoQBg0/USaMY4SfI
e5LsbmgYn71s03MXf9/TgKErBRXiIYPW9aKvpKlfCQ8pGV1/i/rTy+Sj87rk+8+fU+fPVyKUWsjA
gNHm+FmhCPPPVm4qh6Vw/NmuOpfRf1mzfVi7rBq0t5ehHkmW3KVSWY9+v3EttoXjC9iXFIr1OXp5
aoaZZIXpjw3vAlaKwXbuu7lUZhkCgYEA3PGDT2UgWCFjEJjpi2fQzCBfVQC3lgJ8Xwz3EOeNhe+M
mrKb358iDp5o+WgU+S4HJJcGK9uptGgN9GYrf303GPMwmWOvC8xH5fV8WDBYGqMeEi+xFHlS8ymt
MmiWpAkW8/rEjDJama58qzjyEcq+fuW4BJcxOydFHgACSOZIbVkCgYEA2f9RJ7+tOajthShh6LbV
lhSNDjAeauBj5pcg8bZhLaCNWKCUBE2ob+YXvTL6mzx30faY5nutMdJfOI2Au7YqQgx8HeCBkCUi
D5Ngx9yjQ2/vnNQSRjIY2mjj0/tzTlVNGJDxbwUr8DGug8BD6Wz+L1l+s8F3aqAFljp7HLMq8xkC
gYEAsoobgSoH9A+uvPfEKdnPmVRDlS4KLJd/p1OTxz5GV8gXB99zJEa0v7l0vK5F3II8VW4RF5nf
TiCTvj5dwh0OTAQg7qLmDhOauhIg1Cbk20mbADk30IKl7EduZQCtUorh2HB5KY17NxsQNVDEFGqQ
e3zoshT3PITkTnTVY9FrD6kCgYEAwZa5JBpUo6q/Wwu0fuu2mvOfG+VhbbndHY5CBETY4aL9QqI/
L98i4FQt6qeV4zt8kGlz+OIFuQO/6cHHe2rW9haONh4EENTY/Yn8XSAzoBSMbfHqVInyhiq1f6+C
AyM/NryomtW14jTMbFXWOTnANJ4+JTV+baKzs2g1ohP95SkCgYB7RzFmdbiY1ASdGO/vWqc/wLnT
hHID7qgdXU4DP84HMmOX/QG5iV8GtQPTfNJm+m1PEnkg4W24DOqg2gJ3/q7wTROOLwQlJtOmizkC
XVKygdRdax3xMB3Eld5rlIDwzX09ARHrm8badXtrF0NhQPYZVbax8rpJGcgEFPgXEJJ71w==
"#;

#[derive(Debug, Serialize)]
struct ClientAssertionClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
}

#[derive(Debug, Serialize)]
struct IdJagClaims {
    iss: String,
    sub: String,
    aud: String,
    resource: String,
    client_id: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    scope: String,
    groups: Vec<String>,
    roles: Vec<String>,
    tenant: String,
    data_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
}

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Default)]
struct CliHandler;

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

type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

struct AuthDiscoveryCheck<'a> {
    endpoint_url: &'a str,
    metadata_url: Option<&'a str>,
    required_scopes: &'a [String],
    required_extensions: &'a [String],
    authorization_server_metadata_url: Option<&'a str>,
    authorization_server_jwks_url: Option<&'a str>,
    required_jwks_key_ids: &'a [String],
    required_grant_types: &'a [String],
    required_grant_profiles: &'a [String],
    required_token_auth_methods: &'a [String],
}

async fn connect(args: &Args) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = &args.bearer_token {
        config = config.auth_header(token.clone());
    } else if let Some(secret) = &args.internal_token_secret {
        config = config.auth_header(issue_internal_conformance_token(args, secret)?);
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    Ok(CliHandler.serve(transport).await?)
}

async fn cmd_auth_discovery(check: AuthDiscoveryCheck<'_>) -> Result<()> {
    let metadata_url = match check.metadata_url {
        Some(value) => value.to_string(),
        None => infer_protected_resource_metadata_url(check.endpoint_url)?,
    };
    let http = reqwest::Client::new();
    let metadata = http
        .get(&metadata_url)
        .send()
        .await?
        .error_for_status()?
        .json::<AuthDiscoveryMetadata>()
        .await?;
    if metadata.resource.is_empty() {
        return Err(anyhow!("protected-resource metadata has empty resource"));
    }
    if metadata.authorization_servers.is_empty() {
        return Err(anyhow!(
            "protected-resource metadata has no authorization servers"
        ));
    }
    if !metadata
        .bearer_methods_supported
        .iter()
        .any(|method| method == "header")
    {
        return Err(anyhow!(
            "protected-resource metadata does not support header bearer tokens"
        ));
    }
    for scope in check.required_scopes {
        if !metadata
            .scopes_supported
            .iter()
            .any(|candidate| candidate == scope)
        {
            return Err(anyhow!(
                "protected-resource metadata is missing required scope `{scope}`"
            ));
        }
    }
    for extension in check.required_extensions {
        if !metadata.extensions.contains_key(extension) {
            return Err(anyhow!(
                "protected-resource metadata is missing required extension `{extension}`"
            ));
        }
    }
    if let Some(authorization_server_metadata_url) = check.authorization_server_metadata_url {
        let authorization_server_metadata = http
            .get(authorization_server_metadata_url)
            .send()
            .await?
            .error_for_status()?
            .json::<AuthorizationServerDiscoveryMetadata>()
            .await?;
        if authorization_server_metadata.issuer.is_empty() {
            return Err(anyhow!("authorization-server metadata has empty issuer"));
        }
        if authorization_server_metadata.token_endpoint.is_empty() {
            return Err(anyhow!(
                "authorization-server metadata has empty token endpoint"
            ));
        }
        if authorization_server_metadata.jwks_uri.is_none() {
            return Err(anyhow!("authorization-server metadata has no jwks_uri"));
        }
        for grant_type in check.required_grant_types {
            if !authorization_server_metadata
                .grant_types_supported
                .iter()
                .any(|candidate| candidate == grant_type)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required grant type `{grant_type}`"
                ));
            }
        }
        for grant_profile in check.required_grant_profiles {
            if !authorization_server_metadata
                .authorization_grant_profiles_supported
                .iter()
                .any(|candidate| candidate == grant_profile)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required grant profile `{grant_profile}`"
                ));
            }
        }
        for auth_method in check.required_token_auth_methods {
            if !authorization_server_metadata
                .token_endpoint_auth_methods_supported
                .iter()
                .any(|candidate| candidate == auth_method)
            {
                return Err(anyhow!(
                    "authorization-server metadata is missing required token auth method `{auth_method}`"
                ));
            }
        }
        if !check.required_jwks_key_ids.is_empty() {
            let jwks_url = check
                .authorization_server_jwks_url
                .or(authorization_server_metadata.jwks_uri.as_deref())
                .ok_or_else(|| anyhow!("authorization-server JWKS URL is required"))?;
            let jwks = http
                .get(jwks_url)
                .send()
                .await?
                .error_for_status()?
                .json::<JwkSet>()
                .await?;
            for key_id in check.required_jwks_key_ids {
                if !jwks
                    .keys
                    .iter()
                    .any(|key| key.common.key_id.as_deref() == Some(key_id.as_str()))
                {
                    return Err(anyhow!(
                        "authorization-server JWKS is missing required key id `{key_id}`"
                    ));
                }
            }
        }
    }

    let response = http.get(check.endpoint_url).send().await?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow!(
            "unauthenticated MCP endpoint returned {}, expected 401",
            response.status()
        ));
    }
    let challenge = response
        .headers()
        .get(WWW_AUTHENTICATE)
        .ok_or_else(|| anyhow!("401 response is missing WWW-Authenticate"))?
        .to_str()?;
    if !challenge.starts_with("Bearer ") {
        return Err(anyhow!("WWW-Authenticate is not a Bearer challenge"));
    }
    if !challenge.contains("resource_metadata=") {
        return Err(anyhow!(
            "Bearer challenge is missing protected-resource metadata"
        ));
    }
    for scope in check.required_scopes {
        if !challenge.contains(scope) {
            return Err(anyhow!(
                "Bearer challenge is missing required scope `{scope}`"
            ));
        }
    }

    println!(
        "auth discovery ok: resource={}, authorization_servers={}, scopes={}, extensions={}",
        metadata.resource,
        metadata.authorization_servers.len(),
        metadata.scopes_supported.len(),
        metadata.extensions.len()
    );
    Ok(())
}

fn cmd_gateway_jwks() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&conformance_jwks()?)?);
    Ok(())
}

fn cmd_gateway_private_key_der_b64() {
    println!(
        "{}",
        CONFORMANCE_RSA_PRIVATE_KEY_DER_B64
            .lines()
            .collect::<String>()
    );
}

struct ClientAssertionInput {
    client_id: String,
    audience: String,
    jwt_id: Option<String>,
    ttl_minutes: i64,
}

struct TokenExchangeInput {
    token_url: String,
    client_assertion: ClientAssertionInput,
    scopes: Vec<String>,
}

struct IdJagInput {
    issuer: String,
    audience: String,
    resource: String,
    client_id: String,
    subject: String,
    scopes: Vec<String>,
    tenant: String,
    groups: Vec<String>,
    roles: Vec<String>,
    data_labels: Vec<String>,
    jwt_id: Option<String>,
    ttl_minutes: i64,
}

struct IdJagTokenExchangeInput {
    token_url: String,
    id_jag: IdJagInput,
    requested_scopes: Vec<String>,
}

fn build_client_assertion(input: &ClientAssertionInput) -> Result<String> {
    if input.ttl_minutes <= 0 {
        return Err(anyhow!("ttl_minutes must be greater than zero"));
    }
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(input.ttl_minutes))
        .ok_or_else(|| anyhow!("ttl_minutes produces an invalid expiration timestamp"))?;
    let jwt_id = input
        .jwt_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let claims = ClientAssertionClaims {
        iss: input.client_id.clone(),
        sub: input.client_id.clone(),
        aud: input.audience.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id,
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
}

fn cmd_gateway_client_assertion(input: ClientAssertionInput) -> Result<()> {
    println!("{}", build_client_assertion(&input)?);
    Ok(())
}

async fn cmd_gateway_token_exchange(input: TokenExchangeInput) -> Result<()> {
    if input.scopes.is_empty() {
        return Err(anyhow!("at least one --scope is required"));
    }
    let assertion = build_client_assertion(&input.client_assertion)?;
    let scope = input.scopes.join(" ");
    let client_id = input.client_assertion.client_id.clone();
    let form_body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "client_credentials")
        .append_pair("client_id", &client_id)
        .append_pair("scope", &scope)
        .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE_JWT_BEARER)
        .append_pair("client_assertion", &assertion)
        .finish();
    let response = reqwest::Client::new()
        .post(&input.token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("token endpoint returned {status}: {body}"));
    }
    let token_response: TokenEndpointResponse = serde_json::from_str(&body)?;
    if token_response.token_type != "Bearer" {
        return Err(anyhow!(
            "token endpoint returned token_type `{}`",
            token_response.token_type
        ));
    }
    if token_response.access_token.is_empty() {
        return Err(anyhow!("token endpoint returned an empty access_token"));
    }
    if token_response.expires_in == 0 {
        return Err(anyhow!("token endpoint returned expires_in=0"));
    }
    if token_response.scope.is_empty() {
        return Err(anyhow!("token endpoint returned an empty scope"));
    }
    println!("{}", token_response.access_token);
    Ok(())
}

fn build_id_jag(input: &IdJagInput) -> Result<String> {
    if input.ttl_minutes <= 0 {
        return Err(anyhow!("ttl_minutes must be greater than zero"));
    }
    if input.scopes.is_empty() {
        return Err(anyhow!("at least one ID-JAG scope is required"));
    }
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(input.ttl_minutes))
        .ok_or_else(|| anyhow!("ttl_minutes produces an invalid expiration timestamp"))?;
    let jwt_id = input
        .jwt_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let claims = IdJagClaims {
        iss: input.issuer.clone(),
        sub: input.subject.clone(),
        aud: input.audience.clone(),
        resource: input.resource.clone(),
        client_id: input.client_id.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id,
        scope: input.scopes.join(" "),
        groups: input.groups.clone(),
        roles: input.roles.clone(),
        tenant: input.tenant.clone(),
        data_labels: input.data_labels.clone(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
}

fn cmd_gateway_id_jag(input: IdJagInput) -> Result<()> {
    println!("{}", build_id_jag(&input)?);
    Ok(())
}

async fn cmd_gateway_id_jag_token_exchange(input: IdJagTokenExchangeInput) -> Result<()> {
    let assertion = build_id_jag(&input.id_jag)?;
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
        .append_pair("client_id", &input.id_jag.client_id)
        .append_pair("assertion", &assertion);
    if !input.requested_scopes.is_empty() {
        serializer.append_pair("scope", &input.requested_scopes.join(" "));
    }
    let response = reqwest::Client::new()
        .post(&input.token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(serializer.finish())
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("token endpoint returned {status}: {body}"));
    }
    let token_response: TokenEndpointResponse = serde_json::from_str(&body)?;
    if token_response.token_type != "Bearer" {
        return Err(anyhow!(
            "token endpoint returned token_type `{}`",
            token_response.token_type
        ));
    }
    if token_response.access_token.is_empty() {
        return Err(anyhow!("token endpoint returned an empty access_token"));
    }
    if token_response.expires_in == 0 {
        return Err(anyhow!("token endpoint returned expires_in=0"));
    }
    if token_response.scope.is_empty() {
        return Err(anyhow!("token endpoint returned an empty scope"));
    }
    println!("{}", token_response.access_token);
    Ok(())
}

fn conformance_jwks() -> Result<JwkSet> {
    let mut jwk = Jwk::from_encoding_key(&conformance_encoding_key()?, Algorithm::RS256)?;
    jwk.common.key_id = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(JwkSet { keys: vec![jwk] })
}

fn conformance_encoding_key() -> Result<EncodingKey> {
    let der_text = CONFORMANCE_RSA_PRIVATE_KEY_DER_B64
        .lines()
        .collect::<String>();
    let der = BASE64_STANDARD.decode(der_text)?;
    Ok(EncodingKey::from_rsa_der(&der))
}

fn unix_seconds(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp before Unix epoch"))
}

fn infer_protected_resource_metadata_url(endpoint_url: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(endpoint_url)?;
    let path = url.path().trim_end_matches('/');
    if !path.starts_with("/mcp/") {
        return Err(anyhow!(
            "cannot infer protected-resource metadata URL for non-gateway MCP path `{path}`"
        ));
    }
    url.set_path(&format!("/.well-known/oauth-protected-resource{path}"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn issue_internal_conformance_token(args: &Args, secret: &str) -> Result<String> {
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        InternalTokenSecret::new(secret.to_string())?,
    );
    let principal_issuer = TokenIssuer::new("https://conformance.veoveo.local")?;
    let principal_subject = TokenSubject::new("conformance")?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new("local")?),
        groups: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("media:use")?].into_iter().collect(),
        data_labels: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let token = issuer.issue(
        GatewayProfileId::new(args.internal_profile.clone())?,
        ServerSlug::new(args.internal_server.clone())?,
        principal,
        Utc::now() + TimeDelta::minutes(30),
    )?;
    Ok(token.bearer_token)
}

async fn read_resource_json(client: &Client, uri: &str) -> Result<Value> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let text = result
        .contents
        .iter()
        .find_map(|c| match c {
            rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no text contents"))?;
    Ok(serde_json::from_str(&text)?)
}

async fn read_resource_blob(client: &Client, uri: &str) -> Result<(Vec<u8>, Option<String>)> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let (blob, mime_type) = result
        .contents
        .iter()
        .find_map(|c| match c {
            rmcp::model::ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            } => Some((blob.clone(), mime_type.clone())),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no blob contents"))?;
    Ok((BASE64_STANDARD.decode(blob)?, mime_type))
}

async fn cmd_info(client: &Client) -> Result<()> {
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("no server info"))?;
    println!(
        "server: {} v{}",
        info.server_info.name, info.server_info.version
    );
    println!("protocol: {}", info.protocol_version);
    println!(
        "capabilities: {}",
        serde_json::to_string_pretty(&info.capabilities)?
    );
    if let Some(instructions) = &info.instructions {
        println!("instructions:\n{instructions}");
    }
    let tools = client.list_tools(Default::default()).await?;
    for tool in tools.tools {
        println!(
            "\ntool `{}` (task support: {:?})",
            tool.name,
            tool.execution.as_ref().map(|e| &e.task_support)
        );
        println!("  {}", tool.description.as_deref().unwrap_or(""));
        println!(
            "  input schema: {}",
            serde_json::to_string(&tool.input_schema)?
        );
        if let Some(schema) = &tool.output_schema {
            println!("  output schema: {}", serde_json::to_string(schema)?);
        }
    }
    let prompts = client.list_prompts(Default::default()).await?;
    for prompt in prompts.prompts {
        println!(
            "prompt `{}` — {}",
            prompt.name,
            prompt.description.unwrap_or_default()
        );
    }
    let templates = client.list_resource_templates(Default::default()).await?;
    for t in templates.resource_templates {
        println!(
            "template: {} — {}",
            t.uri_template,
            t.description.unwrap_or_default()
        );
    }
    Ok(())
}

fn cmd_models_from_catalog(
    catalog: Value,
    query: Option<String>,
    ty: Option<String>,
) -> Result<()> {
    let models = catalog.as_array().ok_or_else(|| anyhow!("bad catalog"))?;
    let needle = query.map(|q| q.to_lowercase());
    let mut shown = 0usize;
    for m in models {
        let id = m["model_id"].as_str().unwrap_or_default();
        let mtype = m["type"].as_str().unwrap_or_default();
        let desc = m["description"].as_str().unwrap_or_default();
        if let Some(t) = &ty
            && mtype != t
        {
            continue;
        }
        if let Some(n) = &needle
            && !id.to_lowercase().contains(n)
            && !desc.to_lowercase().contains(n)
        {
            continue;
        }
        let price = m["base_price"]
            .as_f64()
            .map(|p| format!("${p}"))
            .unwrap_or_default();
        println!("{id}  [{mtype}] {price}");
        let short: String = desc.chars().take(110).collect();
        println!("    {short}");
        shown += 1;
    }
    println!("\n{shown} / {} models", models.len());
    Ok(())
}

async fn cmd_complete(client: &Client, uris: &ProviderUris, prefix: String) -> Result<()> {
    let result = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(uris.model_template()),
            ArgumentInfo::new("model_id", prefix),
        ))
        .await?;
    for v in &result.completion.values {
        println!("{v}");
    }
    println!(
        "\n{} shown, total {:?}, has_more {:?}",
        result.completion.values.len(),
        result.completion.total,
        result.completion.has_more
    );
    Ok(())
}

async fn cmd_prompts(client: &Client) -> Result<()> {
    let prompts = client.list_prompts(Default::default()).await?;
    for prompt in prompts.prompts {
        println!(
            "{} — {}",
            prompt.name,
            prompt.description.unwrap_or_default()
        );
        for argument in prompt.arguments.unwrap_or_default() {
            println!(
                "    {}{} — {}",
                argument.name,
                if argument.required == Some(true) {
                    " *"
                } else {
                    ""
                },
                argument.description.unwrap_or_default()
            );
        }
    }
    if let Some(cursor) = prompts.next_cursor {
        println!("\nnext cursor: {cursor}");
    }
    Ok(())
}

async fn cmd_resource(client: &Client, uri: String) -> Result<()> {
    let value = read_resource_json(client, &uri).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

async fn cmd_prompt(client: &Client, name: String, arguments: Option<String>) -> Result<()> {
    let arguments = arguments
        .map(|raw| serde_json::from_str::<Value>(&raw))
        .transpose()?
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("prompt arguments must be a JSON object"))
        })
        .transpose()?;
    let mut params = GetPromptRequestParams::new(name);
    if let Some(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    let result = client.get_prompt(params).await?;
    if let Some(description) = result.description {
        println!("{description}");
    }
    for message in result.messages {
        match message.content {
            ContentBlock::Text(text) => println!("\n{:?}:\n{}", message.role, text.text),
            other => println!("\n{:?}:\n{other:?}", message.role),
        }
    }
    Ok(())
}

async fn cmd_tasks(client: &Client) -> Result<()> {
    let result = client
        .send_request(ClientRequest::ListTasksRequest(ListTasksRequest::default()))
        .await?;
    let ServerResult::ListTasksResult(result) = result else {
        return Err(anyhow!("expected ListTasksResult, got {result:?}"));
    };
    for task in &result.tasks {
        println!(
            "{} {:?} {}",
            task.task_id,
            task.status,
            task.status_message.as_deref().unwrap_or_default()
        );
    }
    println!("{} task(s)", result.tasks.len());
    if let Some(cursor) = result.next_cursor {
        println!("next cursor: {cursor}");
    }
    Ok(())
}

fn print_call_tool_result(result: &CallToolResult) -> Vec<String> {
    let mut outputs = Vec::new();
    for block in result.content.iter() {
        match block {
            ContentBlock::Text(t) => println!("{}", t.text),
            ContentBlock::ResourceLink(link) => {
                println!(
                    "output: {} ({})",
                    link.uri,
                    link.mime_type.as_deref().unwrap_or("unknown")
                );
                outputs.push(link.uri.clone());
            }
            other => println!("{other:?}"),
        }
    }
    if let Some(structured) = &result.structured_content {
        println!("structured: {structured}");
    }
    outputs
}

fn extension_for_mime(mime_type: Option<&str>) -> &'static str {
    match mime_type.and_then(|m| m.split(';').next()) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        Some("image/gif") => "gif",
        Some("video/mp4") => "mp4",
        Some("video/webm") => "webm",
        Some("audio/mpeg") => "mp3",
        Some("audio/wav") => "wav",
        _ => "bin",
    }
}

async fn save_output_uri(
    client: &Client,
    uris: &ProviderUris,
    http: &reqwest::Client,
    output_dir: &std::path::Path,
    uri: &str,
) -> Result<()> {
    let (name, bytes) = if let Some(sha256) = uris.parse_artifact_uri(uri) {
        let (bytes, mime_type) = read_resource_blob(client, uri).await?;
        let ext = extension_for_mime(mime_type.as_deref());
        (format!("{sha256}.{ext}"), bytes)
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        let name = uri
            .split('?')
            .next()
            .and_then(|p| p.rsplit('/').next())
            .filter(|n| !n.is_empty())
            .unwrap_or("output.bin")
            .to_string();
        let bytes = http
            .get(uri)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
        (name, bytes)
    } else {
        return Err(anyhow!("unsupported output resource uri: {uri}"));
    };

    let path = output_dir.join(name);
    std::fs::write(&path, &bytes)?;
    println!("saved {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

async fn cmd_run(
    client: &Client,
    uris: &ProviderUris,
    model_id: String,
    input: String,
    output_dir: PathBuf,
    cancel: bool,
) -> Result<()> {
    let input: Value = serde_json::from_str(&input)?;

    // tools/call augmented with SEP-1319 task metadata + a progress token.
    let mut params = CallToolRequestParams::new("run")
        .with_arguments(
            serde_json::json!({ "model": model_id, "input": input })
                .as_object()
                .cloned()
                .unwrap(),
        )
        .with_task(TaskMetadata::new().with_ttl(3_600_000));
    params.set_progress_token(ProgressToken(NumberOrString::String(Arc::from("run"))));

    let created = client
        .send_request(ClientRequest::CallToolRequest(Request::new(params)))
        .await?;
    let ServerResult::CreateTaskResult(created) = created else {
        return Err(anyhow!("expected CreateTaskResult, got {created:?}"));
    };
    let task_id = created.task.task_id.clone();
    println!(
        "task {task_id} created (status {:?}, poll {}ms)",
        created.task.status,
        created.task.poll_interval.unwrap_or(3000)
    );

    if cancel {
        let result = client
            .send_request(ClientRequest::CancelTaskRequest(Request::new(
                CancelTaskParams::new(task_id.clone()),
            )))
            .await?;
        println!("cancel result: {result:?}");
        return Ok(());
    }

    // Poll tasks/get, honoring the server's suggested interval. Subscribe to
    // the prediction resource as soon as the statusMessage names it.
    let poll_ms = created.task.poll_interval.unwrap_or(3000);
    let mut subscribed = false;
    let final_task = loop {
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        let info = client
            .send_request(ClientRequest::GetTaskRequest(Request::new(
                GetTaskParams::new(task_id.clone()),
            )))
            .await?;
        let ServerResult::GetTaskResult(info) = info else {
            return Err(anyhow!("expected GetTaskResult, got {info:?}"));
        };
        let message = info.task.status_message.clone().unwrap_or_default();
        println!("poll: {:?} — {message}", info.task.status);

        let prediction_prefix = format!("{}://prediction/", uris.scheme());
        if !subscribed && let Some(idx) = message.find(&prediction_prefix) {
            let uri: String = message[idx..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches([';', ','])
                .to_string();
            client
                .subscribe(SubscribeRequestParams::new(uri.clone()))
                .await?;
            println!("subscribed to {uri}");
            subscribed = true;
        }

        match info.task.status {
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                break info.task;
            }
            _ => {}
        }
    };

    if final_task.status != TaskStatus::Completed {
        // tasks/result surfaces the failure detail as a JSON-RPC error.
        let err = client
            .send_request(ClientRequest::GetTaskPayloadRequest(Request::new(
                GetTaskPayloadParams::new(task_id.clone()),
            )))
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown error".into());
        return Err(anyhow!("task ended {:?}: {err}", final_task.status));
    }

    let payload = client
        .send_request(ClientRequest::GetTaskPayloadRequest(Request::new(
            GetTaskPayloadParams::new(task_id.clone()),
        )))
        .await?;
    let result: CallToolResult = match payload {
        ServerResult::CallToolResult(r) => r,
        ServerResult::CustomResult(c) => serde_json::from_value(c.0)?,
        other => return Err(anyhow!("unexpected tasks/result payload: {other:?}")),
    };
    let outputs = print_call_tool_result(&result);

    if !outputs.is_empty() {
        std::fs::create_dir_all(&output_dir)?;
        let http = reqwest::Client::new();
        for uri in outputs {
            save_output_uri(client, uris, &http, &output_dir, &uri).await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args = Args::parse();
    match &args.cmd {
        Cmd::AuthDiscovery {
            metadata_url,
            required_scopes,
            required_extensions,
            authorization_server_metadata_url,
            authorization_server_jwks_url,
            required_jwks_key_ids,
            required_grant_types,
            required_grant_profiles,
            required_token_auth_methods,
        } => {
            return cmd_auth_discovery(AuthDiscoveryCheck {
                endpoint_url: &args.url,
                metadata_url: metadata_url.as_deref(),
                required_scopes,
                required_extensions,
                authorization_server_metadata_url: authorization_server_metadata_url.as_deref(),
                authorization_server_jwks_url: authorization_server_jwks_url.as_deref(),
                required_jwks_key_ids,
                required_grant_types,
                required_grant_profiles,
                required_token_auth_methods,
            })
            .await;
        }
        Cmd::GatewayJwks => return cmd_gateway_jwks(),
        Cmd::GatewayPrivateKeyDerB64 => {
            cmd_gateway_private_key_der_b64();
            return Ok(());
        }
        Cmd::GatewayClientAssertion {
            client_id,
            audience,
            jwt_id,
            ttl_minutes,
        } => {
            return cmd_gateway_client_assertion(ClientAssertionInput {
                client_id: client_id.clone(),
                audience: audience.clone(),
                jwt_id: jwt_id.clone(),
                ttl_minutes: *ttl_minutes,
            });
        }
        Cmd::GatewayTokenExchange {
            token_url,
            client_id,
            audience,
            scopes,
            jwt_id,
            ttl_minutes,
        } => {
            return cmd_gateway_token_exchange(TokenExchangeInput {
                token_url: token_url.clone(),
                client_assertion: ClientAssertionInput {
                    client_id: client_id.clone(),
                    audience: audience.clone(),
                    jwt_id: jwt_id.clone(),
                    ttl_minutes: *ttl_minutes,
                },
                scopes: scopes.clone(),
            })
            .await;
        }
        Cmd::GatewayIdJag {
            issuer,
            audience,
            resource,
            client_id,
            subject,
            scopes,
            tenant,
            groups,
            roles,
            data_labels,
            jwt_id,
            ttl_minutes,
        } => {
            return cmd_gateway_id_jag(IdJagInput {
                issuer: issuer.clone(),
                audience: audience.clone(),
                resource: resource.clone(),
                client_id: client_id.clone(),
                subject: subject.clone(),
                scopes: scopes.clone(),
                tenant: tenant.clone(),
                groups: groups.clone(),
                roles: roles.clone(),
                data_labels: data_labels.clone(),
                jwt_id: jwt_id.clone(),
                ttl_minutes: *ttl_minutes,
            });
        }
        Cmd::GatewayIdJagTokenExchange {
            token_url,
            issuer,
            audience,
            resource,
            client_id,
            subject,
            id_jag_scopes,
            scopes,
            tenant,
            groups,
            roles,
            data_labels,
            jwt_id,
            ttl_minutes,
        } => {
            return cmd_gateway_id_jag_token_exchange(IdJagTokenExchangeInput {
                token_url: token_url.clone(),
                id_jag: IdJagInput {
                    issuer: issuer.clone(),
                    audience: audience.clone(),
                    resource: resource.clone(),
                    client_id: client_id.clone(),
                    subject: subject.clone(),
                    scopes: id_jag_scopes.clone(),
                    tenant: tenant.clone(),
                    groups: groups.clone(),
                    roles: roles.clone(),
                    data_labels: data_labels.clone(),
                    jwt_id: jwt_id.clone(),
                    ttl_minutes: *ttl_minutes,
                },
                requested_scopes: scopes.clone(),
            })
            .await;
        }
        _ => {}
    }

    let client = connect(&args).await?;
    let uris = ProviderUris::new(args.scheme);

    let result = match args.cmd {
        Cmd::AuthDiscovery { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayJwks => unreachable!("handled before MCP connection"),
        Cmd::GatewayPrivateKeyDerB64 => unreachable!("handled before MCP connection"),
        Cmd::GatewayClientAssertion { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayTokenExchange { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayIdJag { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayIdJagTokenExchange { .. } => unreachable!("handled before MCP connection"),
        Cmd::Info => cmd_info(&client).await,
        Cmd::Models { query, r#type } => {
            let catalog = read_resource_json(&client, &uris.models_uri()).await?;
            cmd_models_from_catalog(catalog, query, r#type)
        }
        Cmd::Complete { prefix } => cmd_complete(&client, &uris, prefix).await,
        Cmd::Prompts => cmd_prompts(&client).await,
        Cmd::Resource { uri } => cmd_resource(&client, uri).await,
        Cmd::Prompt { name, arguments } => cmd_prompt(&client, name, arguments).await,
        Cmd::Tasks => cmd_tasks(&client).await,
        Cmd::Schema { model_id } => {
            let value = read_resource_json(&client, &uris.model_uri(&model_id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Prediction { id } => {
            let value = read_resource_json(&client, &uris.prediction_uri(&id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Usage { task_id } => {
            let value = read_resource_json(&client, &uris.usage_task_uri(&task_id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Artifact { sha256, output_dir } => {
            std::fs::create_dir_all(&output_dir)?;
            let uri = uris.artifact_uri(&sha256);
            let http = reqwest::Client::new();
            save_output_uri(&client, &uris, &http, &output_dir, &uri).await
        }
        Cmd::Run {
            model_id,
            input,
            output_dir,
            cancel,
        } => cmd_run(&client, &uris, model_id, input, output_dir, cancel).await,
    };

    client.cancel().await?;
    result
}
