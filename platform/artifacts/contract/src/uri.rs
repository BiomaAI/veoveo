use std::{fmt, str::FromStr, sync::LazyLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceScheme, ResourceUri, ResourceUriBuilder, ResourceUriError,
    ResourceUriParts, UriAuthority, UriSegment,
};

use crate::{ARTIFACT_PLANE_SCHEME, ArtifactId};

static PLANE_SCHEME: LazyLock<ResourceScheme> = LazyLock::new(|| {
    ResourceScheme::parse(ARTIFACT_PLANE_SCHEME).expect("declared Artifact scheme")
});

/// The Artifact occurrence addressed by a neutral or server-presented resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAddress {
    Plane(ArtifactId),
    Presented {
        scheme: ResourceScheme,
        artifact_id: ArtifactId,
    },
}

/// A validated Artifact address. Parsing preserves accepted wire spelling;
/// constructors emit lowercase hyphenated occurrence IDs.
/// ```compile_fail
/// use veoveo_artifact_contract::ArtifactUri;
/// ArtifactUri::plane("0197f78e-f2f0-7a6e-8a5d-f41c691e4471");
/// ```
/// ```compile_fail
/// use veoveo_artifact_contract::ArtifactUri;
/// use veoveo_types::WorkContextId;
/// ArtifactUri::plane(WorkContextId::parse("operations").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
/// ArtifactUri::presented("unchecked-scheme", ArtifactId::new());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(description = "Neutral or server-presented Artifact occurrence URI.")]
pub struct ArtifactUri {
    wire: ResourceUri,
    address: ArtifactAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactUriError {
    Components(ResourceUriError),
    Route,
    Occurrence,
}

impl fmt::Display for ArtifactUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Components(error) => error.fmt(f),
            Self::Route => f.write_str("expected a neutral or server-presented Artifact URI without escapes, query, or extra path segments"),
            Self::Occurrence => f.write_str("Artifact URI must name a UUIDv7 occurrence"),
        }
    }
}
impl std::error::Error for ArtifactUriError {}
impl From<ResourceUriError> for ArtifactUriError {
    fn from(value: ResourceUriError) -> Self {
        Self::Components(value)
    }
}

impl ArtifactUri {
    pub fn plane(artifact_id: ArtifactId) -> Self {
        let wire = ResourceUriBuilder::from_components(
            &PLANE_SCHEME,
            UriAuthority::new(artifact_id.to_string()).expect("UUID is an unescaped authority"),
        )
        .and_then(ResourceUriBuilder::build)
        .expect("typed Artifact plane address");
        Self {
            wire,
            address: ArtifactAddress::Plane(artifact_id),
        }
    }

    pub fn presented(scheme: &ResourceScheme, artifact_id: ArtifactId) -> Self {
        let wire = ResourceUriBuilder::from_components(
            scheme,
            UriAuthority::new("artifact").expect("declared Artifact authority"),
        )
        .expect("Artifact authority is valid for every resource scheme")
        .segment(UriSegment::new(artifact_id.to_string()).expect("UUID is a path segment"))
        .build()
        .expect("typed Artifact presentation");
        Self {
            wire,
            address: ArtifactAddress::Presented {
                scheme: scheme.clone(),
                artifact_id,
            },
        }
    }

    pub fn parse(value: &str) -> Result<Self, ArtifactUriError> {
        let parts = ResourceUriParts::parse(value)?;
        if parts.has_query() || value.contains('%') {
            return Err(ArtifactUriError::Route);
        }
        let scheme = ResourceScheme::parse(parts.scheme()).map_err(|_| ArtifactUriError::Route)?;
        let mut path = parts.path_segments();
        let address = match (path.next(), path.next()) {
            (None, None) if scheme == *PLANE_SCHEME => ArtifactAddress::Plane(
                ArtifactId::parse(parts.authority()).map_err(|_| ArtifactUriError::Occurrence)?,
            ),
            (Some(id), None) if parts.authority() == "artifact" => ArtifactAddress::Presented {
                scheme,
                artifact_id: ArtifactId::parse(id).map_err(|_| ArtifactUriError::Occurrence)?,
            },
            _ => return Err(ArtifactUriError::Route),
        };
        drop(path);
        Ok(Self {
            wire: parts.into_uri(),
            address,
        })
    }

    pub fn artifact_id(&self) -> ArtifactId {
        match &self.address {
            ArtifactAddress::Plane(id)
            | ArtifactAddress::Presented {
                artifact_id: id, ..
            } => *id,
        }
    }
    pub fn address(&self) -> &ArtifactAddress {
        &self.address
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
    pub fn as_resource_uri(&self) -> &ResourceUri {
        &self.wire
    }
}

impl ResourceAddress for ArtifactUri {
    type Error = ArtifactUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl fmt::Display for ArtifactUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.wire.fmt(f)
    }
}
impl AsRef<str> for ArtifactUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl FromStr for ArtifactUri {
    type Err = ArtifactUriError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for ArtifactUri {
    type Error = ArtifactUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<ArtifactUri> for String {
    fn from(value: ArtifactUri) -> Self {
        value.wire.into()
    }
}
