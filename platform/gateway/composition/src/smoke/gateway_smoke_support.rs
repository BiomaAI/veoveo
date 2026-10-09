//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: authorization discovery, resources (+templates),
//! completions, final-extension tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).
use veoveo_gateway_contract::SecretReference;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use veoveo_media_mcp::contract::GenerationPredictionSummary;
use veoveo_media_mcp::contract::MediaGenerationResult;

use anyhow::Result;
use anyhow::anyhow;
use axum::Form as AxumForm;
use axum::Json as AxumJson;
use axum::Router as AxumRouter;
use axum::body::Bytes as AxumBytes;
use axum::extract::Path as AxumPath;
use axum::extract::Query as AxumQuery;
use axum::extract::Request as AxumRequest;
use axum::extract::State as AxumState;
use axum::http::HeaderMap as AxumHeaderMap;
use axum::http::StatusCode as AxumStatusCode;
use axum::http::header::AUTHORIZATION;
use axum::middleware as axum_middleware;
use axum::middleware::Next as AxumNext;
use axum::response::IntoResponse as AxumIntoResponse;
use axum::routing::get as axum_get;
use axum::routing::post as axum_post;
use axum_server::tls_rustls::RustlsConfig;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::TimeDelta;
use chrono::Utc;
use clap::Parser;
use jsonwebtoken::Algorithm;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::encode;
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::jwk::JwkSet;
use rcgen::generate_simple_self_signed;

use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResponse;
use rmcp::model::CallToolResult;
use rmcp::model::CompleteRequestParams;
use rmcp::model::CompleteResult;
use rmcp::model::CompletionInfo;
use rmcp::model::ContentBlock;
use rmcp::model::GetPromptRequestParams;
use rmcp::model::GetPromptResult;
use rmcp::model::Implementation;
use rmcp::model::JsonObject;
use rmcp::model::ListPromptsResult;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::ListToolsResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::Prompt;
use rmcp::model::PromptArgument;
use rmcp::model::PromptMessage;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::ReadResourceResponse;
use rmcp::model::ReadResourceResult;
use rmcp::model::Reference;
use rmcp::model::Resource;
use rmcp::model::ResourceContents;
use rmcp::model::ResourceTemplate;
use rmcp::model::Role;
use rmcp::model::ServerCapabilities;
use rmcp::model::ServerConfig;
use rmcp::model::Tool;
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use url::Url;
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_artifact_contract::ComplianceMetadata;
use veoveo_frames_mcp::contract::CoordinateOperationProvenance;
use veoveo_mcp_contract::AccessTokenSubject;
use veoveo_mcp_contract::AnalyticalRuntimeDeployment;
use veoveo_mcp_contract::DataLabelDefinition;
use veoveo_mcp_contract::DataRetentionPolicy;
use veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER;
use veoveo_mcp_contract::GatewayAuthorizationCodeRecord;
use veoveo_mcp_contract::GatewayAuthorizationRequest;
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_mcp_contract::GatewayControlPlaneRevision;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_mcp_contract::GatewayInternalTokenVerifier;
use veoveo_mcp_contract::GatewayInternalTrustBundle;
use veoveo_mcp_contract::GatewayJwtRevocation;
use veoveo_mcp_contract::GatewayJwtRevocationApplyResult;
use veoveo_mcp_contract::GatewayJwtRevocationPruneResult;
use veoveo_mcp_contract::GatewayJwtRevocationRequest;
use veoveo_mcp_contract::GatewayProfile;
use veoveo_mcp_contract::GatewayResourceProjection;
use veoveo_mcp_contract::GatewayResourceSubscription;
use veoveo_mcp_contract::IdentityProvider;
use veoveo_mcp_contract::IdentityProviderDeployment;
use veoveo_mcp_contract::IdentityProviderOidcClientRegistration;
use veoveo_mcp_contract::IngressDeployment;
use veoveo_mcp_contract::McpSurfaceCapabilities;
use veoveo_mcp_contract::OAuthClientRegistration;
use veoveo_mcp_contract::ObjectStoreDeployment;
use veoveo_mcp_contract::PlatformStoreDeployment;
use veoveo_mcp_contract::PolicyDecision;
use veoveo_mcp_contract::PolicyRule;
use veoveo_mcp_contract::PolicySet;
use veoveo_mcp_contract::Principal;
use veoveo_mcp_contract::PrincipalAuditAttributes;
use veoveo_mcp_contract::ProfileServerExposure;
use veoveo_mcp_contract::ResourceAuthorizationServer;
use veoveo_mcp_contract::SecretManagerDeployment;
use veoveo_mcp_contract::SelfHostedDeploymentPlan;
use veoveo_mcp_contract::SelfHostedDeploymentProfile;
use veoveo_mcp_contract::ServerManifest;
use veoveo_mcp_contract::ServerSlug;
use veoveo_mcp_contract::ServiceToServiceSecurity;
use veoveo_mcp_contract::TelemetryDeployment;
use veoveo_mcp_contract::TenantDefinition;
use veoveo_mcp_contract::TenantModel;
use veoveo_mcp_contract::TokenIssuer;
use veoveo_mcp_contract::UpstreamEndpoint;
use veoveo_mcp_contract::UsageRecord;
use veoveo_mcp_contract::UsageReport;

#[path = "utility/cli.rs"]
mod cli;
#[path = "utility/client_signing.rs"]
mod client_signing;
#[path = "utility/control_plane.rs"]
mod control_plane;
#[path = "utility/fake_services.rs"]
mod fake_services;
#[path = "utility/schema.rs"]
mod schema;
#[path = "utility/tokens.rs"]
mod tokens;
use cli::Args;
use cli::Cmd;
use control_plane::*;
use fake_services::*;
use schema::cmd_contract_schemas;
use tokens::*;
#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    let args = Args::parse();
    match &args.cmd {
        Cmd::GatewayPilotSmokeControlPlane {
            base,
            output,
            frames_upstream_url,
            optimization_upstream_url,
        } => cmd_gateway_pilot_smoke_control_plane(
            base.clone(),
            output.clone(),
            frames_upstream_url.clone(),
            optimization_upstream_url.clone(),
        ),
        Cmd::ContractSchemas { output_dir } => cmd_contract_schemas(output_dir.clone()),
        Cmd::GatewayJwks => cmd_gateway_jwks(),
        Cmd::GatewayPrivateKeyDerB64 => {
            cmd_gateway_private_key_der_b64();
            Ok(())
        }
        Cmd::GatewaySmokeControlPlane {
            base,
            output,
            idp_base_url,
            trusted_ca_path,
        } => cmd_gateway_smoke_control_plane(
            base.clone(),
            output.clone(),
            idp_base_url.clone(),
            trusted_ca_path.clone(),
        ),
        Cmd::GatewayTwoServerSmokeControlPlane {
            base,
            output,
            media_upstream_url,
            simulation_upstream_url,
        } => cmd_gateway_two_server_smoke_control_plane(
            base.clone(),
            output.clone(),
            media_upstream_url.clone(),
            simulation_upstream_url.clone(),
        ),
        Cmd::GatewayAgentSmokeControlPlane {
            base,
            output,
            duckdb_upstream_url,
        } => cmd_gateway_agent_smoke_control_plane(
            base.clone(),
            output.clone(),
            duckdb_upstream_url.clone(),
        ),
        Cmd::GatewayFakeOidcIdp {
            port,
            cert_pem,
            key_pem,
            ready_file,
            issuer,
            client_id,
            client_secret,
        } => {
            cmd_gateway_fake_oidc_idp(
                *port,
                cert_pem.clone(),
                key_pem.clone(),
                ready_file.clone(),
                issuer.clone(),
                client_id.clone(),
                client_secret.clone(),
            )
            .await
        }
        Cmd::OtlpHttpSink {
            port,
            ready_file,
            hits_file,
        } => cmd_otlp_http_sink(*port, ready_file.clone(), hits_file.clone()).await,
        Cmd::FakeHostedMcp {
            port,
            server,
            scheme,
            internal_trust_jwks,
            ready_file,
        } => {
            cmd_fake_hosted_mcp(
                *port,
                server.clone(),
                scheme.clone(),
                internal_trust_jwks.clone(),
                ready_file.clone(),
            )
            .await
        }
        Cmd::GatewayClientAssertion {
            client_id,
            audience,
            jwt_id,
            ttl_minutes,
        } => cmd_gateway_client_assertion(ClientAssertionInput {
            client_id: client_id.clone(),
            audience: audience.clone(),
            jwt_id: jwt_id.clone(),
            ttl_minutes: *ttl_minutes,
        }),
        Cmd::GatewayTokenExchange {
            token_url,
            client_id,
            audience,
            resource,
            scopes,
            work_context,
            jwt_id,
            ttl_minutes,
            client_key_file,
            client_key_id,
        } => {
            cmd_gateway_token_exchange(TokenExchangeInput {
                token_url: token_url.clone(),
                client_assertion: ClientAssertionInput {
                    client_id: client_id.clone(),
                    audience: audience.clone().unwrap_or_else(|| token_url.clone()),
                    jwt_id: jwt_id.clone(),
                    ttl_minutes: *ttl_minutes,
                },
                resource: resource.clone(),
                scopes: scopes.clone(),
                work_context: work_context.clone(),
                signing: client_signing::resolve(
                    token_url,
                    client_key_file.as_deref(),
                    client_key_id.as_deref(),
                )?,
            })
            .await
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
            principal_assurances,
            jwt_id,
            ttl_minutes,
        } => cmd_gateway_id_jag(IdJagInput {
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
            principal_assurances: principal_assurances.clone(),
            jwt_id: jwt_id.clone(),
            ttl_minutes: *ttl_minutes,
        }),
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
            principal_assurances,
            jwt_id,
            ttl_minutes,
        } => {
            cmd_gateway_id_jag_token_exchange(IdJagTokenExchangeInput {
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
                    principal_assurances: principal_assurances.clone(),
                    jwt_id: jwt_id.clone(),
                    ttl_minutes: *ttl_minutes,
                },
                requested_scopes: scopes.clone(),
            })
            .await
        }
    }
}
