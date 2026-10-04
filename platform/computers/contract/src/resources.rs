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
use veoveo_types::{ResourceFieldCodec, ResourceUri};

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
#[veoveo_types::resource_address(routes(ComputerResourceErrorAddresses), traits = copied, schema = string, wire, to_uri = copied)]
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
#[veoveo_types::resource_address(components(ComputerResourceErrorAddresses), constructor = owned, template = "computer://computers/{computer_id}/automation/{grant_id}", traits = copied, parse = none, to_uri = copied, no_display)]
pub struct AutomationGrantUri {
    #[resource(variable="computer_id", error=|_| ComputerResourceError, accessor = computer_id, owned_accessor)]
    computer: ComputerId,
    #[resource(variable="grant_id", error=|_| ComputerResourceError, accessor = grant_id, owned_accessor)]
    grant: AutomationGrantId,
}

#[veoveo_types::resource_address(components(ComputerResourceErrorAddresses), constructor = owned, template = "computer://computers/{computer_id}", traits = copied, parse = none, to_uri = copied, no_display)]
pub struct ComputerResultUri(
    #[resource(variable="computer_id", error=|_| ComputerResourceError, accessor = computer_id, owned_accessor)]
     ComputerId,
);
impl ComputerResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}
fn schema_computer_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://computers", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}
#[veoveo_types::resource_address(components(ComputerResourceErrorAddresses), constructor = owned, template = "computer://executions/{execution_id}", traits = copied, parse = none, to_uri = copied, no_display)]
pub struct ExecutionResultUri(
    #[resource(variable="execution_id", error=|_| ComputerResourceError, accessor = execution_id, owned_accessor)]
     ExecutionId,
);
impl ExecutionResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}
fn schema_execution_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://executions", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}
#[veoveo_types::resource_address(components(ComputerResourceErrorAddresses), constructor = owned, template = "computer://transfers/{transfer_id}", traits = copied, parse = none, to_uri = copied, no_display)]
pub struct FileTransferResultUri(
    #[resource(variable="transfer_id", error=|_| ComputerResourceError, accessor = transfer_id, owned_accessor)]
     FileTransferId,
);
impl FileTransferResultUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}
fn schema_transfer_id(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"string",
        "pattern":concat!("^computer://transfers", "/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    })
}

#[doc(hidden)]
pub struct ComputerResourceErrorAddresses;
impl veoveo_types::ResourceProfile for ComputerResourceErrorAddresses {
    type Error = ComputerResourceError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| ComputerResourceError,
        };
    const SCHEMA: Option<veoveo_types::ResourceSchema> = Some(veoveo_types::ResourceSchema {
        schema: |name, generator| match name {
            "AutomationGrantUri" => {
                schemars::json_schema!({
                    "type": "string",
                    "pattern": "^computer://computers/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/automation/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
                })
            }
            "ComputerResultUri" => (schema_computer_id)(generator),
            "ExecutionResultUri" => (schema_execution_id)(generator),
            "FileTransferResultUri" => (schema_transfer_id)(generator),
            _ => unreachable!("owner schema declaration"),
        },
        inline: false,
    });
}
