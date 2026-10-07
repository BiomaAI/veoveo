//! Linux NVIDIA Vulkan/CUDA ownership for device-resident JPEG input.
//!
//! Only the serial renderer thread calls these APIs. The shared buffer has one
//! writer (Vulkan), then one reader (CUDA), separated by an external queue-family
//! release and a completed fence. CUDA completion precedes all resource release.
use std::{
    fs::File,
    os::fd::{AsRawFd, FromRawFd, IntoRawFd},
    sync::Arc,
    time::{Duration, Instant},
};

use ash::vk;
use cudarc::driver::{CudaContext, result, sys};

use super::gpu_jpeg::GpuJpegError;

pub(super) const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
pub(super) struct RgbLayout {
    pub width: i32,
    pub height: i32,
    pub pitch: usize,
    pub storage_bytes: u64,
}

impl RgbLayout {
    pub fn new(width: u32, height: u32) -> Result<Self, GpuJpegError> {
        let pitch = width.checked_mul(3).ok_or(GpuJpegError::Layout)?;
        let length = u64::from(pitch)
            .checked_mul(u64::from(height))
            .ok_or(GpuJpegError::Layout)?;
        if width == 0 || height == 0 || length > u64::from(u32::MAX) {
            return Err(GpuJpegError::Layout);
        }
        Ok(Self {
            width: i32::try_from(width).map_err(|_| GpuJpegError::Layout)?,
            height: i32::try_from(height).map_err(|_| GpuJpegError::Layout)?,
            pitch: usize::try_from(pitch).map_err(|_| GpuJpegError::Layout)?,
            storage_bytes: (length + 3) & !3,
        })
    }
}

pub(super) struct VulkanCuda {
    device: wgpu::Device,
    native: ash::Device,
    instance: ash::Instance,
    physical: vk::PhysicalDevice,
    queue: vk::Queue,
    family: u32,
    pub context: Arc<CudaContext>,
    pub uuid: [u8; 16],
}

impl VulkanCuda {
    pub fn new(device: &wgpu::Device) -> Result<Self, GpuJpegError> {
        // SAFETY: native objects are borrowed from this live device, never destroyed here.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }
            .ok_or(GpuJpegError::VulkanRequired)?;
        if !hal
            .enabled_device_extensions()
            .contains(&ash::khr::external_memory_fd::NAME)
        {
            return Err(GpuJpegError::ExternalMemoryUnsupported);
        }
        let instance = hal.shared_instance().raw_instance().clone();
        let physical = hal.raw_physical_device();
        let mut id = vk::PhysicalDeviceIDProperties::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
        // SAFETY: valid physical device and output chain.
        unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
        if properties.properties.vendor_id != 0x10de {
            return Err(GpuJpegError::NvidiaRequired);
        }
        let uuid = id.device_uuid;
        let mut context = None;
        for ordinal in 0..CudaContext::device_count()? {
            let candidate = CudaContext::new(ordinal as usize)?;
            if candidate.uuid()?.bytes.map(|v| v as u8) == uuid {
                context = Some(candidate);
                break;
            }
        }
        let context = context.ok_or(GpuJpegError::DeviceMismatch)?;
        context.bind_to_thread()?;
        let info = vk::PhysicalDeviceExternalBufferInfo::default()
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
            .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let mut external = vk::ExternalBufferProperties::default();
        // SAFETY: valid physical device and output structure.
        unsafe {
            instance.get_physical_device_external_buffer_properties(physical, &info, &mut external)
        };
        let features = external.external_memory_properties.external_memory_features;
        if !features.contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
            || features.contains(vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY)
        {
            return Err(GpuJpegError::ExternalMemoryUnsupported);
        }
        Ok(Self {
            device: device.clone(),
            native: hal.raw_device().clone(),
            instance,
            physical,
            queue: hal.raw_queue(),
            family: hal.queue_family_index(),
            context,
            uuid,
        })
    }

    pub fn buffer(&self, layout: RgbLayout, deadline: Instant) -> Result<SharedRgb, GpuJpegError> {
        let mut external = vk::ExternalMemoryBufferCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let info = vk::BufferCreateInfo::default()
            .size(layout.storage_bytes)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut external);
        // SAFETY: all objects belong to the retained device. RAII starts before allocation.
        let raw = unsafe { self.native.create_buffer(&info, None)? };
        let mut allocation = Allocation {
            device: self.native.clone(),
            buffer: raw,
            memory: vk::DeviceMemory::null(),
        };
        let requirements = unsafe { self.native.get_buffer_memory_requirements(raw) };
        let memory = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical)
        };
        let index = (0..memory.memory_type_count)
            .find(|i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && memory.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .ok_or(GpuJpegError::ExternalMemoryUnsupported)?;
        let mut export = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index)
            .push_next(&mut export);
        allocation.memory = unsafe { self.native.allocate_memory(&info, None)? };
        unsafe { self.native.bind_buffer_memory(raw, allocation.memory, 0)? };
        // The imported WGPU buffer must already be initialized. A bounded native fill
        // runs before WGPU acquires the buffer; no image bytes traverse the host.
        self.submit_native(deadline, |command| unsafe {
            self.native
                .cmd_fill_buffer(command, raw, 0, layout.storage_bytes, 0);
            self.native.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .buffer(raw)
                    .offset(0)
                    .size(layout.storage_bytes)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)],
                &[],
            );
        })?;
        let exporter = ash::khr::external_memory_fd::Device::new(&self.instance, &self.native);
        let fd = unsafe {
            exporter.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(allocation.memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
            )?
        };
        // SAFETY: this is a fresh owned FD. Cudarc transfers it only on successful import.
        let file = unsafe { File::from_raw_fd(fd) };
        let mapped = CudaMapping::new(
            self.context.clone(),
            file,
            requirements.size,
            layout.storage_bytes,
        )?;
        let descriptor = wgpu::BufferDescriptor {
            label: Some("View shared GPU RGB"),
            size: layout.storage_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        // SAFETY: initialized native buffer from this device, with matching descriptor.
        // Vulkan memory stays caller-owned. WGPU owns/destroys the VkBuffer.
        let buffer = unsafe {
            self.device
                .create_buffer_from_hal::<wgpu::hal::api::Vulkan>(
                    wgpu::hal::vulkan::Buffer::from_raw(raw),
                    &descriptor,
                )
        };
        allocation.buffer = vk::Buffer::null();
        Ok(SharedRgb {
            mapped: Some(mapped),
            buffer: Some(buffer),
            _allocation: allocation,
            device: self.device.clone(),
            raw,
            layout,
        })
    }

    pub fn release_to_cuda(&self, rgb: &SharedRgb, deadline: Instant) -> Result<(), GpuJpegError> {
        self.submit_native(deadline, |command| unsafe {
            self.native.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[vk::BufferMemoryBarrier::default()
                    .buffer(rgb.raw)
                    .offset(0)
                    .size(rgb.layout.storage_bytes)
                    .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::empty())
                    .src_queue_family_index(self.family)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)],
                &[],
            );
        })
    }

    fn submit_native(
        &self,
        deadline: Instant,
        record: impl FnOnce(vk::CommandBuffer),
    ) -> Result<(), GpuJpegError> {
        let pool = unsafe {
            self.native.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(self.family),
                None,
            )?
        };
        let mut submission = NativeSubmission {
            device: self.native.clone(),
            pool,
            fence: vk::Fence::null(),
        };
        let command = unsafe {
            self.native.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?
        }[0];
        unsafe {
            self.native
                .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())?
        };
        record(command);
        unsafe { self.native.end_command_buffer(command)? };
        submission.fence = unsafe {
            self.native
                .create_fence(&vk::FenceCreateInfo::default(), None)?
        };
        if let Err(error) = unsafe {
            self.native.queue_submit(
                self.queue,
                &[vk::SubmitInfo::default().command_buffers(&[command])],
                submission.fence,
            )
        } {
            super::gpu_jpeg::fatal_in_flight("Vulkan JPEG submit outcome", error);
        }
        match unsafe {
            self.native.wait_for_fences(
                &[submission.fence],
                true,
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_nanos() as u64,
            )
        } {
            Ok(()) => Ok(()),
            Err(error) => super::gpu_jpeg::fatal_in_flight("Vulkan JPEG submission", error),
        }
    }
}

struct NativeSubmission {
    device: ash::Device,
    pool: vk::CommandPool,
    fence: vk::Fence,
}
impl Drop for NativeSubmission {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}

struct Allocation {
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            if self.buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.buffer, None);
            }
            if self.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.memory, None);
            }
        }
    }
}

trait MappingApi {
    type Memory: Copy;
    type Pointer: Copy;
    fn import(&self, file: File, bytes: u64) -> Result<Self::Memory, GpuJpegError>;
    fn map(&self, memory: Self::Memory, bytes: u64) -> Result<Self::Pointer, GpuJpegError>;
    fn free_mapping(&self, pointer: Self::Pointer) -> Result<(), GpuJpegError>;
    fn free_import(&self, memory: Self::Memory) -> Result<(), GpuJpegError>;
}
struct CudaMemoryApi {
    context: Arc<CudaContext>,
}
impl MappingApi for CudaMemoryApi {
    type Memory = sys::CUexternalMemory;
    type Pointer = sys::CUdeviceptr;
    fn import(&self, file: File, bytes: u64) -> Result<Self::Memory, GpuJpegError> {
        self.context.bind_to_thread()?;
        // SAFETY: fresh owned Vulkan opaque FD and its checked allocation size.
        let memory = unsafe {
            result::external_memory::import_external_memory_opaque_fd(file.as_raw_fd(), bytes)?
        };
        let _transferred_fd = file.into_raw_fd();
        Ok(memory)
    }
    fn map(&self, memory: Self::Memory, bytes: u64) -> Result<Self::Pointer, GpuJpegError> {
        let pointer = unsafe { result::external_memory::get_mapped_buffer(memory, 0, bytes)? };
        if pointer == 0 {
            return Err(GpuJpegError::Layout);
        }
        Ok(pointer)
    }
    fn free_mapping(&self, pointer: Self::Pointer) -> Result<(), GpuJpegError> {
        self.context.bind_to_thread()?;
        unsafe { result::memory_free(pointer)? };
        Ok(())
    }
    fn free_import(&self, memory: Self::Memory) -> Result<(), GpuJpegError> {
        self.context.bind_to_thread()?;
        unsafe { result::external_memory::destroy_external_memory(memory)? };
        Ok(())
    }
}
struct MappingOwner<A: MappingApi> {
    api: A,
    external: Option<A::Memory>,
    pointer: Option<A::Pointer>,
}
impl<A: MappingApi> MappingOwner<A> {
    fn admit(
        api: A,
        file: File,
        allocation_bytes: u64,
        mapped_bytes: u64,
    ) -> Result<Self, GpuJpegError> {
        if mapped_bytes == 0 || mapped_bytes > allocation_bytes {
            return Err(GpuJpegError::Layout);
        }
        let external = api.import(file, allocation_bytes)?;
        let mut owner = Self {
            api,
            external: Some(external),
            pointer: None,
        };
        // A map failure drops the import, without freeing an uninitialized pointer.
        owner.pointer = Some(owner.api.map(external, mapped_bytes)?);
        Ok(owner)
    }
}
impl<A: MappingApi> Drop for MappingOwner<A> {
    fn drop(&mut self) {
        // Encoder completion is a prerequisite. A native release failure cannot
        // fall through to freeing backing Vulkan memory that CUDA still owns.
        if let Some(pointer) = self.pointer.take() {
            self.api.free_mapping(pointer).unwrap_or_else(|error| {
                super::gpu_jpeg::fatal_in_flight("CUDA JPEG mapped-buffer release", error)
            });
        }
        if let Some(memory) = self.external.take() {
            self.api.free_import(memory).unwrap_or_else(|error| {
                super::gpu_jpeg::fatal_in_flight("CUDA JPEG external-memory release", error)
            });
        }
    }
}
pub(super) struct CudaMapping(MappingOwner<CudaMemoryApi>);
impl CudaMapping {
    fn new(
        context: Arc<CudaContext>,
        file: File,
        allocation_bytes: u64,
        mapped_bytes: u64,
    ) -> Result<Self, GpuJpegError> {
        Ok(Self(MappingOwner::admit(
            CudaMemoryApi { context },
            file,
            allocation_bytes,
            mapped_bytes,
        )?))
    }
    pub fn pointer(&self) -> *mut u8 {
        self.0.pointer.expect("admitted CUDA mapping") as *mut u8
    }
}

pub(super) struct SharedRgb {
    pub mapped: Option<CudaMapping>,
    pub buffer: Option<wgpu::Buffer>,
    _allocation: Allocation,
    device: wgpu::Device,
    raw: vk::Buffer,
    pub layout: RgbLayout,
}
impl Drop for SharedRgb {
    fn drop(&mut self) {
        // CUDA has completed before this value drops. Its mapped pointer and imported
        // memory must disappear before WGPU destroys the buffer and Vulkan frees memory.
        drop(self.mapped.take());
        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
            drop(buffer);
        }
        if let Err(error) = super::gpu_jpeg::native_call("WGPU resource release poll", || {
            self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(DRAIN_TIMEOUT),
            })
        }) {
            super::gpu_jpeg::fatal_in_flight("Vulkan JPEG resource release", error);
        }
        // `allocation` then frees the caller-owned backing memory.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    struct MemoryBoundary {
        events: Rc<RefCell<Vec<&'static str>>>,
        map_fails: bool,
        completed: Rc<Cell<bool>>,
    }
    impl MappingApi for MemoryBoundary {
        type Memory = u8;
        type Pointer = u8;
        fn import(&self, _file: File, _bytes: u64) -> Result<u8, GpuJpegError> {
            self.events.borrow_mut().push("import");
            Ok(1)
        }
        fn map(&self, _memory: u8, _bytes: u64) -> Result<u8, GpuJpegError> {
            self.events.borrow_mut().push("map");
            if self.map_fails {
                Err(GpuJpegError::Layout)
            } else {
                Ok(2)
            }
        }
        fn free_mapping(&self, _pointer: u8) -> Result<(), GpuJpegError> {
            assert!(self.completed.get(), "mapping released before completion");
            self.events.borrow_mut().push("free mapping");
            Ok(())
        }
        fn free_import(&self, _memory: u8) -> Result<(), GpuJpegError> {
            self.events.borrow_mut().push("free import");
            Ok(())
        }
    }

    #[test]
    fn pre_submit_map_failure_releases_only_import() {
        let events = Rc::new(RefCell::new(vec![]));
        let api = MemoryBoundary {
            events: events.clone(),
            map_fails: true,
            completed: Rc::new(Cell::new(false)),
        };
        assert!(MappingOwner::admit(api, tempfile::tempfile().unwrap(), 16, 4).is_err());
        assert_eq!(*events.borrow(), ["import", "map", "free import"]);
    }

    #[test]
    fn post_submit_error_observes_completion_before_mapping_and_import_release() {
        let events = Rc::new(RefCell::new(vec![]));
        let completed = Rc::new(Cell::new(false));
        let api = MemoryBoundary {
            events: events.clone(),
            map_fails: false,
            completed: completed.clone(),
        };
        {
            let _owner = MappingOwner::admit(api, tempfile::tempfile().unwrap(), 16, 4).unwrap();
            let result = super::super::gpu_jpeg::settle_native_status(
                "retrieval fixture",
                super::super::nvjpeg_bindings::nvjpegStatus_t::NVJPEG_STATUS_EXECUTION_FAILED,
                || {
                    completed.set(true);
                    events.borrow_mut().push("completion");
                },
            );
            assert!(matches!(result, Err(GpuJpegError::Nvjpeg { .. })));
        }
        assert_eq!(
            *events.borrow(),
            ["import", "map", "completion", "free mapping", "free import"]
        );
    }

    #[test]
    fn packed_rgb_admits_odd_pitch_with_word_aligned_storage() {
        let layout = RgbLayout::new(3, 2).unwrap();
        assert_eq!(layout.pitch, 9);
        assert_eq!(layout.storage_bytes, 20);
        assert_eq!((layout.width, layout.height), (3, 2));
    }

    #[test]
    fn rgb_layout_refuses_zero_and_overflow_before_native_allocation() {
        for (width, height) in [(0, 1), (1, 0), (u32::MAX, 1), (65536, 65536)] {
            assert!(matches!(
                RgbLayout::new(width, height),
                Err(GpuJpegError::Layout)
            ));
        }
    }
}
