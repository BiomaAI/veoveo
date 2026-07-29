//! Recording-scoped Rerun Data Protocol playback.
//!
//! The durable Veoveo recording catalog remains authoritative. This module
//! derives a Rerun dataset from immutable shards, issues short-lived
//! recording-scoped read sessions, and exposes only the Redap methods required
//! by the viewer.

use std::{
    collections::{BTreeMap, HashMap},
    pin::Pin,
    str::FromStr as _,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, TimeDelta, Utc};
use futures::Stream;
use re_auth::{Claims, Jwt, Permission, RedapProvider, VerificationOptions};
use re_log_types::{EntryId, EntryName, StoreId, StoreKind};
use re_protos::{
    cloud::v1alpha1::{
        self as proto,
        ext::{CreateDatasetEntryRequest, RegisterWithDatasetDataframe},
        rerun_cloud_service_server::RerunCloudService,
    },
    common::v1alpha1::ext::IfDuplicateBehavior,
    headers::RerunHeadersInjectorExt as _,
};
use re_server::{RerunCloudHandler, RerunCloudHandlerBuilder};
use re_uri::{DatasetSegmentUri, Fragment, Origin};
use sha2::{Digest as _, Sha256};
use tokio::sync::Mutex as AsyncMutex;
use tonic::{Request, Response, Status};
use url::Url;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{RecordingId, RecordingState};

use crate::{
    RecordingPlaybackPlan,
    contract::{PlaybackAccess, PlaybackArchive, PlaybackManifest},
};

pub const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.io/recording-playback/v2";
pub const PLAYBACK_SESSION_HEADER: &str = "x-veoveo-playback-session";
const TOKEN_ISSUER: &str = "veoveo-recording-playback";
const SESSION_TTL: TimeDelta = TimeDelta::minutes(5);
const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);
const TOKEN_RENEW_WINDOW: TimeDelta = TimeDelta::minutes(10);
const MAX_SESSIONS: usize = 1_024;
const MAX_CATALOGS: usize = 64;

type BoxedStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

#[derive(Clone)]
pub struct PlaybackManager {
    inner: Arc<PlaybackManagerInner>,
}

struct PlaybackManagerInner {
    provider: RedapProvider,
    public_origin: Origin,
    allowed_host: String,
    sessions: Mutex<HashMap<String, PlaybackSession>>,
    catalogs: Mutex<HashMap<String, Arc<CatalogSlot>>>,
}

#[derive(Clone)]
struct PlaybackSession {
    recording_id: String,
    identity: SessionIdentity,
    token: String,
    token_expires_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionIdentity {
    actor_id: String,
    tenant: Option<String>,
}

struct CatalogSlot {
    state: AsyncMutex<Option<DerivedCatalog>>,
    accessed_at: Mutex<DateTime<Utc>>,
}

struct DerivedCatalog {
    handler: Arc<RerunCloudHandler>,
    dataset_id: EntryId,
    segment_id: String,
    layers: BTreeMap<String, CatalogLayer>,
    revision: String,
    byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogLayer {
    segment_id: String,
    ordinal: i64,
    byte_len: u64,
    sha256: String,
}

#[derive(Clone)]
struct AuthorizedCatalog {
    handler: Arc<RerunCloudHandler>,
    dataset_id: EntryId,
    subject: String,
}

impl PlaybackManager {
    pub fn new(signing_key_base64: &str, public_base_url: &str) -> Result<Self> {
        let provider = RedapProvider::from_secret_key_base64(signing_key_base64)
            .context("RECORDING_PLAYBACK_TOKEN_KEY must be canonical base64")?;
        let public_url = Url::parse(public_base_url)
            .context("RECORDING_PLAYBACK_PUBLIC_URL must be an absolute URL")?;
        ensure!(
            matches!(public_url.scheme(), "http" | "https")
                && public_url.host_str().is_some()
                && matches!(public_url.path(), "" | "/")
                && public_url.query().is_none()
                && public_url.fragment().is_none(),
            "RECORDING_PLAYBACK_PUBLIC_URL must be an http(s) origin without a path, query, or fragment"
        );
        let authority = match public_url.port() {
            Some(port) => format!(
                "{}:{port}",
                public_url.host_str().expect("host was validated")
            ),
            None => public_url
                .host_str()
                .expect("host was validated")
                .to_owned(),
        };
        let public_origin =
            Origin::from_str(&format!("rerun+{}://{authority}", public_url.scheme()))
                .context("constructing the public Redap origin")?;
        Ok(Self {
            inner: Arc::new(PlaybackManagerInner {
                provider,
                public_origin,
                allowed_host: public_url
                    .host_str()
                    .expect("host was validated")
                    .to_owned(),
                sessions: Mutex::new(HashMap::new()),
                catalogs: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub async fn prepare_manifest(
        &self,
        identity: &GatewayInternalIdentity,
        plan: RecordingPlaybackPlan,
        requested_session: Option<&str>,
    ) -> Result<PlaybackManifest> {
        let archive = if plan.archive_segments.is_empty() {
            None
        } else {
            Some(self.ensure_catalog(&plan).await?)
        };
        let access = self.renew_or_issue(identity, plan.recording_id, requested_session)?;
        self.prune_catalogs();
        Ok(PlaybackManifest {
            schema: PLAYBACK_MANIFEST_SCHEMA.to_owned(),
            recording_id: plan.recording_id.to_string(),
            application_id: plan.application_id,
            recording_key: plan.recording_key,
            state: recording_state(plan.state).to_owned(),
            started_at: plan.started_at.to_rfc3339(),
            ended_at: plan.ended_at.map(|value| value.to_rfc3339()),
            access,
            archive,
            live: plan.live.map(|live| live.descriptor),
        })
    }

    pub fn scoped_redap_service(&self) -> ScopedRedapService {
        ScopedRedapService {
            manager: self.clone(),
        }
    }

    async fn ensure_catalog(&self, plan: &RecordingPlaybackPlan) -> Result<PlaybackArchive> {
        let recording_id = plan.recording_id.to_string();
        let slot = {
            let mut catalogs = self
                .inner
                .catalogs
                .lock()
                .map_err(|_| anyhow::anyhow!("playback catalog cache is poisoned"))?;
            catalogs
                .entry(recording_id.clone())
                .or_insert_with(|| {
                    Arc::new(CatalogSlot {
                        state: AsyncMutex::new(None),
                        accessed_at: Mutex::new(Utc::now()),
                    })
                })
                .clone()
        };
        if let Ok(mut accessed_at) = slot.accessed_at.lock() {
            *accessed_at = Utc::now();
        }

        let expected_layers = plan
            .archive_segments
            .iter()
            .map(|segment| {
                (
                    layer_name(segment.ordinal, &segment.segment_id.to_string()),
                    CatalogLayer {
                        segment_id: segment.segment_id.to_string(),
                        ordinal: segment.ordinal,
                        byte_len: segment.byte_len,
                        sha256: segment.sha256.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let revision = catalog_revision(&expected_layers);
        let byte_len = plan
            .archive_segments
            .iter()
            .try_fold(0_u64, |total, segment| total.checked_add(segment.byte_len))
            .context("recording playback byte length overflow")?;

        let mut state = slot.state.lock().await;
        let must_rebuild = state.as_ref().is_none_or(|catalog| {
            catalog.segment_id != plan.recording_key
                || catalog
                    .layers
                    .iter()
                    .any(|(name, layer)| expected_layers.get(name) != Some(layer))
                || catalog
                    .layers
                    .keys()
                    .any(|name| !expected_layers.contains_key(name))
        });
        if must_rebuild {
            *state = Some(build_catalog(plan, &expected_layers, revision.clone(), byte_len).await?);
        } else if state
            .as_ref()
            .is_some_and(|catalog| catalog.revision != revision)
        {
            let catalog = state.as_mut().expect("catalog was checked");
            let missing = plan
                .archive_segments
                .iter()
                .filter(|segment| {
                    !catalog.layers.contains_key(&layer_name(
                        segment.ordinal,
                        &segment.segment_id.to_string(),
                    ))
                })
                .collect::<Vec<_>>();
            register_layers(
                &catalog.handler,
                catalog.dataset_id,
                &plan.recording_key,
                &missing,
            )
            .await?;
            catalog.layers = expected_layers;
            catalog.revision = revision.clone();
            catalog.byte_len = byte_len;
        }
        let catalog = state.as_ref().expect("catalog was initialized");
        let uri = DatasetSegmentUri {
            origin: self.inner.public_origin.clone(),
            dataset_id: catalog.dataset_id.id,
            segment_id: catalog.segment_id.clone().into(),
            fragment: Fragment::default(),
        }
        .to_string();
        Ok(PlaybackArchive {
            uri,
            dataset_id: catalog.dataset_id.to_string(),
            segment_id: catalog.segment_id.clone(),
            revision: catalog.revision.clone(),
            rrd_version: "0.35.0".to_owned(),
            optimization_profile: "object-store".to_owned(),
            byte_len: catalog.byte_len,
            layer_count: catalog.layers.len(),
        })
    }

    fn renew_or_issue(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
        requested_session: Option<&str>,
    ) -> Result<PlaybackAccess> {
        let now = Utc::now();
        let recording_id = recording_id.to_string();
        let identity = SessionIdentity::from(identity);
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("playback session store is poisoned"))?;
        sessions.retain(|_, session| session.expires_at > now);

        let session_id = requested_session
            .filter(|value| value.len() <= 128)
            .and_then(|session_id| {
                sessions
                    .get(session_id)
                    .filter(|session| {
                        session.recording_id == recording_id && session.identity == identity
                    })
                    .map(|_| session_id.to_owned())
            })
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

        let needs_token = sessions
            .get(&session_id)
            .is_none_or(|session| session.token_expires_at <= now + TOKEN_RENEW_WINDOW);
        let (token, token_expires_at) = if needs_token {
            (
                self.inner
                    .provider
                    .token(
                        TOKEN_TTL,
                        TOKEN_ISSUER,
                        session_id.clone(),
                        Permission::Read,
                        Some(&self.inner.allowed_host),
                    )
                    .context("issuing recording-scoped Redap token")?
                    .to_string(),
                now + TimeDelta::from_std(TOKEN_TTL)?,
            )
        } else {
            let session = sessions.get(&session_id).expect("token source exists");
            (session.token.clone(), session.token_expires_at)
        };
        let expires_at = now + SESSION_TTL;
        sessions.insert(
            session_id.clone(),
            PlaybackSession {
                recording_id,
                identity,
                token: token.clone(),
                token_expires_at,
                expires_at,
            },
        );
        if sessions.len() > MAX_SESSIONS
            && let Some(oldest) = sessions
                .iter()
                .filter(|(candidate, _)| *candidate != &session_id)
                .min_by_key(|(_, session)| session.expires_at)
                .map(|(candidate, _)| candidate.clone())
        {
            sessions.remove(&oldest);
        }
        Ok(PlaybackAccess {
            session_id,
            redap_token: token,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    async fn authorized_catalog<T>(
        &self,
        request: &Request<T>,
    ) -> Result<AuthorizedCatalog, Status> {
        let authorization = request
            .metadata()
            .get("authorization")
            .ok_or_else(|| Status::unauthenticated("missing recording playback token"))?
            .to_str()
            .map_err(|_| Status::unauthenticated("invalid recording playback token"))?;
        let token = authorization
            .strip_prefix("Bearer ")
            .or_else(|| authorization.strip_prefix("bearer "))
            .ok_or_else(|| Status::unauthenticated("invalid recording playback token"))?;
        let token = Jwt::try_from(token.to_owned())
            .map_err(|_| Status::unauthenticated("invalid recording playback token"))?;
        let claims = self
            .inner
            .provider
            .verify(&token, VerificationOptions::default())
            .map_err(|_| Status::unauthenticated("invalid recording playback token"))?;
        ensure_redap_claims(&claims, &self.inner.allowed_host)?;
        let subject = claims.sub().to_owned();
        let recording_id = {
            let now = Utc::now();
            let mut sessions = self
                .inner
                .sessions
                .lock()
                .map_err(|_| Status::internal("playback session store is unavailable"))?;
            sessions.retain(|_, session| session.expires_at > now);
            sessions
                .get(&subject)
                .map(|session| session.recording_id.clone())
                .ok_or_else(|| Status::unauthenticated("recording playback session expired"))?
        };
        let slot = self
            .inner
            .catalogs
            .lock()
            .map_err(|_| Status::internal("playback catalog cache is unavailable"))?
            .get(&recording_id)
            .cloned()
            .ok_or_else(|| Status::unavailable("recording playback catalog is not prepared"))?;
        if let Ok(mut accessed_at) = slot.accessed_at.lock() {
            *accessed_at = Utc::now();
        }
        let state = slot.state.lock().await;
        let catalog = state
            .as_ref()
            .ok_or_else(|| Status::unavailable("recording playback catalog is not prepared"))?;
        Ok(AuthorizedCatalog {
            handler: catalog.handler.clone(),
            dataset_id: catalog.dataset_id,
            subject,
        })
    }

    fn prune_catalogs(&self) {
        let now = Utc::now();
        let active_recordings = self
            .inner
            .sessions
            .lock()
            .ok()
            .map(|mut sessions| {
                sessions.retain(|_, session| session.expires_at > now);
                sessions
                    .values()
                    .map(|session| session.recording_id.clone())
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let Ok(mut catalogs) = self.inner.catalogs.lock() else {
            return;
        };
        while catalogs.len() > MAX_CATALOGS {
            let Some(oldest) = catalogs
                .iter()
                .filter(|(recording_id, _)| !active_recordings.contains(*recording_id))
                .min_by_key(|(_, slot)| slot.accessed_at.lock().ok().map(|value| *value))
                .map(|(recording_id, _)| recording_id.clone())
            else {
                break;
            };
            catalogs.remove(&oldest);
        }
    }
}

impl From<&GatewayInternalIdentity> for SessionIdentity {
    fn from(identity: &GatewayInternalIdentity) -> Self {
        Self {
            actor_id: identity.actor.id.to_string(),
            tenant: identity.actor.tenant.as_ref().map(ToString::to_string),
        }
    }
}

async fn build_catalog(
    plan: &RecordingPlaybackPlan,
    layers: &BTreeMap<String, CatalogLayer>,
    revision: String,
    byte_len: u64,
) -> Result<DerivedCatalog> {
    let dataset_id = playback_dataset_id(plan.recording_id)?;
    let handler = Arc::new(RerunCloudHandlerBuilder::new().build());
    let create: proto::CreateDatasetEntryRequest = CreateDatasetEntryRequest {
        name: EntryName::new(format!("recording-{}", plan.recording_id))
            .context("constructing recording dataset name")?,
        id: Some(dataset_id),
    }
    .into();
    handler
        .create_dataset_entry(Request::new(create))
        .await
        .context("creating the derived recording dataset")?;
    let segments = plan.archive_segments.iter().collect::<Vec<_>>();
    register_layers(&handler, dataset_id, &plan.recording_key, &segments).await?;
    Ok(DerivedCatalog {
        handler,
        dataset_id,
        segment_id: plan.recording_key.clone(),
        layers: layers.clone(),
        revision,
        byte_len,
    })
}

pub fn playback_store_id(recording_id: RecordingId, segment_id: &str) -> Result<StoreId> {
    Ok(StoreId::new(
        StoreKind::Recording,
        playback_dataset_id(recording_id)?.to_string(),
        segment_id,
    ))
}

fn playback_dataset_id(recording_id: RecordingId) -> Result<EntryId> {
    let recording_uuid = uuid::Uuid::parse_str(&recording_id.to_string())
        .context("recording playback id is not a UUID")?;
    Ok(EntryId::from(re_tuid::Tuid::from_bytes(
        *recording_uuid.as_bytes(),
    )))
}

async fn register_layers(
    handler: &RerunCloudHandler,
    dataset_id: EntryId,
    expected_segment_id: &str,
    segments: &[&crate::PlaybackArchiveSegmentPlan],
) -> Result<()> {
    if segments.is_empty() {
        return Ok(());
    }
    let mut data_sources = Vec::with_capacity(segments.len());
    for segment in segments {
        let storage_url = Url::from_file_path(&segment.path).map_err(|()| {
            anyhow::anyhow!(
                "recording archive path is not an absolute file URL: {}",
                segment.path.display()
            )
        })?;
        data_sources.push(proto::DataSource {
            storage_url: Some(storage_url.to_string()),
            layer: Some(layer_name(segment.ordinal, &segment.segment_id.to_string())),
            prefix: false,
            typ: proto::DataSourceKind::Rrd as i32,
        });
    }
    let on_duplicate: re_protos::common::v1alpha1::IfDuplicateBehavior =
        IfDuplicateBehavior::Error.into();
    let response = handler
        .register_with_dataset(
            Request::new(proto::RegisterWithDatasetRequest {
                data_sources,
                on_duplicate: on_duplicate as i32,
            })
            .with_entry_id(dataset_id),
        )
        .await
        .context("registering immutable recording layers")?
        .into_inner();
    let data = response
        .data
        .context("Rerun registration omitted its result dataframe")?;
    let data: re_chunk::external::arrow::array::RecordBatch = data
        .try_into()
        .context("decoding Rerun registration result dataframe")?;
    let registered = RegisterWithDatasetDataframe::try_from(data)
        .context("validating Rerun registration result dataframe")?;
    ensure!(
        registered.rerun_segment_id.len() == segments.len(),
        "Rerun registered {} of {} immutable recording layers",
        registered.rerun_segment_id.len(),
        segments.len()
    );
    for segment_id in registered.rerun_segment_id.into_iter_owned() {
        ensure!(
            segment_id.as_str() == expected_segment_id,
            "immutable recording shard belongs to Rerun segment {}, expected {}",
            segment_id.as_str(),
            expected_segment_id
        );
    }
    Ok(())
}

fn layer_name(ordinal: i64, segment_id: &str) -> String {
    format!("shard-{ordinal:020}-{segment_id}")
}

fn catalog_revision(layers: &BTreeMap<String, CatalogLayer>) -> String {
    let mut digest = Sha256::new();
    for (name, layer) in layers {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(layer.sha256.as_bytes());
        digest.update(layer.byte_len.to_be_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn recording_state(state: RecordingState) -> &'static str {
    match state {
        RecordingState::Live => "live",
        RecordingState::Ready => "ready",
        RecordingState::Sealing => "sealing",
        RecordingState::Sealed => "sealed",
        RecordingState::Interrupted => "interrupted",
        RecordingState::Failed => "failed",
    }
}

fn ensure_redap_claims(claims: &Claims, allowed_host: &str) -> Result<(), Status> {
    let exact_host_scope = match claims {
        Claims::Redap(claims) => claims.allowed_hosts.as_slice() == [allowed_host],
        #[allow(unreachable_patterns)]
        _ => false,
    };
    if claims.iss() != TOKEN_ISSUER || !claims.has_read_permission() || !exact_host_scope {
        return Err(Status::permission_denied(
            "recording playback token has invalid scope",
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct ScopedRedapService {
    manager: PlaybackManager,
}

macro_rules! impl_scoped_redap_service {
    (
        read_unary { $( $read_unary:ident : $read_request:ty => $read_response:ty ),* $(,)? }
        read_stream { $( $read_stream:ident / $read_associated:ident : $read_stream_request:ty => $read_stream_response:ty ),* $(,)? }
        deny_unary { $( $deny_unary:ident : $deny_request:ty => $deny_response:ty ),* $(,)? }
        deny_stream { $( $deny_stream:ident / $deny_associated:ident : $deny_stream_request:ty => $deny_stream_response:ty ),* $(,)? }
    ) => {
        #[tonic::async_trait]
        impl RerunCloudService for ScopedRedapService {
            $(
                async fn $read_unary(
                    &self,
                    request: Request<$read_request>,
                ) -> Result<Response<$read_response>, Status> {
                    let authorized = self.manager.authorized_catalog(&request).await?;
                    authorized.handler.$read_unary(request).await
                }
            )*

            $(
                type $read_associated = BoxedStream<$read_stream_response>;

                async fn $read_stream(
                    &self,
                    request: Request<$read_stream_request>,
                ) -> Result<Response<Self::$read_associated>, Status> {
                    let authorized = self.manager.authorized_catalog(&request).await?;
                    let response = authorized.handler.$read_stream(request).await?;
                    Ok(Response::new(Box::pin(response.into_inner())))
                }
            )*

            $(
                async fn $deny_unary(
                    &self,
                    _request: Request<$deny_request>,
                ) -> Result<Response<$deny_response>, Status> {
                    Err(Status::permission_denied(
                        "recording playback is a read-only Redap surface",
                    ))
                }
            )*

            $(
                type $deny_associated = BoxedStream<$deny_stream_response>;

                async fn $deny_stream(
                    &self,
                    _request: Request<$deny_stream_request>,
                ) -> Result<Response<Self::$deny_associated>, Status> {
                    Err(Status::permission_denied(
                        "recording playback is a read-only Redap surface",
                    ))
                }
            )*

            async fn who_am_i(
                &self,
                request: Request<proto::WhoAmIRequest>,
            ) -> Result<Response<proto::WhoAmIResponse>, Status> {
                let authorized = self.manager.authorized_catalog(&request).await?;
                Ok(Response::new(proto::WhoAmIResponse {
                    user_id: Some(authorized.subject),
                    can_read: true,
                    can_write: false,
                }))
            }

            async fn find_entries(
                &self,
                request: Request<proto::FindEntriesRequest>,
            ) -> Result<Response<proto::FindEntriesResponse>, Status> {
                let authorized = self.manager.authorized_catalog(&request).await?;
                let mut response = authorized
                    .handler
                    .find_entries(request)
                    .await?
                    .into_inner();
                let dataset_id: re_protos::common::v1alpha1::EntryId =
                    authorized.dataset_id.into();
                response
                    .entries
                    .retain(|entry| entry.id.as_ref() == Some(&dataset_id));
                Ok(Response::new(response))
            }

            async fn write_chunks(
                &self,
                _request: Request<tonic::Streaming<proto::WriteChunksRequest>>,
            ) -> Result<Response<proto::WriteChunksResponse>, Status> {
                Err(Status::permission_denied(
                    "recording playback is a read-only Redap surface",
                ))
            }

            async fn write_table(
                &self,
                _request: Request<tonic::Streaming<proto::WriteTableRequest>>,
            ) -> Result<Response<proto::WriteTableResponse>, Status> {
                Err(Status::permission_denied(
                    "recording playback is a read-only Redap surface",
                ))
            }
        }
    };
}

impl_scoped_redap_service! {
    read_unary {
        version: proto::VersionRequest => proto::VersionResponse,
        read_dataset_entry: proto::ReadDatasetEntryRequest => proto::ReadDatasetEntryResponse,
        get_segment_table_schema: proto::GetSegmentTableSchemaRequest => proto::GetSegmentTableSchemaResponse,
        get_dataset_manifest_schema: proto::GetDatasetManifestSchemaRequest => proto::GetDatasetManifestSchemaResponse,
        get_dataset_schema: proto::GetDatasetSchemaRequest => proto::GetDatasetSchemaResponse,
    }
    read_stream {
        do_bandwidth_test / DoBandwidthTestStream: proto::DoBandwidthTestRequest => proto::DoBandwidthTestResponse,
        watch_events / WatchEventsStream: proto::WatchEventsRequest => proto::WatchEventsResponse,
        scan_segment_table / ScanSegmentTableStream: proto::ScanSegmentTableRequest => proto::ScanSegmentTableResponse,
        scan_dataset_manifest / ScanDatasetManifestStream: proto::ScanDatasetManifestRequest => proto::ScanDatasetManifestResponse,
        get_rrd_manifest / GetRrdManifestStream: proto::GetRrdManifestRequest => proto::GetRrdManifestResponse,
        get_assets_for_segment / GetAssetsForSegmentStream: proto::GetAssetsForSegmentRequest => proto::GetAssetsForSegmentResponse,
        query_dataset / QueryDatasetStream: proto::QueryDatasetRequest => proto::QueryDatasetResponse,
        fetch_chunks / FetchChunksStream: proto::FetchChunksRequest => proto::FetchChunksResponse,
    }
    deny_unary {
        delete_entry: proto::DeleteEntryRequest => proto::DeleteEntryResponse,
        update_entry: proto::UpdateEntryRequest => proto::UpdateEntryResponse,
        create_dataset_entry: proto::CreateDatasetEntryRequest => proto::CreateDatasetEntryResponse,
        create_table_entry: proto::CreateTableEntryRequest => proto::CreateTableEntryResponse,
        update_dataset_entry: proto::UpdateDatasetEntryRequest => proto::UpdateDatasetEntryResponse,
        read_table_entry: proto::ReadTableEntryRequest => proto::ReadTableEntryResponse,
        update_table_entry: proto::UpdateTableEntryRequest => proto::UpdateTableEntryResponse,
        register_with_dataset: proto::RegisterWithDatasetRequest => proto::RegisterWithDatasetResponse,
        register_table: proto::RegisterTableRequest => proto::RegisterTableResponse,
        get_table_schema: proto::GetTableSchemaRequest => proto::GetTableSchemaResponse,
        query_tasks: proto::QueryTasksRequest => proto::QueryTasksResponse,
        cancel_tasks: proto::CancelTasksRequest => proto::CancelTasksResponse,
        do_maintenance: proto::DoMaintenanceRequest => proto::DoMaintenanceResponse,
        do_global_maintenance: proto::DoGlobalMaintenanceRequest => proto::DoGlobalMaintenanceResponse,
    }
    deny_stream {
        unregister_from_dataset / UnregisterFromDatasetStream: proto::UnregisterFromDatasetRequest => proto::UnregisterFromDatasetResponse,
        scan_table / ScanTableStream: proto::ScanTableRequest => proto::ScanTableResponse,
        query_tasks_on_completion / QueryTasksOnCompletionStream: proto::QueryTasksOnCompletionRequest => proto::QueryTasksOnCompletionResponse,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        path::{Path, PathBuf},
    };

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use re_build_info::CrateVersion;
    use re_log_encoding::{EncodingOptions, rrd::Encoder};
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;
    use veoveo_mcp_contract::{
        AccessSubject, DataLabelId, GatewayProfileId, GroupId, InvocationAuthority,
        InvocationProvenance, JwtId, PolicyVersion, Principal, PrincipalAssurance, PrincipalId,
        PrincipalKind, RoleId, ScopeName, ServerSlug, TenantId, TokenIssuer, TokenSubject,
        WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    use veoveo_platform_store::SegmentId;

    use super::*;

    #[test]
    fn public_playback_origin_and_key_are_validated() {
        let key = STANDARD.encode([7_u8; 32]);
        assert!(PlaybackManager::new(&key, "https://veoveo.example").is_ok());
        assert!(PlaybackManager::new(&key, "https://veoveo.example/path").is_err());
        assert!(PlaybackManager::new("not-base64", "https://veoveo.example").is_err());
    }

    #[test]
    fn revision_is_stable_and_sensitive_to_shard_identity() {
        let layers = BTreeMap::from([(
            "shard".to_owned(),
            CatalogLayer {
                segment_id: "segment".to_owned(),
                ordinal: 0,
                byte_len: 42,
                sha256: "abc".to_owned(),
            },
        )]);
        let first = catalog_revision(&layers);
        assert_eq!(first, catalog_revision(&layers));
        let mut changed = layers;
        changed.get_mut("shard").unwrap().sha256 = "def".to_owned();
        assert_ne!(first, catalog_revision(&changed));
    }

    #[tokio::test]
    async fn redap_access_is_recording_scoped_and_append_only() {
        let directory = tempfile::tempdir().unwrap();
        let recording_id = RecordingId::new();
        let recording_key = "inspection-flight";
        let first_segment = SegmentId::new();
        let first_path = write_rrd(
            directory.path(),
            "first.rrd",
            recording_key,
            "sensor/first",
            1.0,
        );
        let manager = manager();
        let identity = identity("operator-a");
        let first_plan = plan(
            recording_id,
            recording_key,
            vec![archive_plan(first_segment, 0, first_path)],
        );

        let first_manifest = manager
            .prepare_manifest(&identity, first_plan.clone(), None)
            .await
            .unwrap();
        let first_archive = first_manifest.archive.as_ref().unwrap();
        assert_eq!(first_manifest.schema, PLAYBACK_MANIFEST_SCHEMA);
        assert_eq!(first_archive.layer_count, 1);
        assert_eq!(first_archive.segment_id, recording_key);
        assert!(
            first_archive
                .uri
                .starts_with("rerun://veoveo.example:443/dataset/"),
            "unexpected Redap URI: {}",
            first_archive.uri
        );

        let service = manager.scoped_redap_service();
        let response = service
            .read_dataset_entry(authorized_request(
                proto::ReadDatasetEntryRequest {},
                &first_manifest.access.redap_token,
                Some(playback_dataset_id(recording_id).unwrap()),
            ))
            .await
            .unwrap()
            .into_inner();
        assert!(response.dataset.is_some());
        let who = service
            .who_am_i(authorized_request(
                proto::WhoAmIRequest {},
                &first_manifest.access.redap_token,
                None,
            ))
            .await
            .unwrap()
            .into_inner();
        assert!(who.can_read);
        assert!(!who.can_write);
        assert_eq!(
            who.user_id.as_deref(),
            Some(first_manifest.access.session_id.as_str())
        );

        let entries = service
            .find_entries(authorized_request(
                proto::FindEntriesRequest::default(),
                &first_manifest.access.redap_token,
                None,
            ))
            .await
            .unwrap()
            .into_inner()
            .entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].name.as_deref(),
            Some(format!("recording-{recording_id}").as_str())
        );

        let second_segment = SegmentId::new();
        let second_path = write_rrd(
            directory.path(),
            "second.rrd",
            recording_key,
            "sensor/second",
            2.0,
        );
        let second_plan = plan(
            recording_id,
            recording_key,
            vec![
                archive_plan(
                    first_segment,
                    0,
                    first_plan.archive_segments[0].path.clone(),
                ),
                archive_plan(second_segment, 1, second_path),
            ],
        );
        let second_manifest = manager
            .prepare_manifest(
                &identity,
                second_plan,
                Some(&first_manifest.access.session_id),
            )
            .await
            .unwrap();
        let second_archive = second_manifest.archive.as_ref().unwrap();
        assert_eq!(
            second_manifest.access.session_id,
            first_manifest.access.session_id
        );
        assert_eq!(second_archive.uri, first_archive.uri);
        assert_eq!(second_archive.layer_count, 2);
        assert_ne!(second_archive.revision, first_archive.revision);

        let other_recording_id = RecordingId::new();
        let other_path = write_rrd(
            directory.path(),
            "other.rrd",
            "other-flight",
            "sensor/other",
            3.0,
        );
        manager
            .prepare_manifest(
                &identity,
                plan(
                    other_recording_id,
                    "other-flight",
                    vec![archive_plan(SegmentId::new(), 0, other_path)],
                ),
                None,
            )
            .await
            .unwrap();
        let cross_recording = service
            .read_dataset_entry(authorized_request(
                proto::ReadDatasetEntryRequest {},
                &first_manifest.access.redap_token,
                Some(playback_dataset_id(other_recording_id).unwrap()),
            ))
            .await
            .unwrap_err();
        assert_eq!(cross_recording.code(), tonic::Code::NotFound);

        let unauthenticated = service
            .read_dataset_entry(
                Request::new(proto::ReadDatasetEntryRequest {})
                    .with_entry_id(playback_dataset_id(recording_id).unwrap()),
            )
            .await
            .unwrap_err();
        assert_eq!(unauthenticated.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn mismatched_rrd_segment_identity_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_rrd(
            directory.path(),
            "wrong.rrd",
            "wrong-flight",
            "sensor/value",
            1.0,
        );
        let error = manager()
            .prepare_manifest(
                &identity("operator-a"),
                plan(
                    RecordingId::new(),
                    "expected-flight",
                    vec![archive_plan(SegmentId::new(), 0, path)],
                ),
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("immutable recording shard belongs to Rerun segment wrong-flight")
        );
    }

    #[tokio::test]
    async fn a_playback_session_cannot_be_adopted_by_another_actor() {
        let directory = tempfile::tempdir().unwrap();
        let recording_id = RecordingId::new();
        let path = write_rrd(
            directory.path(),
            "recording.rrd",
            "inspection-flight",
            "sensor/value",
            1.0,
        );
        let manager = manager();
        let plan = plan(
            recording_id,
            "inspection-flight",
            vec![archive_plan(SegmentId::new(), 0, path)],
        );
        let first = manager
            .prepare_manifest(&identity("operator-a"), plan.clone(), None)
            .await
            .unwrap();
        let second = manager
            .prepare_manifest(
                &identity("operator-b"),
                plan,
                Some(&first.access.session_id),
            )
            .await
            .unwrap();
        assert_ne!(second.access.session_id, first.access.session_id);
    }

    fn manager() -> PlaybackManager {
        PlaybackManager::new(&STANDARD.encode([7_u8; 32]), "https://veoveo.example").unwrap()
    }

    fn plan(
        recording_id: RecordingId,
        recording_key: &str,
        archive_segments: Vec<crate::PlaybackArchiveSegmentPlan>,
    ) -> RecordingPlaybackPlan {
        RecordingPlaybackPlan {
            recording_id,
            application_id: "inspection-camera".to_owned(),
            recording_key: recording_key.to_owned(),
            state: RecordingState::Ready,
            started_at: Utc::now(),
            ended_at: Some(Utc::now()),
            archive_segments,
            live: None,
        }
    }

    fn archive_plan(
        segment_id: SegmentId,
        ordinal: i64,
        path: PathBuf,
    ) -> crate::PlaybackArchiveSegmentPlan {
        let bytes = std::fs::read(&path).unwrap();
        crate::PlaybackArchiveSegmentPlan {
            segment_id,
            ordinal,
            byte_len: bytes.len() as u64,
            sha256: Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            path,
        }
    }

    fn write_rrd(
        directory: &Path,
        name: &str,
        recording_key: &str,
        entity: &str,
        value: f64,
    ) -> PathBuf {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id(recording_key)
            .memory()
            .unwrap();
        recording.log(entity, &Scalars::single(value)).unwrap();
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        for message in storage.take() {
            encoder.append(&message).unwrap();
        }
        encoder.finish().unwrap();
        let path = directory.join(name);
        std::fs::write(&path, encoder.into_inner().unwrap()).unwrap();
        path
    }

    fn authorized_request<T>(body: T, token: &str, entry_id: Option<EntryId>) -> Request<T> {
        let mut request = match entry_id {
            Some(entry_id) => Request::new(body).with_entry_id(entry_id),
            None => Request::new(body),
        };
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        request
    }

    fn identity(principal: &str) -> GatewayInternalIdentity {
        let now = Utc::now();
        let actor = Principal {
            id: PrincipalId::new(principal).unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://issuer.example").unwrap(),
            subject: TokenSubject::new(format!("subject-{principal}")).unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::<GroupId>::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::<RoleId>::new(),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            assurances: BTreeSet::<PrincipalAssurance>::new(),
            authenticated_at: Some(now),
        };
        GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("operations").unwrap(),
            server: ServerSlug::new("recording").unwrap(),
            actor: actor.clone(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("flight-operations").unwrap(),
                tenant: TenantId::new("tenant-a").unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(actor.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: actor.id,
                },
            },
            jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + TimeDelta::minutes(5),
        }
    }
}
