# Getting started

This book teaches `wgpu` by guiding the reader through a set of example programs.
You'll use a single Cargo project with multiple binaries in order to share functionality between the examples.

## Project setup

Start by creating a new Cargo library project:

```
$ cargo new --lib wgpu-book
$ cd wgpu-book
```

Add the following to your `Cargo.toml`:

```toml
[dependencies]
futures = "0.3"
wgpu = "0.6"
```

> #### Why `futures`?
>
> `wgpu` makes use of Rust's `async`/`.await` syntax to allow asynchronous execution of potentially blocking operations.
> Mapping buffers for reading and writing is one example.

## Initialization

The first example will simply initialize the library and print some information about the available graphics devices on the system.
Initialization is performed by constructing an `Instance` and specifying the desired backend(s).
Create a new Rust file, `hello.rs`, in the `src/bin` directory, and add the following:

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
  An operating system may provide a software implementation of a particular backend; this is also considered an adapter.
  
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

    println!("{:#06x?}", adapter.get_info());
}
```

When `power_preference` is set to `Default`, the OS is allowed to use its own power policy to select an adapter.
To override this, specify either `LowPower` or `HighPerformance`.
`compatible_surface` is left as `None` for now, since there's no surface to render to yet.

[`BackendBit`]: https://docs.rs/wgpu/0.6/wgpu/struct.BackendBit.html
