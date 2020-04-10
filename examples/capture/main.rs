/// This example shows how to capture an image by rendering it to a texture, copying the texture to
/// a buffer, and retrieving it from the buffer. This could be used for "taking a screenshot," with
/// the added benefit that this method doesn't require a window to be created.
use std::{fs::File, mem::size_of, sync::{Arc, mpsc}};

async fn run() {
    let adapter = wgpu::Adapter::request(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            compatible_surface: None,
        },
        wgpu::BackendBit::PRIMARY,
    )
    .await
    .unwrap();

    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    })
    .await;
    
    let device = Arc::new(device);

    let _poller_canceler = spawn_device_poller(Arc::clone(&device));

    // Rendered image is 256×256 with 32-bit RGBA color
    let size = 256u32;

    // The output buffer lets us retrieve the data as an array
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: (size * size) as u64 * size_of::<u32>() as u64,
        usage: wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_DST,
        label: None,
    });

    let texture_extent = wgpu::Extent3d {
        width: size,
        height: size,
        depth: 1,
    };

    // The render pipeline renders data into this texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
        label: None,
    });

    // Set the background to be red
    let command_buffer = {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &texture.create_default_view(),
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::RED,
            }],
            depth_stencil_attachment: None,
        });

        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            wgpu::TextureCopyView {
                texture: &texture,
                mip_level: 0,
                array_layer: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::BufferCopyView {
                buffer: &output_buffer,
                offset: 0,
                bytes_per_row: size_of::<u32>() as u32 * size,
                rows_per_image: 0,
            },
            texture_extent,
        );

        encoder.finish()
    };

    queue.submit(&[command_buffer]);

    // Write the buffer as a PNG
    if let Ok(mapping) = output_buffer.map_read(0, (size * size) as u64 * size_of::<u32>() as u64).await
    {
        let mut png_encoder = png::Encoder::new(File::create("red.png").unwrap(), size, size);
        png_encoder.set_depth(png::BitDepth::Eight);
        png_encoder.set_color(png::ColorType::RGBA);
        png_encoder
            .write_header()
            .unwrap()
            .write_image_data(mapping.as_slice())
            .unwrap();
    }
}

fn spawn_device_poller(device: Arc<wgpu::Device>) -> impl Drop {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Err(_) = rx.try_recv() {
            device.poll(wgpu::Maintain::Wait);
        }
    });

    struct Cancel(mpsc::Sender<()>);

    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
        }
    }

    Cancel(tx)
}

fn main() {
    env_logger::init();

    futures::executor::block_on(run());
}