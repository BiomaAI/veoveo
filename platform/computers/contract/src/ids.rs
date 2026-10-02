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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{borrow::Cow, fmt, str::FromStr};
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

macro_rules! identity {
    ($name:ident) => {
        identity!($name, UUID_V7, [7], "UUIDv7");
    };
    ($name:ident, $pattern:expr, [$($version:literal),+], $profile:literal) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
            pub fn into_uuid(self) -> Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl TryFrom<Uuid> for $name {
            type Error = ComputerIdentityError;
            fn try_from(id: Uuid) -> Result<Self, Self::Error> {
                if !matches!(id.get_version_num(), $($version)|+)
                    || id.get_variant() != uuid::Variant::RFC4122
                {
                    return Err(ComputerIdentityError(concat!("a canonical RFC ", $profile, " ", stringify!($name))));
                }
                Ok(Self(id))
            }
        }
        impl FromStr for $name {
            type Err = ComputerIdentityError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let invalid = ComputerIdentityError(concat!("a canonical RFC ", $profile, " ", stringify!($name)));
                let id = Uuid::parse_str(value).map_err(|_| invalid)?;
                if id.to_string() != value {
                    return Err(invalid);
                }
                Self::try_from(id)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                let mut schema = Uuid::json_schema(generator);
                schema.insert("pattern".into(), $pattern.into());
                schema.insert("minLength".into(), 36.into());
                schema.insert("maxLength".into(), 36.into());
                schema
            }
        }
    };
}
identity!(ComputerId);
identity!(ExecutionId);
identity!(FileTransferId);
identity!(AutomationGrantId);
identity!(AccessGrantId);
identity!(CliPairingId);
identity!(AccessConnectionId);
identity!(
    RequestId,
    "^[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    [4, 7],
    "UUIDv4 or UUIDv7"
);
identity!(
    ProviderInstanceId,
    "^[0-9a-f]{8}-[0-9a-f]{4}-[478][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    [4, 7, 8],
    "UUIDv4, UUIDv7 or UUIDv8"
);

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
