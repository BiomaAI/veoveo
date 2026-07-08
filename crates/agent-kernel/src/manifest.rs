//! Typed agent manifest: everything that makes one agent an agent.
//!
//! A manifest is data. The kernel executes manifests; agent types (the Pilot,
//! future agents) are manifest + preamble + migrations, never kernel code.
//! Loading is fail-closed: unknown fields, missing environment variables, and
//! out-of-range knobs are hard errors before the agent boots.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub agent: AgentIdentity,
    pub model: ModelConfig,
    pub gateway: GatewayAccess,
    pub episode: EpisodeConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub budgets: BudgetConfig,
    #[serde(default)]
    pub schedule: ScheduleConfig,
    /// Directory of `NNNN_*.sql` domain migrations applied at boot, relative
    /// to the manifest file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations_dir: Option<std::path::PathBuf>,
    /// System preamble for every episode.
    pub preamble: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    /// RRD segment directory, relative to the data dir.
    #[serde(default = "default_rrd_dir")]
    pub rrd_dir: String,
    /// Rotate to a fresh segment once the live one exceeds this size.
    #[serde(default = "default_segment_max_bytes")]
    pub segment_max_bytes: u64,
    /// Domain tables `memory_write` may mutate; the `kernel` schema is never
    /// writable through tools.
    #[serde(default)]
    pub memory_write_tables: Vec<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            rrd_dir: default_rrd_dir(),
            segment_max_bytes: default_segment_max_bytes(),
            memory_write_tables: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    /// Approximate token budget for the assembled episode prompt.
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    /// SQL-backed prompt sections, rendered in ascending priority order.
    #[serde(default)]
    pub sections: Vec<ContextSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    #[serde(default)]
    pub per_episode: PerEpisodeBudget,
    /// Window budget enforced by the scheduler before an episode starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_max_episodes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PerEpisodeBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_calls: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleConfig {
    /// Heartbeat cadence; every tick wakes an episode so silence is bounded.
    #[serde(default = "default_heartbeat_interval_s")]
    pub heartbeat_interval_s: u64,
    /// Debounce between episodes for non-priority wakes.
    #[serde(default)]
    pub min_wake_interval_s: u64,
    /// How long the scheduler drains the bus before starting an episode.
    #[serde(default = "default_wake_coalesce_window_ms")]
    pub wake_coalesce_window_ms: u64,
    /// Grace an in-flight elicitation waits for an inline operator answer
    /// before parking.
    #[serde(default = "default_elicitation_grace_s")]
    pub elicitation_grace_s: u64,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_s: default_heartbeat_interval_s(),
            min_wake_interval_s: 0,
            wake_coalesce_window_ms: default_wake_coalesce_window_ms(),
            elicitation_grace_s: default_elicitation_grace_s(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSection {
    pub name: String,
    /// Lower renders earlier and survives truncation longer.
    pub priority: u8,
    /// Single read-only SELECT over the agent's memory database.
    pub sql: String,
    #[serde(default = "default_section_max_rows")]
    pub max_rows: u64,
    #[serde(default = "default_section_max_tokens")]
    pub max_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentIdentity {
    /// Stable agent id: lowercase alphanumerics, `-` and `_`.
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// OpenAI-compatible chat-completions base URL. `${VAR}` placeholders are
    /// expanded from the environment at load time.
    pub base_url: String,
    /// Environment variable holding the API key.
    pub api_key_env: String,
    /// Model id passed to the completions endpoint.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayAccess {
    /// Gateway base URL the agent connects to (e.g. `http://127.0.0.1:8788`).
    pub url: String,
    /// Gateway profile mounted under `/mcp/{profile}`.
    pub profile: String,
    /// OAuth client id for the client-credentials grant.
    pub client_id: String,
    /// Audience for the private-key JWT client assertion (the public token
    /// endpoint URL, which may differ from the connect URL behind an edge).
    pub audience: String,
    /// Protected resource the token is minted for.
    pub resource: String,
    pub scopes: Vec<String>,
    /// Environment variable holding the base64 DER RSA private key that signs
    /// client assertions.
    pub private_key_env: String,
    /// `kid` the gateway uses to resolve this client's JWKS entry.
    pub private_key_kid: String,
    /// Fraction of the access-token lifetime after which the connection is
    /// rotated before the next episode.
    #[serde(default = "default_token_refresh_fraction")]
    pub token_refresh_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeConfig {
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    #[serde(default = "default_request_timeout_s")]
    pub request_timeout_s: u64,
    #[serde(default = "default_task_deadline_s")]
    pub task_deadline_s: u64,
    #[serde(default = "default_task_poll_interval_ms")]
    pub task_poll_interval_ms: u64,
}

fn default_token_refresh_fraction() -> f64 {
    0.6
}

fn default_rrd_dir() -> String {
    "rrd".to_string()
}

fn default_segment_max_bytes() -> u64 {
    256 * 1024 * 1024
}

fn default_max_context_tokens() -> u64 {
    24_000
}

fn default_section_max_rows() -> u64 {
    50
}

fn default_section_max_tokens() -> u64 {
    2_000
}

fn default_heartbeat_interval_s() -> u64 {
    300
}

fn default_wake_coalesce_window_ms() -> u64 {
    250
}

fn default_elicitation_grace_s() -> u64 {
    30
}

fn default_max_turns() -> usize {
    8
}

fn default_request_timeout_s() -> u64 {
    300
}

fn default_task_deadline_s() -> u64 {
    600
}

fn default_task_poll_interval_ms() -> u64 {
    1_000
}

impl AgentManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading agent manifest {}", path.display()))?;
        let mut manifest: AgentManifest = serde_json::from_str(&raw)
            .with_context(|| format!("parsing agent manifest {}", path.display()))?;
        manifest.model.base_url = expand_env_placeholders(&manifest.model.base_url)?;
        if let (Some(dir), Some(parent)) = (&manifest.migrations_dir, path.parent())
            && dir.is_relative()
        {
            manifest.migrations_dir = Some(parent.join(dir));
        }
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.agent.id.is_empty()
            || !self.agent.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
        {
            bail!("agent.id must be non-empty lowercase alphanumerics, `-` or `_`");
        }
        for (field, value) in [
            ("model.base_url", &self.model.base_url),
            ("model.api_key_env", &self.model.api_key_env),
            ("model.model", &self.model.model),
            ("gateway.url", &self.gateway.url),
            ("gateway.profile", &self.gateway.profile),
            ("gateway.client_id", &self.gateway.client_id),
            ("gateway.audience", &self.gateway.audience),
            ("gateway.resource", &self.gateway.resource),
            ("gateway.private_key_env", &self.gateway.private_key_env),
            ("gateway.private_key_kid", &self.gateway.private_key_kid),
            ("preamble", &self.preamble),
        ] {
            if value.trim().is_empty() {
                bail!("{field} must not be empty");
            }
        }
        if self.gateway.scopes.is_empty() {
            bail!("gateway.scopes must list at least one scope");
        }
        let fraction = self.gateway.token_refresh_fraction;
        if !(fraction > 0.0 && fraction <= 1.0) {
            bail!("gateway.token_refresh_fraction must be in (0, 1], got {fraction}");
        }
        if self.episode.max_turns == 0 {
            bail!("episode.max_turns must be greater than zero");
        }
        if self.schedule.heartbeat_interval_s == 0 {
            bail!("schedule.heartbeat_interval_s must be greater than zero");
        }
        for table in &self.memory.memory_write_tables {
            if table.trim().is_empty() || table.contains('.') {
                bail!("memory.memory_write_tables entries must be bare main-schema table names");
            }
        }
        for section in &self.context.sections {
            if section.name.trim().is_empty() {
                bail!("context.sections entries must be named");
            }
            crate::ledger::ensure_single_select(&section.sql)
                .with_context(|| format!("context section `{}`", section.name))?;
        }
        if let Some(dir) = &self.migrations_dir
            && !dir.is_dir()
        {
            bail!("migrations_dir `{}` is not a directory", dir.display());
        }
        std::env::var(&self.model.api_key_env).with_context(|| {
            format!("model.api_key_env `{}` is not set", self.model.api_key_env)
        })?;
        std::env::var(&self.gateway.private_key_env).with_context(|| {
            format!(
                "gateway.private_key_env `{}` is not set",
                self.gateway.private_key_env
            )
        })?;
        Ok(())
    }

    pub fn mcp_url(&self) -> String {
        format!(
            "{}/mcp/{}",
            self.gateway.url.trim_end_matches('/'),
            self.gateway.profile
        )
    }

    pub fn token_url(&self) -> String {
        format!("{}/oauth/token", self.gateway.url.trim_end_matches('/'))
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.episode.request_timeout_s)
    }

    pub fn task_deadline(&self) -> Duration {
        Duration::from_secs(self.episode.task_deadline_s)
    }

    pub fn task_poll_interval(&self) -> Duration {
        Duration::from_millis(self.episode.task_poll_interval_ms)
    }
}

/// Expand `${VAR}` placeholders from the environment, failing closed on any
/// unset variable.
fn expand_env_placeholders(value: &str) -> Result<String> {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            bail!("unterminated `${{` placeholder in `{value}`");
        };
        let name = &after[..end];
        let expanded = std::env::var(name)
            .with_context(|| format!("environment variable `{name}` referenced by `{value}`"))?;
        result.push_str(&expanded);
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json() -> serde_json::Value {
        serde_json::json!({
            "agent": { "id": "test-agent", "display_name": "Test Agent" },
            "model": {
                "base_url": "http://127.0.0.1:9/v1",
                "api_key_env": "TEST_MANIFEST_API_KEY",
                "model": "test/model"
            },
            "gateway": {
                "url": "http://127.0.0.1:9",
                "profile": "operator",
                "client_id": "operator-service",
                "audience": "https://veoveo.bioma.ai/oauth/token",
                "resource": "https://veoveo.bioma.ai/mcp/operator",
                "scopes": ["operator:use"],
                "private_key_env": "TEST_MANIFEST_PRIVATE_KEY",
                "private_key_kid": "test-key"
            },
            "episode": {},
            "preamble": "You are a test agent."
        })
    }

    #[test]
    fn manifest_round_trip_and_defaults() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_MANIFEST_API_KEY", "k");
            std::env::set_var("TEST_MANIFEST_PRIVATE_KEY", "p");
        }
        let manifest: AgentManifest = serde_json::from_value(manifest_json()).expect("parses");
        manifest.validate().expect("validates");
        assert_eq!(manifest.episode.max_turns, 8);
        assert!((manifest.gateway.token_refresh_fraction - 0.6).abs() < f64::EPSILON);
        assert_eq!(manifest.mcp_url(), "http://127.0.0.1:9/mcp/operator");
        assert_eq!(manifest.token_url(), "http://127.0.0.1:9/oauth/token");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = manifest_json();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<AgentManifest>(value).is_err());
    }

    #[test]
    fn env_placeholders_expand_and_fail_closed() {
        // SAFETY: test-only env mutation, key is unique to this test.
        unsafe {
            std::env::set_var("TEST_MANIFEST_ACCOUNT", "acct-1");
        }
        assert_eq!(
            expand_env_placeholders("https://api/${TEST_MANIFEST_ACCOUNT}/ai/v1").expect("expands"),
            "https://api/acct-1/ai/v1"
        );
        assert!(expand_env_placeholders("${TEST_MANIFEST_MISSING_VAR}").is_err());
        assert!(expand_env_placeholders("${unterminated").is_err());
    }
}
