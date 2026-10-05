//! The installation's pure catalog registration recipe, shared by all catalog consumers.
use veoveo_gateway_contract::{CatalogRegistry, CatalogRegistryBuilder, GatewayAction};
use veoveo_types::{ExtensionError, ExtensionName};

pub fn registry() -> Result<CatalogRegistry, ExtensionError> {
    let mut builder = CatalogRegistryBuilder::new(
        [
            "branding",
            "identity_providers",
            "authorization_servers",
            "servers",
            "profiles",
            "tenants",
            "work_contexts",
            "policies",
            "data_labels",
            "oauth_clients",
            "oidc_clients",
            "secrets",
            "metadata",
        ]
        .into_iter()
        .map(String::from),
        [
            "gateway",
            "server",
            "tool",
            "resource",
            "resource_template",
            "prompt",
            "task",
            "platform_task",
            "artifact",
            "usage",
        ]
        .into_iter()
        .map(ExtensionName::parse)
        .collect::<Result<Vec<_>, _>>()?,
    );
    builder.register_kernel::<GatewayAction>()?;
    veoveo_agent_runtime::contract::register_catalog(&mut builder)?;
    veoveo_computers_contract::register_catalog(&mut builder)?;
    veoveo_recording_contract::register_catalog(&mut builder)?;
    builder.build()
}

/// Supported Audit read codecs are independent of enabled workloads and schema lanes.
pub fn audit_target_registry() -> Result<
    std::sync::Arc<veoveo_audit_contract::AuditTargetRegistry>,
    veoveo_audit_contract::AuditTargetError,
> {
    let mut builder = veoveo_audit_contract::AuditTargetRegistry::builder();
    veoveo_computers_contract::register_audit_target(&mut builder)?;
    Ok(std::sync::Arc::new(builder.build()))
}
