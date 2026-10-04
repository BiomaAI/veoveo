//! Native public identities owned by Computers, independent of storage and transport.
//! ```compile_fail
//! use veoveo_computers_contract::{ComputerId, RequestId};
//! fn submit(_: RequestId) {}
//! submit(ComputerId::new());
//! ```
//! ```compile_fail
//! use veoveo_computers_contract::{ProviderInstanceId, RequestId};
//! fn connect(_: ProviderInstanceId) {}
//! connect(RequestId::new());
//! ```
use std::fmt;
use uuid::Uuid;
use veoveo_types::{
    IdFailure, IdGrammar, IdMetadata, IdProfile, IdProfileSpec, IdSchema, IdWire, UuidGrammar,
};

const UUID_V7: &str = "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComputerIdentityError(&'static str);
impl fmt::Display for ComputerIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}", self.0)
    }
}
impl std::error::Error for ComputerIdentityError {}

fn computer_id_error(_: &str, metadata: IdMetadata, _: IdFailure) -> ComputerIdentityError {
    ComputerIdentityError(metadata.error_context)
}
#[doc(hidden)]
pub struct ComputerIds;
impl IdProfile for ComputerIds {
    type Error = ComputerIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        wire: IdWire::UuidOutStringIn,
        schema: IdSchema::UuidPattern { pattern: UUID_V7 },
        ..IdProfileSpec::generated_uuid(UuidGrammar::canonical(&[7]), computer_id_error)
    };
}
#[doc(hidden)]
pub struct RequestIds;
impl IdProfile for RequestIds {
    type Error = ComputerIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        grammar: IdGrammar::Uuid(UuidGrammar::canonical(&[4, 7]), computer_id_error),
        schema: IdSchema::UuidPattern {
            pattern: "^[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        },
        ..ComputerIds::PROFILE
    };
}
#[doc(hidden)]
pub struct ProviderIds;
impl IdProfile for ProviderIds {
    type Error = ComputerIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        grammar: IdGrammar::Uuid(UuidGrammar::canonical(&[4, 7, 8]), computer_id_error),
        schema: IdSchema::UuidPattern {
            pattern: "^[0-9a-f]{8}-[0-9a-f]{4}-[478][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        },
        ..ComputerIds::PROFILE
    };
}

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(ComputerId)))]
pub struct ComputerId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(ExecutionId)))]
pub struct ExecutionId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(FileTransferId)))]
pub struct FileTransferId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(AutomationGrantId)))]
pub struct AutomationGrantId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(AccessGrantId)))]
pub struct AccessGrantId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(CliPairingId)))]
pub struct CliPairingId(Uuid);

#[veoveo_types::id(uuid(ComputerIds) , fresh, error_context = concat!("a canonical RFC UUIDv7", " ", stringify!(AccessConnectionId)))]
pub struct AccessConnectionId(Uuid);

#[veoveo_types::id(uuid(RequestIds) , fresh,
    error_context = concat!("a canonical RFC UUIDv4 or UUIDv7", " ", stringify!(RequestId)))]
pub struct RequestId(Uuid);

#[veoveo_types::id(uuid(ProviderIds) , fresh,
    error_context = concat!("a canonical RFC UUIDv4, UUIDv7 or UUIDv8", " ", stringify!(ProviderInstanceId)))]
pub struct ProviderInstanceId(Uuid);

impl ExecutionId {
    pub fn task_id(self) -> veoveo_types::TaskId {
        veoveo_types::TaskId::from_uuid(self.0)
    }
}
impl FileTransferId {
    pub fn task_id(self) -> veoveo_types::TaskId {
        veoveo_types::TaskId::from_uuid(self.0)
    }
}
