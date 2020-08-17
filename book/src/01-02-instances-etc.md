# Instances, Adapters, and Devices

## Initialization

The first example will simply initialize the library and print some information about the available graphics devices on the system.
Initialization is performed by constructing an `Instance` and specifying the desired backend(s).
Create a new subdirectory `src/bin/hello/`, and in `src/bin/hello/main.rs`, add the following:

```rust,no_run,noplayground
fn main() {
    let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
}
```

This initializes `wgpu` with any of the available stable backends.
If you want to target a specific backend, you can do that too&mdash;see the docs for [`BackendBit`].

> #### What's `BackendBit::PRIMARY`?
>
> `BackendBit` is a bitflag type that specifies the desired backend(s).
> `BackentBit::PRIMARY` is a combination of all backends for which `wgpu` provides "first-class" support&mdash;namely Vulkan, D3D12, and Metal.
> Experimental and incomplete backends (D3D11 and OpenGL) are designated `BackendBit::SECONDARY`.

## Selecting an adapter

Now that you have an `Instance`, you can query for the available adapters on a system with `Instance::enumerate_adapters`.
This returns an iterator over a series of `Adapter` objects.
Try printing out the info for each adapter (`AdapterInfo`), like this:

```rust,no_run,noplayground
fn main() {
    // let instance = ...

    for adapter in instance.enumerate_adapters(wgpu::BackendBit::PRIMARY) {
        // pretty-print the adapter info, with 4-digit hex PCI codes
        println!("{:#06?}", adapter.get_info());
    }
}
```

Run the new code with

```
cargo run --bin hello
```

Here's an example of what the program output might look like:

```
AdapterInfo {
    name: "GeForce GTX 970",
    vendor: 0x10de,
    device: 0x13c2,
    device_type: DiscreteGpu,
    backend: Vulkan,
}
AdapterInfo {
    name: "NVIDIA GeForce GTX 970",
    vendor: 0x10de,
    device: 0x13c2,
    device_type: DiscreteGpu,
    backend: Dx12,
}
AdapterInfo {
    name: "Intel(R) HD Graphics 4600",
    vendor: 0x8086,
    device: 0x0412,
    device_type: IntegratedGpu,
    backend: Dx12,
}
AdapterInfo {
    name: "Microsoft Basic Render Driver",
    vendor: 0x1414,
    device: 0x008c,
    device_type: VirtualGpu,
    backend: Dx12,
}
```

> ℹ️ &ensp; The list of available adapters will depend on your system, as well as the set of backends passed to `Instance::new`.

From this particular list, you'll notice a few things:
- The same adapter may appear multiple times.
  `wgpu` represents an adapter that supports multiple backends as multiple `Adapter` objects, with one for each backend.
- Adapters have both human- and machine-readable identifiers, in the form of strings and PCI IDs, respectively.
  This makes it easy to both display available devices to end users as well as choose devices programmatically.
- Adapters don't necessarily correspond to physical devices.
  An operating system may provide a software implementation of a particular backend, such as the *Microsoft Basic Render Driver* seen here; this is also considered an adapter.
  
### Automatic adapter selection

If you're not looking for a specific adapter, `Instance` provides a convenience method to select one automatically based on more general criteria: `Instance::request_adapter`.
Replace the loop over `enumerate_adapters()` with the following:

```rust,no_run,no_playground
fn main() {
    // let instance = ...

    let adapter = futures::executor::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            compatible_surface: None,
        })
    )
    .expect("No suitable adapter found.");

    // this format specifier pretty-prints the adapter info, rendering the PCI
    // IDs as 4-digit hex numbers
    println!("{:#06x?}", adapter.get_info());
}
```

When `power_preference` is set to `Default`, the OS is allowed to use its own power policy to select an adapter.
To override this, specify either `LowPower` or `HighPerformance`.
`compatible_surface` is left as `None` for now, since there's no surface to render to yet.

## Creating a device

Now that you've selected an adapter to use, you can initialize a logical `Device` which represents a connection to that adapter.
When creating a `Device`, you may specify API [`Features`] that are required for your application, and indicate the minimum necessary [`Limits`] of various resources.
These will be discussed in more depth in later chapters; for now, the defaults are acceptable.

```rust,no_run,no_playground
fn main() {
    // let adapter = ...
    
    let (device, queue) = futures::executor::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            shader_validation: true,
        },
        None,
    ))
    .expect("Failed to create device.");
}
```

A few notes about this code:
- You'll notice that `Adapter::request_device` returns a `(Device, Queue)` rather than just a `Device`.
  In general, the `Device` is used to create resources and record commands; the `Queue` is then responsible for executing those commands.
- `shader_validation` is a feature of `wgpu` which uses runtime reflection to ensure that shaders in a particular pipeline are being used properly.
- `None` is passed as the second argument here to `Adapter::request_device`.
  If this argument is `Some(path)`, then `path` is used as a directory in which to store API traces.
  This functionality requires the `trace` feature to be enabled in your `Cargo.toml`.
  
## Cleaning up

As a last bit of housekeeping, it's a good idea to factor out the two `async` calls into an `async fn` so that, as the code grows, it doesn't have calls to `block_on` littered all over it.
This is a trivial change: just replace the existing calls to `block_on` with `.await`s, and then `block_on` a call to the new function in `main`.

```rust,no_run,no_playground
async fn run() {
    let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
        })
        .await
        .expect("No suitable adapter found.");

    println!("{:#06x?}", adapter.get_info());

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
                shader_validation: true,
            },
            None,
        )
        .await
        .expect("Failed to create device.");
}

fn main() {
    futures::executor::block_on(run());
}
```

Now you're all set up and ready to start issuing commands to the GPU!
First, though, you'll need to be able to see the results of those commands.
In the next section, you'll learn how to create a window with `winit` and a surface that `wgpu` can render to.

[`BackendBit`]: https://docs.rs/wgpu/0.6/wgpu/struct.BackendBit.html
[`Features`]: https://docs.rs/wgpu/0.6/wgpu/struct.Features.html
[`Limits`]: https://docs.rs/wgpu/0.6/wgpu/struct.Limits.html
