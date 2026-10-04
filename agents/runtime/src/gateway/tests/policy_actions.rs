use super::catalog;
use crate::contract::AgentAction;
use std::collections::BTreeSet;
use veoveo_mcp_contract::{GatewayControlPlane, GatewayControlPlaneError, ServerSlug};
use veoveo_types::ActionName;

fn action_name(action: AgentAction) -> ActionName {
    crate::catalog_fixture::registry()
        .action_key::<AgentAction>()
        .expect("installation registers Agents actions")
        .action(action)
        .expect("Agents vocabulary value is admitted")
        .name()
        .clone()
}

fn control_plane_with_agent_actions(actions: BTreeSet<ActionName>) -> GatewayControlPlane {
    let mut config = catalog().control_plane().clone();
    let rule = &mut config.policies[0].rules[0];
    rule.actions = actions;
    rule.servers.clear();
    rule.tools.clear();
    rule.resource_schemes.clear();
    rule.prompts.clear();
    rule.protected_resources.clear();
    config
}

#[test]
fn agent_control_actions_are_gateway_scoped() {
    let actions = BTreeSet::from([
        action_name(AgentAction::AgentsRead),
        action_name(AgentAction::AgentsMessage),
        action_name(AgentAction::AgentsInputRequestAnswer),
    ]);
    let mut config = control_plane_with_agent_actions(actions.clone());
    config
        .validate(&crate::catalog_fixture::registry())
        .expect("agent control is an explicit gateway policy surface");

    config.policies[0].rules[0].servers = BTreeSet::from([ServerSlug::parse("media").unwrap()]);
    let error = config
        .validate(&crate::catalog_fixture::registry())
        .expect_err("agent control cannot inherit an MCP server filter");
    assert_eq!(
        error,
        GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
            policy: config.policies[0].version.clone(),
            rule: config.policies[0].rules[0].id.clone(),
            action: actions.first().unwrap().clone(),
        }
    );
}

#[test]
fn agent_management_actions_are_explicit_gateway_permissions() {
    for action in [
        AgentAction::AgentDefinitionsRead,
        AgentAction::AgentDefinitionsReadContent,
        AgentAction::AgentDefinitionsCreate,
        AgentAction::AgentDefinitionsEdit,
        AgentAction::AgentDefinitionsPublish,
        AgentAction::AgentDefinitionsUse,
        AgentAction::AgentDefinitionsControl,
        AgentAction::AgentDefinitionsArchive,
        AgentAction::AgentDefinitionsTransfer,
        AgentAction::AgentInstancesDeploy,
        AgentAction::AgentInstancesControl,
    ] {
        let name = action_name(action);
        let mut config = control_plane_with_agent_actions(BTreeSet::from([name.clone()]));
        config
            .validate(&crate::catalog_fixture::registry())
            .expect("agent management is an explicit gateway policy surface");
        config.policies[0].rules[0].servers = BTreeSet::from([ServerSlug::parse("media").unwrap()]);
        let error = config
            .validate(&crate::catalog_fixture::registry())
            .expect_err("agent management cannot inherit server-scoped authority");
        assert_eq!(
            error,
            GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
                policy: config.policies[0].version.clone(),
                rule: config.policies[0].rules[0].id.clone(),
                action: name,
            },
            "{action:?} cannot inherit server-scoped authority"
        );
    }
}
