#version 450

out gl_PerVertex {
    vec4 gl_Position;
};

layout(location=0) out vec4 color;

layout(set = 0, binding = 0) uniform Uniforms {
    uint frame;
};

const vec2 quad[6] = vec2[6](
    vec2(0, 1),
    vec2(1, 1),
    vec2(1, 0),
    vec2(0, 1),
    vec2(1, 0),
    vec2(0, 0)
);

void main() {
    // make 10x10 grid of rectangles
    vec2 base = vec2(gl_InstanceIndex%10, gl_InstanceIndex/10);
    vec2 size = vec2(0.09, 0.09);
    vec2 offset = vec2(0.1, 0.1);
    vec2 position = (base*offset  + quad[gl_VertexIndex]*size) * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
    gl_Position = vec4(position, 0.0, 1.0);
    
    // highlight rectangle with the index of currect frame 
    float a = 0.0;
    if( gl_InstanceIndex == frame ) {
        a = 1.0;
    }
    color = vec4(a, 0.0, 0.0, a);
}