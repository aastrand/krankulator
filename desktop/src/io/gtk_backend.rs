use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::rc::Rc;
use std::time::Instant;

use gdk::keys::constants as gdk_key;
use gdk::prelude::*;
use glow::HasContext;
use gtk::prelude::*;
use muda::{Menu, MenuEvent};

use krankulator_core::emu::apu;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::controller;
use krankulator_core::emu::io::{IOHandler, PollResult};
use krankulator_core::emu::memory;

use super::{
    add_recent_rom, apply_gamepad, build_menu_contents, frame_pace, open_rom_dialog,
    populate_recent_submenu, MenuIds, MenuItems,
};
use crate::gamepad::Gamepads;
use crate::settings;
use crate::settings::Settings;

extern "C" {
    fn eglGetProcAddress(name: *const std::ffi::c_char) -> *const std::ffi::c_void;
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
    gl: glow::Context,
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
    kb_state: Rc<Cell<u8>>,
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
    overscan_changed: Cell<bool>,
    menu: Menu,
    menu_ids: MenuIds,
    menu_items: MenuItems,
}

impl GtkPixelsIOHandler {
    pub fn new(_width: u32, _height: u32, rom_name: &str, settings: &Settings) -> Self {
        gtk::init().expect("Failed to initialize GTK");

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        let scale = 4;
        window.set_title(&format!("krankulator — {}", rom_name));
        window.set_default_size(NES_WIDTH * scale, NES_HEIGHT * scale);

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

        {
            let ctx = gl_ctx.clone();
            gl_area.connect_realize(move |area| {
                area.make_current();
                if area.error().is_some() {
                    return;
                }
                let gl = unsafe {
                    glow::Context::from_loader_function_cstr(|name: &CStr| {
                        eglGetProcAddress(name.as_ptr())
                    })
                };
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
                );
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

        {
            let flag = exit_flag.clone();
            window.connect_delete_event(move |_, _| {
                flag.set(true);
                glib::Propagation::Stop
            });
        }

        {
            let kb = kb_state.clone();
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
            window.connect_key_press_event(move |_, event| {
                handle_key(
                    event, true, &kb, &ff, &mt, &save, &load, &cycle, &reset, &overlay, &rw, &fs,
                    &ex, &pp, &sl,
                );
                glib::Propagation::Proceed
            });
        }

        {
            let kb = kb_state.clone();
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
            window.connect_key_release_event(move |_, event| {
                handle_key(
                    event, false, &kb, &ff, &mt, &save, &load, &cycle, &reset, &overlay, &rw, &fs,
                    &ex, &pp, &sl,
                );
                glib::Propagation::Proceed
            });
        }

        menu_items.overscan.set_checked(settings.overscan);
        menu_items.scaling.set_checked(settings.integer_scaling);
        menu_items.scanlines.set_checked(settings.scanlines);

        Self {
            window,
            gl_area,
            gl_ctx,
            rgba_buf,
            gamepads: Gamepads::new(),
            muted,
            last_frame_time: Instant::now(),
            last_frame_ms: 0.0,
            kb_state,
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
            overscan_changed: Cell::new(false),
            menu,
            menu_ids,
            menu_items,
        }
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
    tex_w: f32,
    tex_h: f32,
    integer_scaling: bool,
) -> (i32, i32, i32, i32) {
    let scale_x = win_w / tex_w;
    let scale_y = win_h / tex_h;
    let scale = if integer_scaling {
        scale_x.min(scale_y).floor().max(1.0)
    } else {
        scale_x.min(scale_y)
    };
    let vp_w = tex_w * scale;
    let vp_h = tex_h * scale;
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

        let (vp_x, vp_y, vp_w, vp_h) = compute_viewport(
            win_w as f32,
            win_h as f32,
            NES_WIDTH as f32,
            NES_HEIGHT as f32,
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
) {
    let key = event.keyval();
    let ctrl = event.state().contains(gdk::ModifierType::CONTROL_MASK);
    let mut kb = kb_state.get();

    if pressed && ctrl {
        match key {
            k if k == gdk_key::f || k == gdk_key::F => fullscreen.set(true),
            k if k == gdk_key::q || k == gdk_key::Q => exit.set(true),
            _ => {}
        }
        return;
    }

    match key {
        k if k == gdk_key::z || k == gdk_key::Z => {
            if pressed {
                kb |= controller::A;
            } else {
                kb &= !controller::A;
            }
        }
        k if k == gdk_key::x || k == gdk_key::X => {
            if pressed {
                kb |= controller::B;
            } else {
                kb &= !controller::B;
            }
        }
        k if k == gdk_key::c || k == gdk_key::C => {
            if pressed {
                kb |= controller::START;
            } else {
                kb &= !controller::START;
            }
        }
        k if k == gdk_key::v || k == gdk_key::V => {
            if pressed {
                kb |= controller::SELECT;
            } else {
                kb &= !controller::SELECT;
            }
        }
        k if k == gdk_key::Left => {
            if pressed {
                kb |= controller::LEFT;
            } else {
                kb &= !controller::LEFT;
            }
        }
        k if k == gdk_key::Right => {
            if pressed {
                kb |= controller::RIGHT;
            } else {
                kb &= !controller::RIGHT;
            }
        }
        k if k == gdk_key::Up => {
            if pressed {
                kb |= controller::UP;
            } else {
                kb &= !controller::UP;
            }
        }
        k if k == gdk_key::Down => {
            if pressed {
                kb |= controller::DOWN;
            } else {
                kb &= !controller::DOWN;
            }
        }
        k if k == gdk_key::w || k == gdk_key::W => {
            rewind.set(pressed);
        }
        k if k == gdk_key::space => {
            fast_forward.set(pressed);
        }
        _ => {
            if pressed {
                match key {
                    k if k == gdk_key::s || k == gdk_key::S => save_state.set(true),
                    k if k == gdk_key::a || k == gdk_key::A => load_state.set(true),
                    k if k == gdk_key::q || k == gdk_key::Q => cycle_slot.set(true),
                    k if k == gdk_key::r || k == gdk_key::R => reset.set(true),
                    k if k == gdk_key::m || k == gdk_key::M => muted.set(!muted.get()),
                    k if k == gdk_key::Tab => toggle_overlay.set(true),
                    k if k == gdk_key::F9 => scanlines.set(!scanlines.get()),
                    k if k == gdk_key::F11 => fullscreen.set(true),
                    k if k == gdk_key::i || k == gdk_key::I => {
                        pixel_perfect.set(!pixel_perfect.get());
                    }
                    _ => {}
                }
            }
        }
    }

    kb_state.set(kb);
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
        while gtk::events_pending() {
            gtk::main_iteration();
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
            } else if *id == self.menu_ids.fullscreen {
                self.toggle_fullscreen(&mut toasts);
            } else if *id == self.menu_ids.scaling {
                self.pixel_perfect.set(!self.pixel_perfect.get());
                self.menu_items
                    .scaling
                    .set_checked(self.pixel_perfect.get());
                if self.pixel_perfect.get() {
                    toasts.push("Integer scaling".into());
                } else {
                    toasts.push("Fill scaling".into());
                }
                settings::save_settings(&Settings {
                    integer_scaling: self.pixel_perfect.get(),
                    scanlines: self.scanlines.get(),
                    overscan: self.overscan.get(),
                });
            } else if *id == self.menu_ids.scanlines {
                self.scanlines.set(!self.scanlines.get());
                self.menu_items.scanlines.set_checked(self.scanlines.get());
                if self.scanlines.get() {
                    toasts.push("CRT scanlines ON".into());
                } else {
                    toasts.push("CRT scanlines OFF".into());
                }
                settings::save_settings(&Settings {
                    integer_scaling: self.pixel_perfect.get(),
                    scanlines: self.scanlines.get(),
                    overscan: self.overscan.get(),
                });
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
                settings::save_settings(&Settings {
                    integer_scaling: self.pixel_perfect.get(),
                    scanlines: self.scanlines.get(),
                    overscan: val,
                });
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
            toasts,
            open_rom: open_rom_path,
            set_overscan: if self.overscan_changed.get() {
                self.overscan_changed.set(false);
                Some(self.overscan.get())
            } else {
                None
            },
        };

        if let Some(ref path) = result.open_rom {
            add_recent_rom(path);
            self.refresh_recent_menu();
        }

        apply_gamepad(&mut self.gamepads, self.kb_state.get(), mem, &mut result);

        result
    }

    fn frame_time_ms(&self) -> Option<f64> {
        Some(self.last_frame_ms)
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        self.last_frame_ms = frame_pace(&mut self.last_frame_time, self.fast_forward.get());

        {
            let mut rgba = self.rgba_buf.borrow_mut();
            let pixel_count = buf.data.len() / 3;
            for i in 0..pixel_count {
                let src = i * 3;
                let dst = i * 4;
                if dst + 3 < rgba.len() {
                    rgba[dst] = buf.data[src];
                    rgba[dst + 1] = buf.data[src + 1];
                    rgba[dst + 2] = buf.data[src + 2];
                    rgba[dst + 3] = 255;
                }
            }
        }

        self.gl_area.queue_render();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }
}
