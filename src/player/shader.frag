#version 330 core

in vec2 uv;
out vec4 frag_color;

uniform sampler2D tex_y;
uniform sampler2D tex_u;
uniform sampler2D tex_v;
uniform usampler2D tex_mask;
//uniform float time;
uniform float flash;
uniform vec3 effect_color;

void main() {
    // YUV -> RGB conversion
    float y = texture(tex_y, uv).r;
    float u = texture(tex_u, uv).r - 0.5;
    float v = texture(tex_v, uv).r - 0.5;
    vec3 video = vec3(y + 1.402 * v,
                      y - 0.344 * u - 0.714 * v,
                      y + 1.772 * u);

    // Effect (flash with color)
    vec3 effect = effect_color * flash;

    // Mask index (0-255)
    uint mask = texture(tex_mask, uv).r;

    bool is_led = mask == 1u;
    bool is_screen = mask >= 2u;
    bool par_screen = mask == 2u;
    bool hex_screen = mask == 3u;
    bool lower_screen = mask == 4u;

    vec3 out_color = vec3(0,0,0);
    if (is_led) {
        out_color = effect;
    }
    if (is_screen) {
        out_color = video;
    }


    frag_color = vec4(out_color, 1.0);
}
