use ash::{
    version::{EntryV1_0, InstanceV1_0},
    vk::{self, Handle},
};
use std::{borrow::Cow, convert::TryInto};
use wgpu::{
    Adapter, Device, Extent3d, Instance, Queue, ShaderFlags, ShaderSource, TextureAspect,
    TextureView, TextureViewDescriptor, TextureViewDimension,
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

fn main() {
    let event_loop = EventLoop::new();
    let window = winit::window::Window::new(&event_loop).unwrap();

    pollster::block_on(run(event_loop, window));
}

async fn run(event_loop: EventLoop<()>, window: Window) {
    let xr_entry = openxr::Entry::load().unwrap();

    // Initialize OpenXR
    let mut enabled_extensions = openxr::ExtensionSet::default();

    // Note: At time of writing, Oculus does not support this extension, but it's usable through the
    // OpenVR runtime instead on Oculus.
    enabled_extensions.khr_vulkan_enable2 = true;

    let xr_instance = xr_entry
        .create_instance(
            &openxr::ApplicationInfo {
                application_name: "wgpu-hello-vr-triangle",
                application_version: 0,
                engine_name: "wgpu-hello-vr-triangle",
                engine_version: 0,
            },
            &enabled_extensions,
            &[],
        )
        .unwrap();
    let instance_props = xr_instance.properties().unwrap();
    println!(
        "Loaded OpenXR runtime: {} {}",
        instance_props.runtime_name, instance_props.runtime_version,
    );

    // Fetch a head mounted display to render to
    let xr_system = xr_instance
        .system(openxr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .unwrap();
    let environment_blend_mode = xr_instance
        .enumerate_environment_blend_modes(xr_system, VIEW_TYPE)
        .unwrap()[0];

    // Initialize graphics context
    let (
        vk_entry,
        vk_instance,
        vk_physical_device,
        queue_family_index,
        vk_device,
        instance,
        adapter,
        device,
        queue,
    ) = initialize_wgpu_openxr(&xr_instance, xr_system);
    let surface = unsafe { instance.create_surface(&window) };

    // Load the shaders from disk
    let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        flags: ShaderFlags::all(),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let swapchain_format = adapter.get_swap_chain_preferred_format(&surface).unwrap();

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[swapchain_format.into()],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
    });

    let size = window.inner_size();
    let mut sc_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Mailbox,
    };

    let mut swap_chain = device.create_swap_chain(&surface, &sc_desc);

    // Start the OpenXR session
    let (xr_session, mut frame_wait, mut frame_stream) = unsafe {
        xr_instance
            .create_session::<openxr::Vulkan>(
                xr_system,
                &openxr::vulkan::SessionCreateInfo {
                    instance: vk_instance.handle().as_raw() as _,
                    physical_device: vk_physical_device.as_raw() as _,
                    device: vk_device.handle().as_raw() as _,
                    queue_family_index,
                    queue_index: 0,
                },
            )
            .unwrap()
    };

    // Create a room-scale reference space
    let stage = xr_session
        .create_reference_space(openxr::ReferenceSpaceType::STAGE, openxr::Posef::IDENTITY)
        .unwrap();

    let mut event_storage = openxr::EventDataBuffer::new();
    let mut session_running = false;
    let mut swapchain = None;

    event_loop.run(move |event, _, control_flow| {
        // Have the closure take ownership of the resources.
        // `event_loop.run` never returns, therefore we must do this to ensure
        // the resources are properly cleaned up.
        let _ = (
            &vk_entry,
            &vk_instance,
            &vk_device,
            &instance,
            &adapter,
            &shader,
            &pipeline_layout,
        );

        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Recreate the swap chain with the new size
                sc_desc.width = size.width;
                sc_desc.height = size.height;
                swap_chain = device.create_swap_chain(&surface, &sc_desc);
            }
            Event::RedrawRequested(_) => {
                let frame = swap_chain
                    .get_current_frame()
                    .expect("Failed to acquire next swap chain texture")
                    .output;
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[wgpu::RenderPassColorAttachment {
                            view: &frame.view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline);
                    rpass.draw(0..3, 0..1);
                }

                queue.submit(Some(encoder.finish()));
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::MainEventsCleared => {
                // Handle OpenXR events
                while let Some(event) = xr_instance.poll_event(&mut event_storage).unwrap() {
                    match event {
                        openxr::Event::SessionStateChanged(e) => {
                            // Session state change is where we can begin and end sessions, as well as
                            // find quit messages!
                            println!("Entered state {:?}", e.state());
                            match e.state() {
                                openxr::SessionState::READY => {
                                    xr_session.begin(VIEW_TYPE).unwrap();
                                    session_running = true;
                                }
                                openxr::SessionState::STOPPING => {
                                    xr_session.end().unwrap();
                                    session_running = false;
                                }
                                openxr::SessionState::EXITING
                                | openxr::SessionState::LOSS_PENDING => {
                                    *control_flow = ControlFlow::Exit;
                                }
                                _ => {}
                            }
                        }
                        openxr::Event::InstanceLossPending(_) => {
                            *control_flow = ControlFlow::Exit;
                        }
                        openxr::Event::EventsLost(e) => {
                            println!("Lost {} OpenXR events", e.lost_event_count());
                        }
                        _ => {}
                    }
                }

                // Render to HMD only if we have an active session
                if session_running {
                    // Block until the previous frame is finished displaying, and is ready for
                    // another one. Also returns a prediction of when the next frame will be
                    // displayed, for use with predicting locations of controllers, viewpoints, etc.
                    let xr_frame_state = frame_wait.wait().unwrap();

                    // Must be called before any rendering is done!
                    frame_stream.begin().unwrap();

                    // Only render if we should
                    if !xr_frame_state.should_render {
                        // Early bail
                        frame_stream
                            .end(
                                xr_frame_state.predicted_display_time,
                                environment_blend_mode,
                                &[],
                            )
                            .unwrap();
                        return;
                    }

                    // If we do not have a swapchain yet, create it
                    let (xr_swapchain, resolution, image_views) =
                        swapchain.get_or_insert_with(|| {
                            create_swapchain(&xr_instance, xr_system, &xr_session, &device)
                        });

                    // Check which image we need to render to and wait until the compositor is
                    // done with this image
                    let image_index = xr_swapchain.acquire_image().unwrap();
                    xr_swapchain.wait_image(openxr::Duration::INFINITE).unwrap();
                    let (left_view, right_view) = &image_views[image_index as usize];

                    // Render!
                    let mut encoder = device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: None,
                            color_attachments: &[wgpu::RenderPassColorAttachment {
                                view: &left_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                    store: true,
                                },
                            }],
                            depth_stencil_attachment: None,
                        });
                        rpass.set_pipeline(&render_pipeline);
                        rpass.draw(0..3, 0..1);
                    }
                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: None,
                            color_attachments: &[wgpu::RenderPassColorAttachment {
                                view: &right_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                    store: true,
                                },
                            }],
                            depth_stencil_attachment: None,
                        });
                        rpass.set_pipeline(&render_pipeline);
                        rpass.draw(0..3, 0..1);
                    }

                    // Fetch the view transforms. To minimize latency, we intentionally do this
                    // *after* recording commands to render the scene, i.e. at the last possible
                    // moment before rendering begins in earnest on the GPU. Uniforms dependent on
                    // this data can be sent to the GPU just-in-time by writing them to per-frame
                    // host-visible memory which the GPU will only read once the command buffer is
                    // submitted.
                    let (_, views) = xr_session
                        .locate_views(VIEW_TYPE, xr_frame_state.predicted_display_time, &stage)
                        .unwrap();

                    queue.submit(Some(encoder.finish()));
                    xr_swapchain.release_image().unwrap();

                    // End rendering and submit the images
                    let rect = openxr::Rect2Di {
                        offset: openxr::Offset2Di { x: 0, y: 0 },
                        extent: openxr::Extent2Di {
                            width: resolution.width as _,
                            height: resolution.height as _,
                        },
                    };
                    frame_stream
                        .end(
                            xr_frame_state.predicted_display_time,
                            environment_blend_mode,
                            &[&openxr::CompositionLayerProjection::new()
                                .space(&stage)
                                .views(&[
                                    openxr::CompositionLayerProjectionView::new()
                                        .pose(views[0].pose)
                                        .fov(views[0].fov)
                                        .sub_image(
                                            openxr::SwapchainSubImage::new()
                                                .swapchain(&xr_swapchain)
                                                .image_array_index(0)
                                                .image_rect(rect),
                                        ),
                                    openxr::CompositionLayerProjectionView::new()
                                        .pose(views[1].pose)
                                        .fov(views[1].fov)
                                        .sub_image(
                                            openxr::SwapchainSubImage::new()
                                                .swapchain(&xr_swapchain)
                                                .image_array_index(1)
                                                .image_rect(rect),
                                        ),
                                ])],
                        )
                        .unwrap();
                }
            }
            Event::LoopDestroyed => {
                // TODO: Destroy first WGPU and then raw vulkan handles after
            }
            _ => {}
        }
    });
}

fn initialize_wgpu_openxr(
    xr_instance: &openxr::Instance,
    xr_system: openxr::SystemId,
) -> (
    ash::Entry,
    ash::Instance,
    ash::vk::PhysicalDevice,
    u32,
    ash::Device,
    Instance,
    Adapter,
    Device,
    Queue,
) {
    unsafe {
        // This must always be called before vulkan init
        let _requirements = xr_instance
            .graphics_requirements::<openxr::Vulkan>(xr_system)
            .unwrap();

        // Initialize Vulkan instance
        let vk_entry = ash::Entry::new().unwrap();

        let vk_extensions = wgpu::Instance::required_vulkan_extensions(&vk_entry);
        let mut extension_names_raw = vec![];
        for extension in &vk_extensions {
            extension_names_raw.push(extension.as_ptr());
        }

        let vk_target_version = vk::make_version(1, 1, 0);
        let vk_app_info = vk::ApplicationInfo::builder()
            .application_version(0)
            .engine_version(0)
            .api_version(vk_target_version);

        let vk_instance = xr_instance
            .create_vulkan_instance(
                xr_system,
                std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr),
                &vk::InstanceCreateInfo::builder()
                    .application_info(&vk_app_info)
                    .enabled_extension_names(&extension_names_raw) as *const _
                    as *const _,
            )
            .expect("XR error creating Vulkan instance")
            .map_err(vk::Result::from_raw)
            .expect("Vulkan error creating Vulkan instance");
        let vk_instance = ash::Instance::load(
            vk_entry.static_fn(),
            vk::Instance::from_raw(vk_instance as _),
        );

        // Find the physical device we actually need to initialize with
        let vk_physical_device = vk::PhysicalDevice::from_raw(
            xr_instance
                .vulkan_graphics_device(xr_system, vk_instance.handle().as_raw() as _)
                .unwrap() as _,
        );

        let queue_family_index = vk_instance
            .get_physical_device_queue_family_properties(vk_physical_device)
            .into_iter()
            .enumerate()
            .find_map(|(queue_family_index, info)| {
                if info.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    Some(queue_family_index as u32)
                } else {
                    None
                }
            })
            .expect("Vulkan device has no graphics queue");

        // Initialize WGPU instance using our Vulkan instance
        let instance =
            wgpu::Instance::new_raw_vulkan(vk_entry.clone(), vk_instance.clone(), vk_extensions);
        let adapter = instance.adapter_from_raw_vulkan(vk_physical_device);

        // Create the Vulkan logical device
        let desc = wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        };
        let vk_device_extensions = adapter.required_vulkan_device_extensions(&desc);
        let mut device_extension_names_raw = vec![];
        for extension in &vk_device_extensions {
            device_extension_names_raw.push(extension.as_ptr());
        }

        let queue_create_infos = [vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&[1.0])
            .build()];
        let mut vulkan11_features = vk::PhysicalDeviceVulkan11Features {
            multiview: vk::TRUE,
            ..Default::default()
        };
        let create_device_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extension_names_raw)
            .push_next(&mut vulkan11_features);
        let vk_device_raw = xr_instance
            .create_vulkan_device(
                xr_system,
                std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr),
                vk_physical_device.as_raw() as _,
                &create_device_info as *const _ as *const _,
            )
            .expect("XR error creating Vulkan device")
            .map_err(vk::Result::from_raw)
            .expect("Vulkan error creating Vulkan device");
        let vk_device = ash::Device::load(
            vk_instance.fp_v1_0(),
            vk::Device::from_raw(vk_device_raw as _),
        );

        // Initialize WGPU device using our Device instance
        let (device, queue) =
            adapter.device_from_raw_vulkan(vk_device.clone(), queue_family_index, &desc, None);

        (
            vk_entry,
            vk_instance,
            vk_physical_device,
            queue_family_index,
            vk_device,
            instance,
            adapter,
            device,
            queue,
        )
    }
}

fn create_swapchain(
    xr_instance: &openxr::Instance,
    xr_system: openxr::SystemId,
    xr_session: &openxr::Session<openxr::Vulkan>,
    device: &Device,
) -> (
    openxr::Swapchain<openxr::Vulkan>,
    vk::Extent2D,
    Vec<(TextureView, TextureView)>,
) {
    println!("Creating OpenXR swapchain");

    // Fetch the views we need to render to (the eye screens on the HMD)
    let views = xr_instance
        .enumerate_view_configuration_views(xr_system, VIEW_TYPE)
        .unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0], views[1]);

    // Create the OpenXR swapchain
    let color_format = vk::Format::B8G8R8A8_SRGB;
    let resolution = vk::Extent2D {
        width: views[0].recommended_image_rect_width,
        height: views[0].recommended_image_rect_height,
    };
    let xr_swapchain = xr_session
        .create_swapchain(&openxr::SwapchainCreateInfo {
            create_flags: openxr::SwapchainCreateFlags::EMPTY,
            usage_flags: openxr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | openxr::SwapchainUsageFlags::SAMPLED,
            format: color_format.clone().as_raw() as _,
            sample_count: 1,
            width: resolution.width,
            height: resolution.height,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        })
        .unwrap();

    // Create image views for the swapchain
    let image_views: Vec<_> = xr_swapchain
        .enumerate_images()
        .unwrap()
        .into_iter()
        .map(|image| {
            // Create a WGPU image view for this image
            // TODO: Right now we're using separate image views per eye, we need
            // multiview support in WGPU
            unsafe {
                (
                    device.create_raw_vulkan_texture_view(
                        vk::Image::from_raw(image),
                        vk::ImageViewType::TYPE_2D,
                        &TextureViewDescriptor {
                            label: None,
                            format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
                            dimension: Some(TextureViewDimension::D2Array),
                            aspect: TextureAspect::All,
                            base_mip_level: 0,
                            mip_level_count: Some(1u32.try_into().unwrap()),
                            base_array_layer: 0,
                            array_layer_count: Some(1.try_into().unwrap()),
                        },
                        Extent3d {
                            width: resolution.width,
                            height: resolution.height,
                            depth_or_array_layers: 1,
                        },
                    ),
                    device.create_raw_vulkan_texture_view(
                        vk::Image::from_raw(image),
                        vk::ImageViewType::TYPE_2D,
                        &TextureViewDescriptor {
                            label: None,
                            format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
                            dimension: Some(TextureViewDimension::D2Array),
                            aspect: TextureAspect::All,
                            base_mip_level: 0,
                            mip_level_count: Some(1u32.try_into().unwrap()),
                            base_array_layer: 1,
                            array_layer_count: Some(1.try_into().unwrap()),
                        },
                        Extent3d {
                            width: resolution.width,
                            height: resolution.height,
                            depth_or_array_layers: 1,
                        },
                    ),
                )
            }
        })
        .collect();

    (xr_swapchain, resolution, image_views)
}

const VIEW_TYPE: openxr::ViewConfigurationType = openxr::ViewConfigurationType::PRIMARY_STEREO;
