use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;
pub use veoveo_mcp_contract::HOSTED_MCP_CONTRACT_REVISION;

/// Hosted-server conformance profile schema.
pub const HOSTED_SERVER_PROFILE_SCHEMA: &str = "veoveo.io/mcp-conformance-profile/v1";

/// Supported hosted-server conformance profile schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HostedServerProfileSchema {
    /// Hosted-server conformance profile version 1.
    #[serde(rename = "veoveo.io/mcp-conformance-profile/v1")]
    V1,
}

/// Whether a protocol surface must, may, or must not be advertised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceExpectation {
    /// The server must advertise and answer the surface.
    Required,
    /// The surface is checked when advertised.
    Optional,
    /// The server must not advertise the surface.
    Forbidden,
}

/// HTTP boundary checks applicable to a hosted server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpBoundaryProfile {
    /// The unauthenticated MCP endpoint must return HTTP 401.
    #[serde(default)]
    pub require_authentication_rejection: bool,
    /// Host header that the server must reject with HTTP 421.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected_host: Option<String>,
    /// Health endpoint that must return success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_url: Option<String>,
    /// Readiness endpoint that must return success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_url: Option<String>,
}

/// Capability-driven checks for the generic MCP protocol surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceProfile {
    pub tools: SurfaceExpectation,
    pub resources: SurfaceExpectation,
    pub resource_templates: SurfaceExpectation,
    pub prompts: SurfaceExpectation,
    pub completions: SurfaceExpectation,
    pub tasks: SurfaceExpectation,
    pub subscriptions: SurfaceExpectation,
    /// Tool names that must be listed.
    #[serde(default)]
    pub required_tools: BTreeSet<String>,
    /// Resource URIs that must be listed.
    #[serde(default)]
    pub required_resources: BTreeSet<String>,
    /// Resource URI templates that must be listed.
    #[serde(default)]
    pub required_resource_templates: BTreeSet<String>,
    /// Prompt names that must be listed.
    #[serde(default)]
    pub required_prompts: BTreeSet<String>,
}

/// Domain-neutral profile for one running hosted MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostedServerConformanceProfile {
    pub schema_version: HostedServerProfileSchema,
    /// Installation-local identity for this conformance selection.
    pub profile_id: String,
    /// Exact hosted-server contract revision selected by the compatibility manifest.
    pub contract_revision: String,
    /// Streamable HTTP MCP endpoint.
    pub endpoint: String,
    /// Expected server slug.
    pub server_slug: String,
    /// URI schemes this server is permitted to own.
    pub owned_resource_schemes: BTreeSet<String>,
    pub http: HttpBoundaryProfile,
    pub surfaces: SurfaceProfile,
}

impl HostedServerConformanceProfile {
    /// Validates controlled profile fields before any network request.
    pub fn validate(&self) -> Result<()> {
        require_token("profileId", &self.profile_id)?;
        require_token("contractRevision", &self.contract_revision)?;
        require_token("serverSlug", &self.server_slug)?;
        validate_url("endpoint", &self.endpoint)?;
        if let Some(url) = &self.http.health_url {
            validate_url("healthUrl", url)?;
        }
        if let Some(url) = &self.http.readiness_url {
            validate_url("readinessUrl", url)?;
        }
        if let Some(host) = &self.http.rejected_host {
            ensure!(
                !host.trim().is_empty()
                    && !host.contains('/')
                    && !host.chars().any(char::is_whitespace),
                "rejectedHost must be one HTTP Host authority"
            );
        }
        ensure!(
            !self.owned_resource_schemes.is_empty(),
            "ownedResourceSchemes cannot be empty"
        );
        for scheme in &self.owned_resource_schemes {
            ensure!(
                valid_scheme(scheme),
                "owned resource scheme {scheme:?} is invalid"
            );
        }
        for (field, values) in [
            ("requiredTools", &self.surfaces.required_tools),
            ("requiredResources", &self.surfaces.required_resources),
            (
                "requiredResourceTemplates",
                &self.surfaces.required_resource_templates,
            ),
            ("requiredPrompts", &self.surfaces.required_prompts),
        ] {
            for value in values {
                ensure!(!value.trim().is_empty(), "{field} contains an empty value");
            }
        }
        if self.surfaces.tools == SurfaceExpectation::Forbidden
            && !self.surfaces.required_tools.is_empty()
        {
            bail!("requiredTools cannot be set when tools are forbidden");
        }
        if self.surfaces.resources == SurfaceExpectation::Forbidden
            && !self.surfaces.required_resources.is_empty()
        {
            bail!("requiredResources cannot be set when resources are forbidden");
        }
        if self.surfaces.resource_templates == SurfaceExpectation::Forbidden
            && !self.surfaces.required_resource_templates.is_empty()
        {
            bail!("requiredResourceTemplates cannot be set when templates are forbidden");
        }
        if self.surfaces.prompts == SurfaceExpectation::Forbidden
            && !self.surfaces.required_prompts.is_empty()
        {
            bail!("requiredPrompts cannot be set when prompts are forbidden");
        }
        Ok(())
    }
}

fn validate_url(field: &str, value: &str) -> Result<()> {
    let url = Url::parse(value)?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{field} must use HTTP or HTTPS"
    );
    ensure!(url.host_str().is_some(), "{field} must contain a host");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "{field} must not embed credentials"
    );
    ensure!(
        url.fragment().is_none(),
        "{field} must not contain a fragment"
    );
    Ok(())
}

fn require_token(field: &str, value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value == value.trim()
            && !value.chars().any(char::is_whitespace),
        "{field} must be one non-empty token"
    );
    Ok(())
}

fn valid_scheme(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_lowercase())
        && chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.')
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        HostedServerConformanceProfile, HostedServerProfileSchema, HttpBoundaryProfile,
        SurfaceExpectation, SurfaceProfile,
    };

    fn profile() -> HostedServerConformanceProfile {
        HostedServerConformanceProfile {
            schema_version: HostedServerProfileSchema::V1,
            profile_id: "extension-ci".to_owned(),
            contract_revision: "veoveo.io/hosted-mcp/v1".to_owned(),
            endpoint: "https://extension.example.internal/domain/mcp".to_owned(),
            server_slug: "domain".to_owned(),
            owned_resource_schemes: BTreeSet::from(["domain".to_owned()]),
            http: HttpBoundaryProfile {
                require_authentication_rejection: true,
                rejected_host: Some("untrusted.invalid".to_owned()),
                health_url: None,
                readiness_url: None,
            },
            surfaces: SurfaceProfile {
                tools: SurfaceExpectation::Required,
                resources: SurfaceExpectation::Required,
                resource_templates: SurfaceExpectation::Optional,
                prompts: SurfaceExpectation::Optional,
                completions: SurfaceExpectation::Optional,
                tasks: SurfaceExpectation::Required,
                subscriptions: SurfaceExpectation::Optional,
                required_tools: BTreeSet::from(["inspect".to_owned()]),
                required_resources: BTreeSet::new(),
                required_resource_templates: BTreeSet::new(),
                required_prompts: BTreeSet::new(),
            },
        }
    }

    #[test]
    fn validates_a_private_hosted_server_profile() {
        profile().validate().unwrap();
    }

    #[test]
    fn rejects_credentials_and_cross_domain_requirements() {
        let mut profile = profile();
        profile.endpoint = "https://token@example.internal/mcp".to_owned();
        assert!(profile.validate().is_err());

        profile.endpoint = "https://example.internal/mcp".to_owned();
        profile.surfaces.tools = SurfaceExpectation::Forbidden;
        assert!(profile.validate().is_err());
    }
}
