use std::path::PathBuf;

use clap::{Parser, Subcommand};
use veoveo_mcp_contract::ArtifactId;

#[derive(Parser)]
#[command(name = "conformance", about = "Veoveo MCP conformance client")]
pub(super) struct Args {
    /// MCP endpoint of the server under test.
    #[arg(long, default_value = "http://localhost:8787/media/mcp", global = true)]
    pub(super) url: String,
    /// URI scheme used by the server's Veoveo resources.
    #[arg(long, default_value = "media", global = true)]
    pub(super) scheme: String,
    /// Bearer token sent to the MCP endpoint under test.
    #[arg(long, env = "MCP_BEARER_TOKEN", global = true, hide_env_values = true)]
    pub(super) bearer_token: Option<String>,
    /// Base64 PKCS#8 Ed25519 gateway signing key for direct hosted-server conformance.
    #[arg(
        long,
        env = "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
        global = true,
        hide_env_values = true,
        conflicts_with = "bearer_token"
    )]
    pub(super) internal_signing_key_der_b64: Option<String>,
    /// `kid` for direct hosted-server conformance assertions.
    #[arg(
        long,
        env = "VEOVEO_INTERNAL_SIGNING_KEY_ID",
        default_value = veoveo_mcp_contract::DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID,
        global = true
    )]
    pub(super) internal_signing_key_id: String,
    /// Server slug for direct hosted-server conformance.
    #[arg(long, default_value = "media", global = true)]
    pub(super) internal_server: String,
    /// Veoveo profile id embedded in direct hosted-server conformance assertions.
    #[arg(long, default_value = "operator", global = true)]
    pub(super) internal_profile: String,
    /// Principal subject embedded in direct hosted-server conformance assertions.
    /// Vary this to act as a different principal (e.g. to assert that a
    /// non-owner is denied an artifact they hold no grant for).
    #[arg(long, default_value = "conformance", global = true)]
    pub(super) internal_principal_subject: String,
    /// Tenant embedded in direct hosted-server conformance assertions. Vary this
    /// to assert the plane's hard cross-tenant isolation.
    #[arg(long, default_value = "local", global = true)]
    pub(super) internal_tenant: String,
    /// Scopes embedded in direct hosted-server conformance assertions.
    #[arg(long = "internal-scope", default_value = "operator:use", global = true)]
    pub(super) internal_scopes: Vec<String>,
    #[command(subcommand)]
    pub(super) cmd: Cmd,
}

#[derive(Subcommand)]
pub(super) enum Cmd {
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
    /// Write a duckdb-upstream gateway smoke control plane for agent task tests.
    GatewayAgentSmokeControlPlane {
        /// Base gateway control plane JSON.
        #[arg(long)]
        base: PathBuf,
        /// Output gateway control plane JSON.
        #[arg(long)]
        output: PathBuf,
        /// DuckDB MCP upstream URL.
        #[arg(long)]
        duckdb_upstream_url: String,
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
        #[arg(long, default_value = "veoveo")]
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
        /// HMAC secret used to sign webhook deliveries.
        #[arg(long, default_value = "whsec_smoke-webhook-secret", hide = true)]
        webhook_secret: String,
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
        /// Public Ed25519 JWKS used to verify gateway identity assertions.
        #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
        internal_trust_jwks: String,
        /// File touched after the listener is ready.
        #[arg(long)]
        ready_file: Option<PathBuf>,
    },
    /// Write a frames+optimization gateway smoke control plane for pilot agent tests.
    GatewayPilotSmokeControlPlane {
        /// Base gateway control plane JSON.
        #[arg(long)]
        base: PathBuf,
        /// Output gateway control plane JSON.
        #[arg(long)]
        output: PathBuf,
        /// Frames MCP upstream URL.
        #[arg(long)]
        frames_upstream_url: String,
        /// Optimization MCP upstream URL.
        #[arg(long)]
        optimization_upstream_url: String,
    },
    /// Serve a scripted OpenAI-compatible chat-completions endpoint for agent smoke tests.
    FakeOpenaiLlm {
        /// HTTP listen port.
        #[arg(long)]
        port: u16,
        /// File touched after the listener is ready.
        #[arg(long)]
        ready_file: Option<PathBuf>,
    },
    /// Print a private-key JWT client assertion signed by the conformance private key.
    GatewayClientAssertion {
        /// OAuth client id used as issuer and subject.
        #[arg(long, default_value = "operator-service")]
        client_id: String,
        /// Token endpoint audience claim.
        #[arg(long, default_value = "https://veoveo.example/oauth/token")]
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
        #[arg(long, default_value = "operator-service")]
        client_id: String,
        /// Client assertion audience claim. Defaults to the token endpoint URL.
        #[arg(long)]
        audience: Option<String>,
        /// MCP protected resource for the requested profile.
        #[arg(long)]
        resource: Option<String>,
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
        #[arg(long, default_value = "https://veoveo.example/oauth")]
        audience: String,
        /// MCP protected resource claim.
        #[arg(long, default_value = "https://veoveo.example/mcp/operator")]
        resource: String,
        /// Registered MCP client id.
        #[arg(long, default_value = "operator-local-public")]
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
        /// Principal assurance claim. Repeat for multiple assurances, e.g. us_person.
        #[arg(long = "principal-assurance")]
        principal_assurances: Vec<String>,
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
        #[arg(long, default_value = "https://veoveo.example/oauth")]
        audience: String,
        /// MCP protected resource claim.
        #[arg(long, default_value = "https://veoveo.example/mcp/operator")]
        resource: String,
        /// Registered MCP client id.
        #[arg(long, default_value = "operator-local-public")]
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
        /// Principal assurance claim. Repeat for multiple assurances, e.g. us_person.
        #[arg(long = "principal-assurance")]
        principal_assurances: Vec<String>,
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
    /// Verify the MCP Apps surface: extension declaration, app view
    /// resources, tool links, and self-contained HTML.
    AppsCheck,
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
    /// Call one tool directly.
    Call {
        /// Tool name to invoke.
        #[arg(long)]
        tool_name: String,
        /// Tool arguments as a JSON object.
        #[arg(long)]
        arguments: String,
        /// Invoke through the explicit core-task compatibility projection.
        #[arg(long)]
        task: bool,
    },
    /// Invoke one tool through the canonical final MCP task extension.
    TaskCall {
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
        artifact_id: ArtifactId,
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
