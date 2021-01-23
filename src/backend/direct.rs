use crate::{
    backend::{error::ContextError, native_gpu_future},
    AdapterInfo, BindGroupDescriptor, BindGroupLayoutDescriptor, BindingResource,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Features, Label,
    Limits, LoadOp, MapMode, Operations, PipelineLayoutDescriptor, RenderBundleEncoderDescriptor,
    RenderPipelineDescriptor, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource,
    SwapChainStatus, TextureDescriptor, TextureFormat, TextureViewDescriptor,
};

use arrayvec::ArrayVec;
use parking_lot::Mutex;
use smallvec::SmallVec;
use std::{
    borrow::Cow::Borrowed,
    error::Error,
    fmt,
    future::{ready, Ready},
    marker::PhantomData,
    ops::Range,
    slice,
    sync::Arc,
};
use typed_arena::Arena;

const LABEL: &str = "label";

pub struct Context(pub(crate) wgc::hub::Global<wgc::hub::IdentityManagerFactory>);

impl Drop for Context {
    fn drop(&mut self) {
        //nothing
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context").field("type", &"Native").finish()
    }
}

impl Context {
    pub(crate) fn global(&self) -> &wgc::hub::Global<wgc::hub::IdentityManagerFactory> {
        &self.0
    }

    pub fn enumerate_adapters(&self, backends: wgt::BackendBit) -> Vec<wgc::id::AdapterId> {
        self.0
            .enumerate_adapters(wgc::instance::AdapterInputs::Mask(backends, |_| {
                PhantomData
            }))
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub unsafe fn create_surface_from_core_animation_layer(
        self: &Arc<Self>,
        layer: *mut std::ffi::c_void,
    ) -> crate::Surface {
        let surface = wgc::instance::Surface {
            #[cfg(feature = "vulkan-portability")]
            vulkan: None, //TODO: create_surface_from_layer ?
            metal: self
                .0
                .instance
                .metal
                .as_ref()
                .map(|inst| inst.create_surface_from_layer(std::mem::transmute(layer))),
        };

        let id = self.0.surfaces.process_id(PhantomData);
        self.0
            .surfaces
            .register(id, surface, &mut wgc::hub::Token::root());
        crate::Surface {
            context: Arc::clone(self),
            id,
        }
    }

    fn handle_error(
        &self,
        sink_mutex: &Mutex<ErrorSinkRaw>,
        cause: impl Error + Send + Sync + 'static,
        label_key: &'static str,
        label: Label,
        string: &'static str,
    ) {
        let error = ContextError {
            string,
            cause: Box::new(cause),
            label: label.unwrap_or_default().to_string(),
            label_key,
        };
        let sink = sink_mutex.lock();
        let mut source_opt: Option<&(dyn Error + 'static)> = Some(&error);
        while let Some(source) = source_opt {
            if let Some(device_error) = source.downcast_ref::<wgc::device::DeviceError>() {
                match device_error {
                    wgc::device::DeviceError::OutOfMemory => {
                        return sink.handle_error(crate::Error::OutOfMemoryError {
                            source: Box::new(error),
                        });
                    }
                    _ => {}
                }
            }
            source_opt = source.source();
        }

        // Otherwise, it is a validation error
        sink.handle_error(crate::Error::ValidationError {
            description: self.format_error(&error),
            source: Box::new(error),
        });
    }

    fn handle_error_nolabel(
        &self,
        sink_mutex: &Mutex<ErrorSinkRaw>,
        cause: impl Error + Send + Sync + 'static,
        string: &'static str,
    ) {
        self.handle_error(sink_mutex, cause, "", None, string)
    }

    fn handle_error_fatal(
        &self,
        cause: impl Error + Send + Sync + 'static,
        string: &'static str,
    ) -> ! {
        panic!("Error in {}: {}", string, cause);
    }
}

mod pass_impl {
    use super::Context;
    use smallvec::SmallVec;
    use std::convert::TryInto;
    use std::ops::Range;
    use wgc::command::{bundle_ffi::*, compute_ffi::*, render_ffi::*};

    impl crate::ComputePassInner<Context> for wgc::command::ComputePass {
        fn set_pipeline(&mut self, pipeline: &wgc::id::ComputePipelineId) {
            wgpu_compute_pass_set_pipeline(self, *pipeline)
        }
        fn set_bind_group(
            &mut self,
            index: u32,
            bind_group: &wgc::id::BindGroupId,
            offsets: &[wgt::DynamicOffset],
        ) {
            unsafe {
                wgpu_compute_pass_set_bind_group(
                    self,
                    index,
                    *bind_group,
                    offsets.as_ptr(),
                    offsets.len(),
                )
            }
        }
        fn set_push_constants(&mut self, offset: u32, data: &[u8]) {
            unsafe {
                wgpu_compute_pass_set_push_constant(
                    self,
                    offset,
                    data.len().try_into().unwrap(),
                    data.as_ptr(),
                )
            }
        }
        fn insert_debug_marker(&mut self, label: &str) {
            unsafe {
                let label = std::ffi::CString::new(label).unwrap();
                wgpu_compute_pass_insert_debug_marker(self, label.as_ptr().into(), 0);
            }
        }

        fn push_debug_group(&mut self, group_label: &str) {
            unsafe {
                let label = std::ffi::CString::new(group_label).unwrap();
                wgpu_compute_pass_push_debug_group(self, label.as_ptr().into(), 0);
            }
        }
        fn pop_debug_group(&mut self) {
            wgpu_compute_pass_pop_debug_group(self);
        }
        fn dispatch(&mut self, x: u32, y: u32, z: u32) {
            wgpu_compute_pass_dispatch(self, x, y, z)
        }
        fn dispatch_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
        ) {
            wgpu_compute_pass_dispatch_indirect(self, indirect_buffer.id, indirect_offset)
        }
    }

    impl crate::RenderInner<Context> for wgc::command::RenderPass {
        fn set_pipeline(&mut self, pipeline: &wgc::id::RenderPipelineId) {
            wgpu_render_pass_set_pipeline(self, *pipeline)
        }
        fn set_bind_group(
            &mut self,
            index: u32,
            bind_group: &wgc::id::BindGroupId,
            offsets: &[wgt::DynamicOffset],
        ) {
            unsafe {
                wgpu_render_pass_set_bind_group(
                    self,
                    index,
                    *bind_group,
                    offsets.as_ptr(),
                    offsets.len(),
                )
            }
        }
        fn set_index_buffer(
            &mut self,
            buffer: &super::Buffer,
            index_format: wgt::IndexFormat,
            offset: wgt::BufferAddress,
            size: Option<wgt::BufferSize>,
        ) {
            self.set_index_buffer(buffer.id, index_format, offset, size)
        }
        fn set_vertex_buffer(
            &mut self,
            slot: u32,
            buffer: &super::Buffer,
            offset: wgt::BufferAddress,
            size: Option<wgt::BufferSize>,
        ) {
            wgpu_render_pass_set_vertex_buffer(self, slot, buffer.id, offset, size)
        }
        fn set_push_constants(&mut self, stages: wgt::ShaderStage, offset: u32, data: &[u8]) {
            unsafe {
                wgpu_render_pass_set_push_constants(
                    self,
                    stages,
                    offset,
                    data.len().try_into().unwrap(),
                    data.as_ptr(),
                )
            }
        }
        fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
            wgpu_render_pass_draw(
                self,
                vertices.end - vertices.start,
                instances.end - instances.start,
                vertices.start,
                instances.start,
            )
        }
        fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
            wgpu_render_pass_draw_indexed(
                self,
                indices.end - indices.start,
                instances.end - instances.start,
                indices.start,
                base_vertex,
                instances.start,
            )
        }
        fn draw_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
        ) {
            wgpu_render_pass_draw_indirect(self, indirect_buffer.id, indirect_offset)
        }
        fn draw_indexed_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
        ) {
            wgpu_render_pass_draw_indexed_indirect(self, indirect_buffer.id, indirect_offset)
        }
        fn multi_draw_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
            count: u32,
        ) {
            wgpu_render_pass_multi_draw_indirect(self, indirect_buffer.id, indirect_offset, count)
        }
        fn multi_draw_indexed_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
            count: u32,
        ) {
            wgpu_render_pass_multi_draw_indexed_indirect(
                self,
                indirect_buffer.id,
                indirect_offset,
                count,
            )
        }
        fn multi_draw_indirect_count(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
            count_buffer: &super::Buffer,
            count_buffer_offset: wgt::BufferAddress,
            max_count: u32,
        ) {
            wgpu_render_pass_multi_draw_indirect_count(
                self,
                indirect_buffer.id,
                indirect_offset,
                count_buffer.id,
                count_buffer_offset,
                max_count,
            )
        }
        fn multi_draw_indexed_indirect_count(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
            count_buffer: &super::Buffer,
            count_buffer_offset: wgt::BufferAddress,
            max_count: u32,
        ) {
            wgpu_render_pass_multi_draw_indexed_indirect_count(
                self,
                indirect_buffer.id,
                indirect_offset,
                count_buffer.id,
                count_buffer_offset,
                max_count,
            )
        }
    }

    impl crate::RenderPassInner<Context> for wgc::command::RenderPass {
        fn set_blend_color(&mut self, color: wgt::Color) {
            wgpu_render_pass_set_blend_color(self, &color)
        }
        fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
            wgpu_render_pass_set_scissor_rect(self, x, y, width, height)
        }
        fn set_viewport(
            &mut self,
            x: f32,
            y: f32,
            width: f32,
            height: f32,
            min_depth: f32,
            max_depth: f32,
        ) {
            wgpu_render_pass_set_viewport(self, x, y, width, height, min_depth, max_depth)
        }
        fn set_stencil_reference(&mut self, reference: u32) {
            wgpu_render_pass_set_stencil_reference(self, reference)
        }

        fn insert_debug_marker(&mut self, label: &str) {
            unsafe {
                let label = std::ffi::CString::new(label).unwrap();
                wgpu_render_pass_insert_debug_marker(self, label.as_ptr().into(), 0);
            }
        }

        fn push_debug_group(&mut self, group_label: &str) {
            unsafe {
                let label = std::ffi::CString::new(group_label).unwrap();
                wgpu_render_pass_push_debug_group(self, label.as_ptr().into(), 0);
            }
        }

        fn pop_debug_group(&mut self) {
            wgpu_render_pass_pop_debug_group(self);
        }

        fn execute_bundles<'a, I: Iterator<Item = &'a wgc::id::RenderBundleId>>(
            &mut self,
            render_bundles: I,
        ) {
            wgc::span!(_guard, TRACE, "RenderPass::execute_bundles wrapper");
            let temp_render_bundles = render_bundles.cloned().collect::<SmallVec<[_; 4]>>();
            unsafe {
                wgpu_render_pass_execute_bundles(
                    self,
                    temp_render_bundles.as_ptr(),
                    temp_render_bundles.len(),
                )
            }
        }
    }

    impl crate::RenderInner<Context> for wgc::command::RenderBundleEncoder {
        fn set_pipeline(&mut self, pipeline: &wgc::id::RenderPipelineId) {
            wgpu_render_bundle_set_pipeline(self, *pipeline)
        }
        fn set_bind_group(
            &mut self,
            index: u32,
            bind_group: &wgc::id::BindGroupId,
            offsets: &[wgt::DynamicOffset],
        ) {
            unsafe {
                wgpu_render_bundle_set_bind_group(
                    self,
                    index,
                    *bind_group,
                    offsets.as_ptr(),
                    offsets.len(),
                )
            }
        }
        fn set_index_buffer(
            &mut self,
            buffer: &super::Buffer,
            index_format: wgt::IndexFormat,
            offset: wgt::BufferAddress,
            size: Option<wgt::BufferSize>,
        ) {
            self.set_index_buffer(buffer.id, index_format, offset, size)
        }
        fn set_vertex_buffer(
            &mut self,
            slot: u32,
            buffer: &super::Buffer,
            offset: wgt::BufferAddress,
            size: Option<wgt::BufferSize>,
        ) {
            wgpu_render_bundle_set_vertex_buffer(self, slot, buffer.id, offset, size)
        }

        fn set_push_constants(&mut self, stages: wgt::ShaderStage, offset: u32, data: &[u8]) {
            unsafe {
                wgpu_render_bundle_set_push_constants(
                    self,
                    stages,
                    offset,
                    data.len().try_into().unwrap(),
                    data.as_ptr(),
                )
            }
        }
        fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
            wgpu_render_bundle_draw(
                self,
                vertices.end - vertices.start,
                instances.end - instances.start,
                vertices.start,
                instances.start,
            )
        }
        fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
            wgpu_render_bundle_draw_indexed(
                self,
                indices.end - indices.start,
                instances.end - instances.start,
                indices.start,
                base_vertex,
                instances.start,
            )
        }
        fn draw_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
        ) {
            wgpu_render_bundle_draw_indirect(self, indirect_buffer.id, indirect_offset)
        }
        fn draw_indexed_indirect(
            &mut self,
            indirect_buffer: &super::Buffer,
            indirect_offset: wgt::BufferAddress,
        ) {
            wgpu_render_pass_bundle_indexed_indirect(self, indirect_buffer.id, indirect_offset)
        }
        fn multi_draw_indirect(
            &mut self,
            _indirect_buffer: &super::Buffer,
            _indirect_offset: wgt::BufferAddress,
            _count: u32,
        ) {
            unimplemented!()
        }
        fn multi_draw_indexed_indirect(
            &mut self,
            _indirect_buffer: &super::Buffer,
            _indirect_offset: wgt::BufferAddress,
            _count: u32,
        ) {
            unimplemented!()
        }
        fn multi_draw_indirect_count(
            &mut self,
            _indirect_buffer: &super::Buffer,
            _indirect_offset: wgt::BufferAddress,
            _count_buffer: &super::Buffer,
            _count_buffer_offset: wgt::BufferAddress,
            _max_count: u32,
        ) {
            unimplemented!()
        }
        fn multi_draw_indexed_indirect_count(
            &mut self,
            _indirect_buffer: &super::Buffer,
            _indirect_offset: wgt::BufferAddress,
            _count_buffer: &super::Buffer,
            _count_buffer_offset: wgt::BufferAddress,
            _max_count: u32,
        ) {
            unimplemented!()
        }
    }
}

fn map_buffer_copy_view(view: crate::BufferCopyView) -> wgc::command::BufferCopyView {
    wgc::command::BufferCopyView {
        buffer: view.buffer.id.id,
        layout: view.layout,
    }
}

fn map_texture_copy_view(view: crate::TextureCopyView) -> wgc::command::TextureCopyView {
    wgc::command::TextureCopyView {
        texture: view.texture.id.id,
        mip_level: view.mip_level,
        origin: view.origin,
    }
}

fn map_pass_channel<V: Copy + Default>(
    ops: Option<&Operations<V>>,
) -> wgc::command::PassChannel<V> {
    match ops {
        Some(&Operations {
            load: LoadOp::Clear(clear_value),
            store,
        }) => wgc::command::PassChannel {
            load_op: wgc::command::LoadOp::Clear,
            store_op: if store {
                wgc::command::StoreOp::Store
            } else {
                wgc::command::StoreOp::Clear
            },
            clear_value,
            read_only: false,
        },
        Some(&Operations {
            load: LoadOp::Load,
            store,
        }) => wgc::command::PassChannel {
            load_op: wgc::command::LoadOp::Load,
            store_op: if store {
                wgc::command::StoreOp::Store
            } else {
                wgc::command::StoreOp::Clear
            },
            clear_value: V::default(),
            read_only: false,
        },
        None => wgc::command::PassChannel {
            load_op: wgc::command::LoadOp::Load,
            store_op: wgc::command::StoreOp::Store,
            clear_value: V::default(),
            read_only: true,
        },
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Device {
    id: wgc::id::DeviceId,
    error_sink: ErrorSink,
}

#[derive(Debug)]
pub(crate) struct Buffer {
    id: wgc::id::BufferId,
    error_sink: ErrorSink,
}

#[derive(Debug)]
pub(crate) struct Texture {
    id: wgc::id::TextureId,
    error_sink: ErrorSink,
}

#[derive(Debug)]
pub(crate) struct CommandEncoder {
    id: wgc::id::CommandEncoderId,
    error_sink: ErrorSink,
}

impl crate::Context for Context {
    type AdapterId = wgc::id::AdapterId;
    type DeviceId = Device;
    type QueueId = wgc::id::QueueId;
    type ShaderModuleId = wgc::id::ShaderModuleId;
    type BindGroupLayoutId = wgc::id::BindGroupLayoutId;
    type BindGroupId = wgc::id::BindGroupId;
    type TextureViewId = wgc::id::TextureViewId;
    type SamplerId = wgc::id::SamplerId;
    type BufferId = Buffer;
    type TextureId = Texture;
    type PipelineLayoutId = wgc::id::PipelineLayoutId;
    type RenderPipelineId = wgc::id::RenderPipelineId;
    type ComputePipelineId = wgc::id::ComputePipelineId;
    type CommandEncoderId = CommandEncoder;
    type ComputePassId = wgc::command::ComputePass;
    type RenderPassId = wgc::command::RenderPass;
    type CommandBufferId = wgc::id::CommandBufferId;
    type RenderBundleEncoderId = wgc::command::RenderBundleEncoder;
    type RenderBundleId = wgc::id::RenderBundleId;
    type SurfaceId = wgc::id::SurfaceId;
    type SwapChainId = wgc::id::SwapChainId;

    type SwapChainOutputDetail = SwapChainOutputDetail;

    type RequestAdapterFuture = Ready<Option<Self::AdapterId>>;
    type RequestDeviceFuture =
        Ready<Result<(Self::DeviceId, Self::QueueId), crate::RequestDeviceError>>;
    type MapAsyncFuture = native_gpu_future::GpuFuture<Result<(), crate::BufferAsyncError>>;

    fn init(backends: wgt::BackendBit) -> Self {
        Self(wgc::hub::Global::new(
            "wgpu",
            wgc::hub::IdentityManagerFactory,
            backends,
        ))
    }

    fn instance_create_surface(
        &self,
        handle: &impl raw_window_handle::HasRawWindowHandle,
    ) -> Self::SurfaceId {
        self.0.instance_create_surface(handle, PhantomData)
    }

    fn instance_request_adapter(
        self: &Arc<Self>,
        options: &crate::RequestAdapterOptions,
    ) -> Self::RequestAdapterFuture {
        let id = self.0.request_adapter(
            &wgc::instance::RequestAdapterOptions {
                power_preference: options.power_preference,
                compatible_surface: options.compatible_surface.map(|surface| surface.id),
            },
            wgc::instance::AdapterInputs::Mask(wgt::BackendBit::all(), |_| PhantomData),
        );
        native_gpu_future::start_worker_thread(self.clone());
        ready(id.ok())
    }

    fn adapter_request_device(
        &self,
        adapter: &Self::AdapterId,
        desc: &crate::DeviceDescriptor,
        trace_dir: Option<&std::path::Path>,
    ) -> Self::RequestDeviceFuture {
        let global = &self.0;
        let (device_id, error) = wgc::gfx_select!(*adapter => global.adapter_request_device(
            *adapter,
            &desc.map_label(|l| l.map(Borrowed)),
            trace_dir,
            PhantomData
        ));
        if let Some(err) = error {
            self.handle_error_fatal(err, "Adapter::request_device");
        }
        let device = Device {
            id: device_id,
            error_sink: Arc::new(Mutex::new(ErrorSinkRaw::new())),
        };
        ready(Ok((device, device_id)))
    }

    fn adapter_features(&self, adapter: &Self::AdapterId) -> Features {
        let global = &self.0;
        match wgc::gfx_select!(*adapter => global.adapter_features(*adapter)) {
            Ok(features) => features,
            Err(err) => self.handle_error_fatal(err, "Adapter::features"),
        }
    }

    fn adapter_limits(&self, adapter: &Self::AdapterId) -> Limits {
        let global = &self.0;
        match wgc::gfx_select!(*adapter => global.adapter_limits(*adapter)) {
            Ok(limits) => limits,
            Err(err) => self.handle_error_fatal(err, "Adapter::limits"),
        }
    }

    fn adapter_get_info(&self, adapter: &wgc::id::AdapterId) -> AdapterInfo {
        let global = &self.0;
        match wgc::gfx_select!(*adapter => global.adapter_get_info(*adapter)) {
            Ok(info) => info,
            Err(err) => self.handle_error_fatal(err, "Adapter::get_info"),
        }
    }

    fn device_features(&self, device: &Self::DeviceId) -> Features {
        let global = &self.0;
        match wgc::gfx_select!(device.id => global.device_features(device.id)) {
            Ok(features) => features,
            Err(err) => self.handle_error_fatal(err, "Device::features"),
        }
    }

    fn device_limits(&self, device: &Self::DeviceId) -> Limits {
        let global = &self.0;
        match wgc::gfx_select!(device.id => global.device_limits(device.id)) {
            Ok(limits) => limits,
            Err(err) => self.handle_error_fatal(err, "Device::limits"),
        }
    }

    fn device_get_swap_chain_preferred_format(&self, device: &Self::DeviceId) -> TextureFormat {
        let global = &self.0;
        match wgc::gfx_select!(device.id => global.device_get_swap_chain_preferred_format(device.id))
        {
            Ok(swap_chain_preferred_format) => swap_chain_preferred_format,
            Err(err) => self.handle_error_fatal(err, "Device::get_swap_chain_preferred_format"),
        }
    }

    fn device_create_swap_chain(
        &self,
        device: &Self::DeviceId,
        surface: &Self::SurfaceId,
        desc: &wgt::SwapChainDescriptor,
    ) -> Self::SwapChainId {
        let global = &self.0;
        match wgc::gfx_select!(
            device.id => global.device_create_swap_chain(device.id, *surface, desc)
        ) {
            Ok(sc) => sc,
            Err(err) => self.handle_error_fatal(err, "Device::create_swap_chain"),
        }
    }

    fn device_create_shader_module(
        &self,
        device: &Self::DeviceId,
        desc: &ShaderModuleDescriptor,
    ) -> Self::ShaderModuleId {
        let global = &self.0;
        let descriptor = wgc::pipeline::ShaderModuleDescriptor {
            label: desc.label.map(Borrowed),
            flags: desc.flags,
        };
        let source = match desc.source {
            ShaderSource::SpirV(ref spv) => wgc::pipeline::ShaderModuleSource::SpirV(Borrowed(spv)),
            ShaderSource::Wgsl(ref code) => wgc::pipeline::ShaderModuleSource::Wgsl(Borrowed(code)),
        };
        let (id, error) = wgc::gfx_select!(
            device.id => global.device_create_shader_module(device.id, &descriptor, source, PhantomData)
        );
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_shader_module",
            );
        }
        id
    }

    fn device_create_bind_group_layout(
        &self,
        device: &Self::DeviceId,
        desc: &BindGroupLayoutDescriptor,
    ) -> Self::BindGroupLayoutId {
        let global = &self.0;
        let descriptor = wgc::binding_model::BindGroupLayoutDescriptor {
            label: desc.label.map(Borrowed),
            entries: Borrowed(desc.entries),
        };
        let (id, error) = wgc::gfx_select!(
            device.id => global.device_create_bind_group_layout(device.id, &descriptor, PhantomData)
        );
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_bind_group_layout",
            );
        }
        id
    }

    fn device_create_bind_group(
        &self,
        device: &Self::DeviceId,
        desc: &BindGroupDescriptor,
    ) -> Self::BindGroupId {
        wgc::span!(_guard, TRACE, "Device::create_bind_group wrapper");
        use wgc::binding_model as bm;

        let texture_view_arena: Arena<wgc::id::TextureViewId> = Arena::new();
        let entries = desc
            .entries
            .iter()
            .map(|entry| bm::BindGroupEntry {
                binding: entry.binding,
                resource: match entry.resource {
                    BindingResource::Buffer {
                        buffer,
                        offset,
                        size,
                    } => bm::BindingResource::Buffer(bm::BufferBinding {
                        buffer_id: buffer.id.id,
                        offset,
                        size,
                    }),
                    BindingResource::Sampler(sampler) => bm::BindingResource::Sampler(sampler.id),
                    BindingResource::TextureView(texture_view) => {
                        bm::BindingResource::TextureView(texture_view.id)
                    }
                    BindingResource::TextureViewArray(texture_view_array) => {
                        bm::BindingResource::TextureViewArray(Borrowed(
                            texture_view_arena
                                .alloc_extend(texture_view_array.iter().map(|view| view.id)),
                        ))
                    }
                },
            })
            .collect::<Vec<_>>();
        let descriptor = bm::BindGroupDescriptor {
            label: desc.label.as_ref().map(|label| Borrowed(&label[..])),
            layout: desc.layout.id,
            entries: Borrowed(&entries),
        };

        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_bind_group(
            device.id,
            &descriptor,
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_bind_group",
            );
        }
        id
    }

    fn device_create_pipeline_layout(
        &self,
        device: &Self::DeviceId,
        desc: &PipelineLayoutDescriptor,
    ) -> Self::PipelineLayoutId {
        wgc::span!(_guard, TRACE, "Device::create_pipeline_layout wrapper");

        // Limit is always less or equal to wgc::MAX_BIND_GROUPS, so this is always right
        // Guards following ArrayVec
        assert!(
            desc.bind_group_layouts.len() <= wgc::MAX_BIND_GROUPS,
            "Bind group layout count {} exceeds device bind group limit {}",
            desc.bind_group_layouts.len(),
            wgc::MAX_BIND_GROUPS
        );

        let temp_layouts = desc
            .bind_group_layouts
            .iter()
            .map(|bgl| bgl.id)
            .collect::<ArrayVec<[_; wgc::MAX_BIND_GROUPS]>>();
        let descriptor = wgc::binding_model::PipelineLayoutDescriptor {
            label: desc.label.map(Borrowed),
            bind_group_layouts: Borrowed(&temp_layouts),
            push_constant_ranges: Borrowed(&desc.push_constant_ranges),
        };

        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_pipeline_layout(
            device.id,
            &descriptor,
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_pipeline_layout",
            );
        }
        id
    }

    fn device_create_render_pipeline(
        &self,
        device: &Self::DeviceId,
        desc: &RenderPipelineDescriptor,
    ) -> Self::RenderPipelineId {
        wgc::span!(_guard, TRACE, "Device::create_render_pipeline wrapper");
        use wgc::pipeline as pipe;

        let vertex_stage = pipe::ProgrammableStageDescriptor {
            module: desc.vertex_stage.module.id,
            entry_point: Borrowed(&desc.vertex_stage.entry_point),
        };
        let fragment_stage =
            desc.fragment_stage
                .as_ref()
                .map(|fs| pipe::ProgrammableStageDescriptor {
                    module: fs.module.id,
                    entry_point: Borrowed(&fs.entry_point),
                });
        let vertex_buffers: ArrayVec<[_; wgc::device::MAX_VERTEX_BUFFERS]> = desc
            .vertex_state
            .vertex_buffers
            .iter()
            .map(|vertex_buffer| pipe::VertexBufferDescriptor {
                stride: vertex_buffer.stride,
                step_mode: vertex_buffer.step_mode,
                attributes: Borrowed(vertex_buffer.attributes),
            })
            .collect();
        let vertex_state = pipe::VertexStateDescriptor {
            index_format: desc.vertex_state.index_format,
            vertex_buffers: Borrowed(&vertex_buffers),
        };

        let implicit_pipeline_ids = match desc.layout {
            Some(_) => None,
            None => Some(wgc::device::ImplicitPipelineIds {
                root_id: PhantomData,
                group_ids: &[PhantomData; wgc::MAX_BIND_GROUPS],
            }),
        };
        let descriptor = pipe::RenderPipelineDescriptor {
            label: desc.label.map(Borrowed),
            layout: desc.layout.map(|l| l.id),
            vertex_stage,
            fragment_stage,
            rasterization_state: desc.rasterization_state.clone(),
            primitive_topology: desc.primitive_topology,
            color_states: Borrowed(&desc.color_states),
            depth_stencil_state: desc.depth_stencil_state.clone(),
            vertex_state: vertex_state,
            sample_count: desc.sample_count,
            sample_mask: desc.sample_mask,
            alpha_to_coverage_enabled: desc.alpha_to_coverage_enabled,
        };

        let global = &self.0;
        let (id, _, error) = wgc::gfx_select!(device.id => global.device_create_render_pipeline(
            device.id,
            &descriptor,
            PhantomData,
            implicit_pipeline_ids
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_render_pipeline",
            );
        }
        id
    }

    fn device_create_compute_pipeline(
        &self,
        device: &Self::DeviceId,
        desc: &ComputePipelineDescriptor,
    ) -> Self::ComputePipelineId {
        use wgc::pipeline as pipe;

        let implicit_pipeline_ids = match desc.layout {
            Some(_) => None,
            None => Some(wgc::device::ImplicitPipelineIds {
                root_id: PhantomData,
                group_ids: &[PhantomData; wgc::MAX_BIND_GROUPS],
            }),
        };
        let descriptor = pipe::ComputePipelineDescriptor {
            label: desc.label.map(Borrowed),
            layout: desc.layout.map(|l| l.id),
            compute_stage: pipe::ProgrammableStageDescriptor {
                module: desc.compute_stage.module.id,
                entry_point: Borrowed(&desc.compute_stage.entry_point),
            },
        };

        let global = &self.0;
        let (id, _, error) = wgc::gfx_select!(device.id => global.device_create_compute_pipeline(
            device.id,
            &descriptor,
            PhantomData,
            implicit_pipeline_ids
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_compute_pipeline",
            );
        }
        id
    }

    fn device_create_buffer(
        &self,
        device: &Self::DeviceId,
        desc: &crate::BufferDescriptor<'_>,
    ) -> Self::BufferId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_buffer(
            device.id,
            &desc.map_label(|l| l.map(Borrowed)),
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_buffer",
            );
        }
        Buffer {
            id,
            error_sink: Arc::clone(&device.error_sink),
        }
    }

    fn device_create_texture(
        &self,
        device: &Self::DeviceId,
        desc: &TextureDescriptor,
    ) -> Self::TextureId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_texture(
            device.id,
            &desc.map_label(|l| l.map(Borrowed)),
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_texture",
            );
        }
        Texture {
            id,
            error_sink: Arc::clone(&device.error_sink),
        }
    }

    fn device_create_sampler(
        &self,
        device: &Self::DeviceId,
        desc: &SamplerDescriptor,
    ) -> Self::SamplerId {
        let descriptor = wgc::resource::SamplerDescriptor {
            label: desc.label.map(Borrowed),
            address_modes: [
                desc.address_mode_u,
                desc.address_mode_v,
                desc.address_mode_w,
            ],
            mag_filter: desc.mag_filter,
            min_filter: desc.min_filter,
            mipmap_filter: desc.mipmap_filter,
            lod_min_clamp: desc.lod_min_clamp,
            lod_max_clamp: desc.lod_max_clamp,
            compare: desc.compare,
            anisotropy_clamp: desc.anisotropy_clamp,
            border_color: desc.border_color,
        };

        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_sampler(
            device.id,
            &descriptor,
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_sampler",
            );
        }
        id
    }

    fn device_create_command_encoder(
        &self,
        device: &Self::DeviceId,
        desc: &CommandEncoderDescriptor,
    ) -> Self::CommandEncoderId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(device.id => global.device_create_command_encoder(
            device.id,
            &desc.map_label(|l| l.map(Borrowed)),
            PhantomData
        ));
        if let Some(cause) = error {
            self.handle_error(
                &device.error_sink,
                cause,
                LABEL,
                desc.label,
                "Device::create_command_encoder",
            );
        }
        CommandEncoder {
            id,
            error_sink: Arc::clone(&device.error_sink),
        }
    }

    fn device_create_render_bundle_encoder(
        &self,
        device: &Self::DeviceId,
        desc: &RenderBundleEncoderDescriptor,
    ) -> Self::RenderBundleEncoderId {
        let descriptor = wgc::command::RenderBundleEncoderDescriptor {
            label: desc.label.map(Borrowed),
            color_formats: Borrowed(desc.color_formats),
            depth_stencil_format: desc.depth_stencil_format,
            sample_count: desc.sample_count,
        };
        match wgc::command::RenderBundleEncoder::new(&descriptor, device.id, None) {
            Ok(id) => id,
            Err(e) => panic!("Error in Device::create_render_bundle_encoder: {}", e),
        }
    }

    fn device_drop(&self, device: &Self::DeviceId) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let global = &self.0;
            match wgc::gfx_select!(device.id => global.device_poll(device.id, true)) {
                Ok(()) => (),
                Err(err) => self.handle_error_fatal(err, "Device::drop"),
            }
        }
        //TODO: make this work in general
        #[cfg(not(target_arch = "wasm32"))]
        #[cfg(feature = "metal-auto-capture")]
        {
            let global = &self.0;
            wgc::gfx_select!(device.id => global.device_drop(device.id));
        }
    }

    fn device_poll(&self, device: &Self::DeviceId, maintain: crate::Maintain) {
        let global = &self.0;
        match wgc::gfx_select!(device.id => global.device_poll(
            device.id,
            match maintain {
                crate::Maintain::Poll => false,
                crate::Maintain::Wait => true,
            }
        )) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Device::poll"),
        }
    }

    fn device_on_uncaptured_error(
        &self,
        device: &Self::DeviceId,
        handler: impl crate::UncapturedErrorHandler,
    ) {
        let mut error_sink = device.error_sink.lock();
        error_sink.uncaptured_handler = Box::new(handler);
    }

    fn buffer_map_async(
        &self,
        buffer: &Self::BufferId,
        mode: MapMode,
        range: Range<wgt::BufferAddress>,
    ) -> Self::MapAsyncFuture {
        wgc::span!(_guard, TRACE, "Buffer::buffer_map_async wrapper");

        let future = native_gpu_future::GpuFuture::new();

        type MapAsyncFutureState = Arc::<Mutex<native_gpu_future::SharedState<Result<(), crate::BufferAsyncError>>>>;

        extern "C" fn buffer_map_future_wrapper(
            status: wgc::resource::BufferMapAsyncStatus,
            user_data: *mut u8,
        ) {
            let waker;
            {
                let shared_state = unsafe { MapAsyncFutureState::from_raw(user_data as *mut _) };
                let mut locked = shared_state.lock(); 
                locked.data = Some(match status {
                    wgc::resource::BufferMapAsyncStatus::Success => Ok(()),
                    _ => Err(crate::BufferAsyncError),
                });
                locked.completed = true;
                waker = locked.waker.take();
            };
            if let Some(w) = waker {
                w.wake();
            }
        }

        let operation = wgc::resource::BufferMapOperation {
            host: match mode {
                MapMode::Read => wgc::device::HostMap::Read,
                MapMode::Write => wgc::device::HostMap::Write,
            },
            callback: buffer_map_future_wrapper,
            user_data: MapAsyncFutureState::into_raw(future.shared_state.clone()) as *mut u8,
        };

        let global = &self.0;
        match wgc::gfx_select!(buffer.id => global.buffer_map_async(buffer.id, range, operation)) {
            Ok(()) => (),
            Err(cause) => self.handle_error_nolabel(&buffer.error_sink, cause, "Buffer::map_async"),
        }
        future
    }

    fn buffer_get_mapped_range(
        &self,
        buffer: &Self::BufferId,
        sub_range: Range<wgt::BufferAddress>,
    ) -> BufferMappedRange {
        let size = sub_range.end - sub_range.start;
        let global = &self.0;
        match wgc::gfx_select!(buffer.id => global.buffer_get_mapped_range(
            buffer.id,
            sub_range.start,
            wgt::BufferSize::new(size)
        )) {
            Ok(ptr) => BufferMappedRange {
                ptr,
                size: size as usize,
                phantom: PhantomData,
            },
            Err(err) => self.handle_error_fatal(err, "Buffer::get_mapped_range"),
        }
    }

    fn buffer_unmap(&self, buffer: &Self::BufferId) {
        let global = &self.0;
        match wgc::gfx_select!(buffer.id => global.buffer_unmap(buffer.id)) {
            Ok(()) => (),
            Err(cause) => {
                self.handle_error_nolabel(&buffer.error_sink, cause, "Buffer::buffer_unmap")
            }
        }
    }

    fn swap_chain_get_current_texture_view(
        &self,
        swap_chain: &Self::SwapChainId,
    ) -> (
        Option<Self::TextureViewId>,
        SwapChainStatus,
        Self::SwapChainOutputDetail,
    ) {
        let global = &self.0;
        match wgc::gfx_select!(
            *swap_chain => global.swap_chain_get_current_texture_view(*swap_chain, PhantomData)
        ) {
            Ok(wgc::swap_chain::SwapChainOutput { status, view_id }) => (
                view_id,
                status,
                SwapChainOutputDetail {
                    swap_chain_id: *swap_chain,
                },
            ),
            Err(err) => self.handle_error_fatal(err, "SwapChain::get_current_texture_view"),
        }
    }

    fn swap_chain_present(&self, view: &Self::TextureViewId, detail: &Self::SwapChainOutputDetail) {
        let global = &self.0;
        match wgc::gfx_select!(*view => global.swap_chain_present(detail.swap_chain_id)) {
            Ok(_status) => (),
            Err(err) => self.handle_error_fatal(err, "SwapChain::present"),
        }
    }

    fn texture_create_view(
        &self,
        texture: &Self::TextureId,
        desc: &TextureViewDescriptor,
    ) -> Self::TextureViewId {
        let descriptor = wgc::resource::TextureViewDescriptor {
            label: desc.label.map(Borrowed),
            format: desc.format,
            dimension: desc.dimension,
            aspect: desc.aspect,
            base_mip_level: desc.base_mip_level,
            level_count: desc.level_count,
            base_array_layer: desc.base_array_layer,
            array_layer_count: desc.array_layer_count,
        };
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(
            texture.id => global.texture_create_view(texture.id, &descriptor, PhantomData)
        );
        if let Some(cause) = error {
            self.handle_error(
                &texture.error_sink,
                cause,
                LABEL,
                desc.label,
                "Texture::create_view",
            );
        }
        id
    }

    fn surface_drop(&self, _surface: &Self::SurfaceId) {
        //TODO: swapchain needs to hold the surface alive
        //self.0.surface_drop(*surface)
    }

    fn adapter_drop(&self, adapter: &Self::AdapterId) {
        let global = &self.0;
        wgc::gfx_select!(*adapter => global.adapter_drop(*adapter))
    }

    fn buffer_destroy(&self, buffer: &Self::BufferId) {
        let global = &self.0;
        match wgc::gfx_select!(buffer.id => global.buffer_destroy(buffer.id)) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Buffer::destroy"),
        }
    }
    fn buffer_drop(&self, buffer: &Self::BufferId) {
        let global = &self.0;
        wgc::gfx_select!(buffer.id => global.buffer_drop(buffer.id, false))
    }
    fn texture_destroy(&self, texture: &Self::TextureId) {
        let global = &self.0;
        match wgc::gfx_select!(texture.id => global.texture_destroy(texture.id)) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Texture::destroy"),
        }
    }
    fn texture_drop(&self, texture: &Self::TextureId) {
        let global = &self.0;
        wgc::gfx_select!(texture.id => global.texture_drop(texture.id, false))
    }
    fn texture_view_drop(&self, texture_view: &Self::TextureViewId) {
        let global = &self.0;
        match wgc::gfx_select!(*texture_view => global.texture_view_drop(*texture_view)) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "TextureView::drop"),
        }
    }
    fn sampler_drop(&self, sampler: &Self::SamplerId) {
        let global = &self.0;
        wgc::gfx_select!(*sampler => global.sampler_drop(*sampler))
    }
    fn bind_group_drop(&self, bind_group: &Self::BindGroupId) {
        let global = &self.0;
        wgc::gfx_select!(*bind_group => global.bind_group_drop(*bind_group))
    }
    fn bind_group_layout_drop(&self, bind_group_layout: &Self::BindGroupLayoutId) {
        let global = &self.0;
        wgc::gfx_select!(*bind_group_layout => global.bind_group_layout_drop(*bind_group_layout))
    }
    fn pipeline_layout_drop(&self, pipeline_layout: &Self::PipelineLayoutId) {
        let global = &self.0;
        wgc::gfx_select!(*pipeline_layout => global.pipeline_layout_drop(*pipeline_layout))
    }
    fn shader_module_drop(&self, shader_module: &Self::ShaderModuleId) {
        let global = &self.0;
        wgc::gfx_select!(*shader_module => global.shader_module_drop(*shader_module))
    }
    fn command_buffer_drop(&self, command_buffer: &Self::CommandBufferId) {
        let global = &self.0;
        wgc::gfx_select!(*command_buffer => global.command_buffer_drop(*command_buffer))
    }
    fn render_bundle_drop(&self, render_bundle: &Self::RenderBundleId) {
        let global = &self.0;
        wgc::gfx_select!(*render_bundle => global.render_bundle_drop(*render_bundle))
    }
    fn compute_pipeline_drop(&self, pipeline: &Self::ComputePipelineId) {
        let global = &self.0;
        wgc::gfx_select!(*pipeline => global.compute_pipeline_drop(*pipeline))
    }
    fn render_pipeline_drop(&self, pipeline: &Self::RenderPipelineId) {
        let global = &self.0;
        wgc::gfx_select!(*pipeline => global.render_pipeline_drop(*pipeline))
    }

    fn compute_pipeline_get_bind_group_layout(
        &self,
        pipeline: &Self::ComputePipelineId,
        index: u32,
    ) -> Self::BindGroupLayoutId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(*pipeline => global.compute_pipeline_get_bind_group_layout(*pipeline, index, PhantomData));
        if let Some(err) = error {
            panic!("Error reflecting bind group {}: {}", index, err);
        }
        id
    }
    fn render_pipeline_get_bind_group_layout(
        &self,
        pipeline: &Self::RenderPipelineId,
        index: u32,
    ) -> Self::BindGroupLayoutId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(*pipeline => global.render_pipeline_get_bind_group_layout(*pipeline, index, PhantomData));
        if let Some(err) = error {
            panic!("Error reflecting bind group {}: {}", index, err);
        }
        id
    }

    fn command_encoder_copy_buffer_to_buffer(
        &self,
        encoder: &Self::CommandEncoderId,
        source: &Self::BufferId,
        source_offset: wgt::BufferAddress,
        destination: &Self::BufferId,
        destination_offset: wgt::BufferAddress,
        copy_size: wgt::BufferAddress,
    ) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_copy_buffer_to_buffer(
            encoder.id,
            source.id,
            source_offset,
            destination.id,
            destination_offset,
            copy_size
        )) {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::copy_buffer_to_buffer",
            );
        }
    }

    fn command_encoder_copy_buffer_to_texture(
        &self,
        encoder: &Self::CommandEncoderId,
        source: crate::BufferCopyView,
        destination: crate::TextureCopyView,
        copy_size: wgt::Extent3d,
    ) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_copy_buffer_to_texture(
            encoder.id,
            &map_buffer_copy_view(source),
            &map_texture_copy_view(destination),
            &copy_size
        )) {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::copy_buffer_to_texture",
            );
        }
    }

    fn command_encoder_copy_texture_to_buffer(
        &self,
        encoder: &Self::CommandEncoderId,
        source: crate::TextureCopyView,
        destination: crate::BufferCopyView,
        copy_size: wgt::Extent3d,
    ) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_copy_texture_to_buffer(
            encoder.id,
            &map_texture_copy_view(source),
            &map_buffer_copy_view(destination),
            &copy_size
        )) {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::copy_texture_to_buffer",
            );
        }
    }

    fn command_encoder_copy_texture_to_texture(
        &self,
        encoder: &Self::CommandEncoderId,
        source: crate::TextureCopyView,
        destination: crate::TextureCopyView,
        copy_size: wgt::Extent3d,
    ) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_copy_texture_to_texture(
            encoder.id,
            &map_texture_copy_view(source),
            &map_texture_copy_view(destination),
            &copy_size
        )) {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::copy_texture_to_texture",
            );
        }
    }

    fn command_encoder_begin_compute_pass(
        &self,
        encoder: &Self::CommandEncoderId,
        desc: &ComputePassDescriptor,
    ) -> Self::ComputePassId {
        wgc::command::ComputePass::new(
            encoder.id,
            &wgc::command::ComputePassDescriptor {
                label: desc.label.map(Borrowed),
            },
        )
    }

    fn command_encoder_end_compute_pass(
        &self,
        encoder: &Self::CommandEncoderId,
        pass: &mut Self::ComputePassId,
    ) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(
            encoder.id => global.command_encoder_run_compute_pass(encoder.id, pass)
        ) {
            let name = wgc::gfx_select!(encoder.id => global.command_buffer_label(encoder.id));
            self.handle_error(
                &encoder.error_sink,
                cause,
                "encoder",
                Some(&name),
                "a ComputePass",
            );
        }
    }

    fn command_encoder_begin_render_pass<'a>(
        &self,
        encoder: &Self::CommandEncoderId,
        desc: &crate::RenderPassDescriptor<'a, '_>,
    ) -> Self::RenderPassId {
        wgc::span!(_guard, TRACE, "CommandEncoder::begin_render_pass wrapper");
        let colors = desc
            .color_attachments
            .iter()
            .map(|ca| wgc::command::ColorAttachmentDescriptor {
                attachment: ca.attachment.id,
                resolve_target: ca.resolve_target.map(|rt| rt.id),
                channel: map_pass_channel(Some(&ca.ops)),
            })
            .collect::<ArrayVec<[_; wgc::device::MAX_COLOR_TARGETS]>>();

        let depth_stencil = desc.depth_stencil_attachment.as_ref().map(|dsa| {
            wgc::command::DepthStencilAttachmentDescriptor {
                attachment: dsa.attachment.id,
                depth: map_pass_channel(dsa.depth_ops.as_ref()),
                stencil: map_pass_channel(dsa.stencil_ops.as_ref()),
            }
        });

        wgc::command::RenderPass::new(
            encoder.id,
            &wgc::command::RenderPassDescriptor {
                label: desc.label.map(Borrowed),
                color_attachments: Borrowed(&colors),
                depth_stencil_attachment: depth_stencil.as_ref(),
            },
        )
    }

    fn command_encoder_end_render_pass(
        &self,
        encoder: &Self::CommandEncoderId,
        pass: &mut Self::RenderPassId,
    ) {
        let global = &self.0;
        if let Err(cause) =
            wgc::gfx_select!(encoder.id => global.command_encoder_run_render_pass(encoder.id, pass))
        {
            let name = wgc::gfx_select!(encoder.id => global.command_buffer_label(encoder.id));
            self.handle_error(
                &encoder.error_sink,
                cause,
                "encoder",
                Some(&name),
                "a RenderPass",
            );
        }
    }

    fn command_encoder_finish(&self, encoder: &Self::CommandEncoderId) -> Self::CommandBufferId {
        let descriptor = wgt::CommandBufferDescriptor::default();
        let global = &self.0;
        let (id, error) =
            wgc::gfx_select!(encoder.id => global.command_encoder_finish(encoder.id, &descriptor));
        if let Some(cause) = error {
            self.handle_error_nolabel(&encoder.error_sink, cause, "a CommandEncoder");
        }
        id
    }

    fn command_encoder_insert_debug_marker(&self, encoder: &Self::CommandEncoderId, label: &str) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_insert_debug_marker(encoder.id, &label))
        {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::insert_debug_marker",
            );
        }
    }
    fn command_encoder_push_debug_group(&self, encoder: &Self::CommandEncoderId, label: &str) {
        let global = &self.0;
        if let Err(cause) = wgc::gfx_select!(encoder.id => global.command_encoder_push_debug_group(encoder.id, &label))
        {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::push_debug_group",
            );
        }
    }
    fn command_encoder_pop_debug_group(&self, encoder: &Self::CommandEncoderId) {
        let global = &self.0;
        if let Err(cause) =
            wgc::gfx_select!(encoder.id => global.command_encoder_pop_debug_group(encoder.id))
        {
            self.handle_error_nolabel(
                &encoder.error_sink,
                cause,
                "CommandEncoder::pop_debug_group",
            );
        }
    }

    fn render_bundle_encoder_finish(
        &self,
        encoder: Self::RenderBundleEncoderId,
        desc: &crate::RenderBundleDescriptor,
    ) -> Self::RenderBundleId {
        let global = &self.0;
        let (id, error) = wgc::gfx_select!(encoder.parent() => global.render_bundle_encoder_finish(
            encoder,
            &desc.map_label(|l| l.map(Borrowed)),
            PhantomData
        ));
        if let Some(err) = error {
            self.handle_error_fatal(err, "RenderBundleEncoder::finish");
        }
        id
    }

    fn queue_write_buffer(
        &self,
        queue: &Self::QueueId,
        buffer: &Self::BufferId,
        offset: wgt::BufferAddress,
        data: &[u8],
    ) {
        let global = &self.0;
        match wgc::gfx_select!(
            *queue => global.queue_write_buffer(*queue, buffer.id, offset, data)
        ) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Queue::write_buffer"),
        }
    }

    fn queue_write_texture(
        &self,
        queue: &Self::QueueId,
        texture: crate::TextureCopyView,
        data: &[u8],
        data_layout: wgt::TextureDataLayout,
        size: wgt::Extent3d,
    ) {
        let global = &self.0;
        match wgc::gfx_select!(*queue => global.queue_write_texture(
            *queue,
            &map_texture_copy_view(texture),
            data,
            &data_layout,
            &size
        )) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Queue::write_texture"),
        }
    }

    fn queue_submit<I: Iterator<Item = Self::CommandBufferId>>(
        &self,
        queue: &Self::QueueId,
        command_buffers: I,
    ) {
        let temp_command_buffers = command_buffers.collect::<SmallVec<[_; 4]>>();

        let global = &self.0;
        match wgc::gfx_select!(*queue => global.queue_submit(*queue, &temp_command_buffers)) {
            Ok(()) => (),
            Err(err) => self.handle_error_fatal(err, "Queue::submit"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SwapChainOutputDetail {
    swap_chain_id: wgc::id::SwapChainId,
}

type ErrorSink = Arc<Mutex<ErrorSinkRaw>>;

struct ErrorSinkRaw {
    uncaptured_handler: Box<dyn crate::UncapturedErrorHandler>,
}

impl ErrorSinkRaw {
    fn new() -> ErrorSinkRaw {
        ErrorSinkRaw {
            uncaptured_handler: Box::from(default_error_handler),
        }
    }

    fn handle_error(&self, err: crate::Error) {
        (self.uncaptured_handler)(err);
    }
}

impl fmt::Debug for ErrorSinkRaw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ErrorSink")
    }
}

fn default_error_handler(err: crate::Error) {
    eprintln!("wgpu error: {}\n", err);

    panic!("Handling wgpu errors as fatal by default");
}

#[derive(Debug)]
pub struct BufferMappedRange<'a> {
    ptr: *mut u8,
    size: usize,
    phantom: PhantomData<&'a ()>,
}

impl<'a> crate::BufferMappedRangeSlice for BufferMappedRange<'a> {
    fn slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.size) }
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl<'a> Drop for BufferMappedRange<'a> {
    fn drop(&mut self) {
        // Intentionally left blank so that `BufferMappedRange` still
        // implements `Drop`, to match the web backend
    }
}
