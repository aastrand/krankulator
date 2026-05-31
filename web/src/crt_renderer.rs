use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlProgram, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

use super::document;

const VERT_SRC: &str = include_str!("../../core/src/emu/gfx/shaders/crt_lottes_web.vert");
const FRAG_SRC: &str = include_str!("../../core/src/emu/gfx/shaders/crt_lottes_web.frag");

struct CrtCanvas {
    gl: GL,
    canvas: HtmlCanvasElement,
    program: WebGlProgram,
    texture: WebGlTexture,
    vao: WebGlVertexArrayObject,
    u_output_size: WebGlUniformLocation,
    u_texture_size: WebGlUniformLocation,
    u_input_size: WebGlUniformLocation,
    u_enabled: WebGlUniformLocation,
    canvas_width: u32,
    canvas_height: u32,
    texture_initialized: bool,
}

pub struct CrtRenderer {
    targets: Vec<CrtCanvas>,
    pub enabled: bool,
}

impl CrtCanvas {
    fn new(canvas_id: &str) -> Result<Self, String> {
        let (gl, canvas, canvas_width, canvas_height) = get_webgl2_context(canvas_id)?;

        let program = create_program(&gl, VERT_SRC, FRAG_SRC)?;
        gl.use_program(Some(&program));

        let texture = gl.create_texture().ok_or("Failed to create texture")?;
        gl.active_texture(GL::TEXTURE0);
        gl.bind_texture(GL::TEXTURE_2D, Some(&texture));
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);

        let vao = gl.create_vertex_array().ok_or("Failed to create VAO")?;
        gl.bind_vertex_array(Some(&vao));

        let u_output_size = gl
            .get_uniform_location(&program, "u_output_size")
            .ok_or("Missing u_output_size")?;
        let u_texture_size = gl
            .get_uniform_location(&program, "u_texture_size")
            .ok_or("Missing u_texture_size")?;
        let u_input_size = gl
            .get_uniform_location(&program, "u_input_size")
            .ok_or("Missing u_input_size")?;
        let u_enabled = gl
            .get_uniform_location(&program, "u_enabled")
            .ok_or("Missing u_enabled")?;

        let u_texture_loc = gl.get_uniform_location(&program, "u_texture");
        gl.uniform1i(u_texture_loc.as_ref(), 0);

        Ok(Self {
            gl,
            canvas,
            program,
            texture,
            vao,
            u_output_size,
            u_texture_size,
            u_input_size,
            u_enabled,
            canvas_width,
            canvas_height,
            texture_initialized: false,
        })
    }

    fn render(&mut self, rgba_buf: &[u8], enabled: bool) {
        let gl = &self.gl;

        let dpr = super::window().device_pixel_ratio();
        let w = (self.canvas.client_width() as f64 * dpr) as u32;
        let h = (self.canvas.client_height() as f64 * dpr) as u32;
        if w == 0 || h == 0 {
            return;
        }
        if w != self.canvas_width || h != self.canvas_height {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
            self.canvas_width = w;
            self.canvas_height = h;
        }

        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.active_texture(GL::TEXTURE0);
        gl.bind_texture(GL::TEXTURE_2D, Some(&self.texture));

        let filter = if enabled { GL::LINEAR } else { GL::NEAREST } as i32;
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, filter);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, filter);

        if self.texture_initialized {
            let _ = gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                GL::TEXTURE_2D,
                0,
                0,
                0,
                256,
                240,
                GL::RGBA,
                GL::UNSIGNED_BYTE,
                Some(rgba_buf),
            );
        } else {
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D,
                0,
                GL::RGBA as i32,
                256,
                240,
                0,
                GL::RGBA,
                GL::UNSIGNED_BYTE,
                Some(rgba_buf),
            )
            .unwrap();
            self.texture_initialized = true;
        }

        gl.viewport(0, 0, self.canvas_width as i32, self.canvas_height as i32);
        gl.uniform2f(
            Some(&self.u_output_size),
            self.canvas_width as f32,
            self.canvas_height as f32,
        );
        gl.uniform2f(Some(&self.u_texture_size), 256.0, 240.0);
        gl.uniform2f(Some(&self.u_input_size), 256.0, 240.0);
        gl.uniform1f(Some(&self.u_enabled), if enabled { 1.0 } else { 0.0 });

        gl.draw_arrays(GL::TRIANGLE_STRIP, 0, 4);
    }
}

impl CrtRenderer {
    pub fn new(canvas_ids: &[&str]) -> Result<Self, String> {
        let mut targets = Vec::new();
        for id in canvas_ids {
            match CrtCanvas::new(id) {
                Ok(t) => targets.push(t),
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("WebGL2 init failed for {id}: {e}").into(),
                    );
                }
            }
        }
        if targets.is_empty() {
            return Err("No WebGL2 context available on any canvas".into());
        }
        Ok(Self {
            targets,
            enabled: false,
        })
    }

    pub fn render(&mut self, rgba_buf: &[u8]) {
        for target in &mut self.targets {
            target.render(rgba_buf, self.enabled);
        }
    }
}

fn get_webgl2_context(canvas_id: &str) -> Result<(GL, HtmlCanvasElement, u32, u32), String> {
    let canvas = document()
        .get_element_by_id(canvas_id)
        .and_then(|el| el.dyn_into::<HtmlCanvasElement>().ok())
        .ok_or_else(|| format!("Canvas '{canvas_id}' not found"))?;
    let gl = canvas
        .get_context("webgl2")
        .map_err(|_| "getContext failed")?
        .ok_or("No WebGL2 support")?
        .dyn_into::<GL>()
        .map_err(|_| "Failed to cast to WebGl2RenderingContext")?;
    let dpr = super::window().device_pixel_ratio();
    let w = (canvas.client_width() as f64 * dpr) as u32;
    let h = (canvas.client_height() as f64 * dpr) as u32;
    canvas.set_width(w);
    canvas.set_height(h);
    Ok((gl, canvas, w, h))
}

fn create_program(gl: &GL, vert_src: &str, frag_src: &str) -> Result<WebGlProgram, String> {
    let vert = compile_shader(gl, GL::VERTEX_SHADER, vert_src)?;
    let frag = compile_shader(gl, GL::FRAGMENT_SHADER, frag_src)?;

    let program = gl.create_program().ok_or("Failed to create program")?;
    gl.attach_shader(&program, &vert);
    gl.attach_shader(&program, &frag);
    gl.link_program(&program);

    if !gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        let log = gl.get_program_info_log(&program).unwrap_or_default();
        return Err(format!("Program link failed: {}", log));
    }
    Ok(program)
}

fn compile_shader(gl: &GL, shader_type: u32, source: &str) -> Result<web_sys::WebGlShader, String> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or("Failed to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if !gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        let log = gl.get_shader_info_log(&shader).unwrap_or_default();
        return Err(format!("Shader compile failed: {}", log));
    }
    Ok(shader)
}
