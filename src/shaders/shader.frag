#version 450

layout(binding = 0) uniform sampler2D tex;

layout(location = 0) in vec2 f_uv;
layout(location = 1) in vec4 f_color;

layout(location = 0) out vec4 Target0;

layout(constant_id = 0) const float OUT_GAMMA = 0.0;

void main() {
    Target0 = pow(f_color * texture(tex, f_uv.st), vec4(vec3(OUT_GAMMA), 1.0));
}
