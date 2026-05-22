use std::time::{Duration, Instant};

const NES_FRAME_DURATION: Duration = Duration::from_nanos(16_639_267);

use muda::{
    accelerator::Accelerator, AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem,
    PredefinedMenuItem, Submenu,
};
use pixels::{Pixels, ScalingMode, SurfaceTexture};
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
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::controller;
use krankulator_core::emu::io::{DebugContext, IOHandler, PollResult};
use krankulator_core::emu::memory;
use krankulator_core::util;

use crate::gamepad::Gamepads;

pub struct WinitPixelsIOHandler {
    pixels: Option<Pixels<'static>>,
    event_loop: Option<EventLoop<()>>,
    window: Option<&'static Window>,
    gamepads: Gamepads,
    muted: bool,
    last_frame_time: Instant,
    last_frame_ms: f64,
    kb_state: u8,
    fast_forward: bool,
    pixel_perfect: bool,
    _menu: Menu,
    menu_ids: MenuIds,
    menu_items: MenuItems,
}

struct InitHandler {
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    width: u32,
    height: u32,
    title: String,
}

impl ApplicationHandler for InitHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let scale = 4.0;
        let window_width = (self.width as f32 * scale) as u32;
        let window_height = (self.height as f32 * scale) as u32;
        let attrs = WindowAttributes::default()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(window_width, window_height))
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
    pub fn new(width: u32, height: u32, rom_name: &str) -> Self {
        let mut event_loop = EventLoop::new().unwrap();

        let mut init = InitHandler {
            window: None,
            pixels: None,
            width,
            height,
            title: format!("krankulator — {}", rom_name),
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
            p.set_scaling_mode(ScalingMode::PixelPerfect);
        }

        let (menu, menu_ids, menu_items) = build_menu(init.window.unwrap());

        Self {
            pixels,
            event_loop: Some(event_loop),
            window: init.window,
            gamepads: Gamepads::new(),
            muted: false,
            last_frame_time: Instant::now(),
            last_frame_ms: 0.0,
            kb_state: 0,
            fast_forward: false,
            pixel_perfect: true,
            _menu: menu,
            menu_ids,
            menu_items,
        }
    }
}

fn load_window_icon() -> Option<Icon> {
    static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let data = NSData::with_bytes(ICON_PNG);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon() {}

#[allow(unused_variables)]
struct MenuIds {
    open_rom: muda::MenuId,
    quit: muda::MenuId,
    reset: muda::MenuId,
    save_state: muda::MenuId,
    load_state: muda::MenuId,
    cycle_slot: muda::MenuId,
    fullscreen: muda::MenuId,
    scaling: muda::MenuId,
}

struct MenuItems {
    fullscreen: CheckMenuItem,
    scaling: CheckMenuItem,
    recent_submenu: Submenu,
    recent_items: Vec<(muda::MenuId, String)>,
}

const MAX_RECENT_ROMS: usize = 10;

fn recent_roms_path() -> Option<std::path::PathBuf> {
    crate::config_dir().map(|d| d.join("recent_roms.txt"))
}

fn load_recent_roms() -> Vec<String> {
    recent_roms_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn save_recent_roms(roms: &[String]) {
    if let Some(path) = recent_roms_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, roms.join("\n"));
    }
}

pub(crate) fn add_recent_rom(path: &str) {
    let mut roms = load_recent_roms();
    roms.retain(|r| r != path);
    roms.insert(0, path.to_string());
    roms.truncate(MAX_RECENT_ROMS);
    save_recent_roms(&roms);
}

fn populate_recent_submenu(submenu: &Submenu) -> Vec<(muda::MenuId, String)> {
    let roms = load_recent_roms();
    let mut items = Vec::new();
    for rom in &roms {
        let label = std::path::Path::new(rom)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(rom);
        let item = MenuItem::new(label, true, None::<Accelerator>);
        items.push((item.id().clone(), rom.clone()));
        submenu.append(&item).unwrap();
    }
    if roms.is_empty() {
        let empty = MenuItem::new("No Recent ROMs", false, None::<Accelerator>);
        submenu.append(&empty).unwrap();
    }
    items
}

fn about_metadata() -> AboutMetadata {
    static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
    let icon = image::load_from_memory(ICON_PNG).ok().map(|img| {
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        muda::Icon::from_rgba(rgba.into_raw(), w, h).unwrap()
    });
    AboutMetadata {
        name: Some("Krankulator".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("Anders Astrand".into()),
        comments: Some("A cycle-stepped NES emulator".into()),
        website: Some("https://github.com/aastrand/krankulator".into()),
        icon,
        ..Default::default()
    }
}

#[allow(unused_variables)]
fn build_menu(window: &Window) -> (Menu, MenuIds, MenuItems) {
    let menu = Menu::new();

    let file_menu = Submenu::new("File", true);
    let open_rom = MenuItem::new(
        "Open ROM...",
        true,
        Some("CmdOrCtrl+O".parse::<Accelerator>().unwrap()),
    );
    let open_rom_id = open_rom.id().clone();
    file_menu.append(&open_rom).unwrap();
    let recent_submenu = Submenu::new("Recent", true);
    let recent_items = populate_recent_submenu(&recent_submenu);
    file_menu.append(&recent_submenu).unwrap();
    let quit = MenuItem::new(
        "Exit",
        true,
        Some("CmdOrCtrl+Q".parse::<Accelerator>().unwrap()),
    );
    let quit_id = quit.id().clone();
    file_menu.append(&PredefinedMenuItem::separator()).unwrap();
    file_menu.append(&quit).unwrap();

    let emu_menu = Submenu::new("Emulation", true);
    let reset = MenuItem::new(
        "Reset",
        true,
        Some("CmdOrCtrl+R".parse::<Accelerator>().unwrap()),
    );
    let reset_id = reset.id().clone();
    let save_state = MenuItem::new(
        "Save State",
        true,
        Some("CmdOrCtrl+S".parse::<Accelerator>().unwrap()),
    );
    let save_state_id = save_state.id().clone();
    let load_state = MenuItem::new(
        "Load State",
        true,
        Some("CmdOrCtrl+L".parse::<Accelerator>().unwrap()),
    );
    let load_state_id = load_state.id().clone();
    let cycle_slot = MenuItem::new(
        "Cycle Save Slot",
        true,
        Some("CmdOrCtrl+Q".parse::<Accelerator>().unwrap()),
    );
    let cycle_slot_id = cycle_slot.id().clone();
    emu_menu.append(&reset).unwrap();
    emu_menu.append(&PredefinedMenuItem::separator()).unwrap();
    emu_menu.append(&save_state).unwrap();
    emu_menu.append(&load_state).unwrap();
    emu_menu.append(&cycle_slot).unwrap();

    let view_menu = Submenu::new("Display", true);
    let fullscreen = CheckMenuItem::new(
        "Fullscreen",
        true,
        false,
        Some("CmdOrCtrl+F".parse::<Accelerator>().unwrap()),
    );
    let fullscreen_id = fullscreen.id().clone();
    let scaling = CheckMenuItem::new("Integer Scaling", true, true, None::<Accelerator>);
    let scaling_id = scaling.id().clone();
    view_menu.append(&fullscreen).unwrap();
    view_menu.append(&scaling).unwrap();

    let help_menu = Submenu::new("Help", true);
    help_menu
        .append(&PredefinedMenuItem::about(
            Some("About Krankulator"),
            Some(about_metadata()),
        ))
        .unwrap();

    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::new("Krankulator", true);
        app_menu
            .append(&PredefinedMenuItem::about(None, Some(about_metadata())))
            .unwrap();
        app_menu.append(&PredefinedMenuItem::separator()).unwrap();
        app_menu.append(&PredefinedMenuItem::quit(None)).unwrap();
        menu.append(&app_menu).unwrap();
    }

    menu.append(&file_menu).unwrap();
    menu.append(&emu_menu).unwrap();
    menu.append(&view_menu).unwrap();
    menu.append(&help_menu).unwrap();

    #[cfg(target_os = "macos")]
    {
        menu.init_for_nsapp();
    }

    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::HasWindowHandle;
        if let Ok(handle) = window.window_handle() {
            if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                unsafe { menu.init_for_hwnd(h.hwnd.get() as _).unwrap() };
            }
        }
    }

    let ids = MenuIds {
        open_rom: open_rom_id,
        quit: quit_id,
        reset: reset_id,
        save_state: save_state_id,
        load_state: load_state_id,
        cycle_slot: cycle_slot_id,
        fullscreen: fullscreen_id,
        scaling: scaling_id,
    };
    let items = MenuItems {
        fullscreen,
        scaling,
        recent_submenu,
        recent_items,
    };
    (menu, ids, items)
}

struct PollHandler<'a> {
    pixels: &'a mut Pixels<'static>,
    window: &'static Window,
    apu: &'a mut apu::APU,
    muted: &'a mut bool,
    pixel_perfect: &'a mut bool,
    kb_state: &'a mut u8,
    fast_forward: &'a mut bool,
    exit: bool,
    save_state: bool,
    load_state: bool,
    cycle_slot: bool,
    reset: bool,
    toggle_overlay: bool,
    toasts: Vec<String>,
    open_rom: bool,
    recent_rom_path: Option<String>,
    menu_ids: &'a MenuIds,
    menu_items: &'a MenuItems,
}

fn toggle_fullscreen(window: &Window, menu_item: &CheckMenuItem, toasts: &mut Vec<String>) {
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
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;

                    match key {
                        KeyCode::F11 => {
                            if pressed {
                                toggle_fullscreen(
                                    self.window,
                                    &self.menu_items.fullscreen,
                                    &mut self.toasts,
                                );
                            }
                        }
                        KeyCode::KeyI => {
                            if pressed {
                                *self.pixel_perfect = !*self.pixel_perfect;
                                self.menu_items.scaling.set_checked(*self.pixel_perfect);
                                if *self.pixel_perfect {
                                    self.pixels.set_scaling_mode(ScalingMode::PixelPerfect);
                                    self.toasts.push("Integer scaling".into());
                                } else {
                                    self.pixels.set_scaling_mode(ScalingMode::Fill);
                                    self.toasts.push("Fill scaling".into());
                                }
                                self.window.request_redraw();
                            }
                        }
                        KeyCode::KeyM => {
                            if pressed {
                                *self.muted ^= true;
                            }
                        }
                        KeyCode::KeyR => {
                            if pressed {
                                self.reset = true;
                            }
                        }
                        KeyCode::KeyS => {
                            if pressed {
                                self.save_state = true;
                            }
                        }
                        KeyCode::KeyA => {
                            if pressed {
                                self.load_state = true;
                            }
                        }
                        KeyCode::KeyQ => {
                            if pressed {
                                self.cycle_slot = true;
                            }
                        }
                        KeyCode::Digit1 => {
                            if pressed {
                                self.apu.toggle_mute_bit(0x01, "Pulse1");
                            }
                        }
                        KeyCode::Digit2 => {
                            if pressed {
                                self.apu.toggle_mute_bit(0x02, "Pulse2");
                            }
                        }
                        KeyCode::Digit3 => {
                            if pressed {
                                self.apu.toggle_mute_bit(0x04, "Triangle");
                            }
                        }
                        KeyCode::Digit4 => {
                            if pressed {
                                self.apu.toggle_mute_bit(0x08, "Noise");
                            }
                        }
                        KeyCode::Digit5 => {
                            if pressed {
                                self.apu.toggle_mute_bit(0x10, "DMC");
                            }
                        }
                        KeyCode::Digit0 => {
                            if pressed {
                                let on = !self.apu.get_master_mute();
                                self.apu.set_master_mute(on);
                            }
                        }
                        KeyCode::Tab => {
                            if pressed {
                                self.toggle_overlay = true;
                            }
                        }
                        KeyCode::Space => {
                            *self.fast_forward = pressed;
                        }
                        KeyCode::KeyZ => {
                            if pressed {
                                *self.kb_state |= controller::A;
                            } else {
                                *self.kb_state &= !controller::A;
                            }
                        }
                        KeyCode::KeyX => {
                            if pressed {
                                *self.kb_state |= controller::B;
                            } else {
                                *self.kb_state &= !controller::B;
                            }
                        }
                        KeyCode::KeyC => {
                            if pressed {
                                *self.kb_state |= controller::START;
                            } else {
                                *self.kb_state &= !controller::START;
                            }
                        }
                        KeyCode::KeyV => {
                            if pressed {
                                *self.kb_state |= controller::SELECT;
                            } else {
                                *self.kb_state &= !controller::SELECT;
                            }
                        }
                        KeyCode::ArrowLeft => {
                            if pressed {
                                *self.kb_state |= controller::LEFT;
                            } else {
                                *self.kb_state &= !controller::LEFT;
                            }
                        }
                        KeyCode::ArrowRight => {
                            if pressed {
                                *self.kb_state |= controller::RIGHT;
                            } else {
                                *self.kb_state &= !controller::RIGHT;
                            }
                        }
                        KeyCode::ArrowUp => {
                            if pressed {
                                *self.kb_state |= controller::UP;
                            } else {
                                *self.kb_state &= !controller::UP;
                            }
                        }
                        KeyCode::ArrowDown => {
                            if pressed {
                                *self.kb_state |= controller::DOWN;
                            } else {
                                *self.kb_state &= !controller::DOWN;
                            }
                        }
                        _ => {}
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
}

impl IOHandler for WinitPixelsIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        if !self.muted {
            println!("{}", logline);
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

            kb_state: &mut self.kb_state,
            fast_forward: &mut self.fast_forward,
            exit: false,
            save_state: false,
            load_state: false,
            cycle_slot: false,
            reset: false,
            toggle_overlay: false,
            toasts: Vec::new(),
            open_rom: false,
            recent_rom_path: None,
            menu_ids: &self.menu_ids,
            menu_items: &self.menu_items,
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
                    handler.pixels.set_scaling_mode(ScalingMode::PixelPerfect);
                    handler.toasts.push("Integer scaling".into());
                } else {
                    handler.pixels.set_scaling_mode(ScalingMode::Fill);
                    handler.toasts.push("Fill scaling".into());
                }
                handler.window.request_redraw();
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

        let open_rom = if handler.open_rom {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Open NES ROM")
                .add_filter("NES ROMs", &["nes"])
                .add_filter("All files", &["*"]);
            if let Some(dir) = crate::load_last_rom_dir() {
                dialog = dialog.set_directory(&dir);
            }
            dialog.pick_file().map(|p| {
                if let Some(dir) = p.parent() {
                    crate::save_last_rom_dir(dir);
                }
                p.to_string_lossy().into_owned()
            })
        } else {
            handler.recent_rom_path.take()
        };

        let mut result = PollResult {
            exit: handler.exit,
            save_state: handler.save_state,
            load_state: handler.load_state,
            cycle_slot: handler.cycle_slot,
            reset: handler.reset,
            toggle_overlay: handler.toggle_overlay,
            toasts: handler.toasts,
            open_rom,
        };

        if let Some(ref path) = result.open_rom {
            add_recent_rom(path);
            self.refresh_recent_menu();
        }

        let gp = self.gamepads.poll();
        for msg in gp.toasts {
            result.toasts.push(msg);
        }

        // Merge keyboard and gamepad for player 0 (OR logic: either source can press)
        let mut p0_state = self.kb_state;
        if let Some(s) = &gp.states[0] {
            if s.a {
                p0_state |= controller::A;
            }
            if s.b {
                p0_state |= controller::B;
            }
            if s.start {
                p0_state |= controller::START;
            }
            if s.select {
                p0_state |= controller::SELECT;
            }
            if s.up {
                p0_state |= controller::UP;
            }
            if s.down {
                p0_state |= controller::DOWN;
            }
            if s.left {
                p0_state |= controller::LEFT;
            }
            if s.right {
                p0_state |= controller::RIGHT;
            }
            if s.save_state {
                result.save_state = true;
            }
            if s.load_state {
                result.load_state = true;
            }
            if s.cycle_slot {
                result.cycle_slot = true;
            }
        }
        mem.controllers()[0].load_status(p0_state);

        // Player 2: gamepad only
        if let Some(s) = &gp.states[1] {
            let c = &mut mem.controllers()[1];
            let mut state: u8 = 0;
            if s.a {
                state |= controller::A;
            }
            if s.b {
                state |= controller::B;
            }
            if s.start {
                state |= controller::START;
            }
            if s.select {
                state |= controller::SELECT;
            }
            if s.up {
                state |= controller::UP;
            }
            if s.down {
                state |= controller::DOWN;
            }
            if s.left {
                state |= controller::LEFT;
            }
            if s.right {
                state |= controller::RIGHT;
            }
            c.load_status(state);
        }

        self.event_loop = Some(event_loop);
        result
    }

    fn frame_time_ms(&self) -> Option<f64> {
        Some(self.last_frame_ms)
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let elapsed = self.last_frame_time.elapsed();
        self.last_frame_ms = elapsed.as_secs_f64() * 1000.0;
        if !self.fast_forward && elapsed < NES_FRAME_DURATION {
            let sleep_duration = NES_FRAME_DURATION - elapsed;
            if sleep_duration > Duration::from_millis(1) {
                std::thread::sleep(sleep_duration - Duration::from_millis(1));
            }
            while self.last_frame_time.elapsed() < NES_FRAME_DURATION {
                std::hint::spin_loop();
            }
        }
        self.last_frame_time = Instant::now();

        let window = self.window.unwrap();
        let pixels = self.pixels.as_mut().unwrap();
        let size = window.inner_size();
        let _ = pixels.resize_surface(size.width, size.height);
        let frame = pixels.frame_mut();
        let pixel_count = buf.data.len() / 3;
        for i in 0..pixel_count {
            let rgb = &buf.data[i * 3..i * 3 + 3];
            let j = i * 4;
            if j + 3 < frame.len() {
                frame[j] = rgb[0];
                frame[j + 1] = rgb[1];
                frame[j + 2] = rgb[2];
                frame[j + 3] = 255;
            }
        }
        pixels.render().unwrap();
        self.window.unwrap().request_redraw();
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
                                writeln!(io, "mem[0x{:x}] = 0x{:x}", addr, v)?;
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
                            writeln!(io, "cpu.pc = 0x{:x}", v)?;
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
