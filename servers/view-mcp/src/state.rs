use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    fmt::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    camera::{
        CameraProjection, PerspectiveProjection,
        primitives::{Frustum, Sphere},
    },
    math::{Mat4, Vec3A, primitives::ViewFrustum},
};
use chrono::Utc;
use futures::{StreamExt, stream};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    cache::WeightedLru,
    contract::{
        AttributionSet, CaptureFrameRequest, CaptureLimits, CapturedFrame, CloseViewRequest,
        CloseViewResult, ContractError, CreateViewRequest, DeadlineBehavior, FrameId, FrameRecord,
        LayerId, MAX_TILE_RESOURCE_BYTES, PreviewScenePolicy, PreviewSceneRecord,
        SCENE_DEADLINE_MS, SCENE_MAX_TILES, SceneTileRecord, SetCameraRequest, ViewId, ViewRecord,
    },
    decode::{CpuTileContent, decode_glb},
    geodesy::{
        camera_ecef_basis, camera_world_transform, geodetic_to_ecef, resolve_camera,
        world_from_ecef,
    },
    renderer::{RenderFrameRequest, RenderTile, RendererError, RendererHandle},
    source::{LayerCatalog, SourceError, TileSource, credential_free_location, looks_like_tileset},
    tiles::traversal::{
        Selection, SelectionHistory, SelectionParams, TileReadiness, TileTree, select,
    },
    uris,
};

pub type OwnerId = String;

#[derive(Debug, Clone)]
pub struct ViewServiceConfig {
    pub capture_limits: CaptureLimits,
    pub max_views: usize,
    pub max_views_per_owner: usize,
    pub max_frames: usize,
    pub max_frame_bytes: u64,
    pub max_single_frame_bytes: u64,
    pub decoded_cache_bytes: u64,
    pub max_concurrent_loads: usize,
    pub max_tree_nodes: usize,
    pub detail_falloff_meters: f64,
}

pub struct ViewService {
    config: ViewServiceConfig,
    catalog: LayerCatalog,
    renderer: RendererHandle,
    views: RwLock<HashMap<ViewId, InternalView>>,
    layers: AsyncMutex<HashMap<LayerId, Arc<AsyncMutex<LayerRuntime>>>>,
    frames: Mutex<FrameStore>,
    tiles: Mutex<TileTokenRegistry>,
}

/// Opaque preview-tile tokens handed out in scene manifests. Entries map a
/// sha256 token back to a credential-free content location; like the frame
/// store, the registry is in-process state.
const MAX_TILE_TOKENS: usize = 8_192;

#[derive(Default)]
struct TileTokenRegistry {
    entries: HashMap<String, TileTokenEntry>,
    order: VecDeque<String>,
}

#[derive(Clone)]
struct TileTokenEntry {
    layer: LayerId,
    location: String,
}

impl TileTokenRegistry {
    fn register(&mut self, key: String, entry: TileTokenEntry) {
        if self.entries.insert(key.clone(), entry).is_none() {
            self.order.push_back(key);
            while self.order.len() > MAX_TILE_TOKENS {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                self.entries.remove(&oldest);
            }
        }
    }

    fn get(&self, key: &str) -> Option<TileTokenEntry> {
        self.entries.get(key).cloned()
    }
}

struct InternalView {
    owner: OwnerId,
    record: ViewRecord,
    close_token: CancellationToken,
}

struct FrameStore {
    records: HashMap<FrameId, StoredFrame>,
    order: VecDeque<FrameId>,
    bytes: u64,
}

struct StoredFrame {
    owner: OwnerId,
    frame: Arc<CapturedFrame>,
}

struct LayerRuntime {
    source: Arc<TileSource>,
    tree: Option<TileTree>,
    states: Vec<NodeState>,
    decoded: WeightedLru<String, CpuTileContent>,
    history: SelectionHistory,
    detail_falloff_meters: f64,
}

#[derive(Debug, Clone)]
enum NodeState {
    Empty,
    Pending,
    Ready(String),
    External,
}

enum LoadedContent {
    Tile {
        content: Arc<CpuTileContent>,
        content_hash: String,
    },
    External(Box<crate::tiles::schema::Tileset>),
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl ViewService {
    pub fn new(config: ViewServiceConfig, catalog: LayerCatalog, renderer: RendererHandle) -> Self {
        Self {
            config,
            catalog,
            renderer,
            views: RwLock::new(HashMap::new()),
            layers: AsyncMutex::new(HashMap::new()),
            frames: Mutex::new(FrameStore {
                records: HashMap::new(),
                order: VecDeque::new(),
                bytes: 0,
            }),
            tiles: Mutex::new(TileTokenRegistry::default()),
        }
    }

    pub fn adapter(&self) -> &crate::renderer::GpuAdapterStatus {
        self.renderer.adapter()
    }

    pub fn layers(&self) -> &[crate::source::LayerSummary] {
        self.catalog.summaries()
    }

    pub async fn create_view(
        &self,
        owner: &str,
        request: CreateViewRequest,
    ) -> Result<ViewRecord, ServiceError> {
        if self.catalog.get(&request.scene_layer).is_none() {
            return Err(ServiceError::LayerNotFound(request.scene_layer));
        }
        let camera = request.camera.validate()?;
        let resolved_camera = resolve_camera(&camera)?;
        let mut views = self.views.write().await;
        if views.len() >= self.config.max_views {
            return Err(ServiceError::ViewLimit);
        }
        if views.values().filter(|view| view.owner == owner).count()
            >= self.config.max_views_per_owner
        {
            return Err(ServiceError::OwnerViewLimit);
        }
        let now = Utc::now();
        let view_id = ViewId::new(Uuid::now_v7().simple().to_string())?;
        let record = ViewRecord {
            view_uri: uris::view(&view_id),
            view_id: view_id.clone(),
            scene_layer: request.scene_layer,
            revision: 1,
            camera,
            resolved_camera,
            created_at: now,
            updated_at: now,
        };
        views.insert(
            view_id,
            InternalView {
                owner: owner.to_owned(),
                record: record.clone(),
                close_token: CancellationToken::new(),
            },
        );
        Ok(record)
    }

    pub async fn set_camera(
        &self,
        owner: &str,
        request: SetCameraRequest,
    ) -> Result<ViewRecord, ServiceError> {
        let camera = request.camera.validate()?;
        let resolved_camera = resolve_camera(&camera)?;
        let mut views = self.views.write().await;
        let view = views
            .get_mut(&request.view_id)
            .ok_or(ServiceError::ViewNotFound)?;
        require_owner(view, owner)?;
        if view.record.revision != request.expected_revision {
            return Err(ServiceError::RevisionConflict {
                expected: request.expected_revision,
                actual: view.record.revision,
            });
        }
        view.record.revision += 1;
        view.record.camera = camera;
        view.record.resolved_camera = resolved_camera;
        view.record.updated_at = Utc::now();
        Ok(view.record.clone())
    }

    pub async fn close_view(
        &self,
        owner: &str,
        request: CloseViewRequest,
    ) -> Result<CloseViewResult, ServiceError> {
        let mut views = self.views.write().await;
        let view = views
            .get(&request.view_id)
            .ok_or(ServiceError::ViewNotFound)?;
        require_owner(view, owner)?;
        if view.record.revision != request.expected_revision {
            return Err(ServiceError::RevisionConflict {
                expected: request.expected_revision,
                actual: view.record.revision,
            });
        }
        let view = views.remove(&request.view_id).expect("view checked above");
        view.close_token.cancel();
        Ok(CloseViewResult {
            view_id: request.view_id,
            closed: true,
        })
    }

    pub async fn get_view(
        &self,
        owner: &str,
        view_id: &ViewId,
    ) -> Result<ViewRecord, ServiceError> {
        let views = self.views.read().await;
        let view = views.get(view_id).ok_or(ServiceError::ViewNotFound)?;
        require_owner(view, owner)?;
        Ok(view.record.clone())
    }

    pub async fn list_views(&self, owner: &str) -> Vec<ViewRecord> {
        let views = self.views.read().await;
        let mut result: Vec<_> = views
            .values()
            .filter(|view| view.owner == owner)
            .map(|view| view.record.clone())
            .collect();
        result.sort_by_key(|view| view.created_at);
        result
    }

    pub fn get_frame(
        &self,
        owner: &str,
        frame_id: &FrameId,
    ) -> Result<Arc<CapturedFrame>, ServiceError> {
        let frames = self.frames.lock();
        let stored = frames
            .records
            .get(frame_id)
            .ok_or(ServiceError::FrameNotFound)?;
        if stored.owner != owner {
            return Err(ServiceError::FrameNotFound);
        }
        Ok(stored.frame.clone())
    }

    pub fn list_frames(&self, owner: &str) -> Vec<FrameRecord> {
        let frames = self.frames.lock();
        frames
            .order
            .iter()
            .filter_map(|id| frames.records.get(id))
            .filter(|stored| stored.owner == owner)
            .map(|stored| stored.frame.record.clone())
            .collect()
    }

    /// Render-cut manifest for the view's current camera and requested preview
    /// policy. The raw GLB bytes it references land in the source byte cache as
    /// a side effect and stay servable through `read_tile_bytes`.
    pub async fn preview_scene(
        &self,
        owner: &str,
        view_id: &ViewId,
        policy: PreviewScenePolicy,
        cancellation: CancellationToken,
    ) -> Result<PreviewSceneRecord, ServiceError> {
        policy.validate(&self.config.capture_limits)?;
        let view = self.get_view(owner, view_id).await?;
        let runtime = self.layer_runtime(&view.scene_layer).await?;
        let deadline = Instant::now() + Duration::from_millis(SCENE_DEADLINE_MS);
        let resolved = view.resolved_camera.clone();
        let (selection, render_tiles, manifest) = {
            let mut runtime = tokio::select! {
                () = cancellation.cancelled() => return Err(ServiceError::Cancelled),
                runtime = runtime.lock() => runtime,
            };
            runtime
                .ensure_open(&cancellation, self.config.max_tree_nodes)
                .await?;
            let (selection, render_tiles) = runtime
                .prepare_render_cut(
                    &resolved,
                    policy.width_px,
                    policy.height_px,
                    f64::from(policy.max_screen_error_px),
                    deadline,
                    DeadlineBehavior::ReturnBestAvailable,
                    self.config.max_concurrent_loads,
                    self.config.max_tree_nodes,
                    &cancellation,
                )
                .await?;
            let manifest = runtime.scene_tiles(&selection.render);
            (selection, render_tiles, manifest)
        };
        if cancellation.is_cancelled() {
            return Err(ServiceError::Cancelled);
        }
        let source = self
            .catalog
            .get(&view.scene_layer)
            .ok_or_else(|| ServiceError::LayerNotFound(view.scene_layer.clone()))?;
        let attribution = render_tiles
            .iter()
            .flat_map(|tile| tile.content.attribution.iter().cloned())
            .collect::<BTreeSet<_>>();
        let truncated = manifest.len() > SCENE_MAX_TILES;
        let mut tiles = Vec::new();
        {
            let mut registry = self.tiles.lock();
            for (location, ecef_from_content) in manifest.into_iter().take(SCENE_MAX_TILES) {
                let location = credential_free_location(&location);
                let tile_key = sha256_hex(format!("{}\n{location}", view.scene_layer).as_bytes());
                let byte_length = source.cached_content_length(&location);
                let oversize = byte_length.is_some_and(|length| length > MAX_TILE_RESOURCE_BYTES);
                registry.register(
                    tile_key.clone(),
                    TileTokenEntry {
                        layer: view.scene_layer.clone(),
                        location,
                    },
                );
                tiles.push(SceneTileRecord {
                    tile_uri: uris::tile(&tile_key),
                    ecef_from_content,
                    byte_length,
                    oversize,
                });
            }
        }
        Ok(PreviewSceneRecord {
            view_id: view.view_id,
            view_revision: view.revision,
            scene_layer: view.scene_layer,
            local_origin: resolved.position,
            local_from_ecef: world_from_ecef(resolved.position).to_cols_array(),
            resolved_camera: resolved,
            width_px: policy.width_px,
            height_px: policy.height_px,
            max_screen_error_px: f64::from(policy.max_screen_error_px),
            detail_complete: selection.detail_complete,
            truncated,
            attribution: AttributionSet {
                lines: attribution.into_iter().collect(),
            },
            tiles,
        })
    }

    /// Raw draco GLB bytes for a preview-tile token, from the source byte
    /// cache or a refetch under the source's own credential and host rules.
    pub async fn read_tile_bytes(
        &self,
        tile_key: &str,
        cancellation: CancellationToken,
    ) -> Result<(Arc<Vec<u8>>, &'static str), ServiceError> {
        let entry = self
            .tiles
            .lock()
            .get(tile_key)
            .ok_or(ServiceError::TileNotFound)?;
        let source = self
            .catalog
            .get(&entry.layer)
            .ok_or(ServiceError::TileNotFound)?;
        let response = source.load_content(&entry.location, &cancellation).await?;
        if response.bytes.len() as u64 > MAX_TILE_RESOURCE_BYTES {
            return Err(ServiceError::TileTooLarge);
        }
        Ok((response.bytes, "model/gltf-binary"))
    }

    pub async fn capture_frame(
        &self,
        owner: &str,
        request: CaptureFrameRequest,
        cancellation: CancellationToken,
    ) -> Result<Arc<CapturedFrame>, ServiceError> {
        let view = self.capture_snapshot(owner, &request).await?;
        self.capture_snapshot_frame(owner, view, request.policy, cancellation, false)
            .await
    }

    pub async fn capture_snapshot(
        &self,
        owner: &str,
        request: &CaptureFrameRequest,
    ) -> Result<ViewRecord, ServiceError> {
        request.policy.validate(&self.config.capture_limits)?;
        let views = self.views.read().await;
        let view = views
            .get(&request.view_id)
            .ok_or(ServiceError::ViewNotFound)?;
        require_owner(view, owner)?;
        if view.record.revision != request.expected_revision {
            return Err(ServiceError::RevisionConflict {
                expected: request.expected_revision,
                actual: view.record.revision,
            });
        }
        Ok(view.record.clone())
    }

    pub async fn capture_recoverable_frame(
        &self,
        owner: &str,
        view: ViewRecord,
        policy: crate::contract::CapturePolicy,
        cancellation: CancellationToken,
    ) -> Result<Arc<CapturedFrame>, ServiceError> {
        self.capture_snapshot_frame(owner, view, policy, cancellation, true)
            .await
    }

    pub async fn capture_live_snapshot_frame(
        &self,
        owner: &str,
        view: ViewRecord,
        policy: crate::contract::CapturePolicy,
        cancellation: CancellationToken,
    ) -> Result<Arc<CapturedFrame>, ServiceError> {
        self.capture_snapshot_frame(owner, view, policy, cancellation, false)
            .await
    }

    async fn capture_snapshot_frame(
        &self,
        owner: &str,
        view: ViewRecord,
        policy: crate::contract::CapturePolicy,
        cancellation: CancellationToken,
        permit_detached_snapshot: bool,
    ) -> Result<Arc<CapturedFrame>, ServiceError> {
        policy.validate(&self.config.capture_limits)?;
        let close_token = {
            let views = self.views.read().await;
            match views.get(&view.view_id) {
                Some(current) => {
                    require_owner(current, owner)?;
                    Some(current.close_token.clone())
                }
                None if permit_detached_snapshot => None,
                None => return Err(ServiceError::ViewNotFound),
            }
        };
        let combined = CancellationToken::new();
        let _watcher = AbortOnDrop({
            let combined = combined.clone();
            tokio::spawn(async move {
                if let Some(close_token) = close_token {
                    tokio::select! {
                        () = cancellation.cancelled() => combined.cancel(),
                        () = close_token.cancelled() => combined.cancel(),
                    }
                } else {
                    cancellation.cancelled().await;
                    combined.cancel();
                }
            })
        });

        let runtime = self.layer_runtime(&view.scene_layer).await?;
        let deadline = Instant::now() + Duration::from_millis(u64::from(policy.deadline_ms));
        let resolved = view.resolved_camera.clone();
        let (selection, tiles) = {
            let mut runtime = tokio::select! {
                () = combined.cancelled() => return Err(ServiceError::Cancelled),
                runtime = runtime.lock() => runtime,
            };
            runtime
                .ensure_open(&combined, self.config.max_tree_nodes)
                .await?;
            runtime
                .prepare_render_cut(
                    &resolved,
                    policy.width_px,
                    policy.height_px,
                    f64::from(policy.max_screen_error_px),
                    deadline,
                    policy.deadline_behavior,
                    self.config.max_concurrent_loads,
                    self.config.max_tree_nodes,
                    &combined,
                )
                .await?
        };
        if combined.is_cancelled() {
            return Err(ServiceError::Cancelled);
        }

        let attribution = tiles
            .iter()
            .flat_map(|tile| tile.content.attribution.iter().cloned())
            .collect::<BTreeSet<_>>();
        let rendered = self
            .renderer
            .capture(RenderFrameRequest {
                camera: resolved.clone(),
                local_origin: resolved.position,
                width_px: policy.width_px,
                height_px: policy.height_px,
                encoding: policy.encoding,
                tiles,
            })
            .await?;
        if combined.is_cancelled() {
            return Err(ServiceError::Cancelled);
        }
        if rendered.bytes.len() as u64 > self.config.max_single_frame_bytes {
            return Err(ServiceError::FrameTooLarge);
        }
        let frame_id = FrameId::new(Uuid::now_v7().simple().to_string())?;
        let actual_sse = if selection.actual_max_screen_error_px.is_finite() {
            selection
                .actual_max_screen_error_px
                .min(f64::from(f32::MAX)) as f32
        } else {
            f32::MAX
        };
        let record = FrameRecord {
            frame_uri: uris::frame(&frame_id),
            frame_id: frame_id.clone(),
            view_id: view.view_id,
            view_revision: view.revision,
            scene_layer: view.scene_layer,
            captured_at: Utc::now(),
            resolved_camera: resolved,
            width_px: policy.width_px,
            height_px: policy.height_px,
            mime_type: rendered.mime_type.to_owned(),
            byte_length: rendered.bytes.len() as u64,
            detail_complete: selection.detail_complete,
            actual_max_screen_error_px: actual_sse,
            visible_tile_count: selection.render.len() as u32,
            pending_tile_count: selection.loads.len() as u32,
            attribution: crate::contract::AttributionSet {
                lines: attribution.into_iter().collect(),
            },
        };
        let frame = Arc::new(CapturedFrame {
            record,
            bytes: rendered.bytes,
        });
        self.store_frame(owner.to_owned(), frame.clone());
        Ok(frame)
    }

    async fn layer_runtime(
        &self,
        layer_id: &LayerId,
    ) -> Result<Arc<AsyncMutex<LayerRuntime>>, ServiceError> {
        let mut layers = self.layers.lock().await;
        if let Some(runtime) = layers.get(layer_id) {
            return Ok(runtime.clone());
        }
        let source = self
            .catalog
            .get(layer_id)
            .ok_or_else(|| ServiceError::LayerNotFound(layer_id.clone()))?;
        let runtime = Arc::new(AsyncMutex::new(LayerRuntime {
            source,
            tree: None,
            states: Vec::new(),
            decoded: WeightedLru::new(self.config.decoded_cache_bytes),
            history: SelectionHistory::default(),
            detail_falloff_meters: self.config.detail_falloff_meters,
        }));
        layers.insert(layer_id.clone(), runtime.clone());
        Ok(runtime)
    }

    fn store_frame(&self, owner: OwnerId, frame: Arc<CapturedFrame>) {
        let mut frames = self.frames.lock();
        frames.bytes = frames.bytes.saturating_add(frame.record.byte_length);
        frames.order.push_back(frame.record.frame_id.clone());
        frames
            .records
            .insert(frame.record.frame_id.clone(), StoredFrame { owner, frame });
        while frames.order.len() > self.config.max_frames
            || frames.bytes > self.config.max_frame_bytes
        {
            let Some(oldest) = frames.order.pop_front() else {
                break;
            };
            if let Some(old) = frames.records.remove(&oldest) {
                frames.bytes = frames.bytes.saturating_sub(old.frame.record.byte_length);
            }
        }
    }
}

impl LayerRuntime {
    async fn ensure_open(
        &mut self,
        cancellation: &CancellationToken,
        max_tree_nodes: usize,
    ) -> Result<(), ServiceError> {
        if self.tree.is_some() {
            return Ok(());
        }
        let (tileset, _) = self.source.load_root(cancellation).await?;
        let tree = TileTree::build(&tileset)?;
        if tree.len() > max_tree_nodes {
            return Err(ServiceError::TreeNodeLimit);
        }
        self.states = vec![NodeState::Empty; tree.len()];
        self.history.resize(tree.len());
        self.tree = Some(tree);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_render_cut(
        &mut self,
        camera: &crate::contract::GeodeticCameraPose,
        width: u32,
        height: u32,
        max_sse: f64,
        deadline: Instant,
        deadline_behavior: DeadlineBehavior,
        max_concurrent_loads: usize,
        max_tree_nodes: usize,
        cancellation: &CancellationToken,
    ) -> Result<(Selection, Vec<RenderTile>), ServiceError> {
        loop {
            if cancellation.is_cancelled() {
                return Err(ServiceError::Cancelled);
            }
            let selection_camera =
                selection_camera(camera, width, height, max_sse, self.detail_falloff_meters);
            let readiness = self.readiness();
            let tree = self.tree.as_ref().expect("opened");
            let culled = |tile: usize| {
                let (center, radius) = tree.nodes[tile].volume.bounding_sphere();
                let local_center = selection_camera.local_from_ecef.transform_point3(center);
                let sphere = Sphere {
                    center: Vec3A::from(local_center.as_vec3()),
                    radius: (radius * 1.25) as f32,
                };
                !selection_camera.frustum.intersects_sphere(&sphere, false)
            };
            let selection = select(
                tree,
                &readiness,
                &self.history,
                &culled,
                selection_camera.params,
            );
            tracing::debug!(
                layer = %self.source.layer_id(),
                nodes = self.tree.as_ref().expect("opened").len(),
                render_tiles = selection.render.len(),
                load_candidates = selection.loads.len(),
                covered = selection.covered,
                detail_complete = selection.detail_complete,
                "selected 3D Tiles render cut"
            );
            self.history
                .absorb(&selection, self.tree.as_ref().expect("opened").len());
            let deadline_reached = Instant::now() >= deadline;
            if selection.covered && (selection.detail_complete || deadline_reached) {
                if deadline_reached
                    && !selection.detail_complete
                    && deadline_behavior == DeadlineBehavior::Fail
                {
                    return Err(ServiceError::CaptureDeadline);
                }
                let tiles = self.render_tiles(&selection.render)?;
                return Ok((selection, tiles));
            }
            if deadline_reached {
                return Err(ServiceError::NoCoveredRenderCut);
            }
            let requests: Vec<_> = selection
                .loads
                .iter()
                .filter_map(|request| {
                    let state = self.states.get(request.tile)?;
                    if matches!(state, NodeState::Empty)
                        || matches!(state, NodeState::Ready(key) if !self.decoded.contains_key(key))
                    {
                        let uri = self.tree.as_ref()?.nodes[request.tile]
                            .content_uri
                            .clone()?;
                        Some((request.tile, uri))
                    } else {
                        None
                    }
                })
                .take(max_concurrent_loads)
                .collect();
            if requests.is_empty() {
                if selection.covered && deadline_behavior == DeadlineBehavior::ReturnBestAvailable {
                    let tiles = self.render_tiles(&selection.render)?;
                    return Ok((selection, tiles));
                }
                return Err(ServiceError::NoCoveredRenderCut);
            }
            for (tile, _) in &requests {
                self.states[*tile] = NodeState::Pending;
            }
            let source = self.source.clone();
            let cancellation = cancellation.clone();
            let results: Vec<_> = stream::iter(requests)
                .map(|(tile, uri)| {
                    let source = source.clone();
                    let cancellation = cancellation.clone();
                    async move {
                        let result = async {
                            let response = source.load_content(&uri, &cancellation).await?;
                            if looks_like_tileset(&response.bytes) {
                                let external = source
                                    .parse_external_tileset(&response.bytes, &response.location)?;
                                Ok::<_, ServiceError>((
                                    uri,
                                    LoadedContent::External(Box::new(external)),
                                ))
                            } else {
                                let bytes = response.bytes.clone();
                                let content_hash = sha256_hex(bytes.as_slice());
                                let decoded =
                                    tokio::task::spawn_blocking(move || decode_glb(&bytes))
                                        .await
                                        .map_err(ServiceError::DecodeTask)??;
                                Ok((
                                    uri,
                                    LoadedContent::Tile {
                                        content: Arc::new(decoded),
                                        content_hash,
                                    },
                                ))
                            }
                        }
                        .await;
                        (tile, result)
                    }
                })
                .buffer_unordered(max_concurrent_loads)
                .collect()
                .await;

            let mut first_load_error = None;
            for result in results {
                match result {
                    (
                        tile,
                        Ok((
                            uri,
                            LoadedContent::Tile {
                                content,
                                content_hash,
                            },
                        )),
                    ) => {
                        let bytes = content.estimated_bytes;
                        let content_key = format!("{uri}#sha256:{content_hash}");
                        self.decoded.insert(content_key.clone(), content, bytes);
                        self.states[tile] = NodeState::Ready(content_key);
                    }
                    (tile, Ok((_, LoadedContent::External(tileset)))) => {
                        let tree = self.tree.as_mut().expect("opened");
                        tree.graft(tile, &tileset)?;
                        if tree.len() > max_tree_nodes {
                            return Err(ServiceError::TreeNodeLimit);
                        }
                        self.states[tile] = NodeState::External;
                        self.states.resize(tree.len(), NodeState::Empty);
                        self.history.resize(tree.len());
                    }
                    (tile, Err(error)) => {
                        tracing::warn!(layer = %self.source.layer_id(), error = %error, "tile load failed");
                        self.states[tile] = NodeState::Empty;
                        if first_load_error.is_none() {
                            first_load_error = Some(error);
                        }
                    }
                }
            }
            if let Some(error) = first_load_error {
                if selection.covered
                    && deadline_behavior == DeadlineBehavior::ReturnBestAvailable
                    && matches!(
                        error,
                        ServiceError::Source(SourceError::RequestBudgetExhausted)
                    )
                {
                    let tiles = self.render_tiles(&selection.render)?;
                    return Ok((selection, tiles));
                }
                return Err(error);
            }
        }
    }

    fn readiness(&self) -> Vec<TileReadiness> {
        self.states
            .iter()
            .enumerate()
            .map(|(index, state)| match state {
                NodeState::Empty => TileReadiness::Empty,
                NodeState::Pending => TileReadiness::Pending,
                NodeState::Ready(key) if self.decoded.contains_key(key) => TileReadiness::Ready,
                NodeState::Ready(_) => TileReadiness::Empty,
                NodeState::External => {
                    if self.tree.as_ref().is_some_and(|tree| {
                        tree.nodes
                            .get(index)
                            .is_some_and(|node| node.content_uri.is_none())
                    }) {
                        TileReadiness::Empty
                    } else {
                        TileReadiness::Failed
                    }
                }
            })
            .collect()
    }

    /// Content locations and ECEF transforms for a render cut, for scene
    /// manifests. Transforms are served verbatim from the tree (glTF Y-up to
    /// Z-up baked in at build; CESIUM_RTC stays inside the GLB payload).
    fn scene_tiles(&self, indices: &[usize]) -> Vec<(String, [f64; 16])> {
        let tree = self.tree.as_ref().expect("opened");
        indices
            .iter()
            .filter_map(|&index| {
                let node = &tree.nodes[index];
                node.content_uri
                    .clone()
                    .map(|uri| (uri, node.ecef_from_content.to_cols_array()))
            })
            .collect()
    }

    fn render_tiles(&mut self, indices: &[usize]) -> Result<Vec<RenderTile>, ServiceError> {
        let tree = self.tree.as_ref().expect("opened");
        indices
            .iter()
            .map(|&index| {
                let NodeState::Ready(key) = &self.states[index] else {
                    return Err(ServiceError::RenderCutInvariant);
                };
                let content = self
                    .decoded
                    .get(key)
                    .ok_or(ServiceError::RenderCutInvariant)?;
                Ok(RenderTile {
                    cache_key: format!("{}:{key}", self.source.layer_id()),
                    ecef_from_content: tree.nodes[index].ecef_from_content,
                    content,
                })
            })
            .collect()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

struct SelectionCamera {
    params: SelectionParams,
    frustum: Frustum,
    local_from_ecef: glam::DMat4,
}

fn selection_camera(
    camera: &crate::contract::GeodeticCameraPose,
    width: u32,
    height: u32,
    max_sse: f64,
    detail_falloff_meters: f64,
) -> SelectionCamera {
    let (forward, _, _) = camera_ecef_basis(camera);
    let vertical_fov = f64::from(camera.vertical_fov_degrees).to_radians();
    let tan_half_vertical = (vertical_fov * 0.5).tan();
    let aspect = f64::from(width) / f64::from(height);
    let local_from_ecef = world_from_ecef(camera.position);
    let local_from_camera = camera_world_transform(camera, camera.position);
    let projection = PerspectiveProjection {
        fov: camera.vertical_fov_degrees.to_radians(),
        aspect_ratio: aspect as f32,
        near: 0.1,
        far: 100_000_000.0,
        ..Default::default()
    };
    let clip_from_local = projection.get_clip_from_view()
        * Mat4::from_cols_array(
            &local_from_camera
                .inverse()
                .to_cols_array()
                .map(|v| v as f32),
        );
    SelectionCamera {
        params: SelectionParams {
            camera_position: geodetic_to_ecef(camera.position),
            camera_forward: forward,
            focal_length_px: f64::from(height) / (2.0 * tan_half_vertical),
            max_screen_error_px: max_sse,
            detail_falloff_meters,
            camera_height_meters: camera.position.ellipsoidal_height_meters.max(0.0),
        },
        frustum: Frustum(ViewFrustum::from_clip_from_world(&clip_from_local)),
        local_from_ecef,
    }
}

fn require_owner(view: &InternalView, owner: &str) -> Result<(), ServiceError> {
    if view.owner == owner {
        Ok(())
    } else {
        Err(ServiceError::ViewNotFound)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error("scene layer `{0}` is not configured")]
    LayerNotFound(LayerId),
    #[error("view was not found")]
    ViewNotFound,
    #[error("frame was not found")]
    FrameNotFound,
    #[error("preview tile is unknown or expired; re-read the view scene resource")]
    TileNotFound,
    #[error("preview tile exceeds the per-read byte limit and must be skipped")]
    TileTooLarge,
    #[error("view revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("global view limit is reached")]
    ViewLimit,
    #[error("owner view limit is reached")]
    OwnerViewLimit,
    #[error("capture was cancelled")]
    Cancelled,
    #[error("capture deadline expired before requested detail was available")]
    CaptureDeadline,
    #[error("no covered render cut was available")]
    NoCoveredRenderCut,
    #[error("render cut referenced unavailable content")]
    RenderCutInvariant,
    #[error("tileset exceeds the configured tree-node limit")]
    TreeNodeLimit,
    #[error("tile decode worker failed: {0}")]
    DecodeTask(tokio::task::JoinError),
    #[error("encoded frame exceeds the configured per-frame byte limit")]
    FrameTooLarge,
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Traversal(#[from] crate::tiles::traversal::TraversalError),
    #[error(transparent)]
    Decode(#[from] crate::decode::DecodeError),
    #[error(transparent)]
    Renderer(#[from] RendererError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        CameraDefinition, GeodeticCameraPose, HeadingPitchRoll, Wgs84Position3d,
    };

    fn camera() -> CameraDefinition {
        CameraDefinition::Pose(GeodeticCameraPose {
            position: Wgs84Position3d {
                latitude_degrees: 59.3,
                longitude_degrees: 18.0,
                ellipsoidal_height_meters: 1_000.0,
            },
            orientation: HeadingPitchRoll {
                heading_degrees: 0.0,
                pitch_degrees: -45.0,
                roll_degrees: 0.0,
            },
            vertical_fov_degrees: 45.0,
        })
    }

    #[test]
    fn selection_parameters_use_physical_viewport_height() {
        let pose = resolve_camera(&camera()).unwrap();
        let parameters = selection_camera(&pose, 1920, 1080, 10.0, 2_000.0).params;
        let expected = 1080.0 / (2.0 * (45f64.to_radians() / 2.0).tan());
        assert!((parameters.focal_length_px - expected).abs() < 1e-9);
    }

    fn token_entry(location: &str) -> TileTokenEntry {
        TileTokenEntry {
            layer: LayerId::new("layer").unwrap(),
            location: location.to_owned(),
        }
    }

    #[test]
    fn tile_tokens_are_deterministic_and_credential_free() {
        let location =
            credential_free_location("https://tile.googleapis.com/v1/t.glb?session=S&key=SECRET");
        assert!(!location.contains("SECRET"));
        let first = sha256_hex(format!("layer\n{location}").as_bytes());
        let second = sha256_hex(format!("layer\n{location}").as_bytes());
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn tile_token_registry_evicts_oldest_beyond_the_cap() {
        let mut registry = TileTokenRegistry::default();
        for index in 0..=MAX_TILE_TOKENS {
            registry.register(format!("key-{index}"), token_entry("loc"));
        }
        assert!(registry.get("key-0").is_none());
        assert!(registry.get(&format!("key-{MAX_TILE_TOKENS}")).is_some());
        assert_eq!(registry.entries.len(), MAX_TILE_TOKENS);
    }

    #[test]
    fn tile_token_reregistration_does_not_duplicate_order() {
        let mut registry = TileTokenRegistry::default();
        registry.register("key".to_owned(), token_entry("a"));
        registry.register("key".to_owned(), token_entry("b"));
        assert_eq!(registry.order.len(), 1);
        assert_eq!(registry.get("key").unwrap().location, "b");
    }
}
