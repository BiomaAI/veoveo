//! Owner-declared actions admitted by the gateway policy registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum ComputerAction {
    #[vocabulary(rename = "computer_attach")]
    Attach,
}

pub fn register_catalog(
    builder: &mut veoveo_gateway_contract::CatalogRegistryBuilder,
) -> Result<(), veoveo_types::ExtensionError> {
    use std::collections::{BTreeMap, BTreeSet};
    use veoveo_gateway_contract::{
        ActionDescriptor, RuleSelector, SelectorRequirement, ServerRequirement,
    };
    use veoveo_types::ExtensionName;
    let selectors = [
        RuleSelector::Profiles,
        RuleSelector::ProtectedResources,
        RuleSelector::Servers,
        RuleSelector::Tools,
        RuleSelector::ResourceSchemes,
        RuleSelector::Prompts,
    ]
    .into_iter()
    .map(|selector| (selector, SelectorRequirement::Optional))
    .collect::<BTreeMap<_, _>>();
    builder.register_actions(vec![(
        ComputerAction::Attach,
        ActionDescriptor {
            access: veoveo_gateway_contract::ActionAccess::Write,
            target_kinds: BTreeSet::from([ExtensionName::parse("resource")?]),
            selectors,
            server: Some(ServerRequirement {
                slug: Some("computers".parse().expect("fixed server")),
                resources: true,
            }),
        },
    )])?;
    builder.register_section::<crate::ComputerWorkerAuthorizationSection>(ExtensionName::parse(
        crate::COMPUTER_WORKER_AUTHORIZATION_SECTION,
    )?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_wire_spellings_remain_closed() {
        let expected = ["computer_attach"];
        assert_eq!(
            ComputerAction::ALL
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for wire in expected {
            let action = wire.parse::<ComputerAction>().unwrap();
            assert_eq!(serde_json::to_value(action).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<ComputerAction>(serde_json::json!(wire)).unwrap(),
                action
            );
            assert!(format!("{wire}_alias").parse::<ComputerAction>().is_err());
        }
    }
}
