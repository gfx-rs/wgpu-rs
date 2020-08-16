# Clearing the Screen

In this section, you'll learn how to clear the screen and handle window resizes.

## The swap chain

`wgpu` represents the framebuffers that will be presented to the screen with a data structure called a `SwapChain`.
If you're coming from Vulkan or D3D12, you'll already be familiar with this term.
Briefly, the `SwapChain` determines the format, size and presentation mode of the frames drawn by `wgpu`.

A `SwapChain` is created using `Device::create_swap_chain`:

```rust,no_run,no_playground
async fn run(event_loop: EventLoop<()>, window: Window) {
    // let (device, queue) = ...
    
    let size = window.inner_size();
    let sc_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Mailbox,
    };

    let mut swap_chain = device.create_swap_chain(&surface, &sc_desc);

    // event_loop.run(...);
}
```

A few notes on this code:
- The `width` and `height` fields of `SwapChainDescriptor` take their values from the window size.
  This is not maintained automatically, so the swap chain needs to be recreated when the window size changes.
- The choice of `format` is platform-dependent. `Bgra8UnormSrgb` is ubiquitous on desktop Vulkan implementations ([Windows], [Linux]), [required on D3D12], and [the default for Metal] regardless of platform. When developing for mobile or the web, a different format may be required.
- `PresentMode::Mailbox` allows the application to render frames more quickly than the display refresh rate, while preventing tearing by displaying the most recent complete frame at every vertical blank.

## Render passes

Clearing the screen in `wgpu` occurs during the render target load at the beginning of a render pass.
A naive approach would be to render a frame every iteration of the event loop.
However, the event loop runs for every input event!
If, for instance, the user has an input device with a polling rate of 500Hz, this could result in the application attempting to render at 500 frames per second.
Instead, it's a good idea to let `winit` "batch" these events together and use either its `MainEventsCleared` or `RedrawRequested` events to decide when to output a frame.

```rust,no_run,no_playground
async fn run(event_loop: EventLoop<()>, window: Window) {
    // let mut swap_chain = ...

    event_loop.run(move |event, _, control_flow| match event {
        // Event::WindowEvent { .. } => ...

        Event::MainEventsCleared => {
            let frame = swap_chain
                .get_current_frame()
                .expect("Failed to acquire texture from swap chain.")
                .output;
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            // scope the render pass so it drops before submitting
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &frame.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                });
            }

            queue.submit(Some(encoder.finish()));
        }
        _ => (),
    });
}
```

This code takes a few steps to clear the screen:
- The output texture is acquired from the swap chain with `SwapChain::get_current_frame`.
- A `CommandEncoder` is created using `Device::create_command_encoder`.
- A `RenderPass` is created with `CommandEncoder::begin_render_pass`.
  The behavior of the pass is specified by a `RenderPassDescriptor`.
  Here, the output texture from the swap chain is specified as the sole color attachment; on load, it is cleared to `Color::BLACK`, and of course the result is stored at the end of the render pass so it's actually visible.
- Note that there's no explicit `RenderPass::finish` call.
  Render passes end automatically when the `RenderPass` object is dropped.
  That's accomplished here by creating the object in its own scope, so that it drops automatically before moving on.
- Finally, `CommandEncoder::finish` is called to obtain a `CommandBuffer` which can be submitted to the GPU with `CommandQueue::submit`.

Render passes are, of course, useful for much more than clearing the screen.
A render pass can encode any (valid) series of operations that uses the same set of output attachments.
This consists of draw operations as well as state-changing operations that affect those draws.
You'll see how pipeline creation and drawing work in the next section.

## Resizing

Before moving on, it's important to cover window resizes.
If you run the code now, you'll see the screen being cleared, but changing the window size will cause invalid state, crashing the application.
This is because the swap chain output is not the same size as the window surface, so there's no way to present the rendered frame.

To fix this, you can handle changes in window size by reacting to the `WindowEvent::Resized` event.
Since the swap chain only needs to be recreated when rendering a new frame, it's sufficient to set a flag when the window size changes and then recreate the swap chain right before acquiring the output texture.

```rust,no_run,no_playground
async fn run(event_loop: EventLoop<()>, window: Window) {
    // let mut swap_chain = ...
    
    let mut size_changed = false;
    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: win_event,
            ..
        } => match win_event {
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::Resized(_) => size_changed = true,
            _ => (),
        },

        Event::MainEventsCleared => {
            if size_changed {
                // remember to clear the flag!
                size_changed = false;

                let size = window.inner_size();
                swap_chain = device.create_swap_chain(&surface, &wgpu::SwapChainDescriptor {
                    width: size.width,
                    height: size.height,
                    // inherit other values from original swap chain
                    ..sc_desc
                });
            }
            
            // let frame = ...
        }
    }
}
```

When you run the example now, you can resize the window freely without crashes!

In the next section, you'll (finally!) get some interesting output on the screen by creating a render pipeline and drawing a triangle.
Take a second to pat yourself on the back, and then continue on to the next section.

[Windows]: https://vulkan.gpuinfo.org/listsurfaceformats.php?platform=windows
[Linux]: https://vulkan.gpuinfo.org/listsurfaceformats.php?platform=linux
[required on D3D12]: https://docs.microsoft.com/en-us/windows/win32/direct3ddxgi/hardware-support-for-direct3d-12-1-formats#dxgi_format_b8g8r8a8_unorm_srgbfcs-91
[the default for Metal]: https://developer.apple.com/documentation/metalkit/mtkview/1535940-colorpixelformat#discussion
