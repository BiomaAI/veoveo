//! Gateway transport, owned HTTP route and health vocabularies.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum GatewayServerHealthState {
    #[vocabulary(rename = "healthy")]
    Healthy,
    #[vocabulary(rename = "degraded")]
    Degraded,
    #[vocabulary(rename = "offline")]
    Offline,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum UpstreamTransport {
    #[vocabulary(rename = "streamable_http")]
    StreamableHttp,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum OwnedRoutePurpose {
    #[vocabulary(rename = "webhook")]
    Webhook,
    #[vocabulary(rename = "artifact_bytes")]
    ArtifactBytes,
    #[vocabulary(rename = "provider_fetchable_files")]
    ProviderFetchableFiles,
    #[vocabulary(rename = "health")]
    Health,
}

/// Registered modules describe route binding independently of backend probe health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ModuleBindingState {
    #[vocabulary(rename = "bound")]
    Bound,
    #[vocabulary(rename = "unbound")]
    Unbound,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleBindingSnapshot {
    pub module: veoveo_modules::ModuleName,
    pub state: ModuleBindingState,
    pub required: bool,
}

/// The authenticated administrator's registered-server health snapshot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHealthReport {
    pub servers: Vec<ServerHealthEntry>,
    pub module_bindings: Vec<ModuleBindingSnapshot>,
}

/// State and timestamp are null until the server's first probe after startup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHealthEntry {
    pub server: veoveo_types::ServerSlug,
    pub state: Option<GatewayServerHealthState>,
    pub checked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_snapshot_preserves_null_probe_and_independent_module_binding() {
        let wire = serde_json::json!({
            "servers": [{"server":"frames", "state":null, "checkedAt":null}],
            "moduleBindings": [{"module":"extension", "state":"unbound", "required":false}]
        });
        let report: ServerHealthReport = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(report.servers[0].state, None);
        assert_eq!(report.module_bindings[0].state, ModuleBindingState::Unbound);
        assert_eq!(serde_json::to_value(report).unwrap(), wire);
    }

    #[test]
    fn health_snapshot_rejects_unknown_fields_and_states() {
        for wire in [
            serde_json::json!({"servers":[], "moduleBindings":[], "extra":true}),
            serde_json::json!({"servers":[{"server":"frames", "state":"unknown", "checkedAt":null}], "moduleBindings":[]}),
            serde_json::json!({"servers":[], "moduleBindings":[{"module":"extension", "state":"bound", "required":false, "extra":true}]}),
        ] {
            assert!(serde_json::from_value::<ServerHealthReport>(wire).is_err());
        }
    }
}
