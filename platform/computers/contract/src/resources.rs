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
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri};

pub const COMPUTERS_URI: &str = "computer://computers";
pub const COMPUTER_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_COMPUTER;
pub const ACCESS_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_ACCESS;
pub const MAINTENANCE_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_MAINTENANCE;
pub const AUTOMATION_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_AUTOMATION;
pub const GRANT_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_GRANT;
pub const PAGE_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_COLLECTION;
pub const DOC_TEMPLATE: &str = ComputerResource::RESOURCE_TEMPLATE_DOCUMENT;

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
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(error=ComputerResourceError, route_error=|_| ComputerResourceError, wire)]
pub enum ComputerResource {
    #[resource(template = "computer://computers{?after}")]
    Collection(#[resource(variable="after", error=|_| ComputerResourceError)] Option<ComputerId>),
    #[resource(template = "computer://computers/{computer_id}")]
    Computer(#[resource(variable="computer_id", error=|_| ComputerResourceError)] ComputerId),
    #[resource(template = "computer://computers/{computer_id}/access")]
    Access(#[resource(variable="computer_id", error=|_| ComputerResourceError)] ComputerId),
    #[resource(template = "computer://computers/{computer_id}/maintenance")]
    Maintenance(#[resource(variable="computer_id", error=|_| ComputerResourceError)] ComputerId),
    #[resource(template = "computer://executions/{execution_id}")]
    Execution(#[resource(variable="execution_id", error=|_| ComputerResourceError)] ExecutionId),
    #[resource(template = "computer://transfers/{transfer_id}")]
    Transfer(#[resource(variable="transfer_id", error=|_| ComputerResourceError)] FileTransferId),
    #[resource(template = "computer://computers/{computer_id}/automation")]
    Automation(#[resource(variable="computer_id", error=|_| ComputerResourceError)] ComputerId),
    #[resource(template = "computer://computers/{computer_id}/automation/{grant_id}")]
    Grant {
        #[resource(variable="computer_id", error=|_| ComputerResourceError)]
        computer: ComputerId,
        #[resource(variable="grant_id", error=|_| ComputerResourceError)]
        grant: AutomationGrantId,
    },
    #[resource(template = "computer://docs")]
    Docs,
    #[resource(template = "computer://docs/{doc_id}")]
    Document(
        #[resource(variable="doc_id", codec=DocumentCodec, error=|_| ComputerResourceError)]
        ComputerDocument,
    ),
    #[resource(template = "computer://contract")]
    Contract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid Computer resource URI")]
pub struct ComputerResourceError;

impl ComputerResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ComputerResourceError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| ComputerResourceError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(self) -> ResourceUri {
        <Self as ResourceAddress>::to_uri(&self).expect("typed Computer resource address")
    }
}
struct DocumentCodec;
impl ResourceFieldCodec<ComputerDocument> for DocumentCodec {
    type Error = ComputerResourceError;
    fn parse(value: &str) -> Result<ComputerDocument, Self::Error> {
        ComputerDocument::parse(value)
    }
    fn text(value: &ComputerDocument) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
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
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="computer://computers/{computer_id}/automation/{grant_id}", error=ComputerResourceError, route_error=|_| ComputerResourceError, wire)]
pub struct AutomationGrantUri {
    #[resource(variable="computer_id", error=|_| ComputerResourceError)]
    computer: ComputerId,
    #[resource(variable="grant_id", error=|_| ComputerResourceError)]
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
        Self::resource_from_parts(computer, grant).expect("typed Automation grant address")
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
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="computer://computers/{computer_id}", error=ComputerResourceError, route_error=|_| ComputerResourceError, wire, schema=schema_computer_id)]
pub struct ComputerResultUri(
    #[resource(variable="computer_id", error=|_| ComputerResourceError)] ComputerId,
);
impl ComputerResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: ComputerId) -> Self {
        Self::resource_from_parts(id).expect("typed Computer result address")
    }
    pub fn computer_id(self) -> ComputerId {
        self.0
    }
    pub fn to_uri(self) -> ResourceUri {
        <Self as ResourceAddress>::to_uri(&self).expect("typed Computer result address")
    }
}
fn schema_computer_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://computers", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="computer://executions/{execution_id}", error=ComputerResourceError, route_error=|_| ComputerResourceError, wire, schema=schema_execution_id)]
pub struct ExecutionResultUri(
    #[resource(variable="execution_id", error=|_| ComputerResourceError)] ExecutionId,
);
impl ExecutionResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: ExecutionId) -> Self {
        Self::resource_from_parts(id).expect("typed Computer result address")
    }
    pub fn execution_id(self) -> ExecutionId {
        self.0
    }
    pub fn to_uri(self) -> ResourceUri {
        <Self as ResourceAddress>::to_uri(&self).expect("typed Computer result address")
    }
}
fn schema_execution_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://executions", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="computer://transfers/{transfer_id}", error=ComputerResourceError, route_error=|_| ComputerResourceError, wire, schema=schema_transfer_id)]
pub struct FileTransferResultUri(
    #[resource(variable="transfer_id", error=|_| ComputerResourceError)] FileTransferId,
);
impl FileTransferResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: FileTransferId) -> Self {
        Self::resource_from_parts(id).expect("typed Computer result address")
    }
    pub fn transfer_id(self) -> FileTransferId {
        self.0
    }
    pub fn to_uri(self) -> ResourceUri {
        <Self as ResourceAddress>::to_uri(&self).expect("typed Computer result address")
    }
}
fn schema_transfer_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://transfers", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}
