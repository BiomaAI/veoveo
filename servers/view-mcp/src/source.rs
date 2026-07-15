use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use futures::StreamExt;
use parking_lot::Mutex;
use reqwest::{StatusCode, header};
use schemars::JsonSchema;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    cache::WeightedLru,
    contract::LayerId,
    tiles::schema::{Tile, Tileset},
};

pub const GOOGLE_P3DT_ROOT_URL: &str = "https://tile.googleapis.com/v1/3dtiles/root.json";

#[derive(Debug, Clone, Deserialize)]
pub struct LayerCatalogFile {
    pub layers: Vec<LayerDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LayerDefinition {
    pub layer_id: LayerId,
    pub label: String,
    pub source: LayerSourceDefinition,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerSourceDefinition {
    GooglePhotorealistic {
        #[serde(default = "default_google_root")]
        root_url: String,
        #[serde(default = "default_google_key_env")]
        api_key_env: String,
        #[serde(default = "default_request_cap")]
        daily_request_cap: u32,
    },
    HttpsTileset {
        root_url: String,
    },
    LocalTileset {
        root_path: PathBuf,
    },
}

fn default_google_root() -> String {
    GOOGLE_P3DT_ROOT_URL.to_owned()
}

fn default_google_key_env() -> String {
    "GOOGLE_MAPS_API_KEY".to_owned()
}

fn default_request_cap() -> u32 {
    2_000
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LayerSummary {
    pub layer_id: LayerId,
    pub label: String,
    pub source_kind: String,
}

#[derive(Clone)]
pub struct LayerCatalog {
    layers: Arc<BTreeMap<LayerId, Arc<TileSource>>>,
    summaries: Arc<Vec<LayerSummary>>,
}

impl LayerCatalog {
    pub fn from_definitions(
        definitions: Vec<LayerDefinition>,
        config: SourceConfig,
    ) -> Result<Self, SourceError> {
        let mut layers = BTreeMap::new();
        let mut summaries = Vec::new();
        for definition in definitions {
            if layers.contains_key(&definition.layer_id) {
                return Err(SourceError::DuplicateLayer(definition.layer_id));
            }
            let (source, source_kind) = TileSource::new(&definition, config.clone())?;
            summaries.push(LayerSummary {
                layer_id: definition.layer_id.clone(),
                label: definition.label,
                source_kind,
            });
            layers.insert(definition.layer_id, Arc::new(source));
        }
        Ok(Self {
            layers: Arc::new(layers),
            summaries: Arc::new(summaries),
        })
    }

    pub fn get(&self, layer_id: &LayerId) -> Option<Arc<TileSource>> {
        self.layers.get(layer_id).cloned()
    }

    pub fn summaries(&self) -> &[LayerSummary] {
        &self.summaries
    }
}

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub raw_cache_bytes: u64,
    pub max_response_bytes: u64,
    pub request_timeout: Duration,
}

#[derive(Debug)]
enum SourceKind {
    Google {
        root: Url,
        origin: Url,
        api_key: SecretString,
        session: Mutex<Option<String>>,
        budget: Mutex<DailyBudget>,
    },
    Https {
        root: Url,
        host: String,
    },
    Local {
        root: PathBuf,
    },
}

#[derive(Debug)]
struct DailyBudget {
    day: u64,
    used: u32,
    cap: u32,
}

impl DailyBudget {
    fn consume(&mut self) -> Result<(), SourceError> {
        let day = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86_400;
        if self.day != day {
            self.day = day;
            self.used = 0;
        }
        if self.used >= self.cap {
            return Err(SourceError::RequestBudgetExhausted);
        }
        self.used += 1;
        Ok(())
    }
}

#[derive(Debug)]
pub struct TileSource {
    layer_id: LayerId,
    kind: SourceKind,
    http: reqwest::Client,
    raw_cache: Mutex<WeightedLru<String, CachedResponse>>,
    config: SourceConfig,
}

#[derive(Debug)]
struct CachedResponse {
    bytes: Arc<Vec<u8>>,
    etag: Option<String>,
    expires_at: Instant,
    cacheable: bool,
}

#[derive(Debug, Clone)]
pub struct SourceBytes {
    pub bytes: Arc<Vec<u8>>,
    pub location: String,
    pub cache_hit: bool,
}

impl TileSource {
    fn new(
        definition: &LayerDefinition,
        config: SourceConfig,
    ) -> Result<(Self, String), SourceError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(config.request_timeout)
            .user_agent("veoveo-view-mcp/0.1")
            .build()?;
        let (kind, source_kind) = match &definition.source {
            LayerSourceDefinition::GooglePhotorealistic {
                root_url,
                api_key_env,
                daily_request_cap,
            } => {
                let root = validate_https_url(root_url)?;
                if root.host_str() != Some("tile.googleapis.com") {
                    return Err(SourceError::InvalidGoogleHost);
                }
                let mut origin = root.clone();
                origin.set_path("/");
                origin.set_query(None);
                origin.set_fragment(None);
                let api_key = std::env::var(api_key_env)
                    .ok()
                    .filter(|value| !value.is_empty())
                    .map(SecretString::from)
                    .ok_or_else(|| SourceError::MissingCredential(api_key_env.clone()))?;
                (
                    SourceKind::Google {
                        root,
                        origin,
                        api_key,
                        session: Mutex::new(None),
                        budget: Mutex::new(DailyBudget {
                            day: 0,
                            used: 0,
                            cap: *daily_request_cap,
                        }),
                    },
                    "google_photorealistic".to_owned(),
                )
            }
            LayerSourceDefinition::HttpsTileset { root_url } => {
                let root = validate_https_url(root_url)?;
                let host = root.host_str().ok_or(SourceError::InvalidUrl)?.to_owned();
                (SourceKind::Https { root, host }, "https_tileset".to_owned())
            }
            LayerSourceDefinition::LocalTileset { root_path } => {
                if !root_path.is_absolute() {
                    return Err(SourceError::LocalRootMustBeAbsolute);
                }
                (
                    SourceKind::Local {
                        root: root_path.clone(),
                    },
                    "local_tileset".to_owned(),
                )
            }
        };
        Ok((
            Self {
                layer_id: definition.layer_id.clone(),
                kind,
                http,
                raw_cache: Mutex::new(WeightedLru::new(config.raw_cache_bytes)),
                config,
            },
            source_kind,
        ))
    }

    pub fn layer_id(&self) -> &LayerId {
        &self.layer_id
    }

    pub async fn load_root(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(Tileset, SourceBytes), SourceError> {
        let location = match &self.kind {
            SourceKind::Google { root, .. } | SourceKind::Https { root, .. } => root.to_string(),
            SourceKind::Local { root } => root.to_string_lossy().into_owned(),
        };
        let response = self.fetch(&location, cancellation).await?;
        let mut tileset = crate::tiles::schema::parse(&response.bytes)?;
        self.adopt_google_session(&tileset);
        self.qualify_tileset_uris(&mut tileset, &response.location)?;
        Ok((tileset, response))
    }

    pub async fn load_content(
        &self,
        location: &str,
        cancellation: &CancellationToken,
    ) -> Result<SourceBytes, SourceError> {
        self.fetch(location, cancellation).await
    }

    pub fn parse_external_tileset(
        &self,
        bytes: &[u8],
        location: &str,
    ) -> Result<Tileset, SourceError> {
        let mut tileset = crate::tiles::schema::parse(bytes)?;
        self.adopt_google_session(&tileset);
        self.qualify_tileset_uris(&mut tileset, location)?;
        Ok(tileset)
    }

    fn qualify_tileset_uris(
        &self,
        tileset: &mut Tileset,
        document_location: &str,
    ) -> Result<(), SourceError> {
        fn qualify(tile: &mut Tile, base: &Url) -> Result<(), SourceError> {
            if let Some(uri) = tile.content_uri_mut() {
                *uri = base
                    .join(uri)
                    .map_err(|_| SourceError::InvalidContentUri)?
                    .to_string();
            }
            for child in &mut tile.children {
                qualify(child, base)?;
            }
            Ok(())
        }

        let base = match &self.kind {
            SourceKind::Local { root } => {
                let path = Url::parse(document_location)
                    .ok()
                    .filter(|url| url.scheme() == "file")
                    .and_then(|url| url.to_file_path().ok())
                    .unwrap_or_else(|| {
                        let document = Path::new(document_location);
                        if document.is_absolute() {
                            document.to_owned()
                        } else {
                            root.join(document)
                        }
                    });
                Url::from_file_path(path).map_err(|_| SourceError::InvalidContentUri)?
            }
            _ => Url::parse(document_location).map_err(|_| SourceError::InvalidContentUri)?,
        };
        qualify(&mut tileset.root, &base)
    }

    fn adopt_google_session(&self, tileset: &Tileset) {
        let SourceKind::Google { session, .. } = &self.kind else {
            return;
        };
        fn find(tile: &Tile) -> Option<String> {
            tile.content_uri()
                .and_then(|uri| {
                    Url::parse(uri)
                        .ok()
                        .or_else(|| Url::parse("https://x.invalid/").ok()?.join(uri).ok())
                })
                .and_then(|url| {
                    url.query_pairs()
                        .find(|(key, _)| key == "session")
                        .map(|(_, value)| value.into_owned())
                })
                .or_else(|| tile.children.iter().find_map(find))
        }
        if let Some(value) = find(&tileset.root) {
            *session.lock() = Some(value);
        }
    }

    async fn fetch(
        &self,
        location: &str,
        cancellation: &CancellationToken,
    ) -> Result<SourceBytes, SourceError> {
        let request_url = self.request_url(location)?;
        let cache_key = cache_key(&request_url);
        let cached = self.raw_cache.lock().get(&cache_key);
        if let Some(cached) = cached.as_ref()
            && cached.cacheable
            && cached.expires_at > Instant::now()
        {
            return Ok(SourceBytes {
                bytes: cached.bytes.clone(),
                location: request_url.to_string(),
                cache_hit: true,
            });
        }

        if let SourceKind::Local { .. } = &self.kind {
            let path = request_url
                .to_file_path()
                .map_err(|_| SourceError::InvalidContentUri)?;
            let bytes = tokio::select! {
                () = cancellation.cancelled() => return Err(SourceError::Cancelled),
                bytes = tokio::fs::read(path) => bytes?,
            };
            if bytes.len() as u64 > self.config.max_response_bytes {
                return Err(SourceError::ResponseTooLarge);
            }
            let bytes = Arc::new(bytes);
            self.raw_cache.lock().insert(
                cache_key,
                Arc::new(CachedResponse {
                    bytes: bytes.clone(),
                    etag: None,
                    expires_at: Instant::now() + Duration::from_secs(86_400),
                    cacheable: true,
                }),
                bytes.len() as u64,
            );
            return Ok(SourceBytes {
                bytes,
                location: request_url.to_string(),
                cache_hit: false,
            });
        }

        if let SourceKind::Google { budget, .. } = &self.kind {
            budget.lock().consume()?;
        }
        let mut request = self.http.get(request_url.clone());
        if let Some(cached) = cached.as_ref()
            && let Some(etag) = &cached.etag
        {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let response = tokio::select! {
            () = cancellation.cancelled() => return Err(SourceError::Cancelled),
            response = request.send() => response?,
        };
        if response.status() == StatusCode::NOT_MODIFIED
            && let Some(cached) = cached
        {
            let freshness = freshness(response.headers());
            let replacement = Arc::new(CachedResponse {
                bytes: cached.bytes.clone(),
                etag: cached.etag.clone(),
                expires_at: Instant::now() + freshness.max_age,
                cacheable: freshness.cacheable,
            });
            self.raw_cache
                .lock()
                .insert(cache_key, replacement, cached.bytes.len() as u64);
            return Ok(SourceBytes {
                bytes: cached.bytes.clone(),
                location: request_url.to_string(),
                cache_hit: true,
            });
        }
        if response.status().is_redirection() {
            return Err(SourceError::RedirectRejected);
        }
        let response = response.error_for_status()?;
        let freshness = freshness(response.headers());
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        loop {
            let next = tokio::select! {
                () = cancellation.cancelled() => return Err(SourceError::Cancelled),
                next = stream.next() => next,
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) as u64 > self.config.max_response_bytes {
                return Err(SourceError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let bytes = Arc::new(bytes);
        if freshness.cacheable {
            self.raw_cache.lock().insert(
                cache_key,
                Arc::new(CachedResponse {
                    bytes: bytes.clone(),
                    etag,
                    expires_at: Instant::now() + freshness.max_age,
                    cacheable: true,
                }),
                bytes.len() as u64,
            );
        }
        Ok(SourceBytes {
            bytes,
            location: request_url.to_string(),
            cache_hit: false,
        })
    }

    fn request_url(&self, location: &str) -> Result<Url, SourceError> {
        match &self.kind {
            SourceKind::Google {
                origin,
                api_key,
                session,
                ..
            } => {
                let mut url = Url::parse(location)
                    .or_else(|_| origin.join(location))
                    .map_err(|_| SourceError::InvalidContentUri)?;
                if url.host_str() != origin.host_str() || url.scheme() != "https" {
                    return Err(SourceError::SourceHostRejected);
                }
                let has_key = url.query_pairs().any(|(key, _)| key == "key");
                let has_session = url.query_pairs().any(|(key, _)| key == "session");
                {
                    let mut pairs = url.query_pairs_mut();
                    if !has_key {
                        pairs.append_pair("key", api_key.expose_secret());
                    }
                    if !has_session && let Some(session) = session.lock().as_deref() {
                        pairs.append_pair("session", session);
                    }
                }
                Ok(url)
            }
            SourceKind::Https { root, host } => {
                let url = Url::parse(location)
                    .or_else(|_| root.join(location))
                    .map_err(|_| SourceError::InvalidContentUri)?;
                if url.scheme() != "https" || url.host_str() != Some(host) {
                    return Err(SourceError::SourceHostRejected);
                }
                Ok(url)
            }
            SourceKind::Local { root } => {
                if let Ok(url) = Url::parse(location)
                    && url.scheme() == "file"
                {
                    let path = url
                        .to_file_path()
                        .map_err(|_| SourceError::InvalidContentUri)?;
                    if !path.starts_with(root.parent().unwrap_or(root)) {
                        return Err(SourceError::LocalPathEscapesRoot);
                    }
                    return Ok(url);
                }
                let path = if Path::new(location).is_absolute() {
                    PathBuf::from(location)
                } else {
                    root.parent().unwrap_or(root).join(location)
                };
                if !path.starts_with(root.parent().unwrap_or(root)) {
                    return Err(SourceError::LocalPathEscapesRoot);
                }
                Url::from_file_path(path).map_err(|_| SourceError::InvalidContentUri)
            }
        }
    }
}

fn validate_https_url(value: &str) -> Result<Url, SourceError> {
    let url = Url::parse(value).map_err(|_| SourceError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(SourceError::InvalidUrl);
    }
    Ok(url)
}

fn cache_key(url: &Url) -> String {
    let mut clean = url.clone();
    let pairs: Vec<_> = clean
        .query_pairs()
        .filter(|(key, _)| key != "key")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    clean.set_query(None);
    if !pairs.is_empty() {
        clean.query_pairs_mut().extend_pairs(pairs);
    }
    clean.to_string()
}

struct Freshness {
    max_age: Duration,
    cacheable: bool,
}

fn freshness(headers: &header::HeaderMap) -> Freshness {
    let cache_control = headers
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let directives: HashMap<_, _> = cache_control
        .split(',')
        .map(str::trim)
        .map(|directive| directive.split_once('=').unwrap_or((directive, "")))
        .collect();
    let cacheable = !directives.contains_key("no-store");
    let seconds = directives
        .get("max-age")
        .and_then(|value| value.trim_matches('"').parse::<u64>().ok())
        .unwrap_or(0);
    Freshness {
        max_age: Duration::from_secs(seconds),
        cacheable,
    }
}

pub fn looks_like_tileset(bytes: &[u8]) -> bool {
    let first = bytes.iter().find(|byte| !byte.is_ascii_whitespace());
    first == Some(&b'{')
        && bytes
            .windows(b"\"geometricError\"".len())
            .any(|window| window == b"\"geometricError\"")
        && bytes
            .windows(b"\"root\"".len())
            .any(|window| window == b"\"root\"")
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("layer `{0}` is defined more than once")]
    DuplicateLayer(LayerId),
    #[error("source URL must be an absolute HTTPS URL without credentials or fragment")]
    InvalidUrl,
    #[error("Google Photorealistic source must use tile.googleapis.com")]
    InvalidGoogleHost,
    #[error("source credential environment `{0}` is missing")]
    MissingCredential(String),
    #[error("local tileset root must be absolute")]
    LocalRootMustBeAbsolute,
    #[error("content URI is invalid")]
    InvalidContentUri,
    #[error("content source host was rejected")]
    SourceHostRejected,
    #[error("local content path escapes its configured root")]
    LocalPathEscapesRoot,
    #[error("source redirect was rejected")]
    RedirectRejected,
    #[error("source response exceeds the configured byte limit")]
    ResponseTooLarge,
    #[error("source request was cancelled")]
    Cancelled,
    #[error("source daily request budget is exhausted")]
    RequestBudgetExhausted,
    #[error("HTTP source failed: {0}")]
    Http(String),
    #[error("local source failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("tileset failed validation: {0}")]
    Schema(#[from] crate::tiles::schema::SchemaError),
}

impl From<reqwest::Error> for SourceError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.without_url().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_never_contains_google_key() {
        let url = Url::parse("https://tile.googleapis.com/x.glb?session=S&key=SECRET").unwrap();
        let key = cache_key(&url);
        assert_eq!(key, "https://tile.googleapis.com/x.glb?session=S");
        assert!(!key.contains("SECRET"));
    }

    #[test]
    fn freshness_honors_no_store_and_max_age() {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("private, max-age=300"),
        );
        let result = freshness(&headers);
        assert!(result.cacheable);
        assert_eq!(result.max_age, Duration::from_secs(300));

        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        assert!(!freshness(&headers).cacheable);
    }

    #[test]
    fn tileset_detection_rejects_glb() {
        assert!(looks_like_tileset(br#"{"geometricError":1,"root":{}}"#));
        assert!(!looks_like_tileset(b"glTF...."));
    }

    #[tokio::test]
    async fn local_root_qualifies_sibling_content_from_file_url() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("tileset.json");
        tokio::fs::write(
            &root,
            br#"{
              "asset":{"version":"1.1"},
              "geometricError":0,
              "root":{
                "boundingVolume":{"sphere":[0,0,0,1]},
                "geometricError":0,
                "content":{"uri":"tile.glb"}
              }
            }"#,
        )
        .await
        .unwrap();
        tokio::fs::write(directory.path().join("tile.glb"), b"glTF")
            .await
            .unwrap();
        let layer_id = LayerId::new("local-test").unwrap();
        let catalog = LayerCatalog::from_definitions(
            vec![LayerDefinition {
                layer_id: layer_id.clone(),
                label: "local".to_owned(),
                source: LayerSourceDefinition::LocalTileset { root_path: root },
            }],
            SourceConfig {
                raw_cache_bytes: 1_024,
                max_response_bytes: 1_024,
                request_timeout: Duration::from_secs(1),
            },
        )
        .unwrap();
        let source = catalog.get(&layer_id).unwrap();
        let (tileset, _) = source.load_root(&CancellationToken::new()).await.unwrap();
        let content_uri = tileset.root.content_uri().unwrap();
        assert_eq!(
            Url::parse(content_uri).unwrap().to_file_path().unwrap(),
            directory.path().join("tile.glb")
        );
        assert_eq!(
            source
                .load_content(content_uri, &CancellationToken::new())
                .await
                .unwrap()
                .bytes
                .as_slice(),
            b"glTF"
        );
    }
}
