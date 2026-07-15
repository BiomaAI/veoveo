use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{Arc, mpsc},
    thread,
};

use bevy::{
    app::SubApps,
    asset::RenderAssetUsages,
    camera::{PerspectiveProjection, Projection, RenderTarget},
    core_pipeline::tonemapping::Tonemapping,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, Mesh, Mesh3d, PrimitiveTopology},
    pbr::{MeshMaterial3d, StandardMaterial},
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, PollType, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderAdapterInfo, RenderDevice},
        settings::{Backends, PowerPreference, RenderCreation, WgpuSettings},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::ExitCondition,
};
use glam::DMat4;
use tokio::sync::oneshot;

use crate::{
    contract::{FrameEncoding, GeodeticCameraPose, Wgs84Position3d},
    decode::{CpuImage, CpuMaterial, CpuTileContent, CpuWrapMode},
    geodesy::{camera_world_transform, world_from_ecef},
};

#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub require_nvidia: bool,
    pub gpu_cache_bytes: u64,
    pub jpeg_quality: u8,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct GpuAdapterStatus {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub vendor: u32,
    pub hardware_accelerated: bool,
    pub nvidia: bool,
}

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct GpuCacheStats {
    pub resident_tiles: usize,
    pub resident_bytes: u64,
    pub tile_uploads: u64,
}

#[derive(Clone)]
pub struct RendererHandle {
    sender: Arc<RendererSender>,
    adapter: GpuAdapterStatus,
}

struct RendererSender {
    commands: mpsc::Sender<RenderCommand>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for RendererSender {
    fn drop(&mut self) {
        let _ = self.commands.send(RenderCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub struct RenderTile {
    pub cache_key: String,
    pub ecef_from_content: DMat4,
    pub content: Arc<CpuTileContent>,
}

pub struct RenderFrameRequest {
    pub camera: GeodeticCameraPose,
    pub local_origin: Wgs84Position3d,
    pub width_px: u32,
    pub height_px: u32,
    pub encoding: FrameEncoding,
    pub tiles: Vec<RenderTile>,
}

#[derive(Debug)]
pub struct RenderedImage {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
}

enum RenderCommand {
    Capture {
        request: RenderFrameRequest,
        response: oneshot::Sender<Result<RenderedImage, RendererError>>,
    },
    Stats {
        response: oneshot::Sender<GpuCacheStats>,
    },
    Shutdown,
}

impl RendererHandle {
    pub fn start(config: RendererConfig) -> Result<Self, RendererError> {
        let (sender, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("view-bevy-renderer".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Renderer::new(config)
                }))
                .map_err(|_| RendererError::InitializationPanic)
                .and_then(|value| value);
                match result {
                    Ok(mut renderer) => {
                        let _ = ready_sender.send(Ok(renderer.adapter.clone()));
                        renderer.run(receiver);
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(RendererError::ThreadSpawn)?;
        let adapter = ready_receiver
            .recv()
            .map_err(|_| RendererError::RendererStopped)??;
        Ok(Self {
            sender: Arc::new(RendererSender {
                commands: sender,
                thread: Some(thread),
            }),
            adapter,
        })
    }

    pub fn adapter(&self) -> &GpuAdapterStatus {
        &self.adapter
    }

    pub async fn capture(
        &self,
        request: RenderFrameRequest,
    ) -> Result<RenderedImage, RendererError> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .commands
            .send(RenderCommand::Capture { request, response })
            .map_err(|_| RendererError::RendererStopped)?;
        receiver.await.map_err(|_| RendererError::RendererStopped)?
    }

    pub async fn stats(&self) -> Result<GpuCacheStats, RendererError> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .commands
            .send(RenderCommand::Stats { response })
            .map_err(|_| RendererError::RendererStopped)?;
        receiver.await.map_err(|_| RendererError::RendererStopped)
    }
}

struct Renderer {
    apps: SubApps,
    adapter: GpuAdapterStatus,
    cache: HashMap<String, GpuTile>,
    cache_clock: u64,
    cache_bytes: u64,
    tile_uploads: u64,
    config: RendererConfig,
}

struct GpuTile {
    primitives: Vec<GpuPrimitive>,
    images: Vec<Handle<Image>>,
    bytes: u64,
    last_used: u64,
}

struct GpuPrimitive {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    node_transform: DMat4,
}

impl Renderer {
    fn new(config: RendererConfig) -> Result<Self, RendererError> {
        let render_plugin = RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                backends: Some(Backends::VULKAN),
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                ..default()
            })),
            synchronous_pipeline_compilation: true,
            ..default()
        };
        let window_plugin = WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(DefaultPlugins.set(window_plugin).set(render_plugin));
        app.finish();
        app.cleanup();
        let adapter = adapter_status(app.world())?;
        if !adapter.hardware_accelerated {
            return Err(RendererError::SoftwareAdapter(adapter.name));
        }
        if config.require_nvidia && !adapter.nvidia {
            return Err(RendererError::NonNvidiaAdapter(adapter.name));
        }
        Ok(Self {
            apps: std::mem::take(app.sub_apps_mut()),
            adapter,
            cache: HashMap::new(),
            cache_clock: 0,
            cache_bytes: 0,
            tile_uploads: 0,
            config,
        })
    }

    fn run(&mut self, receiver: mpsc::Receiver<RenderCommand>) {
        while let Ok(command) = receiver.recv() {
            match command {
                RenderCommand::Capture { request, response } => {
                    let result = self.capture(request);
                    let _ = response.send(result);
                }
                RenderCommand::Stats { response } => {
                    let _ = response.send(GpuCacheStats {
                        resident_tiles: self.cache.len(),
                        resident_bytes: self.cache_bytes,
                        tile_uploads: self.tile_uploads,
                    });
                }
                RenderCommand::Shutdown => {
                    if let Err(error) = self.update() {
                        tracing::warn!(%error, "renderer shutdown update failed");
                    }
                    break;
                }
            }
        }
    }

    fn capture(&mut self, request: RenderFrameRequest) -> Result<RenderedImage, RendererError> {
        if request.tiles.is_empty() {
            return Err(RendererError::EmptyRenderCut);
        }
        self.cache_clock = self.cache_clock.wrapping_add(1);
        let selected: HashSet<String> = request
            .tiles
            .iter()
            .map(|tile| tile.cache_key.clone())
            .collect();
        for tile in &request.tiles {
            self.ensure_tile(tile)?;
            if let Some(cached) = self.cache.get_mut(&tile.cache_key) {
                cached.last_used = self.cache_clock;
            }
        }

        let world_from_ecef = world_from_ecef(request.local_origin);
        let mut spawned = Vec::new();
        for tile in &request.tiles {
            let cached = self
                .cache
                .get(&tile.cache_key)
                .ok_or(RendererError::GpuCacheInvariant)?;
            let rtc = tile
                .content
                .rtc_center_ecef
                .map(DMat4::from_translation)
                .unwrap_or(DMat4::IDENTITY);
            for primitive in &cached.primitives {
                let transform =
                    world_from_ecef * tile.ecef_from_content * rtc * primitive.node_transform;
                let entity = self.apps.main.world_mut().spawn((
                    Mesh3d(primitive.mesh.clone()),
                    MeshMaterial3d(primitive.material.clone()),
                    Transform::from_matrix(bevy::math::Mat4::from_cols_array(
                        &transform.to_cols_array().map(|value| value as f32),
                    )),
                ));
                spawned.push(entity.id());
            }
        }

        let target = self.create_render_target(request.width_px, request.height_px);
        let camera_transform = camera_world_transform(&request.camera, request.local_origin);
        let camera = self
            .apps
            .main
            .world_mut()
            .spawn((
                Camera3d::default(),
                Tonemapping::None,
                Projection::Perspective(PerspectiveProjection {
                    fov: request.camera.vertical_fov_degrees.to_radians(),
                    aspect_ratio: request.width_px as f32 / request.height_px as f32,
                    near: 0.1,
                    far: 100_000_000.0,
                    ..default()
                }),
                Transform::from_matrix(bevy::math::Mat4::from_cols_array(
                    &camera_transform.to_cols_array().map(|value| value as f32),
                )),
                target.clone(),
            ))
            .id();
        spawned.push(camera);

        // Give freshly inserted meshes, materials, images, and the camera one
        // extraction/upload frame before requesting readback. A screenshot
        // requested in the insertion frame can legally capture only the clear
        // color while render assets are still moving into the render world.
        self.update()?;

        let (captured_sender, captured_receiver) = mpsc::sync_channel(1);
        self.apps
            .main
            .world_mut()
            .spawn(Screenshot::image(
                target
                    .as_image()
                    .ok_or(RendererError::RenderTargetInvariant)?
                    .clone(),
            ))
            .observe(move |event: On<ScreenshotCaptured>| {
                let _ = captured_sender.send(event.image.clone());
            });

        let mut captured = None;
        for _ in 0..16 {
            self.update()?;
            if let Ok(image) = captured_receiver.try_recv() {
                captured = Some(image);
                break;
            }
        }
        for entity in spawned {
            if let Ok(entity) = self.apps.main.world_mut().get_entity_mut(entity) {
                entity.despawn();
            }
        }
        if let Some(handle) = target.as_image() {
            self.apps
                .main
                .world_mut()
                .resource_mut::<Assets<Image>>()
                .remove(handle.id());
        }
        self.update()?;
        self.evict(&selected);

        let image = captured.ok_or(RendererError::ScreenshotTimeout)?;
        encode_image(image, request.encoding, self.config.jpeg_quality)
    }

    fn update(&mut self) -> Result<(), RendererError> {
        self.apps.update();
        self.apps
            .main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device()
            .poll(PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| RendererError::DevicePoll(error.to_string()))?;
        Ok(())
    }

    fn create_render_target(&mut self, width: u32, height: u32) -> RenderTarget {
        let mut image = Image::new_uninit(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        self.apps
            .main
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(image)
            .into()
    }

    fn ensure_tile(&mut self, tile: &RenderTile) -> Result<(), RendererError> {
        if self.cache.contains_key(&tile.cache_key) {
            return Ok(());
        }
        let mut primitives = Vec::with_capacity(tile.content.primitives.len());
        let mut images = Vec::new();
        for primitive in &tile.content.primitives {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, primitive.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, primitive.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, primitive.texcoords.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, primitive.colors.clone());
            mesh.insert_indices(Indices::U32(primitive.indices.clone()));
            let mesh = self
                .apps
                .main
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(mesh);
            let (material, image) = self.create_material(&primitive.material);
            if let Some(image) = image {
                images.push(image);
            }
            primitives.push(GpuPrimitive {
                mesh,
                material,
                node_transform: primitive.node_transform,
            });
        }
        self.cache_bytes = self
            .cache_bytes
            .saturating_add(tile.content.estimated_bytes);
        self.tile_uploads = self.tile_uploads.saturating_add(1);
        self.cache.insert(
            tile.cache_key.clone(),
            GpuTile {
                primitives,
                images,
                bytes: tile.content.estimated_bytes,
                last_used: self.cache_clock,
            },
        );
        Ok(())
    }

    fn create_material(
        &mut self,
        material: &CpuMaterial,
    ) -> (Handle<StandardMaterial>, Option<Handle<Image>>) {
        let image = material
            .base_color_texture
            .as_ref()
            .map(|image| self.create_image(image, material));
        let standard = StandardMaterial {
            base_color: Color::linear_rgba(
                material.base_color[0],
                material.base_color[1],
                material.base_color[2],
                material.base_color[3],
            ),
            base_color_texture: image.clone(),
            unlit: material.unlit,
            cull_mode: (!material.double_sided)
                .then_some(bevy::render::render_resource::Face::Back),
            alpha_mode: if material.alpha_blend {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            perceptual_roughness: 1.0,
            metallic: 0.0,
            ..default()
        };
        let handle = self
            .apps
            .main
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(standard);
        (handle, image)
    }

    fn create_image(&mut self, image: &CpuImage, material: &CpuMaterial) -> Handle<Image> {
        let mut asset = Image::new(
            Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            image.rgba8.clone(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        asset.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: address_mode(material.sampler.wrap_u),
            address_mode_v: address_mode(material.sampler.wrap_v),
            ..default()
        });
        self.apps
            .main
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(asset)
    }

    fn evict(&mut self, pinned: &HashSet<String>) {
        while self.cache_bytes > self.config.gpu_cache_bytes {
            let victim = self
                .cache
                .iter()
                .filter(|(key, _)| !pinned.contains(*key))
                .min_by_key(|(_, tile)| tile.last_used)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else { break };
            let Some(tile) = self.cache.remove(&victim) else {
                continue;
            };
            self.cache_bytes = self.cache_bytes.saturating_sub(tile.bytes);
            let world = self.apps.main.world_mut();
            for primitive in tile.primitives {
                world
                    .resource_mut::<Assets<Mesh>>()
                    .remove(primitive.mesh.id());
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .remove(primitive.material.id());
            }
            for image in tile.images {
                world.resource_mut::<Assets<Image>>().remove(image.id());
            }
        }
    }
}

fn adapter_status(world: &World) -> Result<GpuAdapterStatus, RendererError> {
    let info = world
        .get_resource::<RenderAdapterInfo>()
        .ok_or(RendererError::AdapterInfoMissing)?;
    let device_type = format!("{:?}", info.device_type);
    let backend = format!("{:?}", info.backend);
    let hardware_accelerated = device_type != "Cpu";
    let nvidia = info.vendor == 0x10DE || info.name.to_ascii_lowercase().contains("nvidia");
    Ok(GpuAdapterStatus {
        name: info.name.clone(),
        backend,
        device_type,
        vendor: info.vendor,
        hardware_accelerated,
        nvidia,
    })
}

fn address_mode(mode: CpuWrapMode) -> ImageAddressMode {
    match mode {
        CpuWrapMode::ClampToEdge => ImageAddressMode::ClampToEdge,
        CpuWrapMode::MirroredRepeat => ImageAddressMode::MirrorRepeat,
        CpuWrapMode::Repeat => ImageAddressMode::Repeat,
    }
}

fn encode_image(
    image: Image,
    encoding: FrameEncoding,
    jpeg_quality: u8,
) -> Result<RenderedImage, RendererError> {
    let dynamic = image
        .try_into_dynamic()
        .map_err(|error| RendererError::ImageConversion(error.to_string()))?
        .to_rgb8();
    let mut bytes = Vec::new();
    match encoding {
        FrameEncoding::Png => {
            image::DynamicImage::ImageRgb8(dynamic)
                .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
                .map_err(RendererError::ImageEncoding)?;
            Ok(RenderedImage {
                bytes,
                mime_type: "image/png",
            })
        }
        FrameEncoding::Jpeg => {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, jpeg_quality);
            encoder
                .encode_image(&image::DynamicImage::ImageRgb8(dynamic))
                .map_err(RendererError::ImageEncoding)?;
            Ok(RenderedImage {
                bytes,
                mime_type: "image/jpeg",
            })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("renderer thread could not start: {0}")]
    ThreadSpawn(std::io::Error),
    #[error("renderer initialization panicked")]
    InitializationPanic,
    #[error("renderer stopped")]
    RendererStopped,
    #[error("Bevy render adapter information is missing")]
    AdapterInfoMissing,
    #[error("software rendering adapter `{0}` is not allowed")]
    SoftwareAdapter(String),
    #[error("non-NVIDIA rendering adapter `{0}` is not allowed")]
    NonNvidiaAdapter(String),
    #[error("render cut contains no tiles")]
    EmptyRenderCut,
    #[error("GPU tile cache invariant failed")]
    GpuCacheInvariant,
    #[error("render target invariant failed")]
    RenderTargetInvariant,
    #[error("GPU screenshot did not complete")]
    ScreenshotTimeout,
    #[error("GPU device poll failed: {0}")]
    DevicePoll(String),
    #[error("captured image conversion failed: {0}")]
    ImageConversion(String),
    #[error("captured image encoding failed: {0}")]
    ImageEncoding(image::ImageError),
}
