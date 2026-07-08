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
use clap::Parser;
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
        ListResourcesResult, ListTasksRequest, ListToolsResult, PaginatedRequestParams,
        ProgressNotificationParam, Prompt, PromptArgument, PromptMessage,
        ReadResourceRequestParams, ReadResourceResult, Reference, Request, Resource,
        ResourceContents, ResourceTemplate, ResourceUpdatedNotificationParam, Role,
        ServerCapabilities, ServerInfo, ServerResult, SubscribeRequestParams, TaskMetadata,
        TaskStatus, TaskStatusNotificationParam, TaskSupport, Tool, ToolExecution,
        UnsubscribeRequestParams,
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
    AccessTokenSubject, ArtifactMetadata, AuditEvent, AuthAuditEvent, BatchTransformOutput,
    BatchTransformRequest, ComplianceMetadata, ConvertFrameOutput, ConvertFrameRequest,
    CoordinateOperationProvenance, CoordinatePosition, DataLabelDefinition, DataRetentionPolicy,
    DeriveLocalFrameOutput, DeriveLocalFrameRequest, FrameDefinition,
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayInternalIdentity,
    GatewayInternalTokenIssuer, GatewayInternalTokenVerifier, GatewayJwtRevocation,
    GatewayJwtRevocationApplyResult, GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest,
    GatewayProfile, GatewayProfileId, GatewayResourceProjection, GatewayResourceSubscription,
    GatewayTaskMapping, GenerationPredictionSummary, GenerationRunOutput, GeodesicDirectOutput,
    GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest, GeofenceGeometry,
    IdentityProvider, IdentityProviderOidcClientRegistration, InternalTokenSecret,
    McpSurfaceCapabilities, NetworkBoundaryRule, OAuthClientRegistration, ObjectStoreDeployment,
    PolicyDecision, PolicyRule, PolicySet, Principal, PrincipalAuditAttributes, PrincipalId,
    PrincipalKind, ProfileServerExposure, RELATED_TASK_META_KEY, RegulatedDataControls,
    ResourceAuthorizationServer, ScopeName, SecretReference, SelfHostedDeploymentPlan,
    SelfHostedDeploymentProfile, ServerManifest, ServerResourceUris, ServerSlug,
    ServiceToServiceSecurity, StateStoreDeployment, TelemetrySinkDeployment, TenantDefinition,
    TenantId, TokenIssuer, TokenSubject, TransformCrsOutput, TransformCrsRequest, UpstreamEndpoint,
    UsageRecord, UsageReport, ValidateGeofenceOutput, ValidateGeofenceRequest,
};

#[path = "conformance/auth_discovery.rs"]
mod auth_discovery;
#[path = "conformance/cli.rs"]
mod cli;
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
use cli::{Args, Cmd};
use client::connect;
use control_plane::{
    cmd_gateway_agent_smoke_control_plane, cmd_gateway_smoke_control_plane,
    cmd_gateway_two_server_smoke_control_plane,
};
use fake_services::{
    cmd_fake_hosted_mcp, cmd_fake_media_provider, cmd_fake_openai_llm, cmd_gateway_fake_oidc_idp,
    cmd_otlp_http_sink,
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

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> Result<()> {
    install_rustls_provider();
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
        Cmd::GatewayAgentSmokeControlPlane {
            base,
            output,
            duckdb_upstream_url,
        } => {
            return cmd_gateway_agent_smoke_control_plane(
                base.clone(),
                output.clone(),
                duckdb_upstream_url.clone(),
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
        Cmd::FakeOpenaiLlm { port, ready_file } => {
            return cmd_fake_openai_llm(*port, ready_file.clone()).await;
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
            resource,
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
                resource: resource.clone(),
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
            principal_assurances,
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
                principal_assurances: principal_assurances.clone(),
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
            principal_assurances,
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
                    principal_assurances: principal_assurances.clone(),
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
        Cmd::GatewayAgentSmokeControlPlane { .. } => {
            unreachable!("handled before MCP connection")
        }
        Cmd::GatewayFakeOidcIdp { .. } => unreachable!("handled before MCP connection"),
        Cmd::OtlpHttpSink { .. } => unreachable!("handled before MCP connection"),
        Cmd::FakeMediaProvider { .. } => unreachable!("handled before MCP connection"),
        Cmd::FakeHostedMcp { .. } => unreachable!("handled before MCP connection"),
        Cmd::FakeOpenaiLlm { .. } => unreachable!("handled before MCP connection"),
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
            task,
        } => cmd_call(&client, tool_name, arguments, task).await,
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
