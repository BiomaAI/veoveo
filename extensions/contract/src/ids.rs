use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{ExtensionContractError, invalid_identifier};

macro_rules! typed_string {
    ($name:ident, $validator:ident, $pattern:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(!try_from, !into)]
        pub struct $name(#[schemars(regex(pattern = $pattern))] String);

        impl $name {
            /// Constructs a validated value.
            pub fn new(value: impl Into<String>) -> Result<Self, ExtensionContractError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            /// Returns the validated string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ExtensionContractError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ExtensionContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

typed_string!(
    ExtensionId,
    validate_dns_name,
    r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*$"
);
typed_string!(
    ArtifactName,
    validate_dns_name,
    r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*$"
);
typed_string!(
    CompatibilityReleaseId,
    validate_semver,
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
);
typed_string!(
    ReleaseVersion,
    validate_semver,
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
);
typed_string!(
    VersionRequirement,
    validate_version_requirement,
    r"^\S(?:.*\S)?$"
);
typed_string!(ArtifactDigest, validate_digest, r"^sha256:[0-9a-f]{64}$");
typed_string!(
    ArtifactCoordinate,
    validate_coordinate,
    r"^(?!.*:latest$)(?:oci|https|python|cargo)://[^\s#]+$"
);
typed_string!(
    SourceRevision,
    validate_revision,
    r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$"
);

fn validate_dns_name(value: &str) -> Result<(), ExtensionContractError> {
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

fn validate_semver(value: &str) -> Result<(), ExtensionContractError> {
    semver::Version::parse(value).map(|_| ()).map_err(|_| {
        invalid_identifier(
            "semantic version",
            value,
            "must be Semantic Versioning 2.0.0",
        )
    })
}

fn validate_version_requirement(value: &str) -> Result<(), ExtensionContractError> {
    semver::VersionReq::parse(value).map(|_| ()).map_err(|_| {
        invalid_identifier(
            "version requirement",
            value,
            "must be a valid semantic-version requirement",
        )
    })
}

fn validate_digest(value: &str) -> Result<(), ExtensionContractError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(invalid_identifier(
            "artifact digest",
            value,
            "must start with sha256:",
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_identifier(
            "artifact digest",
            value,
            "must contain 64 lowercase hexadecimal digits",
        ));
    }
    Ok(())
}

fn validate_coordinate(value: &str) -> Result<(), ExtensionContractError> {
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

fn validate_revision(value: &str) -> Result<(), ExtensionContractError> {
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
