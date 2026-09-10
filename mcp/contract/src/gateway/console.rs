//! Authenticated Console bootstrap; installation inventory has separate authorization.
use super::{GatewayProfileId, PrincipalId, TenantId, WorkContextId};
use crate::{InvocationMode, WorkContextMembershipLevel};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleBootstrap {
    pub profile: GatewayProfileId,
    pub installation: ConsoleInstallation,
    pub session: ConsoleSession,
    pub can_read_installation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleInstallation {
    pub name: String,
    pub product_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    pub version: String,
    pub offline_mode: bool,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleSession {
    pub display_name: String,
    pub principal_id: PrincipalId,
    pub actor_id: PrincipalId,
    pub tenant_id: TenantId,
    pub tenant_name: String,
    pub work_context: WorkContextId,
    pub work_context_title: String,
    pub membership: WorkContextMembershipLevel,
    pub invocation_mode: InvocationMode,
    pub available_tenants: Vec<ConsoleTenant>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleTenant {
    pub id: TenantId,
    pub name: String,
}

pub fn console_bootstrap_schema() -> schemars::Schema {
    schemars::schema_for!(ConsoleBootstrap)
}
