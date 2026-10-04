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
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

const UUID_V7: &str = "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComputerIdentityError(&'static str);
impl fmt::Display for ComputerIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}", self.0)
    }
}
impl std::error::Error for ComputerIdentityError {}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(ComputerId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct ComputerId(Uuid);
impl ComputerId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<ComputerId> for Uuid {
    fn from(value: ComputerId) -> Self {
        value.0
    }
}
impl TryFrom<String> for ComputerId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for ComputerId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!("a canonical RFC ", "UUIDv7", " ", stringify!(ComputerId)),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(ExecutionId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct ExecutionId(Uuid);
impl ExecutionId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<ExecutionId> for Uuid {
    fn from(value: ExecutionId) -> Self {
        value.0
    }
}
impl TryFrom<String> for ExecutionId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for ExecutionId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!("a canonical RFC ", "UUIDv7", " ", stringify!(ExecutionId)),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(FileTransferId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct FileTransferId(Uuid);
impl FileTransferId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<FileTransferId> for Uuid {
    fn from(value: FileTransferId) -> Self {
        value.0
    }
}
impl TryFrom<String> for FileTransferId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for FileTransferId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!(
                "a canonical RFC ",
                "UUIDv7",
                " ",
                stringify!(FileTransferId)
            ),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(AutomationGrantId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct AutomationGrantId(Uuid);
impl AutomationGrantId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<AutomationGrantId> for Uuid {
    fn from(value: AutomationGrantId) -> Self {
        value.0
    }
}
impl TryFrom<String> for AutomationGrantId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for AutomationGrantId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!(
                "a canonical RFC ",
                "UUIDv7",
                " ",
                stringify!(AutomationGrantId)
            ),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(AccessGrantId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct AccessGrantId(Uuid);
impl AccessGrantId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<AccessGrantId> for Uuid {
    fn from(value: AccessGrantId) -> Self {
        value.0
    }
}
impl TryFrom<String> for AccessGrantId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for AccessGrantId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!("a canonical RFC ", "UUIDv7", " ", stringify!(AccessGrantId)),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(CliPairingId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct CliPairingId(Uuid);
impl CliPairingId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<CliPairingId> for Uuid {
    fn from(value: CliPairingId) -> Self {
        value.0
    }
}
impl TryFrom<String> for CliPairingId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for CliPairingId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!("a canonical RFC ", "UUIDv7", " ", stringify!(CliPairingId)),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[7],concat!("a canonical RFC ","UUIDv7"," ",stringify!(AccessConnectionId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,UUID_V7))]
pub struct AccessConnectionId(Uuid);
impl AccessConnectionId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<AccessConnectionId> for Uuid {
    fn from(value: AccessConnectionId) -> Self {
        value.0
    }
}
impl TryFrom<String> for AccessConnectionId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for AccessConnectionId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[7],
            concat!(
                "a canonical RFC ",
                "UUIDv7",
                " ",
                stringify!(AccessConnectionId)
            ),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[4, 7],concat!("a canonical RFC ","UUIDv4 or UUIDv7"," ",stringify!(RequestId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,"^[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))]
pub struct RequestId(Uuid);
impl RequestId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<RequestId> for Uuid {
    fn from(value: RequestId) -> Self {
        value.0
    }
}
impl TryFrom<String> for RequestId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for RequestId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[4, 7],
            concat!(
                "a canonical RFC ",
                "UUIDv4 or UUIDv7",
                " ",
                stringify!(RequestId)
            ),
        )
        .map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "Uuid")]
#[id(error=ComputerIdentityError,admit=|value| admit_computer_id(value,&[4, 7, 8],concat!("a canonical RFC ","UUIDv4, UUIDv7 or UUIDv8"," ",stringify!(ProviderInstanceId))),generate=Uuid::now_v7,schema=|generator| computer_id_schema(generator,"^[0-9a-f]{8}-[0-9a-f]{4}-[478][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))]
pub struct ProviderInstanceId(Uuid);
impl ProviderInstanceId {
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}
impl From<ProviderInstanceId> for Uuid {
    fn from(value: ProviderInstanceId) -> Self {
        value.0
    }
}
impl TryFrom<String> for ProviderInstanceId {
    type Error = ComputerIdentityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl TryFrom<Uuid> for ProviderInstanceId {
    type Error = ComputerIdentityError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        validate_computer_uuid(
            value,
            &[4, 7, 8],
            concat!(
                "a canonical RFC ",
                "UUIDv4, UUIDv7 or UUIDv8",
                " ",
                stringify!(ProviderInstanceId)
            ),
        )
        .map(Self)
    }
}

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

fn validate_computer_uuid(
    value: Uuid,
    versions: &[usize],
    profile: &'static str,
) -> Result<Uuid, ComputerIdentityError> {
    if !versions.contains(&value.get_version_num()) || value.get_variant() != uuid::Variant::RFC4122
    {
        return Err(ComputerIdentityError(profile));
    }
    Ok(value)
}
fn admit_computer_id(
    value: &str,
    versions: &[usize],
    profile: &'static str,
) -> Result<Uuid, ComputerIdentityError> {
    let id = Uuid::parse_str(value).map_err(|_| ComputerIdentityError(profile))?;
    if id.to_string() != value {
        return Err(ComputerIdentityError(profile));
    }
    validate_computer_uuid(id, versions, profile)
}
fn computer_id_schema(generator: &mut SchemaGenerator, pattern: &str) -> Schema {
    let mut schema = Uuid::json_schema(generator);
    schema.insert("pattern".into(), pattern.into());
    schema.insert("minLength".into(), 36.into());
    schema.insert("maxLength".into(), 36.into());
    schema
}
