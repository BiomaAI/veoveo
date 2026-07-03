//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: authorization discovery, resources (+templates),
//! completions, SEP-1319 tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Result, anyhow};
use axum::{
    Form as AxumForm, Json as AxumJson, Router as AxumRouter,
    body::Bytes as AxumBytes,
    extract::{Path as AxumPath, Query as AxumQuery, Request as AxumRequest, State as AxumState},
    http::{HeaderMap as AxumHeaderMap, StatusCode as AxumStatusCode, header::AUTHORIZATION},
    middleware::{self as axum_middleware, Next as AxumNext},
    response::IntoResponse as AxumIntoResponse,
    routing::{get as axum_get, post as axum_post},
};
use axum_server::tls_rustls::RustlsConfig;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use clap::{Parser, Subcommand};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header, encode,
    jwk::{Jwk, JwkSet},
};
use rcgen::generate_simple_self_signed;
use reqwest::header::WWW_AUTHENTICATE;
use rmcp::{
    ClientHandler, RoleServer, ServerHandler, ServiceExt,
    model::{
        ArgumentInfo, CallToolRequestParams, CallToolResult, CancelTaskParams, ClientCapabilities,
        ClientInfo, ClientRequest, CompleteRequestParams, CompleteResult, CompletionInfo,
        ContentBlock, GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        Implementation, JsonObject, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListTasksRequest, ListToolsResult, NumberOrString,
        PaginatedRequestParams, ProgressNotificationParam, ProgressToken, Prompt, PromptArgument,
        PromptMessage, ReadResourceRequestParams, ReadResourceResult, Reference, Request,
        RequestParamsMeta, Resource, ResourceContents, ResourceTemplate,
        ResourceUpdatedNotificationParam, Role, ServerCapabilities, ServerInfo, ServerResult,
        SubscribeRequestParams, TaskMetadata, TaskStatus, TaskStatusNotificationParam, TaskSupport,
        Tool, ToolExecution, UnsubscribeRequestParams,
    },
    service::{NotificationContext, RequestContext},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;
use veoveo_mcp_contract::{
    AccessTokenSubject, ArtifactMetadata, AuditEvent, AuthAuditEvent, ComplianceMetadata,
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayControlPlane, GatewayInternalIdentity,
    GatewayInternalTokenIssuer, GatewayInternalTokenVerifier, GatewayJwtRevocationApplyResult,
    GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest, GatewayProfileId,
    GatewayResourceProjection, GatewayResourceSubscription, GatewayTaskMapping,
    GenerationPredictionSummary, GenerationRunOutput, InternalTokenSecret, PolicyDecision,
    Principal, PrincipalId, PrincipalKind, ScopeName, SelfHostedDeploymentPlan, ServerManifest,
    ServerResourceUris, ServerSlug, TenantId, TokenIssuer, TokenSubject, UsageRecord, UsageReport,
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
    /// Write JSON Schemas for external Rust/Python/TypeScript contract implementations.
    ContractSchemas {
        /// Directory that receives one .schema.json file per exported contract type.
        #[arg(long, default_value = "schemas")]
        output_dir: PathBuf,
    },
    /// Validate a typed self-hosted deployment profile plan.
    DeploymentValidate {
        /// JSON deployment profile plan.
        #[arg(long)]
        file: PathBuf,
    },
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
    /// Write a gateway smoke control plane with a local HTTPS fake OIDC IdP.
    GatewaySmokeControlPlane {
        /// Base gateway control plane JSON.
        #[arg(long)]
        base: PathBuf,
        /// Output gateway control plane JSON.
        #[arg(long)]
        output: PathBuf,
        /// Fake IdP base URL, e.g. https://127.0.0.1:18803.
        #[arg(long)]
        idp_base_url: String,
        /// PEM CA certificate path trusted by the gateway for this fake IdP.
        #[arg(long)]
        trusted_ca_path: PathBuf,
    },
    /// Write a two-upstream gateway smoke control plane for hosted-server routing tests.
    GatewayTwoServerSmokeControlPlane {
        /// Base gateway control plane JSON.
        #[arg(long)]
        base: PathBuf,
        /// Output gateway control plane JSON.
        #[arg(long)]
        output: PathBuf,
        /// Fake media MCP upstream URL.
        #[arg(long)]
        media_upstream_url: String,
        /// Fake simulation MCP upstream URL.
        #[arg(long)]
        simulation_upstream_url: String,
    },
    /// Serve a local HTTPS fake OIDC IdP for browser authorization-code smoke tests.
    GatewayFakeOidcIdp {
        /// HTTPS listen port.
        #[arg(long)]
        port: u16,
        /// PEM certificate output path. The same file is the gateway trust anchor.
        #[arg(long)]
        cert_pem: PathBuf,
        /// PEM private key output path.
        #[arg(long)]
        key_pem: PathBuf,
        /// File touched after certificate generation and before serving.
        #[arg(long)]
        ready_file: Option<PathBuf>,
        /// OIDC issuer claim.
        #[arg(long, default_value = "https://idp.example.com")]
        issuer: String,
        /// Gateway OIDC client id registered at the IdP.
        #[arg(long, default_value = "veoveo-gateway")]
        client_id: String,
        /// Gateway OIDC client secret expected at the token endpoint.
        #[arg(long, env = "VEOVEO_IDP_OIDC_CLIENT_SECRET", hide_env_values = true)]
        client_secret: String,
    },
    /// Serve a local OTLP HTTP sink for telemetry smoke tests.
    OtlpHttpSink {
        /// HTTP listen port.
        #[arg(long)]
        port: u16,
        /// File touched after the listener is ready.
        #[arg(long)]
        ready_file: Option<PathBuf>,
        /// File receiving one line per OTLP request.
        #[arg(long)]
        hits_file: PathBuf,
    },
    /// Serve a local fake media provider for webhook-only gateway smoke tests.
    FakeMediaProvider {
        /// HTTP listen port.
        #[arg(long)]
        port: u16,
        /// File touched after the listener is ready.
        #[arg(long)]
        ready_file: Option<PathBuf>,
        /// Delay before posting the completion webhook.
        #[arg(long, default_value_t = 250)]
        completion_delay_ms: u64,
    },
    /// Serve a generic hosted MCP server that requires gateway internal authorization.
    FakeHostedMcp {
        /// HTTP listen port.
        #[arg(long)]
        port: u16,
        /// Hosted server slug and mount path segment.
        #[arg(long)]
        server: String,
        /// Server-owned resource URI scheme.
        #[arg(long)]
        scheme: String,
        /// Secret used to verify gateway-issued internal identity assertions.
        #[arg(long, env = "VEOVEO_INTERNAL_TOKEN_SECRET", hide_env_values = true)]
        internal_token_secret: String,
        /// File touched after the listener is ready.
        #[arg(long)]
        ready_file: Option<PathBuf>,
    },
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
    /// List MCP resources visible to the authenticated principal.
    Resources,
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
    /// Call one non-task tool directly.
    Call {
        /// Tool name to invoke.
        #[arg(long)]
        tool_name: String,
        /// Tool arguments as a JSON object.
        #[arg(long)]
        arguments: String,
    },
    /// Autocomplete any resource-template argument via completion/complete.
    CompleteResource {
        /// Resource URI/template reference.
        #[arg(long)]
        uri: String,
        /// Argument name to complete.
        #[arg(long)]
        argument: String,
        /// Completion prefix.
        prefix: String,
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
        /// Tool name to invoke. Direct media uses `run`; the gateway exposes `media__run`.
        #[arg(long, default_value = "run")]
        tool_name: String,
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

#[derive(Clone)]
struct FakeOidcState {
    issuer: String,
    client_id: String,
    client_secret: String,
    codes: Arc<Mutex<BTreeMap<String, FakeOidcCode>>>,
}

#[derive(Debug, Clone)]
struct FakeOidcCode {
    nonce: String,
}

#[derive(Debug, Deserialize)]
struct FakeOidcAuthorizeRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    nonce: String,
}

#[derive(Debug, Deserialize)]
struct FakeOidcTokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: String,
}

#[derive(Debug, Serialize)]
struct FakeOidcTokenResponse {
    id_token: String,
    token_type: &'static str,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct FakeOidcIdTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    nonce: String,
    groups: Vec<String>,
    roles: Vec<String>,
    tenant: String,
    data_labels: Vec<String>,
    email: String,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
}

struct ContractSchema {
    filename: &'static str,
    schema: Value,
}

fn contract_schemas() -> Result<Vec<ContractSchema>> {
    macro_rules! add_schema {
        ($schemas:ident, $filename:literal, $ty:ty) => {{
            $schemas.push(ContractSchema {
                filename: $filename,
                schema: serde_json::to_value(schemars::schema_for!($ty))?,
            });
        }};
    }

    let mut schemas = Vec::new();
    add_schema!(
        schemas,
        "gateway-control-plane.schema.json",
        GatewayControlPlane
    );
    add_schema!(schemas, "server-manifest.schema.json", ServerManifest);
    add_schema!(schemas, "principal.schema.json", Principal);
    add_schema!(
        schemas,
        "access-token-subject.schema.json",
        AccessTokenSubject
    );
    add_schema!(schemas, "policy-decision.schema.json", PolicyDecision);
    add_schema!(schemas, "audit-event.schema.json", AuditEvent);
    add_schema!(schemas, "auth-audit-event.schema.json", AuthAuditEvent);
    add_schema!(
        schemas,
        "gateway-jwt-revocation-request.schema.json",
        GatewayJwtRevocationRequest
    );
    add_schema!(
        schemas,
        "gateway-jwt-revocation-apply-result.schema.json",
        GatewayJwtRevocationApplyResult
    );
    add_schema!(
        schemas,
        "gateway-jwt-revocation-prune-result.schema.json",
        GatewayJwtRevocationPruneResult
    );
    add_schema!(
        schemas,
        "gateway-task-mapping.schema.json",
        GatewayTaskMapping
    );
    add_schema!(
        schemas,
        "gateway-resource-subscription.schema.json",
        GatewayResourceSubscription
    );
    add_schema!(
        schemas,
        "gateway-resource-projection.schema.json",
        GatewayResourceProjection
    );
    add_schema!(
        schemas,
        "gateway-internal-identity.schema.json",
        GatewayInternalIdentity
    );
    add_schema!(
        schemas,
        "self-hosted-deployment-plan.schema.json",
        SelfHostedDeploymentPlan
    );
    add_schema!(
        schemas,
        "compliance-metadata.schema.json",
        ComplianceMetadata
    );
    add_schema!(schemas, "artifact-metadata.schema.json", ArtifactMetadata);
    add_schema!(
        schemas,
        "generation-prediction-summary.schema.json",
        GenerationPredictionSummary
    );
    add_schema!(
        schemas,
        "generation-run-output.schema.json",
        GenerationRunOutput
    );
    add_schema!(schemas, "usage-record.schema.json", UsageRecord);
    add_schema!(schemas, "usage-report.schema.json", UsageReport);
    Ok(schemas)
}

fn cmd_contract_schemas(output_dir: PathBuf) -> Result<()> {
    let schemas = contract_schemas()?;
    std::fs::create_dir_all(&output_dir)?;
    for contract_schema in &schemas {
        let path = output_dir.join(contract_schema.filename);
        let bytes = serde_json::to_vec_pretty(&contract_schema.schema)?;
        std::fs::write(&path, bytes)?;
    }
    println!(
        "wrote {} contract schema(s) to {}",
        schemas.len(),
        output_dir.display()
    );
    Ok(())
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

fn cmd_gateway_smoke_control_plane(
    base: PathBuf,
    output: PathBuf,
    idp_base_url: String,
    trusted_ca_path: PathBuf,
) -> Result<()> {
    let idp_base = Url::parse(&idp_base_url)?;
    if idp_base.scheme() != "https" || idp_base.host().is_none() {
        return Err(anyhow!("--idp-base-url must be an https URL with a host"));
    }
    let idp_base = idp_base_url.trim_end_matches('/');
    let mut control_plane: Value = serde_json::from_str(&std::fs::read_to_string(&base)?)?;
    let identity_providers = control_plane
        .get_mut("identity_providers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no identity_providers array"))?;
    let identity_provider = identity_providers
        .iter_mut()
        .find(|provider| provider.get("id").and_then(Value::as_str) == Some("enterprise"))
        .ok_or_else(|| anyhow!("control plane has no `enterprise` identity provider"))?;
    identity_provider["authorization_endpoint"] = json!(format!("{idp_base}/oauth2/authorize"));
    identity_provider["token_endpoint"] = json!(format!("{idp_base}/oauth2/token"));
    identity_provider["enterprise_managed_authorization_endpoint"] =
        json!(format!("{idp_base}/oauth2/id-jag"));
    identity_provider["trusted_certificate_authorities"] = json!([
        {
            "source": "file",
            "path": trusted_ca_path.to_string_lossy()
        }
    ]);

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

fn cmd_gateway_two_server_smoke_control_plane(
    base: PathBuf,
    output: PathBuf,
    media_upstream_url: String,
    simulation_upstream_url: String,
) -> Result<()> {
    validate_loopback_http_url(&media_upstream_url, "--media-upstream-url")?;
    validate_loopback_http_url(&simulation_upstream_url, "--simulation-upstream-url")?;

    let mut control_plane: Value = serde_json::from_str(&std::fs::read_to_string(&base)?)?;
    configure_fake_server(
        &mut control_plane,
        "media",
        "media",
        "/media",
        "/media/mcp",
        &media_upstream_url,
        "media-plan",
    )?;
    append_server_manifest(
        &mut control_plane,
        json!({
            "slug": "simulation",
            "uri_scheme": "simulation",
            "mount_path": "/simulation",
            "mcp_path": "/simulation/mcp",
            "upstream": {
                "transport": "streamable_http",
                "url": simulation_upstream_url,
                "security": "loopback_http"
            },
            "capabilities": fake_hosted_capabilities(),
            "tools": ["run"],
            "prompts": ["simulation-plan"],
            "required_scopes": ["simulation:use"],
            "owned_routes": [],
            "metadata": {}
        }),
    )?;
    configure_default_profile_for_fake_servers(&mut control_plane)?;
    configure_policy_for_fake_servers(&mut control_plane)?;
    add_scope_to_oauth_clients(&mut control_plane, "simulation:use")?;

    let parsed: GatewayControlPlane = serde_json::from_value(control_plane.clone())?;
    parsed.validate()?;
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

fn validate_loopback_http_url(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value)?;
    if url.scheme() != "http" {
        return Err(anyhow!("{label} must use http for loopback smoke"));
    }
    let Some(host) = url.host_str() else {
        return Err(anyhow!("{label} must include a host"));
    };
    if !matches!(host, "127.0.0.1" | "localhost") {
        return Err(anyhow!("{label} must use a loopback host"));
    }
    Ok(())
}

fn control_plane_array_mut<'a>(
    control_plane: &'a mut Value,
    key: &str,
) -> Result<&'a mut Vec<Value>> {
    control_plane
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no `{key}` array"))
}

fn configure_fake_server(
    control_plane: &mut Value,
    slug: &str,
    scheme: &str,
    mount_path: &str,
    mcp_path: &str,
    upstream_url: &str,
    prompt: &str,
) -> Result<()> {
    let servers = control_plane_array_mut(control_plane, "servers")?;
    let server = servers
        .iter_mut()
        .find(|server| server.get("slug").and_then(Value::as_str) == Some(slug))
        .ok_or_else(|| anyhow!("control plane has no `{slug}` server"))?;
    server["uri_scheme"] = json!(scheme);
    server["mount_path"] = json!(mount_path);
    server["mcp_path"] = json!(mcp_path);
    server["upstream"] = json!({
        "transport": "streamable_http",
        "url": upstream_url,
        "security": "loopback_http"
    });
    server["capabilities"] = fake_hosted_capabilities();
    server["tools"] = json!(["run"]);
    server["prompts"] = json!([prompt]);
    server["owned_routes"] = json!([]);
    Ok(())
}

fn append_server_manifest(control_plane: &mut Value, server: Value) -> Result<()> {
    control_plane_array_mut(control_plane, "servers")?.push(server);
    Ok(())
}

fn fake_hosted_capabilities() -> Value {
    json!({
        "tools": true,
        "resources": true,
        "resource_templates": true,
        "resource_subscriptions": false,
        "prompts": true,
        "completions": true,
        "tasks": false,
        "notifications": false
    })
}

fn configure_default_profile_for_fake_servers(control_plane: &mut Value) -> Result<()> {
    let profiles = control_plane_array_mut(control_plane, "profiles")?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some("default"))
        .ok_or_else(|| anyhow!("control plane has no `default` profile"))?;
    profile["servers"] = json!([
        fake_profile_server_exposure("media", "media"),
        fake_profile_server_exposure("simulation", "simulation"),
    ]);
    Ok(())
}

fn fake_profile_server_exposure(server: &str, scheme: &str) -> Value {
    json!({
        "server": server,
        "tools": {
            "mode": "listed",
            "items": ["run"]
        },
        "resources": {
            "mode": "listed",
            "items": [
                {
                    "kind": "scheme",
                    "scheme": scheme
                }
            ]
        },
        "prompts": {
            "mode": "all"
        },
        "completions": "enabled",
        "tasks": "disabled"
    })
}

fn configure_policy_for_fake_servers(control_plane: &mut Value) -> Result<()> {
    let policies = control_plane_array_mut(control_plane, "policies")?;
    let policy = policies
        .iter_mut()
        .find(|policy| policy.get("version").and_then(Value::as_str) == Some("2026-07-02"))
        .ok_or_else(|| anyhow!("control plane has no `2026-07-02` policy"))?;
    let rules = policy
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("policy has no rules array"))?;
    let media_rule = rules
        .iter_mut()
        .find(|rule| rule.get("id").and_then(Value::as_str) == Some("allow_media_profile_use"))
        .ok_or_else(|| anyhow!("policy has no `allow_media_profile_use` rule"))?;
    *media_rule = fake_policy_rule(
        "allow_media_profile_use",
        "media",
        "media",
        "media-plan",
        "media:use",
    );
    rules.push(fake_policy_rule(
        "allow_simulation_profile_use",
        "simulation",
        "simulation",
        "simulation-plan",
        "simulation:use",
    ));
    Ok(())
}

fn fake_policy_rule(id: &str, server: &str, scheme: &str, prompt: &str, scope: &str) -> Value {
    json!({
        "id": id,
        "effect": "allow",
        "actions": [
            "tools_list",
            "tools_call",
            "resources_list",
            "resources_templates_list",
            "resources_read",
            "prompts_list",
            "prompts_get",
            "completion_complete"
        ],
        "profiles": ["default"],
        "servers": [server],
        "tools": ["run"],
        "resource_schemes": [scheme],
        "prompts": [prompt],
        "required_scopes": [scope],
        "metadata": {}
    })
}

fn add_scope_to_oauth_clients(control_plane: &mut Value, scope: &str) -> Result<()> {
    for client in control_plane_array_mut(control_plane, "oauth_clients")? {
        let scopes = client
            .get_mut("allowed_scopes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("OAuth client has no allowed_scopes array"))?;
        if !scopes
            .iter()
            .any(|candidate| candidate.as_str() == Some(scope))
        {
            scopes.push(json!(scope));
        }
    }
    Ok(())
}

async fn cmd_gateway_fake_oidc_idp(
    port: u16,
    cert_pem: PathBuf,
    key_pem: PathBuf,
    ready_file: Option<PathBuf>,
    issuer: String,
    client_id: String,
    client_secret: String,
) -> Result<()> {
    let certified_key =
        generate_simple_self_signed(vec!["127.0.0.1".to_string(), "localhost".to_string()])?;
    if let Some(parent) = cert_pem.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_pem.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cert_pem, certified_key.cert.pem())?;
    std::fs::write(&key_pem, certified_key.signing_key.serialize_pem())?;

    let state = FakeOidcState {
        issuer,
        client_id,
        client_secret,
        codes: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let router = AxumRouter::new()
        .route("/.well-known/jwks.json", axum_get(fake_oidc_jwks))
        .route("/oauth2/authorize", axum_get(fake_oidc_authorize))
        .route("/oauth2/token", axum_post(fake_oidc_token))
        .with_state(state);
    let config = RustlsConfig::from_pem_file(&cert_pem, &key_pem).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    axum_server::bind_rustls(addr, config)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}

#[derive(Clone)]
struct OtlpSinkState {
    hits_file: PathBuf,
}

async fn cmd_otlp_http_sink(
    port: u16,
    ready_file: Option<PathBuf>,
    hits_file: PathBuf,
) -> Result<()> {
    if let Some(parent) = hits_file.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&hits_file, b"")?;
    let state = OtlpSinkState { hits_file };
    let router = AxumRouter::new()
        .route("/v1/{signal}", axum_post(otlp_sink_hit))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

#[derive(Clone)]
struct FakeMediaProviderState {
    base_url: String,
    http: reqwest::Client,
    completion_delay: Duration,
}

#[derive(Debug, Deserialize)]
struct FakeBillingSearchRequest {
    #[serde(default)]
    prediction_uuids: Vec<String>,
}

async fn cmd_fake_media_provider(
    port: u16,
    ready_file: Option<PathBuf>,
    completion_delay_ms: u64,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let base_url = format!("http://{}", listener.local_addr()?);
    let state = FakeMediaProviderState {
        base_url,
        http: reqwest::Client::new(),
        completion_delay: Duration::from_millis(completion_delay_ms),
    };
    let router = AxumRouter::new()
        .route("/api/v3/models", axum_get(fake_media_models))
        .route("/api/v3/billings/search", axum_post(fake_media_billing))
        .route("/api/v3/{*model_id}", axum_post(fake_media_submit))
        .route("/outputs/fake.png", axum_get(fake_media_output))
        .with_state(state);
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

fn fake_media_envelope(data: Value) -> AxumJson<Value> {
    AxumJson(json!({
        "code": 200,
        "message": "ok",
        "data": data,
    }))
}

async fn fake_media_models() -> AxumJson<Value> {
    fake_media_envelope(json!([
        {
            "model_id": "fake/image",
            "name": "Fake image",
            "type": "image-to-image",
            "description": "Deterministic local smoke-test model.",
            "base_price": 0.01,
            "formula": "fixed smoke price",
            "api_schema": {
                "api_schemas": [
                    {
                        "type": "model_run",
                        "request_schema": {
                            "type": "object",
                            "required": ["prompt"],
                            "properties": {
                                "prompt": { "type": "string" }
                            },
                            "additionalProperties": true
                        }
                    }
                ]
            }
        }
    ]))
}

async fn fake_media_submit(
    AxumState(state): AxumState<FakeMediaProviderState>,
    AxumPath(model_id): AxumPath<String>,
    AxumQuery(query): AxumQuery<BTreeMap<String, String>>,
    AxumJson(input): AxumJson<Value>,
) -> AxumJson<Value> {
    let prediction_id = format!("fake-{}", uuid::Uuid::new_v4());
    let output_url = format!("{}/outputs/fake.png", state.base_url);
    if let Some(webhook_url) = query.get("webhook").cloned() {
        let http = state.http.clone();
        let completion_delay = state.completion_delay;
        let terminal = json!({
            "id": prediction_id,
            "model": model_id,
            "outputs": [output_url],
            "status": "completed",
            "input": input,
            "executionTime": 0.2,
        });
        tokio::spawn(async move {
            tokio::time::sleep(completion_delay).await;
            if let Err(err) = http.post(webhook_url).json(&terminal).send().await {
                eprintln!("fake media provider webhook failed: {err}");
            }
        });
    }

    fake_media_envelope(json!({
        "id": prediction_id,
        "model": model_id,
        "outputs": [],
        "status": "processing",
    }))
}

async fn fake_media_billing(
    AxumJson(request): AxumJson<FakeBillingSearchRequest>,
) -> AxumJson<Value> {
    let prediction_id = request
        .prediction_uuids
        .first()
        .cloned()
        .unwrap_or_else(|| "fake-unknown".to_string());
    fake_media_envelope(json!({
        "items": [
            {
                "uuid": format!("billing-{prediction_id}"),
                "billing_type": "deduct",
                "price": 0.01,
                "created_at": Utc::now(),
                "updated_at": Utc::now(),
                "order": {
                    "uuid": format!("order-{prediction_id}"),
                    "state": "completed",
                    "status": "completed"
                },
                "prediction": {
                    "uuid": prediction_id,
                    "model_uuid": "fake/image",
                    "status": "completed"
                }
            }
        ]
    }))
}

async fn fake_media_output() -> impl AxumIntoResponse {
    let bytes = BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
        .expect("valid embedded PNG");
    ([("content-type", "image/png")], bytes)
}

#[derive(Clone)]
struct FakeHostedMcp {
    server: String,
    scheme: String,
}

#[derive(Clone)]
struct FakeHostedAuthState {
    verifier: GatewayInternalTokenVerifier,
}

impl FakeHostedMcp {
    fn new(server: impl Into<String>, scheme: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            scheme: scheme.into(),
        }
    }

    fn scenarios_uri(&self) -> String {
        format!("{}://scenarios", self.scheme)
    }

    fn scenario_template(&self) -> String {
        format!("{}://scenario/{{scenario_id}}", self.scheme)
    }

    fn scenario_uri(&self, scenario_id: &str) -> String {
        format!("{}://scenario/{scenario_id}", self.scheme)
    }

    fn prompt_name(&self) -> String {
        format!("{}-plan", self.server)
    }

    fn scenario_ids(&self) -> [&'static str; 3] {
        ["orbital-docking", "supply-chain", "thermal-control"]
    }
}

impl ServerHandler for FakeHostedMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info = Implementation::new(self.server.clone(), env!("CARGO_PKG_VERSION"));
        info.instructions = Some(format!(
            "Generic hosted {} MCP fixture for gateway multi-server conformance.",
            self.server
        ));
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let input_schema: JsonObject = serde_json::from_value(json!({
            "type": "object",
            "required": ["scenario"],
            "properties": {
                "scenario": { "type": "string" }
            },
            "additionalProperties": false
        }))
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        let tool = Tool::new(
            "run",
            format!("Run a deterministic {} fixture scenario.", self.server),
            input_schema,
        )
        .with_title(format!("{} run", self.server))
        .with_execution(ToolExecution::new().with_task_support(TaskSupport::Forbidden));
        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if request.name != "run" {
            return Err(rmcp::ErrorData::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let scenario = arguments
            .get("scenario")
            .and_then(Value::as_str)
            .ok_or_else(|| rmcp::ErrorData::invalid_params("missing scenario argument", None))?;
        let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
            "{} fixture accepted scenario {scenario}",
            self.server
        ))]);
        result.structured_content = Some(json!({
            "server": self.server,
            "scheme": self.scheme,
            "scenario": scenario,
            "scenario_uri": self.scenario_uri(scenario)
        }));
        Ok(result)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(self.scenarios_uri(), format!("{} scenarios", self.server))
                    .with_title(format!("{} scenario catalog", self.server))
                    .with_description("Deterministic fixture scenario catalog.")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new(self.scenario_template(), "scenario")
                    .with_title(format!("{} scenario", self.server))
                    .with_description("Deterministic fixture scenario by id.")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let text = if request.uri == self.scenarios_uri() {
            serde_json::to_string(&json!({
                "server": self.server,
                "scheme": self.scheme,
                "scenarios": self.scenario_ids()
                    .into_iter()
                    .map(|scenario_id| json!({
                        "scenario_id": scenario_id,
                        "uri": self.scenario_uri(scenario_id)
                    }))
                    .collect::<Vec<_>>()
            }))
        } else if let Some(scenario_id) = request
            .uri
            .strip_prefix(&format!("{}://scenario/", self.scheme))
        {
            serde_json::to_string(&json!({
                "server": self.server,
                "scenario_id": scenario_id,
                "status": "available",
                "uri": self.scenario_uri(scenario_id)
            }))
        } else {
            return Err(rmcp::ErrorData::resource_not_found(
                format!("unknown fixture resource `{}`", request.uri),
                None,
            ));
        }
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("application/json"),
        ]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        Ok(ListPromptsResult {
            prompts: vec![
                Prompt::new(
                    self.prompt_name(),
                    Some(format!("Draft a {} fixture execution plan.", self.server)),
                    Some(vec![
                        PromptArgument::new("scenario")
                            .with_description("Scenario id from the fixture catalog.")
                            .with_required(true),
                    ]),
                )
                .with_title(format!("{} plan", self.server)),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        if request.name != self.prompt_name() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("unknown prompt `{}`", request.name),
                None,
            ));
        }
        let scenario = request
            .arguments
            .and_then(|args| args.get("scenario").cloned())
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unspecified".to_string());
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Prepare a {} fixture plan for scenario `{scenario}`. Read {} first.",
                self.server,
                self.scenario_uri(&scenario)
            ),
        )])
        .with_description(format!("{} fixture plan", self.server)))
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, rmcp::ErrorData> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if reference.uri != self.scenario_template() || request.argument.name != "scenario_id" {
            return Ok(CompleteResult::default());
        }
        let prefix = request.argument.value.to_lowercase();
        let values = self
            .scenario_ids()
            .into_iter()
            .filter(|scenario_id| scenario_id.starts_with(&prefix))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let completion =
            CompletionInfo::with_pagination(values.clone(), Some(values.len() as u32), false)
                .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        Ok(CompleteResult::new(completion))
    }
}

async fn cmd_fake_hosted_mcp(
    port: u16,
    server: String,
    scheme: String,
    internal_token_secret: String,
    ready_file: Option<PathBuf>,
) -> Result<()> {
    let server_slug = ServerSlug::new(server.clone())?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        server_slug,
        InternalTokenSecret::new(internal_token_secret)?,
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let allowed_hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let mcp_service = StreamableHttpService::new(
        {
            let server = server.clone();
            let scheme = scheme.clone();
            move || Ok(FakeHostedMcp::new(server.clone(), scheme.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts),
    );
    let mcp_router = AxumRouter::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(axum_middleware::from_fn_with_state(
            FakeHostedAuthState { verifier },
            authenticate_fake_hosted_mcp,
        ));
    let router = AxumRouter::new().nest(
        &format!("/{server}"),
        AxumRouter::new()
            .route("/healthz", axum_get(|| async { "ok" }))
            .nest("/mcp", mcp_router),
    );
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

async fn authenticate_fake_hosted_mcp(
    AxumState(state): AxumState<FakeHostedAuthState>,
    mut request: AxumRequest,
    next: AxumNext,
) -> axum::response::Response {
    match verify_fake_hosted_internal_authorization(&state.verifier, request.headers()) {
        Ok(identity) => {
            request
                .extensions_mut()
                .insert::<GatewayInternalIdentity>(identity);
            next.run(request).await
        }
        Err(message) => (AxumStatusCode::UNAUTHORIZED, message).into_response(),
    }
}

fn verify_fake_hosted_internal_authorization(
    verifier: &GatewayInternalTokenVerifier,
    headers: &AxumHeaderMap,
) -> Result<GatewayInternalIdentity, String> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "missing internal authorization".to_string())?;
    let Some((scheme, token)) = header.split_once(' ') else {
        return Err("missing bearer token".to_string());
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err("authorization scheme must be Bearer".to_string());
    }
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("bearer token contains invalid whitespace".to_string());
    }
    verifier.verify(token).map_err(|err| err.to_string())
}

async fn otlp_sink_hit(
    AxumState(state): AxumState<OtlpSinkState>,
    AxumPath(signal): AxumPath<String>,
    body: AxumBytes,
) -> impl AxumIntoResponse {
    match signal.as_str() {
        "logs" | "traces" | "metrics" => {
            use std::io::Write as _;

            let line = format!("{signal} {}\n", body.len());
            let result = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&state.hits_file)
                .and_then(|mut file| file.write_all(line.as_bytes()));
            match result {
                Ok(()) => AxumStatusCode::OK,
                Err(_) => AxumStatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        _ => AxumStatusCode::NOT_FOUND,
    }
}

async fn fake_oidc_jwks() -> impl AxumIntoResponse {
    match conformance_jwks() {
        Ok(jwks) => AxumJson(jwks).into_response(),
        Err(err) => (
            AxumStatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to build JWKS: {err}"),
        )
            .into_response(),
    }
}

async fn fake_oidc_authorize(
    AxumState(state): AxumState<FakeOidcState>,
    AxumQuery(request): AxumQuery<FakeOidcAuthorizeRequest>,
) -> impl AxumIntoResponse {
    if request.response_type != "code"
        || request.client_id != state.client_id
        || request.code_challenge_method != "S256"
        || request.code_challenge.is_empty()
        || !request
            .scope
            .split_whitespace()
            .any(|scope| scope == "openid")
    {
        return (AxumStatusCode::BAD_REQUEST, "invalid authorization request").into_response();
    }
    let code = format!("idp-code-{}", uuid::Uuid::new_v4().simple());
    match state.codes.lock() {
        Ok(mut codes) => {
            codes.insert(
                code.clone(),
                FakeOidcCode {
                    nonce: request.nonce,
                },
            );
        }
        Err(_) => {
            return (
                AxumStatusCode::INTERNAL_SERVER_ERROR,
                "code store unavailable",
            )
                .into_response();
        }
    }
    let mut redirect = match Url::parse(&request.redirect_uri) {
        Ok(url) => url,
        Err(_) => return (AxumStatusCode::BAD_REQUEST, "invalid redirect_uri").into_response(),
    };
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &request.state);
    (
        AxumStatusCode::FOUND,
        [(axum::http::header::LOCATION, redirect.to_string())],
    )
        .into_response()
}

async fn fake_oidc_token(
    AxumState(state): AxumState<FakeOidcState>,
    AxumForm(request): AxumForm<FakeOidcTokenRequest>,
) -> impl AxumIntoResponse {
    if request.grant_type != "authorization_code"
        || request.client_id != state.client_id
        || request.client_secret.as_deref() != Some(state.client_secret.as_str())
        || request.redirect_uri.is_empty()
        || request.code_verifier.is_empty()
    {
        return (AxumStatusCode::UNAUTHORIZED, "invalid token request").into_response();
    }
    let code = match state.codes.lock() {
        Ok(mut codes) => codes.remove(&request.code),
        Err(_) => {
            return (
                AxumStatusCode::INTERNAL_SERVER_ERROR,
                "code store unavailable",
            )
                .into_response();
        }
    };
    let Some(code) = code else {
        return (AxumStatusCode::BAD_REQUEST, "invalid authorization code").into_response();
    };
    match fake_oidc_id_token(&state, &code) {
        Ok(id_token) => AxumJson(FakeOidcTokenResponse {
            id_token,
            token_type: "Bearer",
            expires_in: 300,
        })
        .into_response(),
        Err(err) => (
            AxumStatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to sign ID token: {err}"),
        )
            .into_response(),
    }
}

fn fake_oidc_id_token(state: &FakeOidcState, code: &FakeOidcCode) -> Result<String> {
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(5))
        .ok_or_else(|| anyhow!("ID token expiration overflow"))?;
    let claims = FakeOidcIdTokenClaims {
        iss: state.issuer.clone(),
        sub: "00u-browser-smoke".to_string(),
        aud: state.client_id.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        nonce: code.nonce.clone(),
        groups: vec!["engineering".to_string()],
        roles: vec!["operator".to_string()],
        tenant: "tenant-a".to_string(),
        data_labels: vec!["cui".to_string()],
        email: "browser-smoke@example.com".to_string(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
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
        assurances: Default::default(),
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

async fn cmd_complete(client: &Client, uris: &ServerResourceUris, prefix: String) -> Result<()> {
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

async fn cmd_resources(client: &Client) -> Result<()> {
    let resources = client.list_resources(Default::default()).await?;
    for resource in resources.resources {
        println!(
            "{} — {}",
            resource.uri,
            resource.description.unwrap_or_default()
        );
    }
    if let Some(cursor) = resources.next_cursor {
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

async fn cmd_call(client: &Client, tool_name: String, arguments: String) -> Result<()> {
    let arguments = serde_json::from_str::<Value>(&arguments)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let result = client
        .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
        .await?;
    print_call_tool_result(&result);
    Ok(())
}

async fn cmd_complete_resource(
    client: &Client,
    uri: String,
    argument: String,
    prefix: String,
) -> Result<()> {
    let result = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(uri),
            ArgumentInfo::new(argument, prefix),
        ))
        .await?;
    for value in &result.completion.values {
        println!("{value}");
    }
    println!(
        "\n{} shown, total {:?}, has_more {:?}",
        result.completion.values.len(),
        result.completion.total,
        result.completion.has_more
    );
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
    uris: &ServerResourceUris,
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

fn task_from_cancel_result(result: ServerResult) -> Result<rmcp::model::Task> {
    match result {
        ServerResult::CancelTaskResult(result) => Ok(result.task),
        ServerResult::GetTaskResult(result) => Ok(result.task),
        other => Err(anyhow!("expected task-shaped cancel result, got {other:?}")),
    }
}

async fn cmd_run(
    client: &Client,
    uris: &ServerResourceUris,
    tool_name: String,
    model_id: String,
    input: String,
    output_dir: PathBuf,
    cancel: bool,
) -> Result<()> {
    let input: Value = serde_json::from_str(&input)?;

    // tools/call augmented with SEP-1319 task metadata + a progress token.
    let mut params = CallToolRequestParams::new(tool_name)
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
        let cancelled = task_from_cancel_result(result)?;
        if cancelled.task_id != task_id {
            return Err(anyhow!(
                "tasks/cancel returned task id `{}`, expected `{task_id}`",
                cancelled.task_id
            ));
        }
        if cancelled.status != TaskStatus::Cancelled {
            return Err(anyhow!(
                "tasks/cancel returned status {:?}, expected Cancelled",
                cancelled.status
            ));
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
        let info = client
            .send_request(ClientRequest::GetTaskRequest(Request::new(
                GetTaskParams::new(task_id.clone()),
            )))
            .await?;
        let ServerResult::GetTaskResult(info) = info else {
            return Err(anyhow!("expected GetTaskResult after cancel, got {info:?}"));
        };
        if info.task.task_id != task_id {
            return Err(anyhow!(
                "tasks/get returned task id `{}`, expected `{task_id}`",
                info.task.task_id
            ));
        }
        if info.task.status != TaskStatus::Cancelled {
            return Err(anyhow!(
                "tasks/get returned status {:?}, expected Cancelled",
                info.task.status
            ));
        }
        if client
            .send_request(ClientRequest::GetTaskPayloadRequest(Request::new(
                GetTaskPayloadParams::new(task_id.clone()),
            )))
            .await
            .is_ok()
        {
            return Err(anyhow!("tasks/result unexpectedly succeeded after cancel"));
        }
        println!("cancelled task {task_id} (status Cancelled)");
        return Ok(());
    }

    // Poll tasks/get, honoring the server's suggested interval. Subscribe to
    // the prediction resource as soon as the statusMessage names it.
    let poll_ms = created.task.poll_interval.unwrap_or(3000);
    let mut subscribed_uri = None::<String>;
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
        if subscribed_uri.is_none()
            && let Some(idx) = message.find(&prediction_prefix)
        {
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
            subscribed_uri = Some(uri);
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
        if let Some(uri) = subscribed_uri {
            client
                .unsubscribe(UnsubscribeRequestParams::new(uri.clone()))
                .await?;
            println!("unsubscribed from {uri}");
        }
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
    if let Some(uri) = subscribed_uri {
        client
            .unsubscribe(UnsubscribeRequestParams::new(uri.clone()))
            .await?;
        println!("unsubscribed from {uri}");
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
        Cmd::ContractSchemas { output_dir } => {
            return cmd_contract_schemas(output_dir.clone());
        }
        Cmd::DeploymentValidate { file } => {
            let plan = SelfHostedDeploymentPlan::load_json(file)?;
            println!("ok: {} deployment profile(s)", plan.profiles.len());
            return Ok(());
        }
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
        Cmd::GatewaySmokeControlPlane {
            base,
            output,
            idp_base_url,
            trusted_ca_path,
        } => {
            return cmd_gateway_smoke_control_plane(
                base.clone(),
                output.clone(),
                idp_base_url.clone(),
                trusted_ca_path.clone(),
            );
        }
        Cmd::GatewayTwoServerSmokeControlPlane {
            base,
            output,
            media_upstream_url,
            simulation_upstream_url,
        } => {
            return cmd_gateway_two_server_smoke_control_plane(
                base.clone(),
                output.clone(),
                media_upstream_url.clone(),
                simulation_upstream_url.clone(),
            );
        }
        Cmd::GatewayFakeOidcIdp {
            port,
            cert_pem,
            key_pem,
            ready_file,
            issuer,
            client_id,
            client_secret,
        } => {
            return cmd_gateway_fake_oidc_idp(
                *port,
                cert_pem.clone(),
                key_pem.clone(),
                ready_file.clone(),
                issuer.clone(),
                client_id.clone(),
                client_secret.clone(),
            )
            .await;
        }
        Cmd::OtlpHttpSink {
            port,
            ready_file,
            hits_file,
        } => {
            return cmd_otlp_http_sink(*port, ready_file.clone(), hits_file.clone()).await;
        }
        Cmd::FakeMediaProvider {
            port,
            ready_file,
            completion_delay_ms,
        } => {
            return cmd_fake_media_provider(*port, ready_file.clone(), *completion_delay_ms).await;
        }
        Cmd::FakeHostedMcp {
            port,
            server,
            scheme,
            internal_token_secret,
            ready_file,
        } => {
            return cmd_fake_hosted_mcp(
                *port,
                server.clone(),
                scheme.clone(),
                internal_token_secret.clone(),
                ready_file.clone(),
            )
            .await;
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
    let uris = ServerResourceUris::new(args.scheme);

    let result = match args.cmd {
        Cmd::AuthDiscovery { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayJwks => unreachable!("handled before MCP connection"),
        Cmd::GatewayPrivateKeyDerB64 => unreachable!("handled before MCP connection"),
        Cmd::GatewaySmokeControlPlane { .. } => unreachable!("handled before MCP connection"),
        Cmd::GatewayTwoServerSmokeControlPlane { .. } => {
            unreachable!("handled before MCP connection")
        }
        Cmd::GatewayFakeOidcIdp { .. } => unreachable!("handled before MCP connection"),
        Cmd::OtlpHttpSink { .. } => unreachable!("handled before MCP connection"),
        Cmd::FakeMediaProvider { .. } => unreachable!("handled before MCP connection"),
        Cmd::FakeHostedMcp { .. } => unreachable!("handled before MCP connection"),
        Cmd::ContractSchemas { .. } => unreachable!("handled before MCP connection"),
        Cmd::DeploymentValidate { .. } => unreachable!("handled before MCP connection"),
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
        Cmd::Resources => cmd_resources(&client).await,
        Cmd::Prompts => cmd_prompts(&client).await,
        Cmd::Resource { uri } => cmd_resource(&client, uri).await,
        Cmd::Prompt { name, arguments } => cmd_prompt(&client, name, arguments).await,
        Cmd::Call {
            tool_name,
            arguments,
        } => cmd_call(&client, tool_name, arguments).await,
        Cmd::CompleteResource {
            uri,
            argument,
            prefix,
        } => cmd_complete_resource(&client, uri, argument, prefix).await,
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
            tool_name,
            input,
            output_dir,
            cancel,
        } => {
            cmd_run(
                &client, &uris, tool_name, model_id, input, output_dir, cancel,
            )
            .await
        }
    };

    client.cancel().await?;
    result
}
