//! The Redap address profile emitted by Recording, independent of Rerun runtimes.

use std::{cell::Cell, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::{RecordingContractError, RecordingDatasetId, RecordingId, ids::string_schema};

/// A public HTTP(S) origin represented by the corresponding Redap scheme and port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingRedapOrigin(Url);

impl RecordingRedapOrigin {
    pub fn from_http(value: &str) -> Result<Self, RecordingContractError> {
        let url = parse_url(value)?;
        let scheme = match url.scheme() {
            "http" => "rerun+http",
            "https" => "rerun",
            _ => return Err(RecordingContractError::RedapAddress),
        };
        if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
            return Err(RecordingContractError::RedapAddress);
        }
        Self::from_components(
            scheme,
            network_host(&url)?,
            url.port_or_known_default()
                .ok_or(RecordingContractError::RedapAddress)?,
        )
    }

    fn from_components(
        scheme: &str,
        host: Host<String>,
        port: u16,
    ) -> Result<Self, RecordingContractError> {
        if port == 0 {
            return Err(RecordingContractError::RedapAddress);
        }
        let loopback = match &host {
            Host::Domain(domain) => domain == "localhost",
            Host::Ipv4(ip) => ip.is_loopback(),
            Host::Ipv6(ip) => ip.is_loopback(),
        };
        // Rerun 0.38.1's HTTP rewrite drops the HTTP(S) default port before its
        // localhost default is chosen, even when the input explicitly names it.
        if loopback && matches!((scheme, port), ("rerun+http", 80) | ("rerun", 443)) {
            return Err(RecordingContractError::RedapLoopbackPort);
        }
        // Both bases are custom schemes. URL setters own host, IPv6 and port encoding.
        let base = match scheme {
            "rerun" => "rerun://host",
            "rerun+http" => "rerun+http://host",
            _ => return Err(RecordingContractError::RedapAddress),
        };
        let mut url = Url::parse(base).expect("declared Redap base");
        url.set_host(Some(&host.to_string()))
            .map_err(|_| RecordingContractError::RedapAddress)?;
        url.set_port(Some(port))
            .map_err(|_| RecordingContractError::RedapAddress)?;
        Ok(Self(url))
    }

    fn from_redap_url(url: &Url) -> Result<Self, RecordingContractError> {
        Self::from_components(
            url.scheme(),
            network_host(url)?,
            url.port().ok_or(RecordingContractError::RedapAddress)?,
        )
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn endpoint(&self, route: &str, dataset_id: RecordingDatasetId) -> Url {
        let mut url = self.0.clone();
        url.path_segments_mut()
            .expect("admitted hierarchical origin")
            .clear()
            .extend([route, &dataset_entry(dataset_id)]);
        url
    }
}

fn parse_url(value: &str) -> Result<Url, RecordingContractError> {
    let violation = Cell::new(false);
    let url = Url::options()
        .syntax_violation_callback(Some(&|_| violation.set(true)))
        .parse(value)
        .map_err(|_| RecordingContractError::RedapAddress)?;
    if violation.get()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(RecordingContractError::RedapAddress);
    }
    Ok(url)
}

fn network_host(url: &Url) -> Result<Host<String>, RecordingContractError> {
    let supplied = url.host_str().ok_or(RecordingContractError::RedapAddress)?;
    let host = Host::parse(supplied).map_err(|_| RecordingContractError::RedapAddress)?;
    if supplied != host.to_string()
        || matches!(&host, Host::Ipv4(ip) if ip.is_unspecified())
        || matches!(&host, Host::Ipv6(ip) if ip.is_unspecified())
    {
        return Err(RecordingContractError::RedapAddress);
    }
    Ok(host)
}

// Rerun 0.38.1 TUID display uses the same UUID bytes: uppercase high 64 bits,
// lowercase low 64 bits. Runtime qualification compares these outputs with re_uri.
fn dataset_entry(dataset_id: RecordingDatasetId) -> String {
    let (high, low) = dataset_id.as_uuid().as_u64_pair();
    format!("{high:016X}{low:016x}")
}

fn parse_dataset_entry(value: &str) -> Result<RecordingDatasetId, RecordingContractError> {
    let id = uuid::Uuid::parse_str(value)
        .ok()
        .and_then(|id| RecordingDatasetId::try_from(id).ok())
        .ok_or(RecordingContractError::RedapAddress)?;
    if value != dataset_entry(id) {
        return Err(RecordingContractError::RedapAddress);
    }
    Ok(id)
}

/// The dataset entry used by a catalog grant. Construction requires the dataset owner type.
/// ```compile_fail
/// use veoveo_recording_contract::{RecordingCatalogUri, RecordingId, RecordingRedapOrigin};
/// let origin = RecordingRedapOrigin::from_http("https://example.com").unwrap();
/// RecordingCatalogUri::new(&origin, RecordingId::new());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordingCatalogUri {
    wire: Url,
    dataset_id: RecordingDatasetId,
}

impl RecordingCatalogUri {
    pub fn new(origin: &RecordingRedapOrigin, dataset_id: RecordingDatasetId) -> Self {
        Self {
            wire: origin.endpoint("entry", dataset_id),
            dataset_id,
        }
    }

    pub fn parse(value: &str) -> Result<Self, RecordingContractError> {
        let url = parse_url(value)?;
        let origin = RecordingRedapOrigin::from_redap_url(&url)?;
        let path = url
            .path_segments()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let ["entry", dataset] = path.as_slice() else {
            return Err(RecordingContractError::RedapAddress);
        };
        let result = Self::new(&origin, parse_dataset_entry(dataset)?);
        if url.query().is_some() || result.as_str() != value {
            return Err(RecordingContractError::RedapAddress);
        }
        Ok(result)
    }

    pub fn dataset_id(&self) -> RecordingDatasetId {
        self.dataset_id
    }
}

/// One Recording segment in a Redap dataset, without extra selection parameters.
/// ```compile_fail
/// use veoveo_recording_contract::{PlaybackArchiveUri, RecordingId, RecordingRedapOrigin};
/// let origin = RecordingRedapOrigin::from_http("https://example.com").unwrap();
/// PlaybackArchiveUri::new(&origin, RecordingId::new(), RecordingId::new());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PlaybackArchiveUri {
    wire: Url,
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
}

impl PlaybackArchiveUri {
    pub fn new(
        origin: &RecordingRedapOrigin,
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
    ) -> Self {
        let mut wire = origin.endpoint("dataset", dataset_id);
        wire.query_pairs_mut()
            .append_pair("segment_id", &recording_id.to_string());
        Self {
            wire,
            dataset_id,
            recording_id,
        }
    }

    pub fn parse(value: &str) -> Result<Self, RecordingContractError> {
        let url = parse_url(value)?;
        let origin = RecordingRedapOrigin::from_redap_url(&url)?;
        let path = url
            .path_segments()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let ["dataset", dataset] = path.as_slice() else {
            return Err(RecordingContractError::RedapAddress);
        };
        let mut query = url.query_pairs();
        let Some((name, recording)) = query.next() else {
            return Err(RecordingContractError::RedapAddress);
        };
        if name != "segment_id" || query.next().is_some() {
            return Err(RecordingContractError::RedapAddress);
        }
        let result = Self::new(
            &origin,
            parse_dataset_entry(dataset)?,
            RecordingId::parse(&recording).map_err(|_| RecordingContractError::RedapAddress)?,
        );
        if result.as_str() != value {
            return Err(RecordingContractError::RedapAddress);
        }
        Ok(result)
    }

    pub fn dataset_id(&self) -> RecordingDatasetId {
        self.dataset_id
    }

    pub fn recording_id(&self) -> RecordingId {
        self.recording_id
    }
}

macro_rules! wire_address {
    ($name:ident) => {
        impl $name {
            pub fn as_str(&self) -> &str {
                self.wire.as_str()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
        impl FromStr for $name {
            type Err = RecordingContractError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
        impl TryFrom<String> for $name {
            type Error = RecordingContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire.into()
            }
        }
        string_schema!($name);
    };
}
wire_address!(RecordingCatalogUri);
wire_address!(PlaybackArchiveUri);
