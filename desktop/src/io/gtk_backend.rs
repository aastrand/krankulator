use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gdk::keys::constants as gdk_key;
use gdk::prelude::*;
use glow::HasContext;
use gtk::prelude::*;
use muda::{Menu, MenuEvent};

use krankulator_core::emu::apu;
use krankulator_core::emu::debug::DebugSnapshot;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::{IOHandler, PollResult};
use krankulator_core::emu::memory;

use crate::debug::{DebugUi, PANEL_WIDTH};

use super::{
    add_recent_rom, apply_gamepad, build_menu_contents, display_width, open_rom_dialog, KbState,
    populate_recent_submenu, window_size_for_scale, MenuIds, MenuItems, TurboState, NES_TEX_HEIGHT,
    NTSC_FRAME_DURATION,
};
use crate::bindings::ui::{BindingUi, UiEvent};
use crate::bindings::{Action, InputBindings, KeyId};
use crate::gamepad::Gamepads;
use crate::settings;
use crate::settings::Settings;

extern "C" {
    fn eglGetProcAddress(name: *const std::ffi::c_char) -> *const std::ffi::c_void;
}

fn screensaver_inhibit() -> Option<u32> {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.ScreenSaver",
            "--object-path",
            "/org/freedesktop/ScreenSaver",
            "--method",
            "org.freedesktop.ScreenSaver.Inhibit",
            "krankulator",
            "NES emulation in progress",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .strip_prefix("(uint32 ")?
        .strip_suffix(",)")?
        .parse()
        .ok()
}

fn screensaver_uninhibit(cookie: u32) {
    let _ = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.ScreenSaver",
            "--object-path",
            "/org/freedesktop/ScreenSaver",
            "--method",
            "org.freedesktop.ScreenSaver.UnInhibit",
            &cookie.to_string(),
        ])
        .output();
}

const NES_WIDTH: i32 = 256;
const NES_HEIGHT: i32 = 240;

const VERT_SRC: &str = "#version 330 core
out vec2 v_uv;
void main() {
    float x = float((gl_VertexID & 1) * 2 - 1);
    float y = float((gl_VertexID >> 1) * 2 - 1);
    gl_Position = vec4(x, y, 0.0, 1.0);
    v_uv = vec2((x + 1.0) * 0.5, (1.0 - y) * 0.5);
}
";

const FRAG_SRC: &str = include_str!("../../../core/src/emu/gfx/shaders/crt_lottes_web.frag");

fn desktop_frag_src() -> String {
    FRAG_SRC.replace("#version 300 es\n", "#version 330 core\n")
}

struct GlState {
    program: glow::Program,
    vao: glow::VertexArray,
    texture: glow::Texture,
    u_output_size: glow::UniformLocation,
    u_texture_size: glow::UniformLocation,
    u_input_size: glow::UniformLocation,
    u_enabled: glow::UniformLocation,
}

struct GlContext {
    gl: Arc<glow::Context>,
    state: GlState,
    texture_initialized: bool,
}

pub struct GtkPixelsIOHandler {
    window: gtk::Window,
    gl_area: gtk::GLArea,
    #[allow(dead_code)]
    gl_ctx: Rc<RefCell<Option<GlContext>>>,
    rgba_buf: Rc<RefCell<Vec<u8>>>,
    gamepads: Gamepads,
    muted: Rc<Cell<bool>>,
    last_frame_time: Instant,
    last_frame_ms: f64,
    frame_duration: Duration,
    kb_state: Rc<Cell<u8>>,
    p2_kb_state: Rc<Cell<u8>>,
    turbo_kb_state: Rc<Cell<u8>>,
    p2_turbo_kb_state: Rc<Cell<u8>>,
    turbo: TurboState,
    fast_forward: Rc<Cell<bool>>,
    pixel_perfect: Rc<Cell<bool>>,
    scanlines: Rc<Cell<bool>>,
    exit_flag: Rc<Cell<bool>>,
    save_state_flag: Rc<Cell<bool>>,
    load_state_flag: Rc<Cell<bool>>,
    cycle_slot_flag: Rc<Cell<bool>>,
    reset_flag: Rc<Cell<bool>>,
    toggle_overlay_flag: Rc<Cell<bool>>,
    rewind_flag: Rc<Cell<bool>>,
    fullscreen_flag: Rc<Cell<bool>>,
    overscan: Rc<Cell<bool>>,
    correct_aspect_ratio: Rc<Cell<bool>>,
    window_scale: Cell<u32>,
    scale_up_flag: Rc<Cell<bool>>,
    scale_down_flag: Rc<Cell<bool>>,
    overscan_changed: Cell<bool>,
    menu: Menu,
    menu_ids: MenuIds,
    menu_items: MenuItems,
    screensaver_cookie: Option<u32>,
    bindings: Rc<RefCell<InputBindings>>,
    binding_ui: BindingUi,
    ui_buf: gfx::buf::Buffer,
    binding_ui_active: Rc<Cell<bool>>,
    captured_keys: Rc<RefCell<Vec<KeyId>>>,
    toggle_debug_flag: Rc<Cell<bool>>,
    toggle_pause_flag: Rc<Cell<bool>>,
    debug_active: Rc<Cell<bool>>,
    debug_snapshot: Rc<RefCell<Option<DebugSnapshot>>>,
    frame_clock_mode: bool,
    frame_time_cell: Rc<Cell<f64>>,
    combined_ff: Rc<Cell<bool>>,
}

impl GtkPixelsIOHandler {
    pub fn new(_width: u32, _height: u32, rom_name: &str, settings: &mut Settings) -> Self {
        gtk::init().expect("Failed to initialize GTK");

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        let (win_w, win_h) =
            window_size_for_scale(settings.window_scale, settings.correct_aspect_ratio);
        window.set_title(&format!("krankulator — {}", rom_name));
        window.set_default_size(win_w as i32, win_h as i32);

        if let Some(icon) = load_gtk_icon() {
            window.set_icon(Some(&icon));
        }

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&vbox);

        let (menu, menu_ids, menu_items) = build_menu_contents();
        menu.init_for_gtk_window(&window, Some(&vbox)).unwrap();

        let gl_area = gtk::GLArea::new();
        gl_area.set_can_focus(true);
        gl_area.set_has_depth_buffer(false);
        gl_area.set_has_stencil_buffer(false);
        gl_area.set_required_version(3, 3);
        gl_area.set_auto_render(false);
        vbox.pack_end(&gl_area, true, true, 0);

        let rgba_buf = Rc::new(RefCell::new(vec![
            0u8;
            (NES_WIDTH * NES_HEIGHT * 4) as usize
        ]));
        let gl_ctx: Rc<RefCell<Option<GlContext>>> = Rc::new(RefCell::new(None));
        let pixel_perfect = Rc::new(Cell::new(settings.integer_scaling));
        let scanlines = Rc::new(Cell::new(settings.scanlines));
        let correct_aspect_ratio = Rc::new(Cell::new(settings.correct_aspect_ratio));
        let debug_active: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let debug_snapshot: Rc<RefCell<Option<DebugSnapshot>>> = Rc::new(RefCell::new(None));

        {
            let ctx = gl_ctx.clone();
            gl_area.connect_realize(move |area| {
                area.make_current();
                if area.error().is_some() {
                    return;
                }
                let gl = Arc::new(unsafe {
                    glow::Context::from_loader_function_cstr(|name: &CStr| {
                        eglGetProcAddress(name.as_ptr())
                    })
                });
                let state = unsafe { init_gl(&gl) };
                *ctx.borrow_mut() = Some(GlContext {
                    gl,
                    state,
                    texture_initialized: false,
                });
            });
        }

        {
            let buf = rgba_buf.clone();
            let ctx = gl_ctx.clone();
            let pp = pixel_perfect.clone();
            let sl = scanlines.clone();
            let car = correct_aspect_ratio.clone();
            let da = debug_active.clone();
            let dsnap = debug_snapshot.clone();
            let dui: RefCell<Option<DebugUi>> = RefCell::new(None);
            gl_area.connect_render(move |area, _gl_ctx| {
                let mut ctx_ref = ctx.borrow_mut();
                let Some(gl_ctx) = ctx_ref.as_mut() else {
                    return glib::Propagation::Stop;
                };
                let alloc = area.allocation();
                render_gl(
                    gl_ctx,
                    &buf.borrow(),
                    alloc.width() as u32,
                    alloc.height() as u32,
                    pp.get(),
                    sl.get(),
                    car.get(),
                );
                if da.get() {
                    let mut dui_ref = dui.borrow_mut();
                    let debug_ui = dui_ref.get_or_insert_with(|| DebugUi::new(&gl_ctx.gl));
                    let dsnap_ref = dsnap.borrow();
                    if let Some(snapshot) = dsnap_ref.as_ref() {
                        debug_ui.render(
                            &gl_ctx.gl,
                            snapshot,
                            alloc.width() as u32,
                            alloc.height() as u32,
                        );
                    }
                } else {
                    let mut dui_ref = dui.borrow_mut();
                    if let Some(mut old) = dui_ref.take() {
                        old.destroy();
                    }
                }
                glib::Propagation::Stop
            });
        }

        window.show_all();
        gl_area.grab_focus();

        while gtk::events_pending() {
            gtk::main_iteration();
        }

        let exit_flag = Rc::new(Cell::new(false));
        let kb_state = Rc::new(Cell::new(0u8));
        let p2_kb_state = Rc::new(Cell::new(0u8));
        let turbo_kb_state = Rc::new(Cell::new(0u8));
        let p2_turbo_kb_state = Rc::new(Cell::new(0u8));
        let fast_forward = Rc::new(Cell::new(false));
        let muted = Rc::new(Cell::new(false));
        let save_state_flag = Rc::new(Cell::new(false));
        let load_state_flag = Rc::new(Cell::new(false));
        let cycle_slot_flag = Rc::new(Cell::new(false));
        let reset_flag = Rc::new(Cell::new(false));
        let toggle_overlay_flag = Rc::new(Cell::new(false));
        let rewind_flag = Rc::new(Cell::new(false));
        let fullscreen_flag = Rc::new(Cell::new(false));
        let overscan = Rc::new(Cell::new(settings.overscan));
        let scale_up_flag = Rc::new(Cell::new(false));
        let scale_down_flag = Rc::new(Cell::new(false));
        let bindings = Rc::new(RefCell::new(std::mem::take(&mut settings.bindings)));
        let binding_ui_active = Rc::new(Cell::new(false));
        let captured_keys: Rc<RefCell<Vec<KeyId>>> = Rc::new(RefCell::new(Vec::new()));
        let toggle_debug_flag = Rc::new(Cell::new(false));
        let toggle_pause_flag = Rc::new(Cell::new(false));

        {
            let flag = exit_flag.clone();
            window.connect_delete_event(move |_, _| {
                flag.set(true);
                glib::Propagation::Stop
            });
        }

        {
            let kb = kb_state.clone();
            let p2kb = p2_kb_state.clone();
            let tkb = turbo_kb_state.clone();
            let p2tkb = p2_turbo_kb_state.clone();
            let ff = fast_forward.clone();
            let mt = muted.clone();
            let save = save_state_flag.clone();
            let load = load_state_flag.clone();
            let cycle = cycle_slot_flag.clone();
            let reset = reset_flag.clone();
            let overlay = toggle_overlay_flag.clone();
            let rw = rewind_flag.clone();
            let fs = fullscreen_flag.clone();
            let ex = exit_flag.clone();
            let pp = pixel_perfect.clone();
            let sl = scanlines.clone();
            let car = correct_aspect_ratio.clone();
            let su = scale_up_flag.clone();
            let sd = scale_down_flag.clone();
            let bi = bindings.clone();
            let bua = binding_ui_active.clone();
            let ck = captured_keys.clone();
            let td = toggle_debug_flag.clone();
            let tp = toggle_pause_flag.clone();
            window.connect_key_press_event(move |_, event| {
                if bua.get() {
                    if let Some(key_id) = KeyId::from_gdk(event.keyval()) {
                        ck.borrow_mut().push(key_id);
                    }
                    return glib::Propagation::Proceed;
                }
                if event.keyval() == gdk_key::F10 {
                    bua.set(true);
                    return glib::Propagation::Proceed;
                }
                handle_key(
                    event, true, &kb, &p2kb, &tkb, &p2tkb, &ff, &mt, &save, &load, &cycle, &reset,
                    &overlay, &rw, &fs, &ex, &pp, &sl, &car, &su, &sd, &bi, &td, &tp,
                );
                glib::Propagation::Proceed
            });
        }

        {
            let kb = kb_state.clone();
            let p2kb = p2_kb_state.clone();
            let tkb = turbo_kb_state.clone();
            let p2tkb = p2_turbo_kb_state.clone();
            let ff = fast_forward.clone();
            let mt = muted.clone();
            let save = save_state_flag.clone();
            let load = load_state_flag.clone();
            let cycle = cycle_slot_flag.clone();
            let reset = reset_flag.clone();
            let overlay = toggle_overlay_flag.clone();
            let rw = rewind_flag.clone();
            let fs = fullscreen_flag.clone();
            let ex = exit_flag.clone();
            let pp = pixel_perfect.clone();
            let sl = scanlines.clone();
            let car = correct_aspect_ratio.clone();
            let su = scale_up_flag.clone();
            let sd = scale_down_flag.clone();
            let bi = bindings.clone();
            let bua = binding_ui_active.clone();
            let td = toggle_debug_flag.clone();
            let tp = toggle_pause_flag.clone();
            window.connect_key_release_event(move |_, event| {
                if bua.get() {
                    return glib::Propagation::Proceed;
                }
                handle_key(
                    event, false, &kb, &p2kb, &tkb, &p2tkb, &ff, &mt, &save, &load, &cycle, &reset,
                    &overlay, &rw, &fs, &ex, &pp, &sl, &car, &su, &sd, &bi, &td, &tp,
                );
                glib::Propagation::Proceed
            });
        }

        menu_items.overscan.set_checked(settings.overscan);
        menu_items.scaling.set_checked(settings.integer_scaling);
        menu_items.scanlines.set_checked(settings.scanlines);
        menu_items
            .correct_aspect_ratio
            .set_checked(settings.correct_aspect_ratio);

        Self {
            window,
            gl_area,
            gl_ctx,
            rgba_buf,
            gamepads: Gamepads::new(),
            muted,
            last_frame_time: Instant::now(),
            last_frame_ms: 0.0,
            frame_duration: NTSC_FRAME_DURATION,
            kb_state,
            p2_kb_state,
            turbo_kb_state,
            p2_turbo_kb_state,
            turbo: TurboState::new(),
            fast_forward,
            pixel_perfect,
            scanlines,
            exit_flag,
            save_state_flag,
            load_state_flag,
            cycle_slot_flag,
            reset_flag,
            toggle_overlay_flag,
            rewind_flag,
            fullscreen_flag,
            overscan,
            correct_aspect_ratio,
            window_scale: Cell::new(settings.window_scale),
            scale_up_flag,
            scale_down_flag,
            overscan_changed: Cell::new(false),
            menu,
            menu_ids,
            menu_items,
            screensaver_cookie: screensaver_inhibit(),
            bindings,
            binding_ui: BindingUi::new(),
            ui_buf: gfx::buf::Buffer::new(),
            binding_ui_active,
            captured_keys,
            toggle_debug_flag,
            toggle_pause_flag,
            debug_active,
            debug_snapshot,
            frame_clock_mode: false,
            frame_time_cell: Rc::new(Cell::new(0.0)),
            combined_ff: Rc::new(Cell::new(false)),
        }
    }

    pub fn gl_area(&self) -> &gtk::GLArea {
        &self.gl_area
    }

    pub fn set_frame_clock_mode(&mut self, enabled: bool) {
        self.frame_clock_mode = enabled;
    }

    pub fn fast_forward_flag(&self) -> Rc<Cell<bool>> {
        self.combined_ff.clone()
    }

    pub fn frame_time_cell(&self) -> Rc<Cell<f64>> {
        self.frame_time_cell.clone()
    }

    fn save_display_settings(&self) {
        settings::save_settings(&Settings {
            integer_scaling: self.pixel_perfect.get(),
            scanlines: self.scanlines.get(),
            overscan: self.overscan.get(),
            correct_aspect_ratio: self.correct_aspect_ratio.get(),
            window_scale: self.window_scale.get(),
            bindings: self.bindings.borrow().clone(),
        });
    }

    fn toggle_fullscreen(&self, toasts: &mut Vec<String>) {
        let is_fullscreen = self
            .window
            .window()
            .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
            .unwrap_or(false);
        if is_fullscreen {
            self.window.unfullscreen();
            let _ = self.menu.show_for_gtk_window(&self.window);
            self.menu_items.fullscreen.set_checked(false);
            toasts.push("Windowed".into());
        } else {
            let _ = self.menu.hide_for_gtk_window(&self.window);
            self.window.fullscreen();
            self.menu_items.fullscreen.set_checked(true);
            toasts.push("Fullscreen".into());
        }
    }

    fn refresh_recent_menu(&mut self) {
        let submenu = &self.menu_items.recent_submenu;
        while submenu.remove_at(0).is_some() {}
        self.menu_items.recent_items = populate_recent_submenu(submenu);
    }
}

fn load_gtk_icon() -> Option<gdk::gdk_pixbuf::Pixbuf> {
    let loader = gdk::gdk_pixbuf::PixbufLoader::new();
    loader.write(super::ICON_PNG).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

unsafe fn init_gl(gl: &glow::Context) -> GlState {
    let frag_src = desktop_frag_src();
    let program = gl.create_program().expect("create program");

    let vert = gl.create_shader(glow::VERTEX_SHADER).expect("create vert");
    gl.shader_source(vert, VERT_SRC);
    gl.compile_shader(vert);
    if !gl.get_shader_compile_status(vert) {
        panic!("Vertex shader: {}", gl.get_shader_info_log(vert));
    }

    let frag = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .expect("create frag");
    gl.shader_source(frag, &frag_src);
    gl.compile_shader(frag);
    if !gl.get_shader_compile_status(frag) {
        panic!("Fragment shader: {}", gl.get_shader_info_log(frag));
    }

    gl.attach_shader(program, vert);
    gl.attach_shader(program, frag);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("Program link: {}", gl.get_program_info_log(program));
    }
    gl.delete_shader(vert);
    gl.delete_shader(frag);

    gl.use_program(Some(program));

    let vao = gl.create_vertex_array().expect("create vao");
    gl.bind_vertex_array(Some(vao));

    let texture = gl.create_texture().expect("create texture");
    gl.active_texture(glow::TEXTURE0);
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );

    let u_texture_loc = gl.get_uniform_location(program, "u_texture");
    gl.uniform_1_i32(u_texture_loc.as_ref(), 0);

    let u_output_size = gl.get_uniform_location(program, "u_output_size").unwrap();
    let u_texture_size = gl.get_uniform_location(program, "u_texture_size").unwrap();
    let u_input_size = gl.get_uniform_location(program, "u_input_size").unwrap();
    let u_enabled = gl.get_uniform_location(program, "u_enabled").unwrap();

    GlState {
        program,
        vao,
        texture,
        u_output_size,
        u_texture_size,
        u_input_size,
        u_enabled,
    }
}

fn compute_viewport(
    win_w: f32,
    win_h: f32,
    display_w: f32,
    display_h: f32,
    integer_scaling: bool,
) -> (i32, i32, i32, i32) {
    let scale_x = win_w / display_w;
    let scale_y = win_h / display_h;
    let scale = if integer_scaling {
        scale_x.min(scale_y).floor().max(1.0)
    } else {
        scale_x.min(scale_y)
    };
    let vp_w = display_w * scale;
    let vp_h = display_h * scale;
    let vp_x = (win_w - vp_w) * 0.5;
    let vp_y = (win_h - vp_h) * 0.5;
    (vp_x as i32, vp_y as i32, vp_w as i32, vp_h as i32)
}

fn render_gl(
    ctx: &mut GlContext,
    rgba: &[u8],
    win_w: u32,
    win_h: u32,
    integer_scaling: bool,
    scanlines: bool,
    correct_aspect_ratio: bool,
) {
    let gl = &ctx.gl;
    let state = &ctx.state;
    unsafe {
        gl.use_program(Some(state.program));
        gl.bind_vertex_array(Some(state.vao));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(state.texture));

        let filter = if scanlines {
            glow::LINEAR
        } else {
            glow::NEAREST
        } as i32;
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);

        if ctx.texture_initialized {
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                NES_WIDTH,
                NES_HEIGHT,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
        } else {
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                NES_WIDTH,
                NES_HEIGHT,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
            ctx.texture_initialized = true;
        }

        let disp_w = display_width(correct_aspect_ratio);
        let (vp_x, vp_y, vp_w, vp_h) = compute_viewport(
            win_w as f32,
            win_h as f32,
            disp_w,
            NES_TEX_HEIGHT,
            integer_scaling,
        );

        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.viewport(0, 0, win_w as i32, win_h as i32);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.viewport(vp_x, vp_y, vp_w, vp_h);
        gl.uniform_2_f32(Some(&state.u_output_size), vp_w as f32, vp_h as f32);
        gl.uniform_2_f32(
            Some(&state.u_texture_size),
            NES_WIDTH as f32,
            NES_HEIGHT as f32,
        );
        gl.uniform_2_f32(
            Some(&state.u_input_size),
            NES_WIDTH as f32,
            NES_HEIGHT as f32,
        );
        gl.uniform_1_f32(Some(&state.u_enabled), if scanlines { 1.0 } else { 0.0 });

        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_key(
    event: &gdk::EventKey,
    pressed: bool,
    kb_state: &Rc<Cell<u8>>,
    p2_kb_state: &Rc<Cell<u8>>,
    turbo_kb_state: &Rc<Cell<u8>>,
    p2_turbo_kb_state: &Rc<Cell<u8>>,
    fast_forward: &Rc<Cell<bool>>,
    muted: &Rc<Cell<bool>>,
    save_state: &Rc<Cell<bool>>,
    load_state: &Rc<Cell<bool>>,
    cycle_slot: &Rc<Cell<bool>>,
    reset: &Rc<Cell<bool>>,
    toggle_overlay: &Rc<Cell<bool>>,
    rewind: &Rc<Cell<bool>>,
    fullscreen: &Rc<Cell<bool>>,
    exit: &Rc<Cell<bool>>,
    pixel_perfect: &Rc<Cell<bool>>,
    scanlines: &Rc<Cell<bool>>,
    correct_aspect_ratio: &Rc<Cell<bool>>,
    scale_up: &Rc<Cell<bool>>,
    scale_down: &Rc<Cell<bool>>,
    bindings: &Rc<RefCell<InputBindings>>,
    toggle_debug: &Rc<Cell<bool>>,
    toggle_pause: &Rc<Cell<bool>>,
) {
    let gdk_key = event.keyval();
    let ctrl = event.state().contains(gdk::ModifierType::CONTROL_MASK);

    if pressed && ctrl {
        match gdk_key {
            k if k == gdk_key::f || k == gdk_key::F => fullscreen.set(true),
            k if k == gdk_key::q || k == gdk_key::Q => exit.set(true),
            k if k == gdk_key::plus || k == gdk_key::equal => scale_up.set(true),
            k if k == gdk_key::minus || k == gdk_key::underscore => scale_down.set(true),
            _ => {}
        }
        return;
    }

    let Some(key_id) = KeyId::from_gdk(gdk_key) else {
        return;
    };

    let bindings = bindings.borrow();
    let mut p1_kb = kb_state.get();
    let mut p2_kb = p2_kb_state.get();
    let mut p1_turbo = turbo_kb_state.get();
    let mut p2_turbo = p2_turbo_kb_state.get();

    for action in bindings.keyboard_action(&key_id) {
        if let Some((player, bit)) = action.turbo_controller_bit() {
            let state = if player == 0 {
                &mut p1_turbo
            } else {
                &mut p2_turbo
            };
            if pressed {
                *state |= bit;
            } else {
                *state &= !bit;
            }
            continue;
        }
        match action {
            Action::Fullscreen => {
                if pressed {
                    fullscreen.set(true);
                }
            }
            Action::ToggleScaling => {
                if pressed {
                    let new_val = !pixel_perfect.get();
                    pixel_perfect.set(new_val);
                    if new_val {
                        correct_aspect_ratio.set(false);
                    }
                }
            }
            Action::Mute => {
                if pressed {
                    muted.set(!muted.get());
                }
            }
            Action::Reset => {
                if pressed {
                    reset.set(true);
                }
            }
            Action::SaveState => {
                if pressed {
                    save_state.set(true);
                }
            }
            Action::LoadState => {
                if pressed {
                    load_state.set(true);
                }
            }
            Action::CycleSlot => {
                if pressed {
                    cycle_slot.set(true);
                }
            }
            Action::ToggleOverlay => {
                if pressed {
                    toggle_overlay.set(true);
                }
            }
            Action::ToggleScanlines => {
                if pressed {
                    scanlines.set(!scanlines.get());
                }
            }
            Action::ToggleDebug => {
                if pressed {
                    toggle_debug.set(true);
                }
            }
            Action::Pause => {
                if pressed {
                    toggle_pause.set(true);
                }
            }
            Action::Rewind => {
                rewind.set(pressed);
            }
            Action::FastForward => {
                fast_forward.set(pressed);
            }
            action => {
                if let Some((player, bit)) = action.controller_bit() {
                    let state = if player == 0 { &mut p1_kb } else { &mut p2_kb };
                    if pressed {
                        *state |= bit;
                    } else {
                        *state &= !bit;
                    }
                }
            }
        }
    }

    kb_state.set(p1_kb);
    p2_kb_state.set(p2_kb);
    turbo_kb_state.set(p1_turbo);
    p2_turbo_kb_state.set(p2_turbo);
}

impl IOHandler for GtkPixelsIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        if !self.muted.get() {
            println!("{}", logline);
        }
    }

    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, apu: &mut apu::APU) -> PollResult {
        if !self.frame_clock_mode {
            while gtk::events_pending() {
                gtk::main_iteration();
            }
        }

        let mut recent_rom_path: Option<String> = None;
        let mut exit = self.exit_flag.get();
        let mut open_rom = false;
        let mut save_state = self.save_state_flag.get();
        let mut load_state = self.load_state_flag.get();
        let mut cycle_slot = self.cycle_slot_flag.get();
        let reset = self.reset_flag.get();
        let toggle_overlay = self.toggle_overlay_flag.get();
        let mut toasts: Vec<String> = Vec::new();

        // Sync menu checkmarks with keyboard-toggled state
        if self.menu_items.scanlines.is_checked() != self.scanlines.get() {
            self.menu_items.scanlines.set_checked(self.scanlines.get());
            if self.scanlines.get() {
                toasts.push("CRT scanlines ON".into());
            } else {
                toasts.push("CRT scanlines OFF".into());
            }
        }
        if self.scale_up_flag.get() {
            self.scale_up_flag.set(false);
            let s = self.window_scale.get();
            if s < 6 {
                self.window_scale.set(s + 1);
                let is_fullscreen = self
                    .window
                    .window()
                    .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                    .unwrap_or(false);
                if !is_fullscreen {
                    let (w, h) = window_size_for_scale(s + 1, self.correct_aspect_ratio.get());
                    self.window.resize(w as i32, h as i32);
                }
                toasts.push(format!("{}x scale", s + 1));
                self.save_display_settings();
            }
        }
        if self.scale_down_flag.get() {
            self.scale_down_flag.set(false);
            let s = self.window_scale.get();
            if s > 1 {
                self.window_scale.set(s - 1);
                let is_fullscreen = self
                    .window
                    .window()
                    .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                    .unwrap_or(false);
                if !is_fullscreen {
                    let (w, h) = window_size_for_scale(s - 1, self.correct_aspect_ratio.get());
                    self.window.resize(w as i32, h as i32);
                }
                toasts.push(format!("{}x scale", s - 1));
                self.save_display_settings();
            }
        }

        if self.menu_items.scaling.is_checked() != self.pixel_perfect.get() {
            self.menu_items
                .scaling
                .set_checked(self.pixel_perfect.get());
            if self.pixel_perfect.get() {
                toasts.push("Integer scaling".into());
            } else {
                toasts.push("Fill scaling".into());
            }
        }

        if self.fullscreen_flag.get() {
            self.fullscreen_flag.set(false);
            self.toggle_fullscreen(&mut toasts);
        }

        self.save_state_flag.set(false);
        self.load_state_flag.set(false);
        self.cycle_slot_flag.set(false);
        self.reset_flag.set(false);
        self.toggle_overlay_flag.set(false);

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id();
            if *id == self.menu_ids.open_rom {
                open_rom = true;
            } else if *id == self.menu_ids.quit {
                exit = true;
            } else if *id == self.menu_ids.reset {
            } else if *id == self.menu_ids.save_state {
                save_state = true;
            } else if *id == self.menu_ids.load_state {
                load_state = true;
            } else if *id == self.menu_ids.cycle_slot {
                cycle_slot = true;
            } else if *id == self.menu_ids.input_settings {
                self.binding_ui_active.set(true);
            } else if *id == self.menu_ids.debug_view {
                self.toggle_debug_flag.set(true);
            } else if *id == self.menu_ids.pause {
                self.toggle_pause_flag.set(true);
            } else if *id == self.menu_ids.fullscreen {
                self.toggle_fullscreen(&mut toasts);
            } else if *id == self.menu_ids.scaling {
                self.pixel_perfect.set(!self.pixel_perfect.get());
                self.menu_items
                    .scaling
                    .set_checked(self.pixel_perfect.get());
                if self.pixel_perfect.get() {
                    self.correct_aspect_ratio.set(false);
                    self.menu_items.correct_aspect_ratio.set_checked(false);
                    toasts.push("Integer scaling".into());
                } else {
                    toasts.push("Fill scaling".into());
                }
                self.save_display_settings();
            } else if *id == self.menu_ids.scanlines {
                self.scanlines.set(!self.scanlines.get());
                self.menu_items.scanlines.set_checked(self.scanlines.get());
                if self.scanlines.get() {
                    toasts.push("CRT scanlines ON".into());
                } else {
                    toasts.push("CRT scanlines OFF".into());
                }
                self.save_display_settings();
            } else if *id == self.menu_ids.overscan {
                let val = !self.overscan.get();
                self.overscan.set(val);
                self.overscan_changed.set(true);
                self.menu_items.overscan.set_checked(val);
                if val {
                    toasts.push("Overscan hidden".into());
                } else {
                    toasts.push("Overscan visible".into());
                }
                self.save_display_settings();
            } else if *id == self.menu_ids.correct_aspect_ratio {
                let val = !self.correct_aspect_ratio.get();
                self.correct_aspect_ratio.set(val);
                self.menu_items.correct_aspect_ratio.set_checked(val);
                if val {
                    self.pixel_perfect.set(false);
                    self.menu_items.scaling.set_checked(false);
                    toasts.push("8:7 aspect ratio".into());
                } else {
                    toasts.push("Square pixels".into());
                }
                let is_fullscreen = self
                    .window
                    .window()
                    .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                    .unwrap_or(false);
                if !is_fullscreen {
                    let (w, h) = window_size_for_scale(self.window_scale.get(), val);
                    self.window.resize(w as i32, h as i32);
                }
                self.save_display_settings();
            } else if *id == self.menu_ids.scale_up {
                let s = self.window_scale.get();
                if s < 6 {
                    self.window_scale.set(s + 1);
                    let is_fullscreen = self
                        .window
                        .window()
                        .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                        .unwrap_or(false);
                    if !is_fullscreen {
                        let (w, h) = window_size_for_scale(s + 1, self.correct_aspect_ratio.get());
                        self.window.resize(w as i32, h as i32);
                    }
                    toasts.push(format!("{}x scale", s + 1));
                    self.save_display_settings();
                }
            } else if *id == self.menu_ids.scale_down {
                let s = self.window_scale.get();
                if s > 1 {
                    self.window_scale.set(s - 1);
                    let is_fullscreen = self
                        .window
                        .window()
                        .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                        .unwrap_or(false);
                    if !is_fullscreen {
                        let (w, h) = window_size_for_scale(s - 1, self.correct_aspect_ratio.get());
                        self.window.resize(w as i32, h as i32);
                    }
                    toasts.push(format!("{}x scale", s - 1));
                    self.save_display_settings();
                }
            } else if let Some(path) = self
                .menu_items
                .recent_items
                .iter()
                .find(|(mid, _)| mid == id)
                .map(|(_, p)| p.clone())
            {
                recent_rom_path = Some(path);
            }
        }

        let _ = apu;

        let toggle_debug = self.toggle_debug_flag.get();
        let toggle_pause = self.toggle_pause_flag.get();
        self.toggle_debug_flag.set(false);
        self.toggle_pause_flag.set(false);

        if toggle_debug {
            let now_active = !self.debug_active.get();
            self.debug_active.set(now_active);
            if now_active {
                self.menu_items.debug_view.set_checked(true);
                let is_fullscreen = self
                    .window
                    .window()
                    .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                    .unwrap_or(false);
                if !is_fullscreen {
                    let (w, h) = window_size_for_scale(
                        self.window_scale.get(),
                        self.correct_aspect_ratio.get(),
                    );
                    let panel_extra = (PANEL_WIDTH * 2.0) as i32;
                    self.window.resize(w as i32 + panel_extra, h as i32);
                }
            } else {
                *self.debug_snapshot.borrow_mut() = None;
                self.menu_items.debug_view.set_checked(false);
                let is_fullscreen = self
                    .window
                    .window()
                    .map(|gw| gw.state().contains(gdk::WindowState::FULLSCREEN))
                    .unwrap_or(false);
                if !is_fullscreen {
                    let (w, h) = window_size_for_scale(
                        self.window_scale.get(),
                        self.correct_aspect_ratio.get(),
                    );
                    self.window.resize(w as i32, h as i32);
                }
            }
        }

        if toggle_pause {
            let is_checked = self.menu_items.pause.is_checked();
            self.menu_items.pause.set_checked(!is_checked);
        }

        let open_rom_path = if open_rom {
            open_rom_dialog()
        } else {
            recent_rom_path.take()
        };

        let mut result = PollResult {
            exit,
            save_state,
            load_state,
            cycle_slot,
            reset,
            toggle_overlay,
            rewind: self.rewind_flag.get(),
            fast_forward: self.fast_forward.get(),
            toasts,
            open_rom: open_rom_path,
            set_overscan: if self.overscan_changed.get() {
                self.overscan_changed.set(false);
                Some(self.overscan.get())
            } else {
                None
            },
            toggle_debug,
            toggle_pause,
        };

        if self.binding_ui_active.get() && !self.binding_ui.is_active() {
            self.binding_ui.open();
            self.binding_ui_active.set(true);
        }

        let keys: Vec<KeyId> = self.captured_keys.borrow_mut().drain(..).collect();
        let mut bindings_changed = false;
        for key_id in &keys {
            match self
                .binding_ui
                .handle_key(key_id, &mut self.bindings.borrow_mut())
            {
                UiEvent::Close => {
                    self.binding_ui_active.set(false);
                }
                UiEvent::BindingsChanged => {
                    bindings_changed = true;
                }
                UiEvent::None => {}
            }
        }
        if bindings_changed {
            self.save_display_settings();
        }

        if let Some(ref path) = result.open_rom {
            add_recent_rom(path);
            self.refresh_recent_menu();
        }

        if self.binding_ui.is_active() {
            for btn in self.gamepads.poll_raw_buttons() {
                match self
                    .binding_ui
                    .handle_gamepad_button(btn, &mut self.bindings.borrow_mut())
                {
                    UiEvent::BindingsChanged => {
                        self.save_display_settings();
                    }
                    UiEvent::Close | UiEvent::None => {}
                }
            }
            mem.controllers()[0].load_status(0);
            mem.controllers()[1].load_status(0);
        } else {
            apply_gamepad(
                &mut self.gamepads,
                &self.bindings.borrow(),
                KbState {
                    p1: self.kb_state.get(),
                    p2: self.p2_kb_state.get(),
                    p1_turbo: self.turbo_kb_state.get(),
                    p2_turbo: self.p2_turbo_kb_state.get(),
                },
                &mut self.turbo,
                mem,
                &mut result,
            );
        }

        self.combined_ff.set(result.fast_forward);
        result
    }

    fn frame_time_ms(&self) -> Option<f64> {
        if self.frame_clock_mode {
            Some(self.frame_time_cell.get())
        } else {
            Some(self.last_frame_ms)
        }
    }

    fn set_frame_duration_nanos(&mut self, nanos: u64) {
        self.frame_duration = Duration::from_nanos(nanos);
    }

    fn set_overscan_available(&mut self, available: bool) {
        self.menu_items.overscan.set_enabled(available);
        if !available {
            self.overscan.set(false);
        }
    }

    fn set_debug_snapshot(&mut self, snapshot: DebugSnapshot) {
        *self.debug_snapshot.borrow_mut() = Some(snapshot);
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let elapsed = self.last_frame_time.elapsed();
        self.last_frame_ms = elapsed.as_secs_f64() * 1000.0;

        if !self.frame_clock_mode {
            // GTK-aware frame pacing: process events while waiting instead of
            // blocking with thread::sleep, so the Wayland compositor can deliver
            // frame callbacks and actually present every frame.
            if !self.fast_forward.get() && elapsed < self.frame_duration {
                let remaining = self.frame_duration - elapsed;
                if remaining > Duration::from_millis(2) {
                    let coarse_end = self.frame_duration - Duration::from_millis(2);
                    while self.last_frame_time.elapsed() < coarse_end {
                        if gtk::events_pending() {
                            gtk::main_iteration();
                        } else {
                            std::thread::sleep(Duration::from_micros(500));
                        }
                    }
                }
                while self.last_frame_time.elapsed() < self.frame_duration {
                    if gtk::events_pending() {
                        gtk::main_iteration();
                    } else {
                        std::hint::spin_loop();
                    }
                }
            }
        }
        self.last_frame_time = Instant::now();

        let render_buf = if self.binding_ui.is_active() {
            self.ui_buf.data.copy_from_slice(&buf.data);
            self.binding_ui
                .draw(&mut self.ui_buf, &self.bindings.borrow());
            &self.ui_buf
        } else {
            buf
        };

        {
            let mut rgba = self.rgba_buf.borrow_mut();
            let pixel_count = render_buf.data.len() / 3;
            for i in 0..pixel_count {
                let src = i * 3;
                let dst = i * 4;
                if dst + 3 < rgba.len() {
                    rgba[dst] = render_buf.data[src];
                    rgba[dst + 1] = render_buf.data[src + 1];
                    rgba[dst + 2] = render_buf.data[src + 2];
                    rgba[dst + 3] = 255;
                }
            }
        }

        self.gl_area.queue_render();
    }

    fn exit(&self, s: String) {
        self.save_display_settings();
        if let Some(cookie) = self.screensaver_cookie {
            screensaver_uninhibit(cookie);
        }
        self.log(s);
    }
}
