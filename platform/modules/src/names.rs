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
