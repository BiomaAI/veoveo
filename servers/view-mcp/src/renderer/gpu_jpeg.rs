//! GPU JPEG encoding on the renderer's serial NVIDIA device owner.
use std::{
    borrow::Cow,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use cudarc::driver::CudaStream;

use super::{
    nvjpeg_bindings as sys,
    vulkan_cuda::{DRAIN_TIMEOUT, RgbLayout, STARTUP_TIMEOUT, SharedRgb, VulkanCuda},
};

#[derive(Debug, thiserror::Error)]
pub enum GpuJpegError {
    #[error("GPU JPEG requires NVIDIA Vulkan")]
    VulkanRequired,
    #[error("GPU JPEG requires an NVIDIA device")]
    NvidiaRequired,
    #[error("GPU JPEG requires exportable Vulkan opaque-FD buffer memory")]
    ExternalMemoryUnsupported,
    #[error("Vulkan and CUDA device UUIDs do not match")]
    DeviceMismatch,
    #[error("GPU JPEG dimensions or encoded length exceed the admitted layout")]
    Layout,
    #[error("GPU JPEG library unavailable: {0}")]
    Library(#[from] libloading::Error),
    #[error(
        "GPU JPEG requires nvJPEG {}.{}.{}",
        sys::NVJPEG_VER_MAJOR,
        sys::NVJPEG_VER_MINOR,
        sys::NVJPEG_VER_PATCH
    )]
    LibraryVersion,
    #[error("nvJPEG {operation} failed: {status:?}")]
    Nvjpeg {
        operation: &'static str,
        status: sys::nvjpegStatus_t,
    },
    #[error("CUDA JPEG interop failed: {0}")]
    Cuda(#[from] cudarc::driver::DriverError),
    #[error("Vulkan JPEG interop failed: {0}")]
    Vulkan(#[from] ash::vk::Result),
    #[error("GPU JPEG packing validation failed: {0}")]
    Packing(String),
}

fn check(operation: &'static str, status: sys::nvjpegStatus_t) -> Result<(), GpuJpegError> {
    if status == sys::nvjpegStatus_t::NVJPEG_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(GpuJpegError::Nvjpeg { operation, status })
    }
}

pub(super) fn settle_native_status(
    operation: &'static str,
    status: sys::nvjpegStatus_t,
    complete: impl FnOnce(),
) -> Result<(), GpuJpegError> {
    complete();
    check(operation, status)
}

pub(super) struct GpuJpeg {
    // Native pointers never cross the dedicated renderer thread. No Send/Sync impl.
    api: sys::Nvjpeg,
    handle: sys::nvjpegHandle_t,
    state: sys::nvjpegEncoderState_t,
    params: sys::nvjpegEncoderParams_t,
    stream: Arc<CudaStream>,
    interop: VulkanCuda,
    pipeline: wgpu::ComputePipeline,
    device: wgpu::Device,
    encoded_frames: u64,
    orderly_drained: Arc<AtomicBool>,
}

impl GpuJpeg {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quality: u8,
    ) -> Result<Self, GpuJpegError> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        if !(1..=100).contains(&quality) {
            return Err(GpuJpegError::Layout);
        }
        let orderly_drained = Arc::new(AtomicBool::new(false));
        let device_drained = orderly_drained.clone();
        device.set_device_lost_callback(move |reason, _message| {
            device_loss(reason, device_drained.load(Ordering::Acquire));
        });
        device.on_uncaptured_error(Arc::new(|_error| {
            fatal_in_flight("WGPU uncaptured device error", "native outcome uncertain");
        }));
        let interop = VulkanCuda::new(device)?;
        let stream = interop.context.new_stream()?;
        // SAFETY: generated bindings from the pinned maintained NVIDIA header;
        // the library stays live until all native handles have been destroyed.
        let api = unsafe { sys::Nvjpeg::new("libnvjpeg.so.13")? };
        for (property, expected) in [
            (
                sys::libraryPropertyType_t_MAJOR_VERSION,
                sys::NVJPEG_VER_MAJOR as i32,
            ),
            (
                sys::libraryPropertyType_t_MINOR_VERSION,
                sys::NVJPEG_VER_MINOR as i32,
            ),
            (
                sys::libraryPropertyType_t_PATCH_LEVEL,
                sys::NVJPEG_VER_PATCH as i32,
            ),
        ] {
            let mut value = 0;
            check("version", unsafe {
                api.nvjpegGetProperty(property, &mut value)
            })?;
            if value != expected {
                return Err(GpuJpegError::LibraryVersion);
            }
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("View stored-byte RGB packing"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("packing.wgsl"))),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("View GPU JPEG packing"),
            layout: None,
            module: &shader,
            entry_point: Some("pack"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = futures::executor::block_on(scope.pop()) {
            return Err(GpuJpegError::Packing(error.to_string()));
        }
        let mut encoder = Self {
            api,
            handle: ptr::null_mut(),
            state: ptr::null_mut(),
            params: ptr::null_mut(),
            stream,
            interop,
            pipeline,
            device: device.clone(),
            encoded_frames: 0,
            orderly_drained,
        };
        check("create", unsafe {
            encoder.api.nvjpegCreateSimple(&mut encoder.handle)
        })?;
        check("GPU backend", unsafe {
            encoder.api.nvjpegEncoderStateCreateWithBackend(
                encoder.handle,
                &mut encoder.state,
                sys::nvjpegEncBackend_t::NVJPEG_ENC_BACKEND_GPU,
                encoder.stream.cu_stream().cast(),
            )
        })?;
        check("parameters", unsafe {
            encoder.api.nvjpegEncoderParamsCreate(
                encoder.handle,
                &mut encoder.params,
                encoder.stream.cu_stream().cast(),
            )
        })?;
        let result = (|| {
            check("quality", unsafe {
                encoder.api.nvjpegEncoderParamsSetQuality(
                    encoder.params,
                    i32::from(quality),
                    encoder.stream.cu_stream().cast(),
                )
            })?;
            check("sampling", unsafe {
                encoder.api.nvjpegEncoderParamsSetSamplingFactors(
                    encoder.params,
                    sys::nvjpegChromaSubsampling_t::NVJPEG_CSS_444,
                    encoder.stream.cu_stream().cast(),
                )
            })
        })();
        encoder.complete(deadline);
        result?;
        // Readiness requires the exact pack/interop/GPU encode/retrieve path, with
        // pixels produced by a GPU render pass rather than a host upload.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("View GPU JPEG readiness image"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let view = texture.create_view(&Default::default());
        let mut commands = device.create_command_encoder(&Default::default());
        {
            let _clear = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("View GPU JPEG readiness clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.25,
                            g: 0.5,
                            b: 0.75,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        native_call("WGPU readiness submission", || {
            queue.submit([commands.finish()])
        });
        let bytes = encoder.encode_before(&texture, queue, deadline)?;
        if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Err(GpuJpegError::Layout);
        }
        encoder.encoded_frames = 0;
        Ok(encoder)
    }

    pub fn uuid(&self) -> [u8; 16] {
        self.interop.uuid
    }
    pub fn encoded_frames(&self) -> u64 {
        self.encoded_frames
    }

    pub fn encode_before(
        &mut self,
        texture: &wgpu::Texture,
        queue: &wgpu::Queue,
        deadline: Instant,
    ) -> Result<Vec<u8>, GpuJpegError> {
        self.interop.context.bind_to_thread()?;
        let layout = RgbLayout::new(texture.width(), texture.height())?;
        if layout.storage_bytes > u64::from(self.device.limits().max_storage_buffer_binding_size) {
            return Err(GpuJpegError::Layout);
        }
        let rgb = self.interop.buffer(layout, deadline)?;
        self.pack(texture, queue, &rgb, deadline)?;
        self.interop.release_to_cuda(&rgb, deadline)?;
        let pointer = rgb.mapped.as_ref().expect("live GPU RGB mapping").pointer();
        let source = sys::nvjpegImage_t {
            channel: [pointer, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()],
            pitch: [layout.pitch, 0, 0, 0],
        };
        // SAFETY: Vulkan released this immutable mapped RGB buffer after its fence;
        // source and native owners stay live through the recorded CUDA completion.
        let status = unsafe {
            self.api.nvjpegEncodeImage(
                self.handle,
                self.state,
                self.params,
                &source,
                sys::nvjpegInputFormat_t::NVJPEG_INPUT_RGBI,
                layout.width,
                layout.height,
                self.stream.cu_stream().cast(),
            )
        };
        settle_native_status("encode", status, || self.complete(deadline))?;
        let mut length = 0;
        let status = unsafe {
            self.api.nvjpegEncodeRetrieveBitstream(
                self.handle,
                self.state,
                ptr::null_mut(),
                &mut length,
                self.stream.cu_stream().cast(),
            )
        };
        settle_native_status("bitstream length", status, || self.complete(deadline))?;
        let maximum = layout
            .storage_bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(65536))
            .ok_or(GpuJpegError::Layout)?;
        if length == 0 || length as u64 > maximum {
            return Err(GpuJpegError::Layout);
        }
        let mut bytes = vec![0; length];
        let capacity = length;
        let status = unsafe {
            self.api.nvjpegEncodeRetrieveBitstream(
                self.handle,
                self.state,
                bytes.as_mut_ptr(),
                &mut length,
                self.stream.cu_stream().cast(),
            )
        };
        settle_native_status("bitstream retrieval", status, || self.complete(deadline))?;
        if length > capacity {
            return Err(GpuJpegError::Layout);
        }
        bytes.truncate(length);
        self.encoded_frames = self.encoded_frames.saturating_add(1);
        tracing::info!(encoder = "nvjpeg_cuda_gpu", cuda_device_uuid = %hex::encode(self.uuid()),
            width = layout.width, height = layout.height, encoded_frames = self.encoded_frames,
            "View GPU JPEG completed");
        Ok(bytes)
    }

    fn pack(
        &self,
        texture: &wgpu::Texture,
        queue: &wgpu::Queue,
        rgb: &SharedRgb,
        deadline: Instant,
    ) -> Result<(), GpuJpegError> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("View JPEG input"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: rgb
                        .buffer
                        .as_ref()
                        .expect("live GPU RGB buffer")
                        .as_entire_binding(),
                },
            ],
        });
        let mut commands = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("View JPEG packing"),
            });
        {
            let mut pass = commands.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("View RGB packing"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            let groups = (rgb.layout.storage_bytes / 4).div_ceil(64) as u32;
            pass.dispatch_workgroups(groups.min(16384), groups.div_ceil(16384), 1);
        }
        let submission = native_call("WGPU JPEG submission", || queue.submit([commands.finish()]));
        if let Err(error) = native_call("WGPU JPEG packing poll", || {
            self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(deadline.saturating_duration_since(Instant::now())),
            })
        }) {
            fatal_in_flight("WGPU JPEG packing", error);
        }
        if let Some(error) = futures::executor::block_on(scope.pop()) {
            return Err(GpuJpegError::Packing(error.to_string()));
        }
        Ok(())
    }

    fn complete(&self, deadline: Instant) {
        let event = self
            .stream
            .record_event(None)
            .unwrap_or_else(|error| fatal_in_flight("CUDA JPEG event", error));
        wait_for_completion("CUDA JPEG completion", deadline, || event.try_is_complete());
    }
}

fn wait_for_completion<E: std::fmt::Display>(
    operation: &'static str,
    deadline: Instant,
    mut query: impl FnMut() -> Result<bool, E>,
) {
    loop {
        match query() {
            Ok(true) => return,
            Ok(false) if Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(1))
            }
            Ok(false) => fatal_in_flight(operation, "native completion deadline expired"),
            Err(error) => fatal_in_flight(operation, error),
        }
    }
}

impl Drop for GpuJpeg {
    fn drop(&mut self) {
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        if let Err(error) = native_call("WGPU shutdown drain", || {
            self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(deadline.saturating_duration_since(Instant::now())),
            })
        }) {
            fatal_in_flight("WGPU shutdown drain", error);
        }
        self.complete(deadline);
        self.orderly_drained.store(true, Ordering::Release);
        unsafe {
            if !self.params.is_null() {
                let _ = self.api.nvjpegEncoderParamsDestroy(self.params);
            }
            if !self.state.is_null() {
                let _ = self.api.nvjpegEncoderStateDestroy(self.state);
            }
            if !self.handle.is_null() {
                let _ = self.api.nvjpegDestroy(self.handle);
            }
        }
    }
}

fn device_loss(reason: wgpu::DeviceLostReason, orderly_drained: bool) {
    if reason == wgpu::DeviceLostReason::Destroyed && orderly_drained {
        return;
    }
    fatal_in_flight("WGPU device loss", format_args!("{reason:?}"));
}

/// Only the immediate API call is inside the unwind boundary. Owners of submitted
/// storage stay in the caller and cannot unwind before the process exits.
pub(super) fn native_call<T>(operation: &'static str, call: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
        Ok(value) => value,
        Err(_) => fatal_in_flight(operation, "native API panicked; completion unknown"),
    }
}

pub(super) fn fatal_in_flight(operation: &'static str, error: impl std::fmt::Display) -> ! {
    // Releasing potentially in-flight GPU storage is unsafe. A failed completion
    // fence terminates this process instead of freeing storage or blocking shutdown.
    tracing::error!(operation, %error, "View GPU completion failed; terminating renderer process");
    // SAFETY: Linux _exit terminates without Rust/native destructors or a core dump.
    unsafe { libc::_exit(70) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fatal_deadline_exits_without_destructors() {
        const CHILD: &str = "VIEW_NATIVE_COMPLETION_EXIT_TEST";
        if let Some(path) = std::env::var_os(CHILD) {
            struct DropWitness(std::path::PathBuf);
            impl Drop for DropWitness {
                fn drop(&mut self) {
                    std::fs::write(&self.0, b"dropped").unwrap();
                }
            }
            let _witness = DropWitness(path.into());
            wait_for_completion("owning deadline fixture", Instant::now(), || {
                Ok::<_, std::convert::Infallible>(false)
            });
            panic!("failed completion returned");
        }
        let directory = tempfile::tempdir().unwrap();
        let dropped = directory.path().join("destructor-ran");
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "renderer::gpu_jpeg::tests::fatal_deadline_exits_without_destructors",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, &dropped)
            .kill_on_drop(true);
        let status = tokio::time::timeout(std::time::Duration::from_secs(5), command.status())
            .await
            .expect("fatal completion child exceeded deadline")
            .unwrap();
        assert_eq!(status.code(), Some(70));
        assert!(!dropped.exists(), "in-flight resources ran destructors");
    }

    #[tokio::test]
    async fn fatal_native_failure_exits_before_owner_drop() {
        const CHILD: &str = "VIEW_NATIVE_ERROR_EXIT_TEST";
        if let Some(mode) = std::env::var_os(CHILD) {
            struct Owner(std::path::PathBuf);
            impl Drop for Owner {
                fn drop(&mut self) {
                    std::fs::write(&self.0, b"released").unwrap();
                }
            }
            let path = std::path::PathBuf::from(std::env::var_os("VIEW_NATIVE_DROP_PATH").unwrap());
            let _owner = Owner(path);
            match mode.to_str().unwrap() {
                "device_error" => device_loss(wgpu::DeviceLostReason::Unknown, false),
                "destroyed_before_drain" => device_loss(wgpu::DeviceLostReason::Destroyed, false),
                "device_error_after_drain" => device_loss(wgpu::DeviceLostReason::Unknown, true),
                "poll_panic" => native_call("poll fixture", || panic!("native backend failure")),
                "partial_submit" => native_call("submission fixture", || {
                    // Submission intent is recorded before a backend panic; the
                    // external storage owner must survive this ambiguous outcome.
                    std::fs::write(
                        std::env::var_os("VIEW_NATIVE_SUBMITTED_PATH").unwrap(),
                        b"submitted",
                    )
                    .unwrap();
                    panic!("partial submission failure")
                }),
                _ => unreachable!(),
            }
        }
        for mode in [
            "device_error",
            "destroyed_before_drain",
            "device_error_after_drain",
            "poll_panic",
            "partial_submit",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let dropped = directory.path().join("dropped");
            let submitted = directory.path().join("submitted");
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "renderer::gpu_jpeg::tests::fatal_native_failure_exits_before_owner_drop",
                    "--exact",
                    "--nocapture",
                ])
                .env(CHILD, mode)
                .env("VIEW_NATIVE_DROP_PATH", &dropped)
                .env("VIEW_NATIVE_SUBMITTED_PATH", &submitted)
                .kill_on_drop(true);
            let status = tokio::time::timeout(std::time::Duration::from_secs(5), command.status())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.code(), Some(70), "{mode}");
            assert!(!dropped.exists(), "{mode} released in-flight storage");
            assert_eq!(submitted.exists(), mode == "partial_submit");
        }
    }

    #[test]
    fn orderly_device_destruction_after_drain_returns() {
        device_loss(wgpu::DeviceLostReason::Destroyed, true);
    }

    #[test]
    fn completion_witness_returns_before_deadline() {
        let mut observations = 0;
        wait_for_completion("completion fixture", Instant::now() + DRAIN_TIMEOUT, || {
            observations += 1;
            Ok::<_, std::convert::Infallible>(observations == 2)
        });
        assert_eq!(observations, 2);
    }
}
