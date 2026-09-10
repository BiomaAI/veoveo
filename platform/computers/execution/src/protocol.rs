use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;

/// Static errors deliberately contain no request values or parser excerpts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    InvalidRequest,
    WrongIdentity,
    DirectoryUnavailable,
    InputUnavailable,
    ProgramUnavailable,
}

/// This type has no diagnostic formatting surface.
///
/// ```compile_fail
/// use veoveo_computer_execution::ExecutionRequest;
/// fn cannot_log(request: ExecutionRequest) { let _ = format!("{request:?}"); }
/// ```
pub struct ExecutionRequest(pub(crate) Payload);

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Payload {
    version: u8,
    #[serde(deserialize_with = "bounded_arguments")]
    pub arguments: Vec<String>,
    pub directory: String,
    #[serde(deserialize_with = "unique_environment")]
    pub environment: BTreeMap<String, String>,
    #[serde(with = "binary_input")]
    pub stdin: Vec<u8>,
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.arguments.zeroize();
        self.directory.zeroize();
        for value in self.environment.values_mut() {
            value.zeroize();
        }
        self.stdin.zeroize();
    }
}

impl ExecutionRequest {
    pub fn new(
        arguments: Vec<String>,
        directory: String,
        environment: BTreeMap<String, String>,
        stdin: Vec<u8>,
    ) -> Result<Self, LaunchError> {
        let request = Self(Payload {
            version: crate::PROTOCOL_VERSION,
            arguments,
            directory,
            environment,
            stdin,
        });
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), LaunchError> {
        let request = &self.0;
        let invalid_args = request.arguments.is_empty()
            || request.arguments.len() > 1024
            || request.arguments[0].is_empty()
            || request
                .arguments
                .iter()
                .any(|arg| arg.len() > 32768 || arg.contains('\0'))
            || request.arguments.iter().map(String::len).sum::<usize>() > 32768;
        let invalid_environment = request.environment.len() > 64
            || request.environment.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 256
                    || value.len() > 16384
                    || value.contains('\0')
                    || !key.bytes().enumerate().all(|(index, byte)| {
                        byte == b'_'
                            || byte.is_ascii_alphabetic()
                            || (index > 0 && byte.is_ascii_digit())
                    })
            })
            || request
                .environment
                .iter()
                .map(|(key, value)| key.len() + value.len())
                .sum::<usize>()
                > 16384;
        let directory = &request.directory;
        let invalid_directory = directory.is_empty()
            || directory.len() > 1024
            || directory.chars().any(char::is_control)
            || (directory != "."
                && !Path::new(directory)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))));
        if request.version != crate::PROTOCOL_VERSION
            || invalid_args
            || invalid_environment
            || invalid_directory
            || request.stdin.len() > MAX_INPUT_BYTES
        {
            return Err(LaunchError::InvalidRequest);
        }
        Ok(())
    }

    /// The returned bytes contain private request values. Never log or publish them.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, LaunchError> {
        self.validate()?;
        let bytes =
            Zeroizing::new(serde_json::to_vec(&self.0).map_err(|_| LaunchError::InvalidRequest)?);
        if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
            return Err(LaunchError::InvalidRequest);
        }
        let mut framed = Zeroizing::new(Vec::with_capacity(bytes.len() + 4));
        framed.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        framed.extend_from_slice(&bytes);
        Ok(framed)
    }
}

/// Read exactly the frame. The transport may remain open until the program exits.
pub fn read_frame(mut input: impl Read) -> Result<ExecutionRequest, LaunchError> {
    let mut length = [0; 4];
    input
        .read_exact(&mut length)
        .map_err(|_| LaunchError::InvalidRequest)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(LaunchError::InvalidRequest);
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    input
        .read_exact(&mut bytes)
        .map_err(|_| LaunchError::InvalidRequest)?;
    let request =
        ExecutionRequest(serde_json::from_slice(&bytes).map_err(|_| LaunchError::InvalidRequest)?);
    request.validate()?;
    Ok(request)
}

fn unique_environment<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    struct Unique;
    impl<'de> serde::de::Visitor<'de> for Unique {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded environment object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, String>()? {
                if values.len() >= 64 || values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive environment entries",
                    ));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique)
}

fn bounded_arguments<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    struct Arguments;
    impl<'de> serde::de::Visitor<'de> for Arguments {
        type Value = Vec<String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded argument vector")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            let mut bytes = 0;
            while let Some(value) = access.next_element::<String>()? {
                bytes += value.len();
                if values.len() >= 1024 || bytes > 32768 {
                    return Err(serde::de::Error::custom("arguments exceed their bound"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Arguments)
}

mod binary_input {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        let encoded = Zeroizing::new(STANDARD.encode(bytes));
        serializer.serialize_str(&encoded)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let encoded = Zeroizing::new(String::deserialize(deserializer)?);
        if encoded.len() > MAX_INPUT_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("stdin exceeds its bound"));
        }
        STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| serde::de::Error::custom("invalid encoded stdin"))
    }
}
