use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "conformance",
    about = "Domain-neutral MCP protocol conformance"
)]
pub(super) struct Args {
    #[arg(long, default_value = "http://localhost:8787/mcp", global = true)]
    pub(super) url: String,
    #[arg(long, global = true)]
    pub(super) scheme: Option<String>,
    #[arg(long, env = "MCP_BEARER_TOKEN", hide_env_values = true, global = true)]
    pub(super) bearer_token: Option<String>,
    #[command(subcommand)]
    pub(super) cmd: Cmd,
}
#[derive(Subcommand)]
pub(super) enum Cmd {
    /// Obtain an ordinary public-client OAuth token through browser PKCE.
    OAuthLogin(super::oauth_login::LoginArgs),
    KnowledgeSource(super::source_checks::SourceChecks),
    Certify {
        /// JSON conformance profile.
        #[arg(long)]
        profile: PathBuf,
        /// Machine-readable conformance report.
        #[arg(long, default_value = "conformance-report.json")]
        report: PathBuf,
    },
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
    Info,
    Tools,
    Resources,
    AppsCheck,
    Prompts,
    Resource {
        uri: String,
    },
    Prompt {
        name: String,
        /// Prompt arguments as a JSON object.
        #[arg(long)]
        arguments: Option<String>,
    },
    Call {
        /// Tool name to invoke.
        #[arg(long)]
        tool_name: String,
        /// Tool arguments as a JSON object.
        #[arg(long)]
        arguments: String,
        /// Require the server to create an official durable Task.
        #[arg(long)]
        task: bool,
    },
    TaskCall {
        /// Tool name to invoke.
        #[arg(long)]
        tool_name: String,
        /// Tool arguments as a JSON object.
        #[arg(long)]
        arguments: String,
        /// Maximum time to wait for the durable task to reach a terminal state.
        #[arg(
            long,
            default_value_t = 300,
            value_parser = clap::value_parser!(u64).range(1..)
        )]
        timeout_seconds: u64,
    },
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
    /// Export the two generic certification schemas.
    Schemas {
        #[arg(long)]
        output_dir: PathBuf,
    },
}
