//! Governed virtual Rerun Data Protocol catalogs and playback manifests.
//!
//! The durable Veoveo recording catalog remains authoritative. This module
//! derives policy-scoped catalogs from immutable Artifact-backed layers, maps
//! Redap token subjects to durable grants, and exposes only the required read
//! profile.

use std::{
    collections::{BTreeMap, HashMap},
    pin::Pin,
    str::FromStr as _,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, Utc};
use futures::Stream;
use re_auth::{Claims, Jwt, Permission, RedapProvider, VerificationOptions};
use re_log_types::{ApplicationId, EntryId, EntryName, StoreId, StoreKind};
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
use re_uri::{DatasetSegmentUri, EntryUri, Fragment, Origin};
use tokio::sync::Mutex as AsyncMutex;
use tonic::{Request, Response, Status};
use url::Url;
use veoveo_platform_store::{
    PlatformStore, RecordId, RecordIdKey, RecordingDatasetId, RecordingId, RecordingReadGrantClass,
    RecordingReadGrantId, RecordingReadGrantRecord, RecordingState,
};

use crate::{
    RecordingPlaybackPlan,
    contract::{
        CatalogReadGrant, PlaybackAccess, PlaybackArchive, PlaybackBlueprint, PlaybackManifest,
        PlaybackMapProvider,
    },
};

pub const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.io/recording-playback/v9";
pub const RECORDING_GRANT_HEADER: &str = "x-veoveo-recording-grant";
const TOKEN_ISSUER: &str = "veoveo-recording-playback";
const MAX_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_CATALOG_IDLE: Duration = MAX_TOKEN_TTL;
const MAX_VIRTUAL_CATALOGS: usize = 64;

type BoxedStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

#[derive(Clone)]
pub struct PlaybackManager {
    inner: Arc<PlaybackManagerInner>,
}

struct PlaybackManagerInner {
    provider: RedapProvider,
    store: PlatformStore,
    public_origin: Origin,
    allowed_host: String,
    catalogs: Mutex<HashMap<VirtualCatalogKey, Arc<CatalogSlot>>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct VirtualCatalogKey {
    tenant: RecordId,
    dataset_id: RecordId,
    policy_revision: String,
    admitted_set_digest: String,
    grant_class: RecordingReadGrantClass,
}

struct CatalogSlot {
    state: AsyncMutex<Option<DerivedCatalog>>,
    accessed_at: Mutex<DateTime<Utc>>,
}

struct DerivedCatalog {
    handler: Arc<RerunCloudHandler>,
    dataset_id: EntryId,
    recordings: BTreeMap<String, BTreeMap<String, CatalogLayer>>,
    revision: String,
    byte_len: u64,
    _leases: Vec<crate::layer_cache::CachedLayer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogLayer {
    layer_id: String,
    kind: String,
    ordinal: Option<i64>,
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
    pub fn new(
        signing_key_base64: &str,
        public_base_url: &str,
        store: PlatformStore,
    ) -> Result<Self> {
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
                store,
                public_origin,
                allowed_host: public_url
                    .host_str()
                    .expect("host was validated")
                    .to_owned(),
                catalogs: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub async fn prepare_manifest(
        &self,
        plan: RecordingPlaybackPlan,
        grant: RecordingReadGrantRecord,
    ) -> Result<PlaybackManifest> {
        validate_viewer_grant(&grant, &plan)?;
        let archive = if plan.archive_layers.is_empty() {
            None
        } else {
            Some(self.ensure_catalog(&[&plan], &grant).await?)
        };
        let access = self.issue_access(&grant)?;
        self.prune_catalogs();
        Ok(PlaybackManifest {
            schema: PLAYBACK_MANIFEST_SCHEMA.to_owned(),
            dataset_id: plan.dataset_id.to_string(),
            recording_segment_id: plan.recording_id.to_string(),
            application_id: plan.application_id,
            recording_key: plan.recording_key,
            state: recording_state(plan.state).to_owned(),
            started_at: plan.started_at.to_rfc3339(),
            ended_at: plan.ended_at.map(|value| value.to_rfc3339()),
            catalog_revision: plan.catalog_revision,
            access,
            archive,
            live: plan.live.map(|live| live.descriptor),
            blueprint: plan.blueprint.map(|blueprint| PlaybackBlueprint {
                blueprint_id: blueprint.blueprint_id,
                revision: blueprint.revision,
                sha256: blueprint.sha256,
                byte_len: blueprint.byte_len,
                map_provider: match blueprint.map_provider {
                    veoveo_recording_hub::BlueprintMapProviderSelection::None => {
                        PlaybackMapProvider::None
                    }
                    veoveo_recording_hub::BlueprintMapProviderSelection::OpenStreetMap => {
                        PlaybackMapProvider::OpenStreetMap
                    }
                    veoveo_recording_hub::BlueprintMapProviderSelection::Mapbox => {
                        PlaybackMapProvider::Mapbox
                    }
                    veoveo_recording_hub::BlueprintMapProviderSelection::Mixed => {
                        PlaybackMapProvider::Mixed
                    }
                },
            }),
        })
    }

    pub fn scoped_redap_service(&self) -> ScopedRedapService {
        ScopedRedapService {
            manager: self.clone(),
        }
    }

    pub async fn prepare_catalog_grant(
        &self,
        plans: Vec<RecordingPlaybackPlan>,
        grant: RecordingReadGrantRecord,
    ) -> Result<CatalogReadGrant> {
        ensure!(
            grant.grant_class == RecordingReadGrantClass::CatalogDataset,
            "Catalog SDK access requires a catalog_dataset grant"
        );
        let mut admitted = plans
            .iter()
            .map(|plan| plan.recording_id)
            .collect::<Vec<_>>();
        admitted.sort_unstable();
        admitted.dedup();
        let expected = admitted
            .iter()
            .copied()
            .map(RecordingId::record_id)
            .collect::<Vec<_>>();
        let dataset_id = plans
            .first()
            .map(|plan| plan.dataset_id)
            .context("Catalog SDK grant has no recording plans")?;
        ensure!(
            plans.iter().all(|plan| plan.dataset_id == dataset_id)
                && grant.dataset == dataset_id.record_id()
                && grant.recordings == expected,
            "Catalog SDK grant does not match its admitted dataset"
        );
        let plans_ref = plans.iter().collect::<Vec<_>>();
        self.ensure_catalog(&plans_ref, &grant).await?;
        let access = self.issue_access(&grant)?;
        self.prune_catalogs();
        Ok(CatalogReadGrant {
            schema: veoveo_mcp_contract::RECORDING_CATALOG_GRANT_SCHEMA.to_owned(),
            grant_id: uuid::Uuid::parse_str(&access.grant_id)?,
            dataset_id: dataset_id.as_uuid(),
            recording_segment_ids: admitted.into_iter().map(RecordingId::as_uuid).collect(),
            catalog_revision: grant.catalog_revision,
            entry_uri: EntryUri::new(
                self.inner.public_origin.clone(),
                playback_dataset_id(dataset_id)?,
            )
            .to_string(),
            redap_token: access.redap_token,
            expires_at: access.expires_at,
        })
    }

    async fn ensure_catalog(
        &self,
        plans: &[&RecordingPlaybackPlan],
        grant: &RecordingReadGrantRecord,
    ) -> Result<PlaybackArchive> {
        let plan = plans
            .first()
            .copied()
            .context("virtual catalog has no recording plans")?;
        let key = virtual_catalog_key(grant);
        let slot = {
            let mut catalogs = self
                .inner
                .catalogs
                .lock()
                .map_err(|_| anyhow::anyhow!("playback catalog cache is poisoned"))?;
            catalogs
                .entry(key)
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

        let expected_recordings = plans
            .iter()
            .map(|plan| {
                let layers = plan
                    .archive_layers
                    .iter()
                    .map(|layer| {
                        (
                            layer.layer_name.clone(),
                            CatalogLayer {
                                layer_id: layer.layer_id.to_string(),
                                kind: recording_layer_kind(layer.kind).to_owned(),
                                ordinal: layer.ordinal,
                                byte_len: layer.byte_len,
                                sha256: layer.sha256.clone(),
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                (plan.recording_id.to_string(), layers)
            })
            .collect::<BTreeMap<_, _>>();
        let revision = grant.catalog_revision.clone();
        let byte_len = plans
            .iter()
            .flat_map(|plan| &plan.archive_layers)
            .try_fold(0_u64, |total, layer| total.checked_add(layer.byte_len))
            .context("virtual catalog byte length overflow")?;

        let mut state = slot.state.lock().await;
        let must_rebuild = state.as_ref().is_none_or(|catalog| {
            catalog.recordings != expected_recordings || catalog.revision != revision
        });
        if must_rebuild {
            *state =
                Some(build_catalog(plans, expected_recordings, revision.clone(), byte_len).await?);
        }
        let catalog = state.as_ref().expect("catalog was initialized");
        let uri = DatasetSegmentUri {
            origin: self.inner.public_origin.clone(),
            dataset_id: catalog.dataset_id.id,
            segment_id: plan.recording_id.to_string().into(),
            fragment: Fragment::default(),
        }
        .to_string();
        Ok(PlaybackArchive {
            uri,
            // Rerun displays `EntryId` as mixed-case TUID hex. The public catalog
            // contract keeps the durable UUIDv7 identity instead.
            dataset_id: plan.dataset_id.to_string(),
            recording_segment_id: plan.recording_id.to_string(),
            catalog_revision: catalog.revision.clone(),
            rrd_version: "0.36.3".to_owned(),
            optimization_profile: "object-store".to_owned(),
            byte_len: catalog.byte_len,
            layer_count: plan.archive_layers.len(),
        })
    }

    fn issue_access(&self, grant: &RecordingReadGrantRecord) -> Result<PlaybackAccess> {
        let now = Utc::now();
        ensure!(grant.expires_at > now, "recording read grant expired");
        let remaining = (grant.expires_at - now).to_std()?;
        let token_ttl = std::cmp::min(remaining, MAX_TOKEN_TTL);
        let grant_id = grant_id_from_record(&grant.id)?;
        let token = self
            .inner
            .provider
            .token(
                token_ttl,
                TOKEN_ISSUER,
                grant_id.to_string(),
                Permission::Read,
                Some(&self.inner.allowed_host),
            )
            .context("issuing recording grant Redap token")?
            .to_string();
        Ok(PlaybackAccess {
            grant_id: grant_id.to_string(),
            redap_token: token,
            expires_at: grant.expires_at.to_rfc3339(),
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
        let grant_id = subject
            .parse::<RecordingReadGrantId>()
            .map_err(|_| Status::unauthenticated("invalid recording grant subject"))?;
        let grant = self
            .inner
            .store
            .recording_read_grant_by_id(grant_id)
            .await
            .map_err(|_| Status::internal("recording grant store is unavailable"))?
            .ok_or_else(|| Status::unauthenticated("recording grant expired"))?;
        if !matches!(
            grant.grant_class,
            RecordingReadGrantClass::ViewerSegment | RecordingReadGrantClass::CatalogDataset
        ) {
            return Err(Status::permission_denied(
                "recording grant does not admit Redap access",
            ));
        }
        self.prune_catalogs();
        let key = virtual_catalog_key(&grant);
        let slot = self
            .inner
            .catalogs
            .lock()
            .map_err(|_| Status::internal("playback catalog cache is unavailable"))?
            .get(&key)
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

    pub fn prune_catalogs(&self) {
        let Ok(mut catalogs) = self.inner.catalogs.lock() else {
            return;
        };
        let now = Utc::now();
        catalogs.retain(|_, slot| !catalog_slot_is_idle(slot, now));
        while catalogs.len() > MAX_VIRTUAL_CATALOGS {
            let Some(oldest) = catalogs
                .iter()
                .min_by_key(|(_, slot)| slot.accessed_at.lock().ok().map(|value| *value))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            catalogs.remove(&oldest);
        }
    }
}

fn catalog_slot_is_idle(slot: &CatalogSlot, now: DateTime<Utc>) -> bool {
    slot.accessed_at.lock().ok().is_some_and(|accessed_at| {
        now.signed_duration_since(*accessed_at)
            .to_std()
            .is_ok_and(|idle| idle > MAX_CATALOG_IDLE)
    })
}

fn validate_viewer_grant(
    grant: &RecordingReadGrantRecord,
    plan: &RecordingPlaybackPlan,
) -> Result<()> {
    ensure!(
        grant.grant_class == RecordingReadGrantClass::ViewerSegment,
        "playback manifest requires a viewer_segment grant"
    );
    ensure!(
        grant.dataset == plan.dataset_id.record_id()
            && grant.recordings == [plan.recording_id.record_id()]
            && grant.catalog_revision == plan.catalog_revision,
        "playback grant does not match the requested recording catalog"
    );
    Ok(())
}

fn virtual_catalog_key(grant: &RecordingReadGrantRecord) -> VirtualCatalogKey {
    VirtualCatalogKey {
        tenant: grant.tenant.clone(),
        dataset_id: grant.dataset.clone(),
        policy_revision: grant.policy_revision.clone(),
        admitted_set_digest: grant.admitted_set_digest.clone(),
        grant_class: grant.grant_class,
    }
}

fn grant_id_from_record(record: &RecordId) -> Result<RecordingReadGrantId> {
    ensure!(
        record.table.as_str() == RecordingReadGrantId::TABLE,
        "recording grant record has the wrong table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("recording grant key is not a UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(
        value.get_version_num() == 7,
        "recording grant key is not UUIDv7"
    );
    Ok(RecordingReadGrantId::from_uuid(value))
}

async fn build_catalog(
    plans: &[&RecordingPlaybackPlan],
    expected_recordings: BTreeMap<String, BTreeMap<String, CatalogLayer>>,
    revision: String,
    byte_len: u64,
) -> Result<DerivedCatalog> {
    let plan = plans
        .first()
        .copied()
        .context("virtual catalog has no recording plans")?;
    let dataset_id = playback_dataset_id(plan.dataset_id)?;
    let handler = Arc::new(RerunCloudHandlerBuilder::new().build());
    let create: proto::CreateDatasetEntryRequest = CreateDatasetEntryRequest {
        name: EntryName::new(format!("dataset-{}", plan.dataset_id))
            .context("constructing recording dataset name")?,
        id: Some(dataset_id),
    }
    .into();
    handler
        .create_dataset_entry(Request::new(create))
        .await
        .context("creating the derived recording dataset")?;
    let mut leases = Vec::new();
    for plan in plans {
        ensure!(
            plan.dataset_id == plans[0].dataset_id,
            "virtual catalog recordings must belong to one dataset"
        );
        let archive_layers = plan.archive_layers.iter().collect::<Vec<_>>();
        register_layers(
            &handler,
            dataset_id,
            &plan.recording_id.to_string(),
            &archive_layers,
        )
        .await?;
        leases.extend(plan.archive_layers.iter().map(|layer| layer.cached.clone()));
    }
    Ok(DerivedCatalog {
        handler,
        dataset_id,
        recordings: expected_recordings,
        revision,
        byte_len,
        _leases: leases,
    })
}

pub fn playback_store_id(
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
) -> Result<StoreId> {
    let application_id = ApplicationId::try_new(playback_dataset_id(dataset_id)?.to_string())
        .context("playback dataset id is not a valid Rerun application id")?;
    Ok(StoreId::new(
        StoreKind::Recording,
        application_id,
        recording_id.to_string(),
    ))
}

pub fn playback_application_id(dataset_id: RecordingDatasetId) -> Result<String> {
    Ok(playback_dataset_id(dataset_id)?.to_string())
}

fn playback_dataset_id(dataset_id: RecordingDatasetId) -> Result<EntryId> {
    let dataset_uuid = uuid::Uuid::parse_str(&dataset_id.to_string())
        .context("recording dataset id is not a UUID")?;
    Ok(EntryId::from(re_tuid::Tuid::from_bytes(
        *dataset_uuid.as_bytes(),
    )))
}

async fn register_layers(
    handler: &RerunCloudHandler,
    dataset_id: EntryId,
    expected_recording_id: &str,
    layers: &[&crate::PlaybackArchiveLayerPlan],
) -> Result<()> {
    if layers.is_empty() {
        return Ok(());
    }
    let mut data_sources = Vec::with_capacity(layers.len());
    for layer in layers {
        let storage_url = Url::from_file_path(layer.cached.path()).map_err(|()| {
            anyhow::anyhow!(
                "recording archive path is not an absolute file URL: {}",
                layer.cached.path().display()
            )
        })?;
        data_sources.push(proto::DataSource {
            storage_url: Some(storage_url.to_string()),
            layer: Some(layer.layer_name.clone()),
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
        registered.rerun_segment_id.len() == layers.len(),
        "Rerun registered {} of {} immutable recording layers",
        registered.rerun_segment_id.len(),
        layers.len()
    );
    for segment_id in registered.rerun_segment_id.into_iter_owned() {
        ensure!(
            segment_id.as_str() == expected_recording_id,
            "immutable recording layer belongs to Rerun segment {}, expected {}",
            segment_id.as_str(),
            expected_recording_id
        );
    }
    Ok(())
}

fn recording_layer_kind(kind: veoveo_platform_store::RecordingLayerKind) -> &'static str {
    match kind {
        veoveo_platform_store::RecordingLayerKind::Capture => "capture",
        veoveo_platform_store::RecordingLayerKind::Properties => "properties",
        veoveo_platform_store::RecordingLayerKind::Derived => "derived",
    }
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
    use super::*;

    #[test]
    fn manifest_schema_is_the_v9_hard_cut() {
        assert_eq!(PLAYBACK_MANIFEST_SCHEMA, "veoveo.io/recording-playback/v9");
    }

    #[test]
    fn durable_dataset_uuid_bytes_back_the_rerun_dataset_entry_id() {
        let dataset_id = RecordingDatasetId::new();
        let entry_id = playback_dataset_id(dataset_id).unwrap();
        let dataset_uuid = uuid::Uuid::parse_str(&dataset_id.to_string()).unwrap();
        assert_eq!(entry_id.id.as_bytes(), *dataset_uuid.as_bytes());
        assert_ne!(entry_id.to_string(), dataset_id.to_string());
    }

    #[test]
    fn playback_store_uses_rerun_dataset_entry_and_catalog_recording_uuid() {
        let dataset_id = RecordingDatasetId::new();
        let recording_id = RecordingId::new();
        let store_id = playback_store_id(dataset_id, recording_id).unwrap();
        assert_eq!(
            store_id.application_id().as_str(),
            playback_dataset_id(dataset_id).unwrap().to_string()
        );
        assert_eq!(store_id.recording_id().as_str(), recording_id.to_string());
    }

    #[test]
    fn virtual_catalog_slots_expire_after_the_maximum_token_idle_window() {
        let now = Utc::now();
        let slot = |accessed_at| CatalogSlot {
            state: AsyncMutex::new(None),
            accessed_at: Mutex::new(accessed_at),
        };

        assert!(catalog_slot_is_idle(
            &slot(now - chrono::TimeDelta::minutes(6)),
            now
        ));
        assert!(!catalog_slot_is_idle(
            &slot(now - chrono::TimeDelta::minutes(4)),
            now
        ));
        assert!(!catalog_slot_is_idle(
            &slot(now + chrono::TimeDelta::minutes(1)),
            now
        ));
    }
}

#[cfg(all(test, feature = "redap-conformance"))]
mod official_read_profile {
    use re_server::{RerunCloudHandler, RerunCloudHandlerBuilder};

    fn handler() -> RerunCloudHandler {
        RerunCloudHandlerBuilder::new().build()
    }

    #[tokio::test]
    async fn official_query_dataset_filter_profile() {
        re_redap_tests::query_dataset_unknown_segment_id_returns_empty(handler()).await;
        re_redap_tests::query_dataset_should_fail(handler()).await;
    }

    #[tokio::test]
    async fn official_segment_and_manifest_scan_profile() {
        re_redap_tests::scan_segment_table_filter(handler()).await;
        re_redap_tests::scan_dataset_manifest_filter(handler()).await;
    }

    #[tokio::test]
    async fn official_fetch_chunks_profile() {
        re_redap_tests::multi_dataset_fetch_chunk_completeness(handler()).await;
    }

    #[tokio::test]
    async fn official_rrd_manifest_profile() {
        re_redap_tests::segment_id_not_found(handler()).await;
    }
}
