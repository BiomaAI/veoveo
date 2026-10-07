use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use veoveo_artifact_contract::ArtifactId;

#[derive(Parser)]
#[command(name = "artifact-smoke", about = "Veoveo MCP conformance client")]
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
    Artifact {
        artifact_id: ArtifactId,
        /// Where to save the artifact file.
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
    },
}
