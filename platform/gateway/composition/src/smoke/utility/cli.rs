use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

#[derive(Parser)]
#[command(
    name = "gateway-smoke-support",
    about = "Veoveo MCP conformance client"
)]
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
    /// Work context embedded in direct hosted-server conformance assertions.
    #[arg(long, default_value = "conformance", global = true)]
    pub(super) internal_work_context: String,
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
    ContractSchemas {
        /// Directory that receives one .schema.json file per exported contract type.
        #[arg(long, default_value = "schemas")]
        output_dir: PathBuf,
    },
    GatewayJwks,
    GatewayPrivateKeyDerB64,
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
    GatewayTokenExchange {
        /// Installation-owned RSA PEM signing key. Public fixture signing is loopback-only.
        #[arg(
            long,
            env = "VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE",
            requires = "client_key_id"
        )]
        client_key_file: Option<PathBuf>,
        /// Public identifier of the installation-owned client signing key.
        #[arg(
            long,
            env = "VEOVEO_SERVICE_CLIENT_KEY_ID",
            requires = "client_key_file"
        )]
        client_key_id: Option<String>,
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
        /// Work Context selected for the issued invocation authority.
        #[arg(long)]
        work_context: Option<String>,
        /// Client assertion JWT id claim.
        #[arg(long)]
        jwt_id: Option<String>,
        /// Client assertion lifetime in minutes.
        #[arg(long, default_value_t = 5)]
        ttl_minutes: i64,
    },
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
}
