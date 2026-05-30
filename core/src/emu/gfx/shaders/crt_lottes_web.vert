#version 300 es
precision highp float;

out vec2 v_uv;

void main() {
    float x = float((gl_VertexID & 1) * 2 - 1);
    float y = float((gl_VertexID >> 1) * 2 - 1);
    gl_Position = vec4(x, y, 0.0, 1.0);
    v_uv = vec2((x + 1.0) * 0.5, (1.0 - y) * 0.5);
}
