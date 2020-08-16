# Windows and Surfaces

In this section, you'll create a window with an event loop, handle some events, and create a surface that `wgpu` can render to.

## `winit` crash course

`winit` is the de facto standard for windowing and input handling in Rust.
Add it to your `Cargo.toml`:

```toml
[dependencies]
winit = "0.22"
```

Programming with `winit` centers around the `EventLoop` type, which delivers window and input events to your application.
Creating an event loop is straightforward:

```rust,no_run,no_playground
use winit::event_loop::EventLoop;

fn main() {
    let event_loop = EventLoop::new();

    // ...
}
```

You can use this event loop to create a `Window`.
`winit` provides a handy `WindowBuilder` that allows the window to be configured prior to creation.
Create a new window like this:

```rust,no_run,no_playground
use winit::window::WindowBuilder;

fn main() {
    // let event_loop = ...
    let window = WindowBuilder::new()
        .with_title("Hello, wgpu!")
        .build(&event_loop)
        .expect("Window creation failed.");
}
```

In addition to setting the window title, `WindowBuilder` can be used to set the window size, visibility, decorations, and icon, as well as to disable resizing and enable fullscreen mode.

Pass the newly created `EventLoop` and `Window` to your `run` function:

```rust,no_run,no_playground
use winit::window::Window;

async fn run(event_loop: EventLoop<()>, window: Window) {
    // ...
}

fn main() {
    // let window = ...
    futures::executor::block_on(run(event_loop, window));
}
```

If you look at the argument type for `event_loop`, you'll notice that the `EventLoop` has a generic type paramter of `()`.
`winit` allows an event loop to have a user-defined event type which can be explicitly sent to the event loop and handled by your event handler.
While the "default constructor", `EventLoop::new`, returns an event loop with a user event type of `()` (i.e., no data in user events), you can use `EventLoop::with_user_event` to specify a different type.

Now that `run` has access to the event loop and the window, you can start the event loop and handle incoming events.
Before doing that, though, an important note about `EventLoop::run` is that it does not return to the calling thread, but instead exits entirely.
It's also required to run on the main thread.
As a result, **anything not owned by the closure passed to `EventLoop::run` will not be dropped**.


With that in mind, start the event loop at the end of your `run` function:

```rust,no_run,no_playground
async fn run(event_loop: EventLoop<()>, window: Window) {
    // let (device, queue) = ...

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => *control_flow = ControlFlow::Exit,
        _ => (),
    });
}
```

The closure passed to `EventLoop::run` has the type

```rust,no_run,no_playground
FnMut(Event<T>, &EventLoopWindowTarget<T>, &mut ControlFlow)
```

Don't worry about `EventLoopWindowTarget` for now; it's not necessary unless you intend to create more than one window.
The important arguments to this closure are an `Event<T>`, where `T` is the user event type, and `&mut ControlFlow`, which allows you to change the flow of the event loop.
In this case, a `WindowEvent::CloseRequested` changes `control_flow` to `ControlFlow::Exit`, which will cause the event loop to exit once the current iteration finishes.

If you run the example now, you'll see that a window is created with the specified title, and that it closes when requested.
The content of the window is platform-dependent at this stage; since you haven't drawn anything to it yet, it might be a solid color, or transparent, or full of garbage data.

## Surface creation

In order to get images from the GPU onto the window, `wgpu` provides a `Surface` type to which rendered images may be presented.
Surface creation is simple:

```rust,no_run,no_playground
async fn run(event_loop: EventLoop<()>, window: Window) {
    // let instance = ...

    let surface = unsafe { instance.create_surface(&window) };

    // let adapter = ...
}
```

> #### Why is this `unsafe`?
>
> `Instance::create_surface` doesn't take an argument of type `Window`; rather, it's a generic function that works with any type implementing the `HasRawWindowHandle` trait.
> As the name implies, `HasRawWindowHandle` exposes low-level window handles that aren't guaranteed to be valid, an important consideration when working with platform-specific windowing libraries.
> However, `winit` exposes a high-level interface that ensures the window handle is valid.

Now that you have a window surface, you're ready to set up a rendering pipeline and clear the screen.
That's covered in the next section, as is properly handling changes in window size.
