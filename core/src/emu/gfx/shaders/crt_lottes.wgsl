// CRT-Lottes-Fast - port of Timothy Lottes' optimized CRT shader (public domain)
// Original: https://github.com/libretro/glsl-shaders/blob/master/crt/shaders/crt-lottes-fast.glsl

struct Uniforms {
    output_size: vec2<f32>,
    texture_size: vec2<f32>,
    input_size: vec2<f32>,
    enabled: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 2 - 1);
    let y = f32(i32(vertex_index >> 1u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// Parameters
const CRT_GAMMA: f32 = 2.4;
const SCANLINE_THINNESS: f32 = 0.6;
const SCAN_BLUR: f32 = 2.5;
const MASK_INTENSITY: f32 = 0.5;
const CURVATURE: f32 = 0.03;
const CORNER: f32 = 2.0;
const MASK_TYPE: f32 = 1.0; // 0=none, 1=aperture grille lite (Trinitron), 2=aperture grille, 3=shadow mask

const INPUT_THIN: f32 = 0.5 + 0.5 * SCANLINE_THINNESS;
const INPUT_BLUR: f32 = -1.0 * SCAN_BLUR;
const INPUT_MASK: f32 = 1.0 - MASK_INTENSITY;

fn from_srgb1(c: f32) -> f32 {
    if c <= 0.04045 {
        return c * (1.0 / 12.92);
    }
    return pow(c * (1.0 / 1.055) + (0.055 / 1.055), CRT_GAMMA);
}

fn from_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(from_srgb1(c.r), from_srgb1(c.g), from_srgb1(c.b));
}

fn to_srgb1(c: f32) -> f32 {
    if c < 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 0.41666) - 0.055;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_srgb1(c.r), to_srgb1(c.g), to_srgb1(c.b));
}

fn crts_fetch(uv: vec2<f32>) -> vec3<f32> {
    let scaled = uv * uniforms.input_size / uniforms.texture_size;
    return from_srgb(textureSample(tex, tex_sampler, scaled).rgb);
}

fn crts_max3(a: f32, b: f32, c: f32) -> f32 {
    return max(a, max(b, c));
}

fn crts_tone(contrast: f32, saturation: f32, thin: f32, mask_val: f32) -> vec4<f32> {
    var m = mask_val;
    if MASK_TYPE == 0.0 {
        m = 1.0;
    }
    if MASK_TYPE == 1.0 {
        m = 0.5 + m * 0.5;
    }
    let mid_out = 0.18 / ((1.5 - thin) * (0.5 * m + 0.5));
    let p_mid_in = pow(0.18, contrast);
    let y = ((-p_mid_in) + mid_out) / ((1.0 - p_mid_in) * mid_out);
    let z = ((-p_mid_in) * mid_out + p_mid_in) / (mid_out * (-p_mid_in) + mid_out);
    return vec4<f32>(contrast, y, z, contrast + saturation);
}

fn crts_mask(pos: vec2<f32>, dark: f32) -> vec3<f32> {
    if MASK_TYPE == 2.0 {
        var m = vec3<f32>(dark, dark, dark);
        let x = fract(pos.x * (1.0 / 3.0));
        if x < (1.0 / 3.0) { m.r = 1.0; }
        else if x < (2.0 / 3.0) { m.g = 1.0; }
        else { m.b = 1.0; }
        return m;
    }
    if MASK_TYPE == 1.0 {
        var m = vec3<f32>(1.0, 1.0, 1.0);
        let x = fract(pos.x * (1.0 / 3.0));
        if x < (1.0 / 3.0) { m.r = dark; }
        else if x < (2.0 / 3.0) { m.g = dark; }
        else { m.b = dark; }
        return m;
    }
    if MASK_TYPE == 0.0 {
        return vec3<f32>(1.0, 1.0, 1.0);
    }
    // Shadow mask (type 3)
    var spos = pos;
    spos.x = spos.x + spos.y * 2.9999;
    var m = vec3<f32>(dark, dark, dark);
    let x = fract(spos.x * (1.0 / 6.0));
    if x < (1.0 / 3.0) { m.r = 1.0; }
    else if x < (2.0 / 3.0) { m.g = 1.0; }
    else { m.b = 1.0; }
    return m;
}

fn crts_filter(
    ipos: vec2<f32>,
    input_size_div_output_size: vec2<f32>,
    half_input_size: vec2<f32>,
    rcp_input_size: vec2<f32>,
    rcp_output_size: vec2<f32>,
    two_div_output_size: vec2<f32>,
    input_height: f32,
    warp: vec2<f32>,
    thin: f32,
    blur: f32,
    mask_val: f32,
    tone: vec4<f32>,
) -> vec3<f32> {
    // Apply warp
    var pos = ipos * two_div_output_size - vec2<f32>(1.0, 1.0);
    pos *= vec2<f32>(
        1.0 + (pos.y * pos.y) * warp.x,
        1.0 + (pos.x * pos.x) * warp.y,
    );
    let vin = saturate(
        -(1.0 - (1.0 - saturate(pos.x * pos.x)) * (1.0 - saturate(pos.y * pos.y)))
        * (0.998 + 0.001 * CORNER)
        * input_height + input_height
    );
    pos = pos * half_input_size + half_input_size;

    // Snap to center of first scanline
    let y0 = floor(pos.y - 0.5) + 0.5;

    // 8-tap filter: snap to center of one of four pixels
    let x0 = floor(pos.x - 1.5) + 0.5;
    var p = vec2<f32>(x0 * rcp_input_size.x, y0 * rcp_input_size.y);

    // Fetch 4 nearest texels from 2 nearest scanlines
    let col_a0 = crts_fetch(p);
    p.x += rcp_input_size.x;
    let col_a1 = crts_fetch(p);
    p.x += rcp_input_size.x;
    let col_a2 = crts_fetch(p);
    p.x += rcp_input_size.x;
    let col_a3 = crts_fetch(p);
    p.y += rcp_input_size.y;
    let col_b3 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    let col_b2 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    let col_b1 = crts_fetch(p);
    p.x -= rcp_input_size.x;
    let col_b0 = crts_fetch(p);

    // Vertical filter using sine wave scanlines
    let off = pos.y - y0;
    let pi2: f32 = 6.28318530717958;
    let hlf: f32 = 0.5;
    let scan_a = cos(min(0.5, off * thin) * pi2) * hlf + hlf;
    let scan_b = cos(min(0.5, (-off) * thin + thin) * pi2) * hlf + hlf;

    // Horizontal gaussian filter
    let off0 = pos.x - x0;
    let off1 = off0 - 1.0;
    let off2 = off0 - 2.0;
    let off3 = off0 - 3.0;
    let pix0 = exp2(blur * off0 * off0);
    let pix1 = exp2(blur * off1 * off1);
    let pix2 = exp2(blur * off2 * off2);
    let pix3 = exp2(blur * off3 * off3);
    var pix_t = 1.0 / (pix0 + pix1 + pix2 + pix3);
    pix_t *= vin;

    let sa = scan_a * pix_t;
    let sb = scan_b * pix_t;

    var color = (col_a0 * pix0 + col_a1 * pix1 + col_a2 * pix2 + col_a3 * pix3) * sa
              + (col_b0 * pix0 + col_b1 * pix1 + col_b2 * pix2 + col_b3 * pix3) * sb;

    // Apply phosphor mask
    color *= crts_mask(ipos, mask_val);

    // Tonal curve (auto-exposure compensation)
    let peak = max(1.0 / (256.0 * 65536.0), crts_max3(color.r, color.g, color.b));
    let ratio = color * (1.0 / peak);
    let adjusted_peak = peak * (1.0 / (peak * tone.y + tone.z));
    return ratio * adjusted_peak;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if uniforms.enabled < 0.5 {
        return textureSample(tex, tex_sampler, in.uv);
    }

    let input_size = uniforms.input_size;
    let output_size = uniforms.output_size;
    let ipos = in.uv * output_size * (uniforms.texture_size / input_size);

    var warp_factor: vec2<f32>;
    warp_factor.x = CURVATURE;
    warp_factor.y = (3.0 / 4.0) * warp_factor.x; // 4:3 aspect

    let tone = crts_tone(1.0, 0.0, INPUT_THIN, INPUT_MASK);

    let color = crts_filter(
        ipos,
        input_size / output_size,
        input_size * vec2<f32>(0.5, 0.5),
        1.0 / input_size,
        1.0 / output_size,
        2.0 / output_size,
        input_size.y,
        warp_factor,
        INPUT_THIN,
        INPUT_BLUR,
        INPUT_MASK,
        tone,
    );

    return vec4<f32>(to_srgb(color), 1.0);
}
