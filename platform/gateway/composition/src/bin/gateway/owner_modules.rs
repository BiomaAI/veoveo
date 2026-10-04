//! Concrete Agents and Workspace factory declarations start no workers.
use std::num::NonZeroU16;
use veoveo_agent_runtime::gateway::http::AgentManagementConfig;
use veoveo_mcp_gateway::http::GatewayModules;
use veoveo_modules::ModuleName;

pub(super) fn register_agents(
    modules: &mut GatewayModules,
    config: AgentManagementConfig,
) -> anyhow::Result<()> {
    modules.register(ModuleName::new("agents")?, move |context, scope| {
        Box::pin(async move { veoveo_agent_runtime::gateway::http::module(context, scope, config) })
    })
}

pub(super) fn register_workspace(
    modules: &mut GatewayModules,
    port: NonZeroU16,
    agents: AgentManagementConfig,
) -> anyhow::Result<()> {
    modules.register(ModuleName::new("workspace")?, move |context, scope| {
        Box::pin(async move { veoveo_workspace::gateway::module(context, scope, port, agents) })
    })
}

/// Owner configurations are pure values; registrations defer all workers.
pub(super) fn register(
    modules: &mut GatewayModules,
    required: &mut Vec<ModuleName>,
    port: NonZeroU16,
    context: &veoveo_mcp_gateway::http::GatewayHttpContext,
    templates: std::sync::Arc<veoveo_agent_runtime::gateway::ManagedTemplateCatalog>,
) -> anyhow::Result<()> {
    let models = veoveo_agent_runtime::gateway::http::models::from_env(&context.catalog.current())?;
    let capabilities =
        veoveo_agent_runtime::gateway::capabilities::NativeAgentCapabilityReader::new(
            port,
            &context.deployment,
            veoveo_agent_runtime::gateway::capabilities::client_config(),
        )?
        .shared();
    let config = AgentManagementConfig {
        templates,
        models: std::sync::Arc::new(models),
        capabilities,
        definition_limit: 1000,
        instance_limits: veoveo_agent_runtime::gateway::http::instance_limits()?,
    };
    register_agents(modules, config.clone())?;
    register_workspace(modules, port, config)?;
    required.extend([ModuleName::new("agents")?, ModuleName::new("workspace")?]);
    Ok(())
}
