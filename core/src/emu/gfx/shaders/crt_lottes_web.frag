#version 300 es
// CRT-Lottes-Fast - port of Timothy Lottes' optimized CRT shader (public domain)
precision highp float;

uniform vec2 u_output_size;
uniform vec2 u_texture_size;
uniform vec2 u_input_size;
uniform float u_enabled;
uniform sampler2D u_texture;

in vec2 v_uv;
out vec4 frag_color;

const float CRT_GAMMA = 2.4;
const float SCANLINE_THINNESS = 0.6;
const float SCAN_BLUR = 2.5;
const float MASK_INTENSITY = 0.5;
const float CURVATURE = 0.03;
const float CORNER = 2.0;
const float MASK_TYPE = 1.0;

const float INPUT_THIN = 0.5 + 0.5 * SCANLINE_THINNESS;
const float INPUT_BLUR = -1.0 * SCAN_BLUR;
const float INPUT_MASK = 1.0 - MASK_INTENSITY;

float from_srgb1(float c) {
    if (c <= 0.04045) return c * (1.0 / 12.92);
    return pow(c * (1.0 / 1.055) + (0.055 / 1.055), CRT_GAMMA);
}

vec3 from_srgb(vec3 c) {
    return vec3(from_srgb1(c.r), from_srgb1(c.g), from_srgb1(c.b));
}

float to_srgb1(float c) {
    if (c < 0.0031308) return c * 12.92;
    return 1.055 * pow(c, 0.41666) - 0.055;
}

vec3 to_srgb(vec3 c) {
    return vec3(to_srgb1(c.r), to_srgb1(c.g), to_srgb1(c.b));
}

vec3 crts_fetch(vec2 uv) {
    vec2 scaled = uv * u_input_size / u_texture_size;
    return from_srgb(texture(u_texture, scaled).rgb);
}

vec4 crts_tone(float contrast, float saturation, float thin, float mask_val) {
    float m = mask_val;
    if (MASK_TYPE == 0.0) m = 1.0;
    if (MASK_TYPE == 1.0) m = 0.5 + m * 0.5;
    float mid_out = 0.18 / ((1.5 - thin) * (0.5 * m + 0.5));
    float p_mid_in = pow(0.18, contrast);
    float y = ((-p_mid_in) + mid_out) / ((1.0 - p_mid_in) * mid_out);
    float z = ((-p_mid_in) * mid_out + p_mid_in) / (mid_out * (-p_mid_in) + mid_out);
    return vec4(contrast, y, z, contrast + saturation);
}

vec3 crts_mask(vec2 pos, float dark) {
    if (MASK_TYPE == 2.0) {
        vec3 m = vec3(dark);
        float x = fract(pos.x * (1.0 / 3.0));
        if (x < (1.0 / 3.0)) m.r = 1.0;
        else if (x < (2.0 / 3.0)) m.g = 1.0;
        else m.b = 1.0;
        return m;
    }
    if (MASK_TYPE == 1.0) {
        vec3 m = vec3(1.0);
        float x = fract(pos.x * (1.0 / 3.0));
        if (x < (1.0 / 3.0)) m.r = dark;
        else if (x < (2.0 / 3.0)) m.g = dark;
        else m.b = dark;
        return m;
    }
    if (MASK_TYPE == 0.0) {
        return vec3(1.0);
    }
    // Shadow mask (type 3)
    vec2 spos = pos;
    spos.x += spos.y * 2.9999;
    vec3 m = vec3(dark);
    float x = fract(spos.x * (1.0 / 6.0));
    if (x < (1.0 / 3.0)) m.r = 1.0;
    else if (x < (2.0 / 3.0)) m.g = 1.0;
    else m.b = 1.0;
    return m;
}

vec3 crts_filter(
    vec2 ipos,
    vec2 input_size_div_output_size,
    vec2 half_input_size,
    vec2 rcp_input_size,
    vec2 rcp_output_size,
    vec2 two_div_output_size,
    float input_height,
    vec2 warp,
    float thin,
    float blur,
    float mask_val,
    vec4 tone
) {
    // Apply warp
    vec2 pos = ipos * two_div_output_size - vec2(1.0);
    pos *= vec2(
        1.0 + (pos.y * pos.y) * warp.x,
        1.0 + (pos.x * pos.x) * warp.y
    );
    float vin = clamp(
        -(1.0 - (1.0 - clamp(pos.x * pos.x, 0.0, 1.0)) * (1.0 - clamp(pos.y * pos.y, 0.0, 1.0)))
        * (0.998 + 0.001 * CORNER)
        * input_height + input_height,
        0.0, 1.0
    );
    pos = pos * half_input_size + half_input_size;

    float y0 = floor(pos.y - 0.5) + 0.5;
    float x0 = floor(pos.x - 1.5) + 0.5;
    vec2 p = vec2(x0 * rcp_input_size.x, y0 * rcp_input_size.y);

    vec3 col_a0 = crts_fetch(p);
    p.x += rcp_input_size.x;
    vec3 col_a1 = crts_fetch(p);
    p.x += rcp_input_size.x;
    vec3 col_a2 = crts_fetch(p);
    p.x += rcp_input_size.x;
    vec3 col_a3 = crts_fetch(p);
    p.y += rcp_input_size.y;
    vec3 col_b3 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    vec3 col_b2 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    vec3 col_b1 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    vec3 col_b0 = crts_fetch(p);

    float off = pos.y - y0;
    float pi2 = 6.28318530717958;
    float hlf = 0.5;
    float scan_a = cos(min(0.5, off * thin) * pi2) * hlf + hlf;
    float scan_b = cos(min(0.5, (-off) * thin + thin) * pi2) * hlf + hlf;

    float off0 = pos.x - x0;
    float off1 = off0 - 1.0;
    float off2 = off0 - 2.0;
    float off3 = off0 - 3.0;
    float pix0 = exp2(blur * off0 * off0);
    float pix1 = exp2(blur * off1 * off1);
    float pix2 = exp2(blur * off2 * off2);
    float pix3 = exp2(blur * off3 * off3);
    float pix_t = 1.0 / (pix0 + pix1 + pix2 + pix3);
    pix_t *= vin;

    float sa = scan_a * pix_t;
    float sb = scan_b * pix_t;

    vec3 color = (col_a0 * pix0 + col_a1 * pix1 + col_a2 * pix2 + col_a3 * pix3) * sa
               + (col_b0 * pix0 + col_b1 * pix1 + col_b2 * pix2 + col_b3 * pix3) * sb;

    color *= crts_mask(ipos, mask_val);

    float peak = max(1.0 / (256.0 * 65536.0), max(color.r, max(color.g, color.b)));
    vec3 ratio = color * (1.0 / peak);
    float adjusted_peak = peak * (1.0 / (peak * tone.y + tone.z));
    return ratio * adjusted_peak;
}

void main() {
    if (u_enabled < 0.5) {
        frag_color = texture(u_texture, v_uv);
        return;
    }

    vec2 ipos = v_uv * u_output_size * (u_texture_size / u_input_size);

    vec2 warp_factor;
    warp_factor.x = CURVATURE;
    warp_factor.y = (3.0 / 4.0) * warp_factor.x;

    vec4 tone = crts_tone(1.0, 0.0, INPUT_THIN, INPUT_MASK);

    vec3 color = crts_filter(
        ipos,
        u_input_size / u_output_size,
        u_input_size * vec2(0.5),
        1.0 / u_input_size,
        1.0 / u_output_size,
        2.0 / u_output_size,
        u_input_size.y,
        warp_factor,
        INPUT_THIN,
        INPUT_BLUR,
        INPUT_MASK,
        tone
    );

    frag_color = vec4(to_srgb(color), 1.0);
}
