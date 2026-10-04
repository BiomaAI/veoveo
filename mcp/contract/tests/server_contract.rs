use std::{collections::BTreeSet, sync::LazyLock};

use rmcp::model::{Implementation, Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUri, ScopeName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
enum Permission {
    #[vocabulary(rename = "independent:read")]
    Read,
    #[vocabulary(rename = "independent:write")]
    Write,
}

// The fixture deliberately includes a broken codec to exercise admission failures.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Address {
    Docs,
    Agents,
    Design,
    Contract,
    BrokenRoundTrip,
}
impl ResourceAddress for Address {
    type Error = veoveo_types::IdentifierError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        match uri.as_str() {
            "independent://docs" => Ok(Self::Docs),
            "independent://docs/agents" => Ok(Self::Agents),
            "independent://docs/design" => Ok(Self::Design),
            "independent://contract" => Ok(Self::Contract),
            _ => Err(veoveo_types::IdentifierError::new(
                uri.as_str(),
                "unknown fixture address",
            )),
        }
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(ResourceUri::new(match self {
            Self::Docs | Self::BrokenRoundTrip => "independent://docs",
            Self::Agents => "independent://docs/agents",
            Self::Design => "independent://docs/design",
            Self::Contract => "independent://contract",
        })
        .unwrap())
    }
}

fn documents(owner: &'static str) -> ServerDocs {
    ServerDocs::new(owner)
        .with_doc("agents", "Agent manual", "# Manual")
        .with_doc("design", "Design", "# Design")
}
static DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| documents("independent"));
static FOREIGN_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| documents("foreign"));
static EMPTY_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| ServerDocs::new("independent"));
static DUPLICATE_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| {
    documents("independent").with_doc("agents", "Duplicate manual", "# Duplicate")
});
static BLANK_DOCUMENT: LazyLock<ServerDocs> =
    LazyLock::new(|| documents("independent").with_doc("extra", "Empty", "  "));

const VALID: u8 = 0;
const WRONG_NAME: u8 = 1;
const WRONG_OWNER: u8 = 2;
const NO_RESOURCES: u8 = 3;
const NO_DOCUMENTS: u8 = 4;
const DUPLICATE_DOCUMENT: u8 = 5;
const DUPLICATE_RESOURCE: u8 = 6;
const MISSING_ROOT: u8 = 7;
const MISSING_DOCUMENT_RESOURCE: u8 = 8;
const DUPLICATE_SCOPE: u8 = 9;
const EMPTY_SCOPES: u8 = 10;
const BAD_TEMPLATE: u8 = 11;
const DUPLICATE_TEMPLATE: u8 = 12;
const EMPTY_DOCUMENT_BODY: u8 = 13;
const NO_COMPLETIONS: u8 = 14;
struct Fixture<const CASE: u8>;

impl<const CASE: u8> McpServerContract for Fixture<CASE> {
    type Scope = Permission;
    type Resource = Address;
    fn slug() -> ServerSlug {
        ServerSlug::parse("independent").unwrap()
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("independent").unwrap()
    }
    fn scopes() -> &'static [Permission] {
        match CASE {
            DUPLICATE_SCOPE => &[Permission::Read, Permission::Read],
            EMPTY_SCOPES => &[],
            _ => &[Permission::Read],
        }
    }
    fn documents() -> &'static ServerDocs {
        match CASE {
            WRONG_OWNER => &FOREIGN_DOCUMENTS,
            NO_DOCUMENTS => &EMPTY_DOCUMENTS,
            DUPLICATE_DOCUMENT => &DUPLICATE_DOCUMENTS,
            EMPTY_DOCUMENT_BODY => &BLANK_DOCUMENT,
            _ => &DOCUMENTS,
        }
    }
    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::default();
        config.server_info = Implementation::new(
            if CASE == WRONG_NAME {
                "foreign"
            } else {
                "independent"
            },
            "1.0.0",
        );
        config.capabilities = match CASE {
            NO_RESOURCES => ServerCapabilities::builder().enable_completions().build(),
            NO_COMPLETIONS => ServerCapabilities::builder().enable_resources().build(),
            _ => ServerCapabilities::builder()
                .enable_resources()
                .enable_completions()
                .build(),
        };
        config
    }
    fn resources() -> Result<Vec<McpResource<Address>>, McpSetupError> {
        let mut addresses = vec![
            Address::Docs,
            Address::Agents,
            Address::Design,
            Address::Contract,
        ];
        match CASE {
            DUPLICATE_RESOURCE => addresses.push(Address::Docs),
            MISSING_ROOT => addresses.retain(|address| *address != Address::Contract),
            MISSING_DOCUMENT_RESOURCE => addresses.retain(|address| *address != Address::Design),
            _ => {}
        }
        addresses
            .into_iter()
            .map(|address| {
                McpResource::new(address, |uri| {
                    Resource::new(uri, "Fixture").with_mime_type("text/plain")
                })
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        let text = if CASE == BAD_TEMPLATE {
            "relative/{id}"
        } else {
            "independent://item/{+id}{?cursor}"
        };
        let template =
            ResourceTemplateUri::new(text).map_err(|_| McpSetupError::InvalidTemplate)?;
        let descriptor =
            McpResourceTemplate::new(template, |uri| ResourceTemplate::new(uri, "Item"))?;
        let mut templates = vec![descriptor; if CASE == DUPLICATE_TEMPLATE { 2 } else { 1 }];
        templates.push(McpResourceTemplate::new(
            ResourceTemplateUri::new("independent://docs/{doc_id}").unwrap(),
            |uri| ResourceTemplate::new(uri, "Document"),
        )?);
        Ok(templates)
    }
}

fn rejects<const CASE: u8>(expected: McpSetupError) {
    assert_eq!(McpServerSetup::<Fixture<CASE>>::new().err(), Some(expected));
}

#[test]
fn scope_membership_keeps_unknown_grants_and_rejects_undeclared_permissions() {
    let setup = McpServerSetup::<Fixture<VALID>>::new().unwrap();
    let grants = BTreeSet::from([
        Permission::Read.into(),
        Permission::Write.into(),
        ScopeName::parse("another:read").unwrap(),
    ]);
    assert!(setup.has_scope(&grants, Permission::Read));
    assert!(!setup.has_scope(&grants, Permission::Write));
    assert!(!setup.has_scope(&BTreeSet::new(), Permission::Read));
    assert_eq!(
        setup.scope_names(),
        &BTreeSet::from([Permission::Read.into()])
    );
    rejects::<DUPLICATE_SCOPE>(McpSetupError::DuplicateScope);
}

#[test]
fn servers_without_domain_scopes_have_an_empty_vocabulary() {
    let setup = McpServerSetup::<Fixture<EMPTY_SCOPES>>::new().unwrap();
    assert!(setup.scope_names().is_empty());
    assert!(!setup.has_scope(&BTreeSet::from([Permission::Read.into()]), Permission::Read));
}

#[test]
fn setup_preserves_typed_addresses_metadata_and_rfc6570_declarations() {
    let setup = McpServerSetup::<Fixture<VALID>>::new().unwrap();
    let resources = setup.resources();
    assert_eq!(resources.len(), 4);
    assert!(
        resources
            .windows(2)
            .all(|pair| pair[0].descriptor().uri < pair[1].descriptor().uri)
    );
    for resource in resources {
        assert_eq!(
            resource.address().to_uri().unwrap().as_str(),
            resource.descriptor().uri
        );
        assert_eq!(
            resource.descriptor().mime_type.as_deref(),
            Some("text/plain")
        );
    }
    assert_eq!(
        setup
            .resource_templates()
            .iter()
            .find(|t| t.descriptor().name == "Item")
            .unwrap()
            .template()
            .as_str(),
        "independent://item/{+id}{?cursor}"
    );
    assert_eq!(
        setup.documents().server(),
        setup.server_config().server_info.name
    );
}

#[test]
fn descriptor_builder_cannot_override_address_or_hide_a_broken_codec() {
    assert_eq!(
        McpResource::new(Address::Docs, |_| Resource::new("foreign://docs", "Docs")).unwrap_err(),
        McpSetupError::DescriptorAddressMismatch
    );
    assert_eq!(
        McpResource::new(Address::BrokenRoundTrip, |uri| Resource::new(uri, "Docs")).unwrap_err(),
        McpSetupError::ResourceRoundTrip
    );
    assert_eq!(
        McpResource::new(Address::Docs, |uri| Resource::new(uri, " ")).unwrap_err(),
        McpSetupError::MissingResourceName
    );
}

#[test]
fn inconsistent_server_identity_and_missing_capability_fail_before_serving() {
    rejects::<WRONG_NAME>(McpSetupError::IdentityMismatch);
    rejects::<WRONG_OWNER>(McpSetupError::IdentityMismatch);
    rejects::<NO_RESOURCES>(McpSetupError::MissingResourcesCapability);
    rejects::<NO_COMPLETIONS>(McpSetupError::MissingCompletionsCapability);
}

#[test]
fn document_and_discovery_coverage_is_checked_before_serving() {
    rejects::<NO_DOCUMENTS>(McpSetupError::MissingDocument);
    rejects::<EMPTY_DOCUMENT_BODY>(McpSetupError::MissingDocument);
    rejects::<DUPLICATE_DOCUMENT>(McpSetupError::InvalidDocument);
    rejects::<DUPLICATE_RESOURCE>(McpSetupError::DuplicateResource);
    rejects::<MISSING_ROOT>(McpSetupError::MissingWellKnownResource);
    rejects::<MISSING_DOCUMENT_RESOURCE>(McpSetupError::MissingWellKnownResource);
}

#[test]
fn invalid_or_duplicate_template_declarations_are_rejected() {
    rejects::<BAD_TEMPLATE>(McpSetupError::InvalidTemplate);
    rejects::<DUPLICATE_TEMPLATE>(McpSetupError::DuplicateTemplate);
}

#[test]
fn template_metadata_cannot_override_the_validated_reference() {
    let template = ResourceTemplateUri::new("independent://item/{id}").unwrap();
    assert_eq!(
        McpResourceTemplate::new(template.clone(), |_| ResourceTemplate::new(
            "foreign://{id}",
            "Other"
        ))
        .unwrap_err(),
        McpSetupError::DescriptorTemplateMismatch
    );
    assert_eq!(
        McpResourceTemplate::new(template, |uri| ResourceTemplate::new(uri, " ")).unwrap_err(),
        McpSetupError::InvalidTemplate
    );
}
