//! Typed agent manifest: everything that makes one agent an agent.
//!
//! A manifest is data. The kernel executes manifests; agent types (the Pilot,
//! future agents) are manifest + preamble + migrations, never kernel code.
//! Loading is fail-closed: unknown fields, missing environment variables, and
//! out-of-range knobs are hard errors before the agent boots.

use std::{collections::BTreeSet, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use url::Url;

const MAX_RESOURCE_SUBSCRIPTIONS: usize = 128;
const MAX_RESOURCE_URI_BYTES: usize = 2_048;

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
    /// Stable MCP resources whose updates create durable wakes. A replacement
    /// gateway session restores the complete set before becoming active.
    #[serde(default)]
    pub resource_subscriptions: Vec<ResourceSubscription>,
    /// Directory of `NNNN_*.sql` domain migrations applied at boot, relative
    /// to the manifest file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations_dir: Option<std::path::PathBuf>,
    /// System preamble for every episode.
    pub preamble: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSubscription {
    /// Absolute resource URI. `${VAR}` placeholders are expanded from the
    /// environment while loading the manifest.
    pub uri: String,
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
    /// Domain tables `memory_write` may mutate; the `agent_memory` schema is never
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
    /// Installation-local tenant key used for all platform records.
    pub tenant: String,
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
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayAccess {
    /// Canonical public gateway origin used for HTTP authority and OAuth
    /// identity. `${VAR}` placeholders are expanded at load time.
    pub url: String,
    /// Physical HTTP(S) origin used to reach the gateway from this network.
    /// `${VAR}` placeholders are expanded at load time.
    pub transport_url: String,
    /// Gateway profile mounted under `/mcp/{profile}`.
    pub profile: String,
    /// OAuth client id for the client-credentials grant.
    pub client_id: String,
    /// Work Context used for this automated agent invocation.
    pub work_context: String,
    /// Audience for the private-key JWT client assertion (the public token
    /// endpoint URL, which may differ from the connect URL behind an edge).
    /// `${VAR}` placeholders are expanded at load time.
    pub audience: String,
    /// Protected resource the token is minted for. `${VAR}` placeholders are
    /// expanded at load time.
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

impl AgentManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading agent manifest {}", path.display()))?;
        let mut manifest: AgentManifest = serde_json::from_str(&raw)
            .with_context(|| format!("parsing agent manifest {}", path.display()))?;
        manifest.model.base_url = expand_env_placeholders(&manifest.model.base_url)?;
        manifest.gateway.url = expand_env_placeholders(&manifest.gateway.url)?;
        manifest.gateway.transport_url = expand_env_placeholders(&manifest.gateway.transport_url)?;
        manifest.gateway.audience = expand_env_placeholders(&manifest.gateway.audience)?;
        manifest.gateway.resource = expand_env_placeholders(&manifest.gateway.resource)?;
        for subscription in &mut manifest.resource_subscriptions {
            subscription.uri = expand_env_placeholders(&subscription.uri)?;
        }
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
        if self.agent.tenant.trim().is_empty() || self.agent.tenant.chars().any(char::is_control) {
            bail!("agent.tenant must be non-empty and contain no control characters");
        }
        if let Some(temperature) = self.model.temperature
            && !(0.0..=2.0).contains(&temperature)
        {
            bail!("model.temperature must be in [0, 2], got {temperature}");
        }
        if let Some(top_p) = self.model.top_p
            && !(top_p > 0.0 && top_p <= 1.0)
        {
            bail!("model.top_p must be in (0, 1], got {top_p}");
        }
        if self.model.top_k == Some(0) {
            bail!("model.top_k must be greater than zero");
        }
        for (field, value) in [
            ("model.base_url", &self.model.base_url),
            ("model.api_key_env", &self.model.api_key_env),
            ("model.model", &self.model.model),
            ("gateway.url", &self.gateway.url),
            ("gateway.transport_url", &self.gateway.transport_url),
            ("gateway.profile", &self.gateway.profile),
            ("gateway.client_id", &self.gateway.client_id),
            ("gateway.work_context", &self.gateway.work_context),
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
        let gateway_url = validate_http_origin("gateway.url", &self.gateway.url)?;
        validate_http_origin("gateway.transport_url", &self.gateway.transport_url)?;
        let token_audience = gateway_url
            .join("oauth/token")
            .context("building canonical gateway token audience")?;
        if self.gateway.audience != token_audience.as_str() {
            bail!("gateway.audience must be the canonical gateway token URL `{token_audience}`");
        }
        let resource = url::Url::parse(&self.gateway.resource)
            .context("gateway.resource must be an absolute URL")?;
        if resource.origin() != gateway_url.origin() {
            bail!("gateway.resource must use the canonical gateway origin");
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
        if self.resource_subscriptions.len() > MAX_RESOURCE_SUBSCRIPTIONS {
            bail!(
                "resource_subscriptions must contain at most {MAX_RESOURCE_SUBSCRIPTIONS} entries"
            );
        }
        let mut resource_uris = BTreeSet::new();
        for (index, subscription) in self.resource_subscriptions.iter().enumerate() {
            let uri = subscription.uri.trim();
            if uri.is_empty()
                || uri.len() > MAX_RESOURCE_URI_BYTES
                || uri.chars().any(char::is_control)
            {
                bail!(
                    "resource_subscriptions[{index}].uri must be non-empty, at most \
                     {MAX_RESOURCE_URI_BYTES} bytes, and contain no control characters"
                );
            }
            let parsed = Url::parse(uri).with_context(|| {
                format!("resource_subscriptions[{index}].uri must be an absolute URI")
            })?;
            if !parsed.username().is_empty() || parsed.password().is_some() {
                bail!("resource_subscriptions[{index}].uri must not contain credentials");
            }
            if parsed.fragment().is_some() {
                bail!("resource_subscriptions[{index}].uri must not contain a fragment");
            }
            if !resource_uris.insert(uri) {
                bail!("resource_subscriptions contains duplicate URI `{uri}`");
            }
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
            crate::memory::ensure_single_select(&section.sql)
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
            self.gateway.transport_url.trim_end_matches('/'),
            self.gateway.profile
        )
    }

    pub fn token_url(&self) -> String {
        format!(
            "{}/oauth/token",
            self.gateway.transport_url.trim_end_matches('/')
        )
    }

    pub fn gateway_authority(&self) -> Result<String> {
        let url = validate_http_origin("gateway.url", &self.gateway.url)?;
        let host = match url.host().context("gateway.url has no host")? {
            url::Host::Ipv6(address) => format!("[{address}]"),
            host => host.to_string(),
        };
        Ok(match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        })
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.episode.request_timeout_s)
    }

    pub fn task_deadline(&self) -> Duration {
        Duration::from_secs(self.episode.task_deadline_s)
    }
}

fn validate_http_origin(field: &str, value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).with_context(|| format!("{field} must be an absolute URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("{field} must be an HTTP(S) origin without credentials, path, query, or fragment");
    }
    Ok(url)
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
            "agent": { "tenant": "test", "id": "test-agent", "display_name": "Test Agent" },
            "model": {
                "base_url": "http://127.0.0.1:9/v1",
                "api_key_env": "TEST_MANIFEST_API_KEY",
                "model": "test/model"
            },
            "gateway": {
                "url": "https://veoveo.example",
                "transport_url": "http://127.0.0.1:9",
                "profile": "operator",
                "client_id": "operator-service",
                "work_context": "operations",
                "audience": "https://veoveo.example/oauth/token",
                "resource": "https://veoveo.example/mcp/operator",
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
        assert!(manifest.resource_subscriptions.is_empty());
        assert_eq!(
            manifest.gateway_authority().expect("authority"),
            "veoveo.example"
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = manifest_json();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<AgentManifest>(value).is_err());
    }

    #[test]
    fn model_sampling_parameters_are_bounded() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_MANIFEST_API_KEY", "k");
            std::env::set_var("TEST_MANIFEST_PRIVATE_KEY", "p");
        }
        for (field, invalid) in [
            ("temperature", serde_json::json!(2.1)),
            ("top_p", serde_json::json!(0.0)),
            ("top_k", serde_json::json!(0)),
        ] {
            let mut value = manifest_json();
            value["model"][field] = invalid;
            let manifest: AgentManifest = serde_json::from_value(value).expect("parses");
            assert!(
                manifest.validate().is_err(),
                "accepted invalid model.{field}"
            );
        }
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

    #[test]
    fn gateway_identity_and_transport_fail_closed() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_MANIFEST_API_KEY", "k");
            std::env::set_var("TEST_MANIFEST_PRIVATE_KEY", "p");
        }
        for (field, invalid) in [
            ("url", "https://veoveo.example/path"),
            ("transport_url", "http://gateway.internal:8788/path"),
        ] {
            let mut value = manifest_json();
            value["gateway"][field] = serde_json::json!(invalid);
            let manifest: AgentManifest = serde_json::from_value(value).expect("parses");
            assert!(
                manifest.validate().is_err(),
                "accepted invalid gateway {field}"
            );
        }

        let mut value = manifest_json();
        value["gateway"]["audience"] = serde_json::json!("http://127.0.0.1:9/oauth/token");
        let manifest: AgentManifest = serde_json::from_value(value).expect("parses");
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn gateway_identity_coordinates_expand_together_before_validation() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_GATEWAY_IDENTITY_API_KEY", "k");
            std::env::set_var("TEST_GATEWAY_IDENTITY_PRIVATE_KEY", "p");
            std::env::set_var("TEST_GATEWAY_IDENTITY_ORIGIN", "https://veoveo.example");
            std::env::set_var("TEST_GATEWAY_IDENTITY_TRANSPORT", "http://127.0.0.1:9");
        }
        let mut value = manifest_json();
        value["model"]["api_key_env"] = serde_json::json!("TEST_GATEWAY_IDENTITY_API_KEY");
        value["gateway"]["private_key_env"] =
            serde_json::json!("TEST_GATEWAY_IDENTITY_PRIVATE_KEY");
        value["gateway"]["url"] = serde_json::json!("${TEST_GATEWAY_IDENTITY_ORIGIN}");
        value["gateway"]["transport_url"] = serde_json::json!("${TEST_GATEWAY_IDENTITY_TRANSPORT}");
        value["gateway"]["audience"] =
            serde_json::json!("${TEST_GATEWAY_IDENTITY_ORIGIN}/oauth/token");
        value["gateway"]["resource"] =
            serde_json::json!("${TEST_GATEWAY_IDENTITY_ORIGIN}/mcp/operator");
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("manifest.json");
        std::fs::write(&path, serde_json::to_vec(&value).expect("manifest json"))
            .expect("write manifest");

        let manifest = AgentManifest::load(&path).expect("manifest loads");
        assert_eq!(
            manifest.gateway.audience,
            "https://veoveo.example/oauth/token"
        );
        assert_eq!(
            manifest.gateway.resource,
            "https://veoveo.example/mcp/operator"
        );
    }

    #[test]
    fn declarative_resource_subscriptions_expand_and_validate() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_RESOURCE_SUBSCRIPTION_API_KEY", "k");
            std::env::set_var("TEST_RESOURCE_SUBSCRIPTION_PRIVATE_KEY", "p");
            std::env::set_var("TEST_RESOURCE_SESSION", "simulation-alpha");
        }
        let mut value = manifest_json();
        value["model"]["api_key_env"] = serde_json::json!("TEST_RESOURCE_SUBSCRIPTION_API_KEY");
        value["gateway"]["private_key_env"] =
            serde_json::json!("TEST_RESOURCE_SUBSCRIPTION_PRIVATE_KEY");
        value["resource_subscriptions"] = serde_json::json!([
            {"uri": "telemetry://session/${TEST_RESOURCE_SESSION}/events/latest"},
            {"uri": "telemetry://session/${TEST_RESOURCE_SESSION}/plans"}
        ]);
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("manifest.json");
        std::fs::write(&path, serde_json::to_vec(&value).expect("manifest json"))
            .expect("write manifest");

        let manifest = AgentManifest::load(&path).expect("manifest loads");
        assert_eq!(
            manifest.resource_subscriptions,
            vec![
                ResourceSubscription {
                    uri: "telemetry://session/simulation-alpha/events/latest".to_owned(),
                },
                ResourceSubscription {
                    uri: "telemetry://session/simulation-alpha/plans".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn invalid_resource_subscriptions_fail_closed() {
        // SAFETY: test-only env mutation, keys are unique to this test.
        unsafe {
            std::env::set_var("TEST_MANIFEST_API_KEY", "k");
            std::env::set_var("TEST_MANIFEST_PRIVATE_KEY", "p");
        }
        for uris in [
            vec!["not-an-absolute-uri"],
            vec!["telemetry://user:secret@session/current/plans"],
            vec!["telemetry://session/current/plans#fragment"],
            vec![
                "telemetry://session/current/plans",
                "telemetry://session/current/plans",
            ],
        ] {
            let mut value = manifest_json();
            value["resource_subscriptions"] = serde_json::Value::Array(
                uris.into_iter()
                    .map(|uri| serde_json::json!({"uri": uri}))
                    .collect(),
            );
            let manifest: AgentManifest = serde_json::from_value(value).expect("parses");
            assert!(manifest.validate().is_err());
        }
    }
}
