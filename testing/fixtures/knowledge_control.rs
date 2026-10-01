//! Isolated typed indexing registration; contains no installation credentials.
use std::collections::BTreeSet;
use veoveo_knowledge_contract::{
    CollectionApproval, KnowledgeCollectionApproval, KnowledgeIndexingRegistration,
};
use veoveo_mcp_contract::*;
use veoveo_types::InvocationMode;

pub fn plane() -> GatewayControlPlane {
    let mut plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../configs/gateway.smoke.json")).unwrap();
    plane.servers[0].knowledge = vec![KnowledgeCollectionApproval {
        collection: "media.records".parse().unwrap(),
        mode: CollectionApproval::Index,
        stewards: ["stewards".parse().unwrap()].into(),
        authoritative_for: Default::default(),
        data_labels: Default::default(),
    }];
    let mut policy = plane.policies[0].clone();
    policy.version = "indexing-v1".parse().unwrap();
    policy.rules.truncate(1);
    policy.rules[0].profiles = ["knowledge-indexing".parse().unwrap()].into();
    policy.rules[0].actions = [
        GatewayAction::ResourcesList,
        GatewayAction::ResourcesTemplatesList,
        GatewayAction::ResourcesRead,
        GatewayAction::SubscriptionsListen,
    ]
    .into();
    policy.rules[0].tools.clear();
    policy.rules[0].prompts.clear();
    let mut profile = plane.profiles[0].clone();
    profile.id = "knowledge-indexing".parse().unwrap();
    profile.protected_resource =
        ProtectedResourceId::new("https://veoveo.example/mcp/knowledge-indexing").unwrap();
    profile.policy_version = policy.version.clone();
    profile.artifact_upload = None;
    profile.auth_modes = [AuthMode::OAuthClientCredentials].into();
    for server in &mut profile.servers {
        server.tools = Exposure::None;
        server.prompts = Exposure::None;
        server.completions = CompletionExposure::Disabled;
        server.tasks = TaskExposure::Disabled;
    }
    let client = plane
        .oauth_clients
        .iter_mut()
        .find(|client| client.id.as_str() == "operator-service")
        .unwrap();
    client.invocation_mode = InvocationMode::Automated;
    client.allowed_resources = [profile.protected_resource.clone()].into();
    client.knowledge_indexing = Some(KnowledgeIndexingRegistration {
        collections: BTreeSet::from([plane.servers[0].knowledge[0].collection.clone()]),
    });
    plane.profiles.push(profile);
    plane.policies.push(policy);
    plane
}
