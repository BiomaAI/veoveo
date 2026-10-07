use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

#[derive(Parser)]
#[command(name = "media-smoke", about = "Veoveo MCP conformance client")]
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
    Models {
        query: Option<String>,
        /// Filter by model type (e.g. image-to-image, text-to-video).
        #[arg(long)]
        r#type: Option<String>,
    },
    Complete {
        prefix: String,
    },
    Schema {
        model_id: String,
    },
    Prediction {
        id: String,
    },
    Usage {
        task_id: String,
    },
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
