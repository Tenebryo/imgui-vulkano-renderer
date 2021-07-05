#version 450

layout(set=0, binding=1) uniform sampler2D tex;

layout(location = 0) in vec2 f_uv;
layout(location = 1) in vec4 f_color;

layout(location = 0) out vec4 Target0;

void main() {
    Target0 = f_color * texture(tex, f_uv.st);
}