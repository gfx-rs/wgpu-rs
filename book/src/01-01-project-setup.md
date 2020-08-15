# Project setup

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
> The `futures` crate provides *executors* for running `async` code from a synchronous context, including the simple `block_on` executor used in these examples.

## Project structure

Examples will go in the project's `src/bin/` directory, and can be run with

```
$ cargo run --bin <example>
```

The general project structure will look like this:

```
cargo-book/
  ├ src/
  │ ├ bin/
  │ │ ├ example-01.rs
  │ │ └ example-02.rs
  │ └ lib.rs
  └ Cargo.toml
```

With the basic project structure set up, you're ready to initialize `wgpu` and connect to a graphics device.
You'll learn how to do that in the next section.
