//! Owner-declared actions admitted by the gateway policy registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum AgentAction {
    #[vocabulary(rename = "agents_read")]
    AgentsRead,
    #[vocabulary(rename = "agents_message")]
    AgentsMessage,
    #[vocabulary(rename = "agents_input_request_answer")]
    AgentsInputRequestAnswer,
    #[vocabulary(rename = "agent_definitions_read")]
    AgentDefinitionsRead,
    #[vocabulary(rename = "agent_definitions_read_content")]
    AgentDefinitionsReadContent,
    #[vocabulary(rename = "agent_definitions_create")]
    AgentDefinitionsCreate,
    #[vocabulary(rename = "agent_definitions_edit")]
    AgentDefinitionsEdit,
    #[vocabulary(rename = "agent_definitions_publish")]
    AgentDefinitionsPublish,
    #[vocabulary(rename = "agent_definitions_use")]
    AgentDefinitionsUse,
    #[vocabulary(rename = "agent_definitions_control")]
    AgentDefinitionsControl,
    #[vocabulary(rename = "agent_definitions_archive")]
    AgentDefinitionsArchive,
    #[vocabulary(rename = "agent_definitions_transfer")]
    AgentDefinitionsTransfer,
    #[vocabulary(rename = "agent_instances_deploy")]
    AgentInstancesDeploy,
    #[vocabulary(rename = "agent_instances_control")]
    AgentInstancesControl,
}

pub fn register_catalog(
    builder: &mut veoveo_gateway_contract::CatalogRegistryBuilder,
) -> Result<(), veoveo_types::ExtensionError> {
    use std::collections::{BTreeMap, BTreeSet};
    use veoveo_gateway_contract::{ActionDescriptor, RuleSelector, SelectorRequirement};
    use veoveo_types::ExtensionName;
    let selectors = BTreeMap::from([
        (RuleSelector::Profiles, SelectorRequirement::Optional),
        (
            RuleSelector::ProtectedResources,
            SelectorRequirement::Forbidden,
        ),
        (RuleSelector::Servers, SelectorRequirement::Forbidden),
        (RuleSelector::Tools, SelectorRequirement::Forbidden),
        (
            RuleSelector::ResourceSchemes,
            SelectorRequirement::Forbidden,
        ),
        (RuleSelector::Prompts, SelectorRequirement::Forbidden),
    ]);
    builder.register_actions(
        AgentAction::ALL
            .iter()
            .copied()
            .map(|action| {
                (
                    action,
                    ActionDescriptor {
                        access: match action {
                            AgentAction::AgentsRead
                            | AgentAction::AgentDefinitionsRead
                            | AgentAction::AgentDefinitionsReadContent => {
                                veoveo_gateway_contract::ActionAccess::Read
                            }
                            _ => veoveo_gateway_contract::ActionAccess::Write,
                        },
                        target_kinds: BTreeSet::from([
                            ExtensionName::new("gateway").expect("fixed target kind")
                        ]),
                        selectors: selectors.clone(),
                        server: None,
                    },
                )
            })
            .collect(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_wire_spellings_remain_closed() {
        let expected = [
            "agents_read",
            "agents_message",
            "agents_input_request_answer",
            "agent_definitions_read",
            "agent_definitions_read_content",
            "agent_definitions_create",
            "agent_definitions_edit",
            "agent_definitions_publish",
            "agent_definitions_use",
            "agent_definitions_control",
            "agent_definitions_archive",
            "agent_definitions_transfer",
            "agent_instances_deploy",
            "agent_instances_control",
        ];
        assert_eq!(
            AgentAction::ALL
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for wire in expected {
            let action = wire.parse::<AgentAction>().unwrap();
            assert_eq!(serde_json::to_value(action).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<AgentAction>(serde_json::json!(wire)).unwrap(),
                action
            );
            assert!(format!("{wire}_alias").parse::<AgentAction>().is_err());
        }
    }
}
