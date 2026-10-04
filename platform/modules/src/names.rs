//! Checked owner names and execution declarations without protocol dependencies.
use crate::DeclarationError;

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn module_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}
fn function_name(value: &str) -> bool {
    value.len() <= 512
        && value
            .strip_prefix("fn::")
            .is_some_and(|parts| parts.split("::").all(identifier))
}
fn function_prefix(value: &str) -> bool {
    let without_tail = value.strip_suffix("::").unwrap_or(value);
    function_name(without_tail)
}
fn executable_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn image_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
}
fn extension_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleName(String);
impl ModuleName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !module_name(&value) {
            return Err(DeclarationError::new("invalid ModuleName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for ModuleName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName(String);
impl TableName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !identifier(&value) {
            return Err(DeclarationError::new("invalid TableName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for TableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for TableName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TablePrefix(String);
impl TablePrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !identifier(&value) {
            return Err(DeclarationError::new("invalid TablePrefix"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for TablePrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for TablePrefix {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionName(String);
impl FunctionName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !function_name(&value) {
            return Err(DeclarationError::new("invalid FunctionName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for FunctionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for FunctionName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionPrefix(String);
impl FunctionPrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !function_prefix(&value) {
            return Err(DeclarationError::new("invalid FunctionPrefix"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for FunctionPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for FunctionPrefix {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnalyzerName(String);
impl AnalyzerName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !identifier(&value) {
            return Err(DeclarationError::new("invalid AnalyzerName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for AnalyzerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for AnalyzerName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MigrationName(String);
impl MigrationName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !identifier(&value) {
            return Err(DeclarationError::new("invalid MigrationName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for MigrationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for MigrationName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutionImage(String);
impl ExecutionImage {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !image_target(&value) {
            return Err(DeclarationError::new("invalid ExecutionImage"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ExecutionImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for ExecutionImage {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtensionPointName(String);
impl ExtensionPointName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !extension_name(&value) {
            return Err(DeclarationError::new("invalid ExtensionPointName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ExtensionPointName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for ExtensionPointName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtensionImplementationName(String);
impl ExtensionImplementationName {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !extension_name(&value) {
            return Err(DeclarationError::new("invalid ExtensionImplementationName"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for ExtensionImplementationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::str::FromStr for ExtensionImplementationName {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Zero is the first migration; absence of migrations is a separate state.
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "serialization", serde(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MigrationVersion(u32);
impl MigrationVersion {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u32 {
        self.0
    }
}
impl std::fmt::Display for MigrationVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionCommand(Vec<String>);
impl ExecutionCommand {
    /// argv includes the executable; no shell parsing or interpolation occurs here.
    pub fn new(argv: Vec<String>) -> Result<Self, DeclarationError> {
        if argv.is_empty() || argv.len() > 128 || argv.iter().any(|arg| !executable_text(arg)) {
            return Err(DeclarationError::new(
                "execution command requires an executable and valid argv",
            ));
        }
        Ok(Self(argv))
    }
    pub fn argv(&self) -> &[String] {
        &self.0
    }
}

#[cfg(feature = "serialization")]
fn digest_identity(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[cfg(feature = "serialization")]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompositionIdentity(String);
#[cfg(feature = "serialization")]
impl CompositionIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !digest_identity(&value) {
            return Err(DeclarationError::new("invalid CompositionIdentity"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[cfg(feature = "serialization")]
impl std::str::FromStr for CompositionIdentity {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
#[cfg(feature = "serialization")]
impl std::fmt::Display for CompositionIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serialization")]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LaneIdentity(String);
#[cfg(feature = "serialization")]
impl LaneIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !digest_identity(&value) {
            return Err(DeclarationError::new("invalid LaneIdentity"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[cfg(feature = "serialization")]
impl std::str::FromStr for LaneIdentity {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
#[cfg(feature = "serialization")]
impl std::fmt::Display for LaneIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serialization")]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeBindingKey(String);
#[cfg(feature = "serialization")]
impl RuntimeBindingKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if !extension_name(&value) {
            return Err(DeclarationError::new("invalid RuntimeBindingKey"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[cfg(feature = "serialization")]
impl std::str::FromStr for RuntimeBindingKey {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
#[cfg(feature = "serialization")]
impl std::fmt::Display for RuntimeBindingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for ModuleName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for ModuleName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for ExecutionImage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for ExecutionImage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for CompositionIdentity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for CompositionIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for LaneIdentity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for LaneIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for RuntimeBindingKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for RuntimeBindingKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serialization")]
impl serde::Serialize for ExecutionCommand {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(self.argv(), serializer)
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for ExecutionCommand {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(<Vec<String> as serde::Deserialize>::deserialize(
            deserializer,
        )?)
        .map_err(serde::de::Error::custom)
    }
}

/// Installation-owned preparation epoch, serialized as decimal text to preserve u64 precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstallationGeneration(u64);
impl InstallationGeneration {
    pub fn new(value: u64) -> Result<Self, DeclarationError> {
        if value == 0 {
            return Err(DeclarationError::new(
                "installation generation must be positive",
            ));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl std::str::FromStr for InstallationGeneration {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(DeclarationError::new(
                "installation generation must be canonical positive decimal text",
            ));
        }
        Self::new(
            value
                .parse()
                .map_err(|_| DeclarationError::new("installation generation exceeds u64"))?,
        )
    }
}
impl std::fmt::Display for InstallationGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
#[cfg(feature = "serialization")]
impl serde::Serialize for InstallationGeneration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for InstallationGeneration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <String as serde::Deserialize>::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
/// A public rotation token; it contains no password or secret material.
#[cfg(feature = "serialization")]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialRevision(String);
#[cfg(feature = "serialization")]
impl CredentialRevision {
    pub fn new(value: impl Into<String>) -> Result<Self, DeclarationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        {
            return Err(DeclarationError::new(
                "credential revision must be a nonempty ASCII identifier of at most 128 bytes",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[cfg(feature = "serialization")]
impl std::str::FromStr for CredentialRevision {
    type Err = DeclarationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
#[cfg(feature = "serialization")]
impl std::fmt::Display for CredentialRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl serde::Serialize for CredentialRevision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
#[cfg(feature = "serialization")]
impl<'de> serde::Deserialize<'de> for CredentialRevision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(<String as serde::Deserialize>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

/// Generation and full preparation digest; independent of serialization and execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparationKey {
    generation: InstallationGeneration,
    identity: String,
}
impl PreparationKey {
    pub fn new(
        generation: InstallationGeneration,
        identity: impl Into<String>,
    ) -> Result<Self, crate::DeclarationError> {
        let identity = identity.into();
        if identity.len() != 64
            || !identity
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(crate::DeclarationError::new(
                "preparation identity must be a lowercase SHA-256 digest",
            ));
        }
        Ok(Self {
            generation,
            identity,
        })
    }
    pub fn generation(&self) -> InstallationGeneration {
        self.generation
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
}
