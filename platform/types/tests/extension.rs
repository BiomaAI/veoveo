//! This integration test compiles as a consumer crate and can implement both
//! public traits without introducing a domain registry into veoveo-types.
use std::{collections::BTreeSet, sync::LazyLock};

use veoveo_types::{IdentifierError, ResourceAddress, ResourceUri, ScopeDefinition, ScopeName};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OrchardScope {
    InventoryRead,
    HarvestWrite,
}

impl ScopeDefinition for OrchardScope {
    fn name(self) -> &'static ScopeName {
        match self {
            Self::InventoryRead => {
                static NAME: LazyLock<ScopeName> =
                    LazyLock::new(|| ScopeName::parse("orchard:inventory:read").unwrap());
                &NAME
            }
            Self::HarvestWrite => {
                static NAME: LazyLock<ScopeName> =
                    LazyLock::new(|| ScopeName::parse("orchard:harvest:write").unwrap());
                &NAME
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum OrchardResource {
    Inventory,
}

impl ResourceAddress for OrchardResource {
    type Error = IdentifierError;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        match uri.as_str() {
            "orchard://inventory" => Ok(Self::Inventory),
            _ => Err(IdentifierError::new(
                uri.as_str(),
                "unknown Orchard resource",
            )),
        }
    }

    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        match self {
            Self::Inventory => Ok(ResourceUri::new("orchard://inventory").unwrap()),
        }
    }
}

fn admits(grants: &BTreeSet<ScopeName>, required: OrchardScope) -> bool {
    grants.contains(required.name())
}

#[test]
fn consumer_owned_vocabulary_and_resources_need_no_core_registration() {
    let grants = BTreeSet::from([
        ScopeName::parse("orchard:inventory:read").unwrap(),
        ScopeName::parse("another-server:custom").unwrap(),
    ]);
    assert!(admits(&grants, OrchardScope::InventoryRead));
    assert!(!admits(&grants, OrchardScope::HarvestWrite));
    assert!(!admits(&BTreeSet::new(), OrchardScope::InventoryRead));
    let uri = OrchardResource::Inventory.to_uri().unwrap();
    assert_eq!(
        OrchardResource::parse(&uri).unwrap(),
        OrchardResource::Inventory
    );
    assert!(OrchardResource::parse(&ResourceUri::new("other://inventory").unwrap()).is_err());
    assert!(
        OrchardResource::parse(&ResourceUri::new("orchard://inventory?unknown=1").unwrap())
            .is_err()
    );
}
