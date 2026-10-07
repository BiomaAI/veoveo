//! Checked reader options shared by source producers and materializers.
use std::{collections::BTreeMap, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

const RESERVED_NAMES: &[&str] = &[
    "header",
    "delim",
    "delimiter",
    "sep",
    "timestampformat",
    "timestamp_format",
];

/// A canonical extra-option name. Explicit fields and their aliases have one owner.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbReadOptionName(String);

impl DuckDbReadOptionName {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbReadOptionsError> {
        let value = value.into();
        if !value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
            || !value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(DuckDbReadOptionsError::Name);
        }
        if RESERVED_NAMES.contains(&value.as_str()) {
            return Err(DuckDbReadOptionsError::ReservedName);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for DuckDbReadOptionName {
    type Err = DuckDbReadOptionsError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl TryFrom<String> for DuckDbReadOptionName {
    type Error = DuckDbReadOptionsError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<DuckDbReadOptionName> for String {
    fn from(value: DuckDbReadOptionName) -> Self {
        value.0
    }
}
impl JsonSchema for DuckDbReadOptionName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbReadOptionName".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let schema = schemars::json_schema!({
            "type":"string", "pattern":"^[a-z_]",
            "not":{"anyOf":[{"enum":RESERVED_NAMES},{"pattern":"[^a-z0-9_]"}]},
            "description":"Lowercase reader option name; use explicit fields for header, delimiter and timestamp format."
        });
        use veoveo_types::naming::{
            NamingAuthority, NamingDeclaration, NamingLabel, ScalarNaming, scalar_schema,
        };
        scalar_schema(
            schema,
            ScalarNaming::Standard {
                declaration: NamingDeclaration {
                    authority: NamingAuthority::Standard {
                        document: veoveo_types::HttpsUrl::parse(
                            "https://duckdb.org/docs/stable/data/csv/overview.html",
                        )
                        .expect("declared upstream URL"),
                    },
                    profile: NamingLabel::new("DuckDB reader option names")
                        .expect("declared scalar profile"),
                    version: NamingLabel::new("1.5.6").expect("pinned reader version"),
                    applicability: NamingLabel::new("only extra reader-option dictionary names")
                        .expect("declared scalar applicability"),
                },
            },
        )
        .expect("upstream reader option scalar schema")
    }
}

/// Reader arguments have a closed value shape; the engine owns option-specific meaning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DuckDbReadOptionValue {
    Bool(bool),
    Number(serde_json::Number),
    String(DuckDbReadOptionText),
    Array(Vec<Self>),
}
/// Text passed as a reader argument, including empty strings and whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbReadOptionText(String);
impl DuckDbReadOptionText {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbReadOptionsError> {
        let value = value.into();
        check_text(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for DuckDbReadOptionText {
    type Error = DuckDbReadOptionsError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl FromStr for DuckDbReadOptionText {
    type Err = DuckDbReadOptionsError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl From<DuckDbReadOptionText> for String {
    fn from(value: DuckDbReadOptionText) -> Self {
        value.0
    }
}
impl JsonSchema for DuckDbReadOptionText {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbReadOptionText".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"string","pattern":"^[^\\u0000]*$"})
    }
}

/// Every constructed options value can be rendered without a late shape check.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::{DuckDbReadOptions, DuckDbReadOptionValue};
/// DuckDbReadOptions::default().with_extra("nullstr", DuckDbReadOptionValue::Bool(true));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(from = "ReadOptionsWire", into = "ReadOptionsWire")]
pub struct DuckDbReadOptions(ReadOptionsWire);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct ReadOptionsWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delimiter: Option<DuckDbReadOptionText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp_format: Option<DuckDbReadOptionText>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_options"
    )]
    #[schemars(transform = extra_option_names)]
    extra: BTreeMap<DuckDbReadOptionName, DuckDbReadOptionValue>,
}

impl DuckDbReadOptions {
    pub fn with_header(mut self, header: bool) -> Self {
        self.0.header = Some(header);
        self
    }
    pub fn with_delimiter(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, DuckDbReadOptionsError> {
        let value = DuckDbReadOptionText::new(value)?;
        self.0.delimiter = Some(value);
        Ok(self)
    }
    pub fn with_timestamp_format(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, DuckDbReadOptionsError> {
        let value = DuckDbReadOptionText::new(value)?;
        self.0.timestamp_format = Some(value);
        Ok(self)
    }
    pub fn with_extra(
        mut self,
        name: DuckDbReadOptionName,
        value: DuckDbReadOptionValue,
    ) -> Result<Self, DuckDbReadOptionsError> {
        if self.0.extra.insert(name, value).is_some() {
            return Err(DuckDbReadOptionsError::DuplicateName);
        }
        Ok(self)
    }
    pub fn header(&self) -> Option<bool> {
        self.0.header
    }
    pub fn delimiter(&self) -> Option<&str> {
        self.0.delimiter.as_ref().map(DuckDbReadOptionText::as_str)
    }
    pub fn timestamp_format(&self) -> Option<&str> {
        self.0
            .timestamp_format
            .as_ref()
            .map(DuckDbReadOptionText::as_str)
    }
    pub fn extra(&self) -> &BTreeMap<DuckDbReadOptionName, DuckDbReadOptionValue> {
        &self.0.extra
    }
}
impl From<ReadOptionsWire> for DuckDbReadOptions {
    fn from(wire: ReadOptionsWire) -> Self {
        Self(wire)
    }
}
impl From<DuckDbReadOptions> for ReadOptionsWire {
    fn from(value: DuckDbReadOptions) -> Self {
        value.0
    }
}

fn check_text(value: &str) -> Result<(), DuckDbReadOptionsError> {
    if value.contains('\0') {
        return Err(DuckDbReadOptionsError::Nul);
    }
    Ok(())
}
fn unique_options<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<DuckDbReadOptionName, DuckDbReadOptionValue>, D::Error> {
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = BTreeMap<DuckDbReadOptionName, DuckDbReadOptionValue>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unique reader option names and scalar or array values")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut options = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                if options.insert(key, value).is_some() {
                    return Err(de::Error::custom(DuckDbReadOptionsError::DuplicateName));
                }
            }
            Ok(options)
        }
    }
    deserializer.deserialize_map(Visitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDbReadOptionsError {
    Name,
    ReservedName,
    DuplicateName,
    Nul,
}
impl fmt::Display for DuckDbReadOptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "expected a lowercase ASCII reader option name beginning with a letter or underscore",
            Self::ReservedName => "use the explicit header, delimiter or timestampFormat field instead of an extra option alias",
            Self::DuplicateName => "reader option names must be unique",
            Self::Nul => "reader option text must not contain NUL",
        })
    }
}
impl std::error::Error for DuckDbReadOptionsError {}

fn extra_option_names(schema: &mut schemars::Schema) {
    schema.insert(
        "propertyNames".into(),
        DuckDbReadOptionName::json_schema(&mut schemars::SchemaGenerator::default()).into(),
    );
}
