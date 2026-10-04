use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{ArtifactError, invalid_identifier};

#[veoveo_types::id(text(ArtifactNames))]
#[schemars(!try_from,!into)]
pub struct ArtifactName(
    #[schemars(regex(
        pattern = r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*$"
    ))]
    String,
);

#[veoveo_types::id(text(ReleaseVersions))]
#[schemars(!try_from,!into)]
pub struct ReleaseVersion(
    #[schemars(regex(
        pattern = r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
    ))]
    String,
);

#[veoveo_types::id(prefixed(ArtifactDigests, "sha256:"))]
#[schemars(!try_from,!into)]
pub struct ArtifactDigest(#[schemars(regex(pattern = r"^sha256:[0-9a-f]{64}$"))] String);

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(!try_from, !into)]
pub struct ArtifactCoordinate(
    #[schemars(regex(pattern = r"^(?!.*:latest$)(?:oci|https|python|cargo)://[^\s#]+$"))]
    String,
);

impl ArtifactCoordinate {
    /// Constructs a validated value.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        validate_coordinate(&value)?;
        Ok(Self(value))
    }

    /// Returns the validated string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ArtifactCoordinate {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ArtifactCoordinate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ArtifactCoordinate {
    type Err = ArtifactError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for ArtifactCoordinate {
    type Error = ArtifactError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ArtifactCoordinate> for String {
    fn from(value: ArtifactCoordinate) -> Self {
        value.0
    }
}

#[veoveo_types::id(text(SourceRevisions))]
#[schemars(!try_from,!into)]
pub struct SourceRevision(#[schemars(regex(pattern = r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$"))] String);

#[doc(hidden)]
pub struct SourceRevisions;
impl veoveo_types::IdProfile for SourceRevisions {
    type Error = ArtifactError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_revision(value));
}

#[doc(hidden)]
pub struct ArtifactNames;
impl veoveo_types::IdProfile for ArtifactNames {
    type Error = ArtifactError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_dns_name(value));
}

#[doc(hidden)]
pub struct ReleaseVersions;
impl veoveo_types::IdProfile for ReleaseVersions {
    type Error = ArtifactError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_semver(value));
}

fn validate_dns_name(value: &str) -> Result<(), ArtifactError> {
    if value.is_empty() || value.len() > 253 {
        return Err(invalid_identifier(
            "DNS name",
            value,
            "must contain between 1 and 253 characters",
        ));
    }
    if value.split('.').any(|segment| {
        segment.is_empty()
            || segment.len() > 63
            || !segment.as_bytes()[0].is_ascii_alphanumeric()
            || !segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }) {
        return Err(invalid_identifier(
            "DNS name",
            value,
            "must be a lowercase DNS name",
        ));
    }
    Ok(())
}

fn validate_semver(value: &str) -> Result<(), ArtifactError> {
    semver::Version::parse(value).map(|_| ()).map_err(|_| {
        invalid_identifier(
            "semantic version",
            value,
            "must be Semantic Versioning 2.0.0",
        )
    })
}

fn validate_coordinate(value: &str) -> Result<(), ArtifactError> {
    let parsed = Url::parse(value).map_err(|_| {
        invalid_identifier(
            "artifact coordinate",
            value,
            "must be an absolute URL with a distribution scheme",
        )
    })?;
    if parsed.cannot_be_a_base()
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid_identifier(
            "artifact coordinate",
            value,
            "must have a host and must not contain credentials or a fragment",
        ));
    }
    if !matches!(parsed.scheme(), "oci" | "https" | "python" | "cargo") {
        return Err(invalid_identifier(
            "artifact coordinate",
            value,
            "scheme must be oci, https, python, or cargo",
        ));
    }
    if value.ends_with(":latest") {
        return Err(invalid_identifier(
            "artifact coordinate",
            value,
            "mutable latest tags are prohibited",
        ));
    }
    Ok(())
}

fn validate_revision(value: &str) -> Result<(), ArtifactError> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_identifier(
            "source revision",
            value,
            "must be a full lowercase SHA-1 or SHA-256 object id",
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub struct ArtifactDigests;
impl veoveo_types::IdProfile for ArtifactDigests {
    type Error = ArtifactError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> = veoveo_types::IdProfileSpec::hex(
        veoveo_types::HexGrammar {
            length: 64,
            case: veoveo_types::HexCase::Lower,
            nonzero: false,
        },
        |value, _, failure| {
            invalid_identifier(
                "artifact digest",
                value,
                if matches!(failure, veoveo_types::IdFailure::Prefix) {
                    "must start with sha256:"
                } else {
                    "must contain 64 lowercase hexadecimal digits"
                },
            )
        },
    );
}
