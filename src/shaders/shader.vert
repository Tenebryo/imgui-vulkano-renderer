#version 450

layout(push_constant) uniform VertPC {
    mat4 matrix;
};

layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 uv;
layout(location = 2) in uint col;

layout(location = 0) out vec2 f_uv;
layout(location = 1) out vec4 f_color;

// Built-in:
// vec4 gl_Position

void main() {

    const float ENC_SCALE = 1.0 / 255.0;

    vec4 col_dec = ENC_SCALE * vec4(
        bitfieldExtract(col, 0, 8),
        bitfieldExtract(col, 8, 8),
        bitfieldExtract(col, 16, 8),
        bitfieldExtract(col, 24, 8)
    );

    f_uv = uv;
    f_color = col_dec;
    gl_Position = matrix * vec4(pos.xy, 0, 1);
}