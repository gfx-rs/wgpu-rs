use std::{convert::TryInto, str::FromStr};
use wgpu::util::DeviceExt;

async fn run() {
    let numbers = if std::env::args().len() <= 1 {
        let default = vec![1, 2, 3, 4];
        println!("No numbers were provided, defaulting to {:?}", default);
        default
    } else {
        std::env::args()
            .skip(1)
            .map(|s| u32::from_str(&s).expect("You must pass a list of positive integers!"))
            .collect()
    };

    let times = execute_gpu(numbers).await;
    println!("Times: {:?}", times);
    #[cfg(target_arch = "wasm32")]
    log::info!("Times: {:?}", times);
}

async fn execute_gpu(numbers: Vec<u32>) -> Vec<u32> {
    // Instantiates instance of WebGPU
    let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);

    // `request_adapter` instantiates the general connection to the GPU
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .unwrap();

    // `request_device` instantiates the feature specific connection to the GPU, defining some parameters,
    //  `features` being the available features.
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
            },
            None,
        )
        .await
        .unwrap();

    // Loads the shader from the SPIR-V file.arrayvec
    let cs_module = device.create_shader_module(&wgpu::include_spirv!("shader.comp.spv"));

    // Gets the size in bytes of the buffer.
    let slice_size = numbers.len() * std::mem::size_of::<u32>();
    let size = slice_size as wgpu::BufferAddress;

    // Instantiates buffer without data.
    // `usage` of buffer specifies how it can be used:
    //   `BufferUsage::MAP_READ` allows it to be read (outside the shader).
    //   `BufferUsage::COPY_DST` allows it to be the destination of the copy.
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_DST,
        mapped_at_creation: false,
    });

    // Instantiates buffer with data (`numbers`).
    // Usage allowing the buffer to be:
    //   A storage buffer (can be bound within a bind group and thus available to a shader).
    //   The destination of a copy.
    //   The source of a copy.
    let storage_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Storage Buffer"),
        contents: bytemuck::cast_slice(&numbers),
        usage: wgpu::BufferUsage::STORAGE
            | wgpu::BufferUsage::COPY_DST
            | wgpu::BufferUsage::COPY_SRC,
    });

    // A bind group defines how buffers are accessed by shaders.
    // It is to WebGPU what a descriptor set is to Vulkan.
    // `binding` here refers to the `binding` of a buffer in the shader (`layout(set = 0, binding = 0) buffer`).

    // Here we specifiy the layout of the bind group.
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,                             // The location
            visibility: wgpu::ShaderStage::COMPUTE, // Which shader type in the pipeline this buffer is available to.
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage {
                    // Specifies if the buffer can only be read within the shader
                    read_only: false,
                },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(4),
            },
            count: None,
        }],
    });

    // Instantiates the bind group, once again specifying the binding of buffers.
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: storage_buffer.as_entire_binding(),
        }],
    });

    // A pipeline specifices the operation of a shader

    // Here we specifiy the layout of the pipeline.
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // Instantiates the pipeline.
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        compute_stage: wgpu::ProgrammableStageDescriptor {
            module: &cs_module,
            entry_point: "main",
        },
    });

    // A command encoder executes one or many pipelines.
    // It is to WebGPU what a command buffer is to Vulkan.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
        cpass.set_pipeline(&compute_pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.insert_debug_marker("compute collatz iterations");
        cpass.dispatch(numbers.len() as u32, 1, 1); // Number of cells to run, the (x,y,z) size of item being processed
    }
    // Sets adds copy operation to command encoder.
    // Will copy data from storage buffer on GPU to staging buffer on CPU.
    encoder.copy_buffer_to_buffer(&storage_buffer, 0, &staging_buffer, 0, size);

    // Submits command encoder for processing
    queue.submit(Some(encoder.finish()));

    let buffer_slice = staging_buffer.slice(..);

    // Wait for the slice of `staging_buffer` to be mapped for reading
    if let Ok(()) = buffer_slice.map_async(wgpu::MapMode::Read).await {
        // Gets contents of buffer
        let data = buffer_slice.get_mapped_range();
        // Since contents are got in bytes, this converts these bytes back to u32
        let result = data
            .chunks_exact(4)
            .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
            .collect();

        // With the current interface, we have to make sure all mapped views are
        // dropped before we unmap the buffer.
        drop(data);
        staging_buffer.unmap(); // Unmaps buffer from memory
                                // If you are familiar with C++ these 2 lines can be thought of similarly to:
                                //   delete myPointer;
                                //   myPointer = NULL;
                                // It effectively frees the memory

        // Returns data from buffer
        result
    } else {
        panic!("failed to run compute on gpu!")
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        wgpu_subscriber::initialize_default_subscriber(None);
        pollster::block_on(run());
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn test_compute_1() {
        let input = vec![1, 2, 3, 4];
        pollster::block_on(assert_execute_gpu(input, vec![0, 1, 7, 2]));
    }

    #[test]
    fn test_compute_2() {
        let input = vec![5, 23, 10, 9];
        pollster::block_on(assert_execute_gpu(input, vec![5, 15, 6, 19]));
    }

    #[test]
    fn test_multithreaded_compute() {
        use std::{sync::mpsc, thread, time::Duration};

        let thread_count = 8;

        let (tx, rx) = mpsc::channel();
        for _ in 0..thread_count {
            let tx = tx.clone();
            thread::spawn(move || {
                let input = vec![100, 100, 100];
                pollster::block_on(assert_execute_gpu(input, vec![25, 25, 25]));
                tx.send(true).unwrap();
            });
        }

        for _ in 0..thread_count {
            rx.recv_timeout(Duration::from_secs(10))
                .expect("A thread never completed.");
        }
    }

    async fn assert_execute_gpu(input: Vec<u32>, expected: Vec<u32>) {
        assert_eq!(execute_gpu(input).await, expected);
    }
}
