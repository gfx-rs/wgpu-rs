[[stage(vertex)]]
fn vs_main([[builtin(vertex_index)]] vertex_index: u32) -> [[builtin(position)]] vec4<f32> {
    const i: i32 = i32(vertex_index % 3u);
    const x: f32 = f32(i - 1) * 0.75;
    const y: f32 = f32((i & 1) * 2 - 1) * 0.75 + x * 0.2 + 0.1;
    return vec4<f32>(x, y, 0.0, 1.0);
}

[[stage(fragment)]]
fn fs_main_colored() -> [[location(0)]] vec4<f32> {
    return vec4<f32>(0.8, 0.1, 0.0, 1.0);
}

[[stage(fragment)]]
fn fs_main_white() -> [[location(0)]] vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}