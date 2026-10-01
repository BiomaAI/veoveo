//! Partial evaluation of resource-read policy for database candidate admission.
use crate::{
    PolicyCatalogView,
    evaluation::{evaluate_rules, filter_matches, has_required_scopes, validate_principal},
};
use veoveo_mcp_contract::{Exposure, GatewayAction, PolicyEffect, PolicyReasonCode, Principal};
use veoveo_types::{GatewayProfileId, ResourceSelection, ResourceSelector, ServerSlug};

/// Admit reads in the registered server's own URI scheme. Lexical profile selectors
/// remain part of the returned predicate and must run before ranking or pagination.
/// This uses the same rules as `decide`; resource rules filter by server and scheme,
/// never by a member URI. Projected App resources are outside this API's scope.
/// Callers must prove current authenticated authority and control-plane freshness.
pub fn admit_resource_reads(
    catalog: &impl PolicyCatalogView,
    principal: &Principal,
    profile_id: &GatewayProfileId,
    server: &ServerSlug,
) -> Result<ResourceSelection, PolicyReasonCode> {
    let profile = catalog
        .profile(profile_id)
        .ok_or(PolicyReasonCode::UnknownProfile)?;
    let policy = catalog
        .policy(&profile.policy_version)
        .ok_or(PolicyReasonCode::PolicyDeny)?;
    validate_principal(catalog, principal)?;
    let manifest = catalog
        .server(server)
        .ok_or(PolicyReasonCode::UnknownServer)?;
    let exposure = profile
        .servers
        .iter()
        .find(|entry| &entry.server == server)
        .ok_or(PolicyReasonCode::PolicyDeny)?;
    let selectors = match &exposure.resources {
        Exposure::All => vec![ResourceSelector::Scheme {
            scheme: manifest.uri_scheme.clone(),
        }],
        Exposure::Listed(selectors) if !selectors.is_empty() => selectors.clone(),
        Exposure::None | Exposure::Listed(_) => return Err(PolicyReasonCode::PolicyDeny),
    };
    if !has_required_scopes(&principal.scopes, &profile.required_scopes) {
        return Err(PolicyReasonCode::MissingScope);
    }
    let outcome = evaluate_rules(
        profile,
        policy,
        principal,
        GatewayAction::ResourcesRead,
        |rule| {
            filter_matches(&rule.servers, server)
                && filter_matches(&rule.resource_schemes, &manifest.uri_scheme)
        },
    );
    if outcome.effect != PolicyEffect::Allow {
        return Err(outcome.reason);
    }
    Ok(ResourceSelection {
        scheme: manifest.uri_scheme.clone(),
        selectors,
    })
}
