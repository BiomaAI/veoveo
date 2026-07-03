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

#[path = "conformance/auth_discovery.rs"]
mod auth_discovery;
#[path = "conformance/client.rs"]
mod client;
#[path = "conformance/control_plane.rs"]
mod control_plane;
#[path = "conformance/fake_services.rs"]
mod fake_services;
#[path = "conformance/mcp_commands.rs"]
mod mcp_commands;
#[path = "conformance/schema.rs"]
mod schema;
#[path = "conformance/tokens.rs"]
mod tokens;

use auth_discovery::{AuthDiscoveryCheck, cmd_auth_discovery};
use client::connect;
use control_plane::{cmd_gateway_smoke_control_plane, cmd_gateway_two_server_smoke_control_plane};
use fake_services::{
    cmd_fake_hosted_mcp, cmd_fake_media_provider, cmd_gateway_fake_oidc_idp, cmd_otlp_http_sink,
};
use mcp_commands::{
    cmd_call, cmd_complete, cmd_complete_resource, cmd_info, cmd_models_from_catalog, cmd_prompt,
    cmd_prompts, cmd_resource, cmd_resources, cmd_run, cmd_tasks, read_resource_json,
    save_output_uri,
};
use schema::cmd_contract_schemas;
use tokens::{
    ClientAssertionInput, IdJagInput, IdJagTokenExchangeInput, TokenExchangeInput,
    cmd_gateway_client_assertion, cmd_gateway_id_jag, cmd_gateway_id_jag_token_exchange,
    cmd_gateway_jwks, cmd_gateway_private_key_der_b64, cmd_gateway_token_exchange,
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
