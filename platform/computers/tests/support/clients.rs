//! Two independently authorized browser profiles for the same retained owner.
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayProfileId, OAuthClientId, ProtectedResourceId,
};

pub fn control(mut control: GatewayControlPlane) -> GatewayControlPlane {
    let profile_id = GatewayProfileId::new("workspace").unwrap();
    let resource = ProtectedResourceId::new("https://computers.test/mcp/workspace").unwrap();
    let mut profile = control.profiles[0].clone();
    profile.id = profile_id.clone();
    profile.protected_resource = resource.clone();
    profile.auth_modes = [veoveo_mcp_contract::AuthMode::OidcAuthorizationCodePkce].into();
    control.profiles.push(profile);
    for client in &mut control.oidc_clients {
        client.allowed_resources.insert(resource.clone());
    }
    let client_id = OAuthClientId::new("workspace").unwrap();
    let mut client = control
        .oauth_clients
        .iter()
        .find(|client| client.id.as_str() == "console")
        .unwrap()
        .clone();
    client.id = client_id.clone();
    client.allowed_resources = [resource].into();
    control.oauth_clients.push(client);
    for context in &mut control.work_contexts {
        for membership in &mut context.memberships {
            membership.oauth_clients.insert(client_id.clone());
        }
    }
    for policy in &mut control.policies {
        for rule in &mut policy.rules {
            if rule
                .profiles
                .iter()
                .any(|profile| profile.as_str() == "operator")
            {
                rule.profiles.insert(profile_id.clone());
            }
        }
    }
    control.validate().unwrap();
    control
}

pub async fn workspace(db: &super::TestDb, name: &str) -> veoveo_computers::ComputerActor {
    veoveo_computers::ComputerActor::from_verified(
        &super::browser::identity_for_profile(
            db,
            name,
            "workspace",
            "workspace",
            "https://computers.test/mcp/workspace",
        )
        .await,
    )
    .unwrap()
}
