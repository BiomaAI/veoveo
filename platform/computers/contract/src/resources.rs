//! Complete resource routes shared by hosted Computers and its consumers.
//! ```compile_fail
//! use veoveo_computers_contract::{ExecutionResultUri, FileTransferId};
//! ExecutionResultUri::new(FileTransferId::new());
//! ```
//! ```compile_fail
//! use veoveo_computers_contract::{FileTransferResultUri, ExecutionId};
//! FileTransferResultUri::new(ExecutionId::new());
//! ```
use crate::{AutomationGrantId, ComputerId, ExecutionId, FileTransferId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

pub const COMPUTERS_URI: &str = "computer://computers";
pub const COMPUTER_TEMPLATE: &str = "computer://computers/{computer_id}";
pub const ACCESS_TEMPLATE: &str = "computer://computers/{computer_id}/access";
pub const MAINTENANCE_TEMPLATE: &str = "computer://computers/{computer_id}/maintenance";
pub const AUTOMATION_TEMPLATE: &str = "computer://computers/{computer_id}/automation";
pub const GRANT_TEMPLATE: &str = "computer://computers/{computer_id}/automation/{grant_id}";
pub const PAGE_TEMPLATE: &str = "computer://computers{?after}";
pub const DOC_TEMPLATE: &str = "computer://docs/{doc_id}";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerDocument {
    Agents,
    Design,
}
impl ComputerDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(value: &str) -> Result<Self, ComputerResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(ComputerResourceError),
        }
    }
}

/// URI values convey identity. Current actor and resource policy still admit each read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum ComputerResource {
    Collection(Option<ComputerId>),
    Computer(ComputerId),
    Access(ComputerId),
    Maintenance(ComputerId),
    Execution(ExecutionId),
    Transfer(FileTransferId),
    Automation(ComputerId),
    Grant {
        computer: ComputerId,
        grant: AutomationGrantId,
    },
    Docs,
    Document(ComputerDocument),
    Contract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComputerResourceError;
impl fmt::Display for ComputerResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Computer resource URI")
    }
}
impl std::error::Error for ComputerResourceError {}

impl ComputerResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ComputerResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| ComputerResourceError)?;
        if parts.scheme() != "computer" {
            return Err(ComputerResourceError);
        }
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|part| part.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.authority(), path.as_slice()) {
            ("computers", []) => {
                if parts.query_parameters().keys().any(|key| key != "after") {
                    return Err(ComputerResourceError);
                }
                Self::Collection(
                    parts
                        .query_parameters()
                        .get("after")
                        .map(|value| value.parse())
                        .transpose()
                        .map_err(|_| ComputerResourceError)?,
                )
            }
            _ if parts.has_query() => return Err(ComputerResourceError),
            ("computers", [id]) => Self::Computer(id.parse().map_err(|_| ComputerResourceError)?),
            ("computers", [id, "access"]) => {
                Self::Access(id.parse().map_err(|_| ComputerResourceError)?)
            }
            ("computers", [id, "maintenance"]) => {
                Self::Maintenance(id.parse().map_err(|_| ComputerResourceError)?)
            }
            ("computers", [id, "automation"]) => {
                Self::Automation(id.parse().map_err(|_| ComputerResourceError)?)
            }
            ("computers", [id, "automation", grant]) => Self::Grant {
                computer: id.parse().map_err(|_| ComputerResourceError)?,
                grant: grant.parse().map_err(|_| ComputerResourceError)?,
            },
            ("executions", [id]) => Self::Execution(id.parse().map_err(|_| ComputerResourceError)?),
            ("transfers", [id]) => Self::Transfer(id.parse().map_err(|_| ComputerResourceError)?),
            ("docs", []) => Self::Docs,
            ("docs", [doc]) => Self::Document(ComputerDocument::parse(doc)?),
            ("contract", []) => Self::Contract,
            _ => return Err(ComputerResourceError),
        };
        if resource.to_uri().as_str() != value {
            return Err(ComputerResourceError);
        }
        Ok(resource)
    }
    pub fn to_uri(self) -> ResourceUri {
        let (root, segments) = match self {
            Self::Collection(after) => {
                let mut builder =
                    ResourceUriBuilder::new(COMPUTERS_URI).expect("declared collection");
                if let Some(after) = after {
                    builder = builder
                        .query_pair("after", &after.to_string())
                        .expect("typed Computer position");
                }
                return builder.build().expect("typed Computer collection URI");
            }
            Self::Computer(id) => (COMPUTERS_URI, vec![id.to_string()]),
            Self::Access(id) => (COMPUTERS_URI, vec![id.to_string(), "access".into()]),
            Self::Maintenance(id) => (COMPUTERS_URI, vec![id.to_string(), "maintenance".into()]),
            Self::Automation(id) => (COMPUTERS_URI, vec![id.to_string(), "automation".into()]),
            Self::Grant { computer, grant } => (
                COMPUTERS_URI,
                vec![computer.to_string(), "automation".into(), grant.to_string()],
            ),
            Self::Execution(id) => ("computer://executions", vec![id.to_string()]),
            Self::Transfer(id) => ("computer://transfers", vec![id.to_string()]),
            Self::Docs => ("computer://docs", vec![]),
            Self::Document(doc) => ("computer://docs", vec![doc.as_str().into()]),
            Self::Contract => ("computer://contract", vec![]),
        };
        let mut builder = ResourceUriBuilder::new(root).expect("declared Computer resource root");
        for segment in segments {
            builder =
                builder.segment(UriSegment::new(segment).expect("typed Computer resource segment"));
        }
        builder.build().expect("typed Computer resource URI")
    }
}
impl ResourceAddress for ComputerResource {
    type Error = ComputerResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok((*self).to_uri())
    }
}
impl TryFrom<String> for ComputerResource {
    type Error = ComputerResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<ComputerResource> for String {
    fn from(value: ComputerResource) -> Self {
        value.to_uri().to_string()
    }
}

/// ```compile_fail
/// use veoveo_computers_contract::{computer_uri, ExecutionId};
/// computer_uri(ExecutionId::new());
/// ```
pub fn computer_uri(id: ComputerId) -> ResourceUri {
    ComputerResource::Computer(id).to_uri()
}
pub fn maintenance_uri(id: ComputerId) -> ResourceUri {
    ComputerResource::Maintenance(id).to_uri()
}
pub fn automation_grant_uri(computer: ComputerId, grant: AutomationGrantId) -> ResourceUri {
    ComputerResource::Grant { computer, grant }.to_uri()
}

/// One grant under its owning Computer. A valid address grants no access.
///
/// ```compile_fail
/// use veoveo_computers_contract::{AutomationGrantUri, ComputerId, ExecutionId};
/// AutomationGrantUri::new(ComputerId::new(), ExecutionId::new());
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AutomationGrantUri {
    computer: ComputerId,
    grant: AutomationGrantId,
}
impl JsonSchema for AutomationGrantUri {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AutomationGrantUri".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^computer://computers/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/automation/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
        })
    }
}
impl AutomationGrantUri {
    pub fn new(computer: ComputerId, grant: AutomationGrantId) -> Self {
        Self { computer, grant }
    }
    pub fn computer_id(self) -> ComputerId {
        self.computer
    }
    pub fn grant_id(self) -> AutomationGrantId {
        self.grant
    }
    pub fn to_uri(self) -> ResourceUri {
        automation_grant_uri(self.computer, self.grant)
    }
}
impl From<AutomationGrantUri> for String {
    fn from(value: AutomationGrantUri) -> Self {
        value.to_uri().to_string()
    }
}
impl TryFrom<String> for AutomationGrantUri {
    type Error = ComputerResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match ComputerResource::parse(value)? {
            ComputerResource::Grant { computer, grant } => Ok(Self::new(computer, grant)),
            _ => Err(ComputerResourceError),
        }
    }
}
impl ResourceAddress for AutomationGrantUri {
    type Error = ComputerResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok((*self).to_uri())
    }
}

macro_rules! result_uri {
    ($uri:ident, $id:ident, $variant:ident, $getter:ident, $template:literal, $route:literal) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $uri($id);
        impl $uri {
            pub const TEMPLATE: &'static str = $template;
            pub fn new(id: $id) -> Self {
                Self(id)
            }
            pub fn $getter(self) -> $id {
                self.0
            }
            pub fn to_uri(self) -> ResourceUri {
                ComputerResource::$variant(self.0).to_uri()
            }
        }
        impl JsonSchema for $uri {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($uri).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "pattern": concat!("^computer://", $route, "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
                })
            }
        }
        impl From<$uri> for String {
            fn from(uri: $uri) -> Self {
                uri.to_uri().to_string()
            }
        }
        impl TryFrom<String> for $uri {
            type Error = ComputerResourceError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                match ComputerResource::parse(value)? {
                    ComputerResource::$variant(id) => Ok(Self(id)),
                    _ => Err(ComputerResourceError),
                }
            }
        }
        impl ResourceAddress for $uri {
            type Error = ComputerResourceError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::try_from(value.to_string())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok((*self).to_uri())
            }
        }
    };
}
result_uri!(
    ComputerResultUri,
    ComputerId,
    Computer,
    computer_id,
    "computer://computers/{computer_id}",
    "computers"
);
result_uri!(
    ExecutionResultUri,
    ExecutionId,
    Execution,
    execution_id,
    "computer://executions/{execution_id}",
    "executions"
);
result_uri!(
    FileTransferResultUri,
    FileTransferId,
    Transfer,
    transfer_id,
    "computer://transfers/{transfer_id}",
    "transfers"
);
