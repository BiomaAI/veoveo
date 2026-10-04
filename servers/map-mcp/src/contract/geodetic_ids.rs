use std::fmt;

fn validate_coordinate_id(value: &str) -> Result<(), GeodeticIdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(GeodeticIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(GeodeticIdError::new(
            value,
            "must not contain whitespace, slash, or URI separators",
        ));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        Ok(())
    } else {
        Err(GeodeticIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeodeticIdError {
    value: String,
    rule: &'static str,
}

impl GeodeticIdError {
    pub(super) fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for GeodeticIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for GeodeticIdError {}

#[doc = "Coordinate reference system id, commonly EPSG:4326."]
#[veoveo_types::id(text(GeodeticKeys))]
pub struct CrsId(String);
#[doc = "Geodetic datum id used by a CRS or coordinate operation."]
#[veoveo_types::id(text(GeodeticKeys))]
pub struct DatumId(String);
#[doc = "Reference ellipsoid id, such as WGS84 or GRS80."]
#[veoveo_types::id(text(GeodeticKeys))]
pub struct EllipsoidId(String);

#[doc(hidden)]
pub struct GeodeticKeys;
impl veoveo_types::IdProfile for GeodeticKeys {
    type Error = GeodeticIdError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_coordinate_id(value));
}
