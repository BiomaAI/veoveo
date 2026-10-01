//! Resource ownership and lexical profile selectors shared by reads and declarations.
use veoveo_mcp_contract::{
    Exposure, GatewayProfile, PolicyReasonCode, ResourceProjectionMode, ServerSlug,
};
use veoveo_types::{ResourceTemplateUri, ResourceUri};

use crate::{PolicyCatalogView, evaluation::resource_scheme};

pub(crate) enum ResourcePolicyReference<'a> {
    Concrete(&'a ResourceUri),
    Template(&'a ResourceTemplateUri),
}

impl ResourcePolicyReference<'_> {
    fn as_str(&self) -> &str {
        match self {
            Self::Concrete(uri) => uri.as_str(),
            Self::Template(uri) => uri.as_str(),
        }
    }
}

pub(crate) fn profile_allows_resource(
    catalog: &impl PolicyCatalogView,
    profile: &GatewayProfile,
    server: &ServerSlug,
    reference: ResourcePolicyReference<'_>,
) -> Result<(), PolicyReasonCode> {
    let manifest = catalog
        .server(server)
        .ok_or(PolicyReasonCode::UnknownServer)?;
    let scheme = resource_scheme(reference.as_str()).ok_or(PolicyReasonCode::UnknownResource)?;
    let owns_uri = manifest.uri_scheme == scheme
        || (manifest.resource_projection == ResourceProjectionMode::ServerOwned
            && scheme.as_str() == "ui"
            && reference
                .as_str()
                .starts_with(&format!("ui://{}/", manifest.slug.as_str())));
    if !owns_uri {
        return Err(PolicyReasonCode::UnknownResource);
    }
    let exposure = profile
        .servers
        .iter()
        .find(|exposure| &exposure.server == server)
        .ok_or(PolicyReasonCode::PolicyDeny)?;
    let allowed = match &exposure.resources {
        Exposure::All => true,
        Exposure::None => false,
        Exposure::Listed(selectors) => selectors.iter().any(|selector| match &reference {
            ResourcePolicyReference::Concrete(uri) => selector.matches_uri(uri),
            ResourcePolicyReference::Template(uri) => selector.matches_template(uri),
        }),
    };
    allowed.then_some(()).ok_or(PolicyReasonCode::PolicyDeny)
}
