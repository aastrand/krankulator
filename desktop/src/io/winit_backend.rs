use std::time::{Duration, Instant};

use muda::{Menu, MenuEvent};
use pixels::{Pixels, ScalingMode, SurfaceTexture};
use wgpu::util::DeviceExt;
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Icon, Window, WindowAttributes},
};

use krankulator_core::emu::apu;
use krankulator_core::emu::dbg;
use krankulator_core::emu::debug::DebugSnapshot;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::{DebugContext, IOHandler, PollResult};
use krankulator_core::emu::memory;
use krankulator_core::util;

use crate::debug::DebugUi;

use super::{
    add_recent_rom, apply_gamepad, build_menu_contents, display_width, frame_pace, open_rom_dialog,
    populate_recent_submenu, window_size_for_scale, MenuIds, MenuItems, NES_TEX_HEIGHT,
    NES_TEX_WIDTH, NTSC_FRAME_DURATION,
};
use crate::bindings::ui::{BindingUi, UiEvent};
use crate::bindings::{Action, InputBindings, KeyId};
use crate::gamepad::Gamepads;
use crate::settings::{self, Settings};

struct CrtPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CrtUniforms {
    output_size: [f32; 2],
    texture_size: [f32; 2],
    input_size: [f32; 2],
    enabled: f32,
    _pad: f32,
}

pub struct WinitPixelsIOHandler {
    pixels: Option<Pixels<'static>>,
    event_loop: Option<EventLoop<()>>,
    window: Option<&'static Window>,
    gamepads: Gamepads,
    muted: bool,
    last_frame_time: Instant,
    last_frame_ms: f64,
    kb_state: u8,
    p2_kb_state: u8,
    fast_forward: bool,
    pixel_perfect: bool,
    rewind_held: bool,
    scanlines: bool,
    overscan: bool,
    correct_aspect_ratio: bool,
    window_scale: u32,
    crt: Option<CrtPipeline>,
    frame_duration: Duration,
    _menu: Menu,
    menu_ids: MenuIds,
    menu_items: MenuItems,
    bindings: InputBindings,
    binding_ui: BindingUi,
    ui_buf: gfx::buf::Buffer,
    debug_ui: Option<DebugUi>,
    debug_snapshot: Option<DebugSnapshot>,
}

struct InitHandler {
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    width: u32,
    height: u32,
    title: String,
    window_width: u32,
    window_height: u32,
}

impl ApplicationHandler for InitHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(self.window_width, self.window_height))
            .with_window_icon(load_window_icon());
        let window = event_loop.create_window(attrs).unwrap();
        let window: &'static Window = Box::leak(Box::new(window));
        let size = window.inner_size();
        let surface_texture = SurfaceTexture::new(size.width, size.height, window);
        let pixels = Pixels::new(self.width, self.height, surface_texture).unwrap();
        self.window = Some(window);
        self.pixels = Some(pixels);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        _event: WindowEvent,
    ) {
    }
}

impl WinitPixelsIOHandler {
    pub fn new(width: u32, height: u32, rom_name: &str, settings: &mut Settings) -> Self {
        let mut event_loop = EventLoop::new().unwrap();

        let (win_w, win_h) =
            window_size_for_scale(settings.window_scale, settings.correct_aspect_ratio);
        let mut init = InitHandler {
            window: None,
            pixels: None,
            width,
            height,
            title: format!("krankulator — {rom_name}"),
            window_width: win_w,
            window_height: win_h,
        };

        loop {
            event_loop.pump_app_events(Some(Duration::ZERO), &mut init);
            if init.window.is_some() {
                break;
            }
        }

        set_dock_icon();

        let mut pixels = init.pixels;
        if let Some(p) = pixels.as_mut() {
            if settings.integer_scaling {
                p.set_scaling_mode(ScalingMode::PixelPerfect);
            } else {
                p.set_scaling_mode(ScalingMode::Fill);
            }
        }

        let crt = pixels.as_ref().map(|p| {
            let window_size = init.window.unwrap().inner_size();
            create_crt_pipeline(p, window_size.width, window_size.height, settings.scanlines)
        });

        let _window = init.window.unwrap();
        let (menu, menu_ids, menu_items) = build_menu_contents();
        menu_items.scaling.set_checked(settings.integer_scaling);
        menu_items.scanlines.set_checked(settings.scanlines);
        menu_items.overscan.set_checked(settings.overscan);
        menu_items
            .correct_aspect_ratio
            .set_checked(settings.correct_aspect_ratio);

        #[cfg(target_os = "macos")]
        {
            menu.init_for_nsapp();
        }

        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::HasWindowHandle;
            if let Ok(handle) = _window.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                    unsafe { menu.init_for_hwnd(h.hwnd.get() as _).unwrap() };
                }
            }
        }

        Self {
            pixels,
            event_loop: Some(event_loop),
            window: init.window,
            gamepads: Gamepads::new(),
            muted: false,
            last_frame_time: Instant::now(),
            last_frame_ms: 0.0,
            kb_state: 0,
            p2_kb_state: 0,
            fast_forward: false,
            pixel_perfect: settings.integer_scaling,
            rewind_held: false,
            scanlines: settings.scanlines,
            overscan: settings.overscan,
            correct_aspect_ratio: settings.correct_aspect_ratio,
            window_scale: settings.window_scale,
            crt,
            frame_duration: NTSC_FRAME_DURATION,
            _menu: menu,
            menu_ids,
            menu_items,
            bindings: std::mem::take(&mut settings.bindings),
            binding_ui: BindingUi::new(),
            ui_buf: gfx::buf::Buffer::new(),
            debug_ui: None,
            debug_snapshot: None,
        }
    }
}

fn compute_viewport(
    win_w: f32,
    win_h: f32,
    display_w: f32,
    display_h: f32,
    integer_scaling: bool,
) -> (f32, f32, f32, f32) {
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
    (vp_x, vp_y, vp_w, vp_h)
}

fn create_crt_pipeline(
    pixels: &Pixels,
    output_width: u32,
    output_height: u32,
    enabled: bool,
) -> CrtPipeline {
    let device = pixels.device();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("crt_lottes"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../../core/src/emu/gfx/shaders/crt_lottes.wgsl").into(),
        ),
    });

    let uniforms = CrtUniforms {
        output_size: [output_width as f32, output_height as f32],
        texture_size: [256.0, 240.0],
        input_size: [256.0, 240.0],
        enabled: if enabled { 1.0 } else { 0.0 },
        _pad: 0.0,
    };

    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("crt_uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("crt_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("crt_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("crt_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("crt_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: pixels.surface_texture_format(),
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    CrtPipeline {
        pipeline,
        bind_group_layout,
        sampler,
        uniform_buf,
    }
}

fn load_window_icon() -> Option<Icon> {
    let img = image::load_from_memory(super::ICON_PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let data = NSData::with_bytes(super::ICON_PNG);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon() {}

struct PollHandler<'a> {
    pixels: &'a mut Pixels<'static>,
    window: &'static Window,
    apu: &'a mut apu::APU,
    muted: &'a mut bool,
    pixel_perfect: &'a mut bool,
    scanlines: &'a mut bool,
    overscan: &'a mut bool,
    correct_aspect_ratio: &'a mut bool,
    window_scale: &'a mut u32,
    overscan_changed: bool,
    kb_state: &'a mut u8,
    p2_kb_state: &'a mut u8,
    fast_forward: &'a mut bool,
    exit: bool,
    save_state: bool,
    load_state: bool,
    cycle_slot: bool,
    reset: bool,
    toggle_overlay: bool,
    toggle_debug: bool,
    toggle_pause: bool,
    rewind: &'a mut bool,
    toasts: Vec<String>,
    open_rom: bool,
    recent_rom_path: Option<String>,
    menu_ids: &'a MenuIds,
    menu_items: &'a MenuItems,
    bindings: &'a InputBindings,
    needs_save: bool,
    binding_ui_active: bool,
    captured_keys: Vec<(KeyId, bool)>,
    scale_up: bool,
    scale_down: bool,
    ctrl_held: bool,
}

fn toggle_fullscreen(window: &Window, menu_item: &muda::CheckMenuItem, toasts: &mut Vec<String>) {
    if window.fullscreen().is_some() {
        window.set_fullscreen(None);
        menu_item.set_checked(false);
        toasts.push("Windowed".into());
    } else {
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        menu_item.set_checked(true);
        toasts.push("Fullscreen".into());
    }
}

impl ApplicationHandler for PollHandler<'_> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let _ = self.pixels.resize_surface(size.width, size.height);
            }
            WindowEvent::CloseRequested => {
                self.exit = true;
                event_loop.exit();
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl_held = if cfg!(target_os = "macos") {
                    mods.state().super_key()
                } else {
                    mods.state().control_key()
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    let key_id = KeyId::from_winit(key);

                    if self.binding_ui_active {
                        if pressed {
                            self.captured_keys.push((key_id, pressed));
                        }
                        return;
                    }

                    if pressed && key == KeyCode::F10 {
                        self.binding_ui_active = true;
                        return;
                    }

                    for action in self.bindings.keyboard_action(&key_id) {
                        match action {
                            Action::Fullscreen => {
                                if pressed {
                                    toggle_fullscreen(
                                        self.window,
                                        &self.menu_items.fullscreen,
                                        &mut self.toasts,
                                    );
                                }
                            }
                            Action::ToggleScaling => {
                                if pressed {
                                    *self.pixel_perfect = !*self.pixel_perfect;
                                    if *self.pixel_perfect {
                                        *self.correct_aspect_ratio = false;
                                        self.menu_items.correct_aspect_ratio.set_checked(false);
                                        self.pixels.set_scaling_mode(ScalingMode::PixelPerfect);
                                        self.toasts.push("Integer scaling".into());
                                    } else {
                                        self.pixels.set_scaling_mode(ScalingMode::Fill);
                                        self.toasts.push("Fill scaling".into());
                                    }
                                    self.menu_items.scaling.set_checked(*self.pixel_perfect);
                                    self.window.request_redraw();
                                    self.needs_save = true;
                                }
                            }
                            Action::Mute => {
                                if pressed {
                                    *self.muted ^= true;
                                }
                            }
                            Action::Reset => {
                                if pressed {
                                    self.reset = true;
                                }
                            }
                            Action::SaveState => {
                                if pressed {
                                    self.save_state = true;
                                }
                            }
                            Action::LoadState => {
                                if pressed {
                                    self.load_state = true;
                                }
                            }
                            Action::CycleSlot => {
                                if pressed {
                                    self.cycle_slot = true;
                                }
                            }
                            Action::ToggleOverlay => {
                                if pressed {
                                    self.toggle_overlay = true;
                                }
                            }
                            Action::ToggleScanlines => {
                                if pressed {
                                    *self.scanlines = !*self.scanlines;
                                    if *self.scanlines {
                                        self.toasts.push("CRT scanlines ON".into());
                                    } else {
                                        self.toasts.push("CRT scanlines OFF".into());
                                    }
                                    self.menu_items.scanlines.set_checked(*self.scanlines);
                                    self.needs_save = true;
                                }
                            }
                            Action::ToggleDebug => {
                                if pressed {
                                    self.toggle_debug = true;
                                }
                            }
                            Action::Pause => {
                                if pressed {
                                    self.toggle_pause = true;
                                }
                            }
                            Action::Rewind => {
                                *self.rewind = pressed;
                            }
                            Action::FastForward => {
                                *self.fast_forward = pressed;
                            }
                            action => {
                                if let Some((player, bit)) = action.controller_bit() {
                                    let state = if player == 0 {
                                        &mut *self.kb_state
                                    } else {
                                        &mut *self.p2_kb_state
                                    };
                                    if pressed {
                                        *state |= bit;
                                    } else {
                                        *state &= !bit;
                                    }
                                }
                            }
                        }
                    }

                    // Channel mutes stay hardcoded (not rebindable)
                    if pressed {
                        match key {
                            KeyCode::Digit1 => self.apu.toggle_mute_bit(0x01, "Pulse1"),
                            KeyCode::Digit2 => self.apu.toggle_mute_bit(0x02, "Pulse2"),
                            KeyCode::Digit3 => self.apu.toggle_mute_bit(0x04, "Triangle"),
                            KeyCode::Digit4 => self.apu.toggle_mute_bit(0x08, "Noise"),
                            KeyCode::Digit5 => self.apu.toggle_mute_bit(0x10, "DMC"),
                            KeyCode::Digit0 => {
                                let on = !self.apu.get_master_mute();
                                self.apu.set_master_mute(on);
                            }
                            _ => {}
                        }
                    }

                    if pressed && self.ctrl_held {
                        if let winit::keyboard::Key::Character(ch) = &event.logical_key {
                            match ch.as_str() {
                                "+" | "=" => self.scale_up = true,
                                "-" | "_" => self.scale_down = true,
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl WinitPixelsIOHandler {
    fn refresh_recent_menu(&mut self) {
        let submenu = &self.menu_items.recent_submenu;
        while submenu.remove_at(0).is_some() {}
        self.menu_items.recent_items = populate_recent_submenu(submenu);
    }

    fn save_display_settings(&self) {
        settings::save_settings(&Settings {
            integer_scaling: self.pixel_perfect,
            scanlines: self.scanlines,
            overscan: self.overscan,
            correct_aspect_ratio: self.correct_aspect_ratio,
            window_scale: self.window_scale,
            bindings: self.bindings.clone(),
        });
    }
}

impl IOHandler for WinitPixelsIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        if !self.muted {
            println!("{logline}");
        }
    }

    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, apu: &mut apu::APU) -> PollResult {
        let mut event_loop = self.event_loop.take().unwrap();

        let mut handler = PollHandler {
            pixels: self.pixels.as_mut().unwrap(),
            window: self.window.unwrap(),
            apu,
            muted: &mut self.muted,
            pixel_perfect: &mut self.pixel_perfect,
            scanlines: &mut self.scanlines,
            overscan: &mut self.overscan,
            correct_aspect_ratio: &mut self.correct_aspect_ratio,
            window_scale: &mut self.window_scale,
            overscan_changed: false,
            kb_state: &mut self.kb_state,
            p2_kb_state: &mut self.p2_kb_state,
            fast_forward: &mut self.fast_forward,
            exit: false,
            save_state: false,
            load_state: false,
            cycle_slot: false,
            reset: false,
            toggle_overlay: false,
            toggle_debug: false,
            toggle_pause: false,
            rewind: &mut self.rewind_held,
            toasts: Vec::new(),
            open_rom: false,
            recent_rom_path: None,
            menu_ids: &self.menu_ids,
            menu_items: &self.menu_items,
            bindings: &self.bindings,
            needs_save: false,
            binding_ui_active: self.binding_ui.is_active(),
            captured_keys: Vec::new(),
            scale_up: false,
            scale_down: false,
            ctrl_held: false,
        };

        event_loop.pump_app_events(Some(Duration::ZERO), &mut handler);

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id();
            if *id == handler.menu_ids.open_rom {
                handler.open_rom = true;
            } else if *id == handler.menu_ids.quit {
                handler.exit = true;
            } else if *id == handler.menu_ids.reset {
                handler.reset = true;
            } else if *id == handler.menu_ids.save_state {
                handler.save_state = true;
            } else if *id == handler.menu_ids.load_state {
                handler.load_state = true;
            } else if *id == handler.menu_ids.cycle_slot {
                handler.cycle_slot = true;
            } else if *id == handler.menu_ids.input_settings {
                handler.binding_ui_active = true;
            } else if *id == handler.menu_ids.debug_view {
                handler.toggle_debug = true;
            } else if *id == handler.menu_ids.pause {
                handler.toggle_pause = true;
            } else if *id == handler.menu_ids.fullscreen {
                toggle_fullscreen(
                    handler.window,
                    &self.menu_items.fullscreen,
                    &mut handler.toasts,
                );
            } else if *id == handler.menu_ids.scaling {
                *handler.pixel_perfect = !*handler.pixel_perfect;
                self.menu_items.scaling.set_checked(*handler.pixel_perfect);
                if *handler.pixel_perfect {
                    *handler.correct_aspect_ratio = false;
                    self.menu_items.correct_aspect_ratio.set_checked(false);
                    handler.pixels.set_scaling_mode(ScalingMode::PixelPerfect);
                    handler.toasts.push("Integer scaling".into());
                } else {
                    handler.pixels.set_scaling_mode(ScalingMode::Fill);
                    handler.toasts.push("Fill scaling".into());
                }
                handler.window.request_redraw();
                handler.needs_save = true;
            } else if *id == handler.menu_ids.scanlines {
                *handler.scanlines = !*handler.scanlines;
                self.menu_items.scanlines.set_checked(*handler.scanlines);
                if *handler.scanlines {
                    handler.toasts.push("CRT scanlines ON".into());
                } else {
                    handler.toasts.push("CRT scanlines OFF".into());
                }
                handler.needs_save = true;
            } else if *id == handler.menu_ids.overscan {
                *handler.overscan = !*handler.overscan;
                handler.overscan_changed = true;
                self.menu_items.overscan.set_checked(*handler.overscan);
                if *handler.overscan {
                    handler.toasts.push("Overscan hidden".into());
                } else {
                    handler.toasts.push("Overscan visible".into());
                }
                handler.needs_save = true;
            } else if *id == handler.menu_ids.correct_aspect_ratio {
                *handler.correct_aspect_ratio = !*handler.correct_aspect_ratio;
                self.menu_items
                    .correct_aspect_ratio
                    .set_checked(*handler.correct_aspect_ratio);
                if *handler.correct_aspect_ratio {
                    *handler.pixel_perfect = false;
                    self.menu_items.scaling.set_checked(false);
                    handler.pixels.set_scaling_mode(ScalingMode::Fill);
                    handler.toasts.push("8:7 aspect ratio".into());
                } else {
                    handler.toasts.push("Square pixels".into());
                }
                let (w, h) =
                    window_size_for_scale(*handler.window_scale, *handler.correct_aspect_ratio);
                if handler.window.fullscreen().is_none() {
                    let _ = handler.window.request_inner_size(LogicalSize::new(w, h));
                }
                handler.window.request_redraw();
                handler.needs_save = true;
            } else if *id == handler.menu_ids.scale_up {
                if *handler.window_scale < 6 {
                    *handler.window_scale += 1;
                    let (w, h) =
                        window_size_for_scale(*handler.window_scale, *handler.correct_aspect_ratio);
                    if handler.window.fullscreen().is_none() {
                        let _ = handler.window.request_inner_size(LogicalSize::new(w, h));
                    }
                    handler
                        .toasts
                        .push(format!("{}x scale", *handler.window_scale));
                    handler.needs_save = true;
                }
            } else if *id == handler.menu_ids.scale_down {
                if *handler.window_scale > 1 {
                    *handler.window_scale -= 1;
                    let (w, h) =
                        window_size_for_scale(*handler.window_scale, *handler.correct_aspect_ratio);
                    if handler.window.fullscreen().is_none() {
                        let _ = handler.window.request_inner_size(LogicalSize::new(w, h));
                    }
                    handler
                        .toasts
                        .push(format!("{}x scale", *handler.window_scale));
                    handler.needs_save = true;
                }
            } else if let Some(path) = self
                .menu_items
                .recent_items
                .iter()
                .find(|(mid, _)| mid == id)
                .map(|(_, p)| p.clone())
            {
                handler.recent_rom_path = Some(path);
            }
        }

        if handler.scale_up && *handler.window_scale < 6 {
            *handler.window_scale += 1;
            let (w, h) =
                window_size_for_scale(*handler.window_scale, *handler.correct_aspect_ratio);
            if handler.window.fullscreen().is_none() {
                let _ = handler.window.request_inner_size(LogicalSize::new(w, h));
            }
            handler
                .toasts
                .push(format!("{}x scale", *handler.window_scale));
            handler.needs_save = true;
        }
        if handler.scale_down && *handler.window_scale > 1 {
            *handler.window_scale -= 1;
            let (w, h) =
                window_size_for_scale(*handler.window_scale, *handler.correct_aspect_ratio);
            if handler.window.fullscreen().is_none() {
                let _ = handler.window.request_inner_size(LogicalSize::new(w, h));
            }
            handler
                .toasts
                .push(format!("{}x scale", *handler.window_scale));
            handler.needs_save = true;
        }

        let open_rom = if handler.open_rom {
            open_rom_dialog()
        } else {
            handler.recent_rom_path.take()
        };

        let open_binding_ui = handler.binding_ui_active && !self.binding_ui.is_active();
        let captured_keys = std::mem::take(&mut handler.captured_keys);
        let rewind = *handler.rewind;
        let fast_forward = *handler.fast_forward;
        let mut needs_save = handler.needs_save;
        let mut result = PollResult {
            exit: handler.exit,
            save_state: handler.save_state,
            load_state: handler.load_state,
            cycle_slot: handler.cycle_slot,
            reset: handler.reset,
            toggle_overlay: handler.toggle_overlay,
            rewind,
            fast_forward,
            toasts: handler.toasts,
            open_rom,
            set_overscan: if handler.overscan_changed {
                Some(*handler.overscan)
            } else {
                None
            },
            toggle_debug: handler.toggle_debug,
            toggle_pause: handler.toggle_pause,
        };

        if handler.toggle_debug {
            let window = self.window.unwrap();
            let ws = *handler.window_scale;
            let car = *handler.correct_aspect_ratio;
            if self.debug_ui.is_some() {
                self.debug_ui = None;
                self.debug_snapshot = None;
                self.menu_items.debug_view.set_checked(false);
                if window.fullscreen().is_none() {
                    let (w, h) = window_size_for_scale(ws, car);
                    let _ = window.request_inner_size(LogicalSize::new(w, h));
                }
            } else {
                self.debug_ui = Some(DebugUi::new(window, handler.pixels));
                self.menu_items.debug_view.set_checked(true);
                if window.fullscreen().is_none() {
                    let (w, h) = window_size_for_scale(ws, car);
                    let panel_extra = (crate::debug::PANEL_WIDTH * 2.0) as u32;
                    let _ = window.request_inner_size(LogicalSize::new(w + panel_extra, h));
                }
            }
        }

        if handler.toggle_pause {
            let is_checked = self.menu_items.pause.is_checked();
            self.menu_items.pause.set_checked(!is_checked);
        }

        if open_binding_ui {
            self.binding_ui.open();
        }
        for (key_id, _pressed) in &captured_keys {
            match self.binding_ui.handle_key(key_id, &mut self.bindings) {
                UiEvent::Close | UiEvent::None => {}
                UiEvent::BindingsChanged => {
                    needs_save = true;
                }
            }
        }

        if let Some(ref path) = result.open_rom {
            add_recent_rom(path);
            self.refresh_recent_menu();
        }

        if needs_save {
            self.save_display_settings();
        }

        if self.binding_ui.is_active() {
            for btn in self.gamepads.poll_raw_buttons() {
                match self
                    .binding_ui
                    .handle_gamepad_button(btn, &mut self.bindings)
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
                &self.bindings,
                self.kb_state,
                self.p2_kb_state,
                mem,
                &mut result,
            );
        }

        self.event_loop = Some(event_loop);
        result
    }

    fn frame_time_ms(&self) -> Option<f64> {
        Some(self.last_frame_ms)
    }

    fn set_frame_duration_nanos(&mut self, nanos: u64) {
        self.frame_duration = Duration::from_nanos(nanos);
    }

    fn set_overscan_available(&mut self, available: bool) {
        self.menu_items.overscan.set_enabled(available);
        if !available {
            self.overscan = false;
        }
    }

    fn set_debug_snapshot(&mut self, snapshot: DebugSnapshot) {
        self.debug_snapshot = Some(snapshot);
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        self.last_frame_ms = frame_pace(
            &mut self.last_frame_time,
            self.fast_forward,
            self.frame_duration,
        );

        let render_buf = if self.binding_ui.is_active() {
            self.ui_buf.data.copy_from_slice(&buf.data);
            self.binding_ui.draw(&mut self.ui_buf, &self.bindings);
            &self.ui_buf
        } else {
            buf
        };

        let window = self.window.unwrap();
        let pixels = self.pixels.as_mut().unwrap();
        let size = window.inner_size();
        let _ = pixels.resize_surface(size.width, size.height);
        let frame = pixels.frame_mut();
        let pixel_count = render_buf.data.len() / 3;
        for i in 0..pixel_count {
            let rgb = &render_buf.data[i * 3..i * 3 + 3];
            let j = i * 4;
            if j + 3 < frame.len() {
                frame[j] = rgb[0];
                frame[j + 1] = rgb[1];
                frame[j + 2] = rgb[2];
                frame[j + 3] = 255;
            }
        }

        let mut debug_ui = self.debug_ui.take();
        let debug_snapshot = self.debug_snapshot.take();

        if let Some(crt) = &self.crt {
            let disp_w = display_width(self.correct_aspect_ratio);
            let (vp_x, vp_y, vp_w, vp_h) = compute_viewport(
                size.width as f32,
                size.height as f32,
                disp_w,
                NES_TEX_HEIGHT,
                self.pixel_perfect,
            );
            let uniforms = CrtUniforms {
                output_size: [vp_w, vp_h],
                texture_size: [NES_TEX_WIDTH, NES_TEX_HEIGHT],
                input_size: [NES_TEX_WIDTH, NES_TEX_HEIGHT],
                enabled: if self.scanlines { 1.0 } else { 0.0 },
                _pad: 0.0,
            };
            pixels
                .queue()
                .write_buffer(&crt.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

            let texture_view = pixels
                .texture()
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = pixels
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("crt_bind_group"),
                    layout: &crt.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: crt.uniform_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&crt.sampler),
                        },
                    ],
                });

            let pipeline = &crt.pipeline;
            pixels
                .render_with(|encoder, render_target, _context| {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("crt_render_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: render_target,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        ..Default::default()
                    });
                    rpass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                    rpass.set_pipeline(pipeline);
                    rpass.set_bind_group(0, Some(&bind_group), &[]);
                    rpass.draw(0..4, 0..1);
                    drop(rpass);
                    if let (Some(dui), Some(snap)) = (&mut debug_ui, &debug_snapshot) {
                        dui.prepare_and_render(
                            window,
                            snap,
                            encoder,
                            render_target,
                            &_context.device,
                            &_context.queue,
                        );
                    }
                    Ok(())
                })
                .unwrap();
        } else {
            if let (Some(dui), Some(snap)) = (&mut debug_ui, &debug_snapshot) {
                pixels
                    .render_with(|encoder, render_target, context| {
                        context.scaling_renderer.render(encoder, render_target);
                        dui.prepare_and_render(
                            window,
                            snap,
                            encoder,
                            render_target,
                            &context.device,
                            &context.queue,
                        );
                        Ok(())
                    })
                    .unwrap();
            } else {
                pixels.render().unwrap();
            }
        }

        self.debug_ui = debug_ui;

        window.request_redraw();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }

    #[allow(unused_must_use)]
    fn on_debug(&mut self, ctx: &mut DebugContext) {
        use shrust::{ExecError, Shell, ShellIO};
        use std::io::prelude::*;

        let mut shell = Shell::new(ctx);

        shell.new_command("m", "mem read/write: m <addr> [value]", 1, |io, ctx, w| {
            match util::hex_str_to_u16(w[0]) {
                Ok(addr) => {
                    writeln!(
                        io,
                        "mem[0x{:x}] == 0x{:x}",
                        addr,
                        ctx.mem.cpu_read(addr as _)
                    )?;
                    if w.len() > 1 {
                        match util::hex_str_to_u8(w[1]) {
                            Ok(v) => {
                                ctx.mem.cpu_write(addr as _, v);
                                writeln!(io, "mem[0x{addr:x}] = 0x{v:x}")?;
                            }
                            _ => {
                                writeln!(io, "invalid value: {}", w[1])?;
                            }
                        }
                    }
                }
                _ => {
                    writeln!(io, "invalid address: {}", w[0])?;
                }
            }
            Ok(())
        });

        shell.new_command("o", "opcode lookup", 1, |io, ctx, w| {
            match util::hex_str_to_u8(w[0]) {
                Ok(o) => {
                    writeln!(io, "0x{:x} => {}", o, ctx.lookup.name(o))?;
                }
                _ => {
                    writeln!(io, "invalid opcode: {}", w[0])?;
                }
            };
            Ok(())
        });

        shell.new_command(
            "cpu",
            "edit cpu register: cpu <reg> <value>",
            2,
            |io, ctx, w| {
                match util::hex_str_to_u16(w[1]) {
                    Ok(v) => match w[0] {
                        "a" => {
                            ctx.cpu.a = (v & 0xff) as u8;
                            writeln!(io, "cpu.a = 0x{:x}", ctx.cpu.a)?;
                        }
                        "x" => {
                            ctx.cpu.x = (v & 0xff) as u8;
                            writeln!(io, "cpu.x = 0x{:x}", ctx.cpu.x)?;
                        }
                        "y" => {
                            ctx.cpu.y = (v & 0xff) as u8;
                            writeln!(io, "cpu.y = 0x{:x}", ctx.cpu.y)?;
                        }
                        "sp" => {
                            ctx.cpu.sp = (v & 0xff) as u8;
                            writeln!(io, "cpu.sp = 0x{:x}", ctx.cpu.sp)?;
                        }
                        "status" => {
                            ctx.cpu.status = (v & 0xff) as u8;
                            writeln!(io, "cpu.status = 0x{:x}", ctx.cpu.status)?;
                        }
                        "pc" => {
                            ctx.cpu.pc = v;
                            writeln!(io, "cpu.pc = 0x{v:x}")?;
                        }
                        _ => {
                            writeln!(io, "invalid register: {}", w[0])?;
                        }
                    },
                    _ => {
                        writeln!(io, "invalid value: {}", w[1])?;
                    }
                };
                Ok(())
            },
        );

        shell.new_command("b", "add/remove breakpoint", 0, |io, ctx, w| {
            if !w.is_empty() {
                writeln!(io, "{}", dbg::toggle_breakpoint(w[0], ctx.breakpoints));
            }
            writeln!(io, "breakpoints:")?;
            for b in ctx.breakpoints.iter() {
                writeln!(
                    io,
                    "  0x{:x}: {}",
                    b,
                    ctx.lookup.name(ctx.mem.cpu_read(*b as _))
                )?;
            }
            Ok(())
        });

        shell.new_command_noargs("s", "toggle stepping", |io, ctx| {
            *ctx.stepping = !*ctx.stepping;
            writeln!(io, "stepping: {}", *ctx.stepping)?;
            Ok(())
        });

        shell.new_command_noargs("l", "toggle log output", |io, ctx| {
            *ctx.should_log = !*ctx.should_log;
            writeln!(io, "logging: {}", *ctx.should_log)?;
            Ok(())
        });

        shell.new_command_noargs("v", "toggle verbose mode", |io, ctx| {
            *ctx.verbose = !*ctx.verbose;
            writeln!(io, "verbose: {}", *ctx.verbose)?;
            Ok(())
        });

        shell.new_command_noargs("c", "continue", |_, ctx| {
            *ctx.stepping = false;
            Err(ExecError::Quit)
        });
        shell.new_command_noargs("q", "quit", |_, _| {
            std::process::exit(0);
        });

        shell.run_loop(&mut ShellIO::default());
    }
}
