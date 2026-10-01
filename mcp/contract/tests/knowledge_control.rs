use veoveo_knowledge_contract::{CollectionApproval, KnowledgeSubject};
use veoveo_mcp_contract::*;
#[path = "../../../testing/fixtures/knowledge_control.rs"]
mod fixture;

#[test]
fn indexing_requires_explicit_collection_approval_and_a_read_only_machine_profile() {
    let base = fixture::plane();
    base.validate().unwrap();
    let wire = serde_json::to_string(&base).unwrap();
    assert_eq!(
        serde_json::from_str::<GatewayControlPlane>(&wire).unwrap(),
        base
    );
    for case in 0..10 {
        let mut plane = base.clone();
        let client = plane
            .oauth_clients
            .iter_mut()
            .find(|client| client.knowledge_indexing.is_some())
            .unwrap();
        match case {
            0 => plane.servers[0].knowledge.clear(),
            1 => plane.servers[0].knowledge[0].mode = CollectionApproval::CatalogOnly,
            2 => plane.servers[0].knowledge[0].stewards.clear(),
            3 => {
                plane.servers[0].knowledge[0]
                    .data_labels
                    .insert("unknown".parse().unwrap());
            }
            4 => {
                let copy = plane.servers[0].knowledge[0].clone();
                plane.servers[0].knowledge.push(copy);
            }
            5 => plane.servers[0].knowledge[0].collection = "foreign.records".parse().unwrap(),
            6 => client
                .knowledge_indexing
                .as_mut()
                .unwrap()
                .collections
                .clear(),
            7 => client.invocation_mode = veoveo_types::InvocationMode::Direct,
            8 => plane.profiles.last_mut().unwrap().servers[0].tools = Exposure::All,
            9 => {
                plane.policies.last_mut().unwrap().rules[0]
                    .actions
                    .insert(GatewayAction::ToolsCall);
            }
            _ => unreachable!(),
        }
        assert!(plane.validate().is_err(), "case {case}");
    }
    assert!(KnowledgeSubject::new("Facility inspections").is_ok());
    for invalid in ["", " padded ", "line\nbreak"] {
        assert!(KnowledgeSubject::new(invalid).is_err());
    }
}
