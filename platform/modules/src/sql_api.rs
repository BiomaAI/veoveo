//! Explicit leaf kernel SQL exports; no parser or driver in declarations.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlType {
    Bool,
    String,
    Datetime,
    Object,
    Record(TableName),
    Option(Box<SqlType>),
    Array(Box<SqlType>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlParameter {
    name: String,
    kind: SqlType,
}
impl SqlParameter {
    pub fn new(name: &str, kind: SqlType) -> Result<Self, DeclarationError> {
        check_field(name)?;
        Ok(Self {
            name: name.into(),
            kind,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> &SqlType {
        &self.kind
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlSignature {
    parameters: Vec<SqlParameter>,
    returns: SqlType,
}
impl SqlSignature {
    pub fn new(parameters: Vec<SqlParameter>, returns: SqlType) -> Result<Self, DeclarationError> {
        let mut names = std::collections::BTreeSet::new();
        if parameters.len() > 32 || parameters.iter().any(|p| !names.insert(p.name())) {
            return Err(DeclarationError::new(
                "SQL API parameters must be unique and at most 32",
            ));
        }
        Ok(Self {
            parameters,
            returns,
        })
    }
    pub fn parameters(&self) -> &[SqlParameter] {
        &self.parameters
    }
    pub fn returns(&self) -> &SqlType {
        &self.returns
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlReadProfile {
    tables: Vec<TableName>,
}
impl SqlReadProfile {
    pub fn new(tables: Vec<TableName>) -> Result<Self, DeclarationError> {
        let mut unique = std::collections::BTreeSet::new();
        if tables.is_empty() || tables.iter().any(|t| !unique.insert(t)) {
            return Err(DeclarationError::new(
                "SQL API read profile needs one or more unique owned tables",
            ));
        }
        Ok(Self { tables })
    }
    pub fn tables(&self) -> &[TableName] {
        &self.tables
    }
}
/// One admitted top-level field; nested paths and whole-record changes are excluded.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SqlFieldName(String);
impl SqlFieldName {
    pub fn new(name: &str) -> Result<Self, DeclarationError> {
        check_field(name)?;
        Ok(Self(name.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// The exact owned table and fields a leaf function may update through SET assignments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlUpdateProfile {
    table: TableName,
    fields: Vec<SqlFieldName>,
}
impl SqlUpdateProfile {
    pub fn new(table: TableName, fields: Vec<SqlFieldName>) -> Result<Self, DeclarationError> {
        let mut unique = std::collections::BTreeSet::new();
        if fields.is_empty() || fields.iter().any(|field| !unique.insert(field)) {
            return Err(DeclarationError::new(
                "SQL API update profile needs one or more unique fields",
            ));
        }
        Ok(Self { table, fields })
    }
    pub fn table(&self) -> &TableName {
        &self.table
    }
    pub fn fields(&self) -> &[SqlFieldName] {
        &self.fields
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlEffectProfile {
    ReadOnly,
    OwnedUpdate(SqlUpdateProfile),
}
/// A versioned leaf function with a declared effect profile. The runner qualifies the full exact definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelSqlApi {
    name: FunctionName,
    introduced: MigrationVersion,
    signature: SqlSignature,
    reads: SqlReadProfile,
    effects: SqlEffectProfile,
    definition: &'static str,
}
impl KernelSqlApi {
    pub fn new(
        name: FunctionName,
        introduced: MigrationVersion,
        signature: SqlSignature,
        reads: SqlReadProfile,
        definition: &'static str,
    ) -> Result<Self, DeclarationError> {
        let version = name
            .as_str()
            .rsplit_once("_v")
            .and_then(|(_, v)| v.parse::<u32>().ok());
        if !name.as_str().starts_with("fn::kernel::")
            || version.is_none_or(|v| v == 0 || !name.as_str().ends_with(&format!("_v{v}")))
            || definition.trim().is_empty()
            || definition.len() > 1_048_576
        {
            return Err(DeclarationError::new(
                "kernel SQL API needs a versioned exact function name and bounded definition",
            ));
        }
        Ok(Self {
            name,
            introduced,
            signature,
            reads,
            effects: SqlEffectProfile::ReadOnly,
            definition,
        })
    }
    pub fn with_effects(mut self, effects: SqlEffectProfile) -> Self {
        self.effects = effects;
        self
    }
    pub fn effects(&self) -> &SqlEffectProfile {
        &self.effects
    }
    pub fn name(&self) -> &FunctionName {
        &self.name
    }
    pub fn introduced(&self) -> MigrationVersion {
        self.introduced
    }
    pub fn signature(&self) -> &SqlSignature {
        &self.signature
    }
    pub fn reads(&self) -> &SqlReadProfile {
        &self.reads
    }
    pub fn definition(&self) -> &'static str {
        self.definition
    }
}
fn check_field(name: &str) -> Result<(), DeclarationError> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        || !name.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(DeclarationError::new(
            "SQL field or parameter must be a static lowercase identifier",
        ));
    }
    Ok(())
}
