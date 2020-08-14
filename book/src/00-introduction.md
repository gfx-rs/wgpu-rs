# The wgpu-rs Book

Welcome! This book will teach you how to use `wgpu`, a Rust library for graphics programming.

## What is `wgpu`?
`wgpu` is a safe, ergonomic Rust wrapper that exposes the [WebGPU API].
The library can target both native applications (using Vulkan, D3D12, or Metal) and the web (by using WebGPU directly through WebAssembly).

## Who is this book for?

This book is targeted at Rust developers with an intermediate amount of experience in graphics programming.
If you're a competent Rust programmer and are comfortable with at least one graphics API (Vulkan, Direct3D, Metal, or OpenGL), this book is for you!

## Who is this book *not* for?

If you're not familiar with Rust as a programming language, you're likely to find much of this book difficult to understand.
The best place to start learning Rust is [The Rust Programming Language].

If you're just getting started with graphics programming, this book will not provide the fundamental knowledge you need to be comfortable programming with `wgpu`.
There are a variety of graphics programming tutorials available for free on the web; [Learn OpenGL] is one popular choice.

[WebGPU API]: https://gpuweb.github.io/gpuweb/
[The Rust Programming Language]: https://doc.rust-lang.org/book/
[Learn OpenGL]: https://learnopengl.com/
