#version 330 core

out vec2 uv;

void main() {
    // Two triangles: 0,1,2 and 3,4,5
    // Positions: (-1,-1), (1,-1), (-1,1), (1,-1), (1,1), (-1,1)
    const vec2 positions[6] = vec2[6](
        vec2(-1.0, -1.0),
        vec2( 1.0, -1.0),
        vec2(-1.0,  1.0),
        vec2( 1.0, -1.0),
        vec2( 1.0,  1.0),
        vec2(-1.0,  1.0)
    );
    vec2 pos = positions[gl_VertexID];
    uv = vec2(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    gl_Position = vec4(pos, 0.0, 1.0);
}
