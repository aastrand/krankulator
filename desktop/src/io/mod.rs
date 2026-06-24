#[cfg(target_os = "linux")]
mod gtk_backend;
#[cfg(not(target_os = "linux"))]
mod winit_backend;

#[cfg(target_os = "linux")]
pub use gtk_backend::GtkPixelsIOHandler as PlatformIOHandler;
#[cfg(not(target_os = "linux"))]
pub use winit_backend::WinitPixelsIOHandler as PlatformIOHandler;

use std::time::{Duration, Instant};

use muda::{
    accelerator::{Accelerator, Key, KeyAccelerator, CMD_OR_CTRL},
    AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
};

use krankulator_core::emu::io::PollResult;
use krankulator_core::emu::memory;

use crate::bindings::InputBindings;
use crate::gamepad::Gamepads;

pub(crate) const NTSC_FRAME_DURATION: Duration = Duration::from_nanos(16_639_267);

pub(crate) const NES_TEX_WIDTH: f32 = 256.0;
pub(crate) const NES_TEX_HEIGHT: f32 = 240.0;
// 8:7 pixel aspect ratio — the NES outputs square pixels but CRTs stretch them to 4:3
pub(crate) const PAR: f32 = 8.0 / 7.0;

pub(crate) fn display_width(correct_ar: bool) -> f32 {
    if correct_ar {
        NES_TEX_WIDTH * PAR
    } else {
        NES_TEX_WIDTH
    }
}

pub(crate) fn window_size_for_scale(scale: u32, correct_ar: bool) -> (u32, u32) {
    let w = (display_width(correct_ar) * scale as f32).ceil() as u32;
    let h = NES_TEX_HEIGHT as u32 * scale;
    (w, h)
}

pub(crate) static ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

const MAX_RECENT_ROMS: usize = 10;

#[allow(dead_code)]
pub(crate) struct MenuIds {
    pub open_rom: muda::MenuId,
    pub quit: muda::MenuId,
    pub reset: muda::MenuId,
    pub save_state: muda::MenuId,
    pub load_state: muda::MenuId,
    pub cycle_slot: muda::MenuId,
    pub input_settings: muda::MenuId,
    pub debug_view: muda::MenuId,
    pub pause: muda::MenuId,
    pub fullscreen: muda::MenuId,
    pub scaling: muda::MenuId,
    pub scanlines: muda::MenuId,
    pub overscan: muda::MenuId,
    pub correct_aspect_ratio: muda::MenuId,
    pub scale_up: muda::MenuId,
    pub scale_down: muda::MenuId,
}

#[allow(dead_code)]
pub(crate) struct MenuItems {
    pub debug_view: CheckMenuItem,
    pub pause: CheckMenuItem,
    pub fullscreen: CheckMenuItem,
    pub scaling: CheckMenuItem,
    pub scanlines: CheckMenuItem,
    pub overscan: CheckMenuItem,
    pub correct_aspect_ratio: CheckMenuItem,
    pub recent_submenu: Submenu,
    pub recent_items: Vec<(muda::MenuId, String)>,
}

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

pub(crate) fn build_menu_contents() -> (Menu, MenuIds, MenuItems) {
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
    let reset = MenuItem::new("Reset", true, None::<Accelerator>);
    let reset_id = reset.id().clone();
    let save_state = MenuItem::new("Save State", true, None::<Accelerator>);
    let save_state_id = save_state.id().clone();
    let load_state = MenuItem::new("Load State", true, None::<Accelerator>);
    let load_state_id = load_state.id().clone();
    let cycle_slot = MenuItem::new("Cycle Save Slot", true, None::<Accelerator>);
    let cycle_slot_id = cycle_slot.id().clone();
    let input_settings = MenuItem::new("Input Settings...", true, None::<Accelerator>);
    let input_settings_id = input_settings.id().clone();
    let debug_view = CheckMenuItem::new(
        "Debug View",
        true,
        false,
        Some("F12".parse::<Accelerator>().unwrap()),
    );
    let debug_view_id = debug_view.id().clone();
    let pause = CheckMenuItem::new("Pause", true, false, None::<Accelerator>);
    let pause_id = pause.id().clone();
    emu_menu.append(&reset).unwrap();
    emu_menu.append(&PredefinedMenuItem::separator()).unwrap();
    emu_menu.append(&save_state).unwrap();
    emu_menu.append(&load_state).unwrap();
    emu_menu.append(&cycle_slot).unwrap();
    emu_menu.append(&PredefinedMenuItem::separator()).unwrap();
    emu_menu.append(&pause).unwrap();
    emu_menu.append(&input_settings).unwrap();
    emu_menu.append(&debug_view).unwrap();

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
    let scanlines = CheckMenuItem::new("CRT Scanlines", true, false, None::<Accelerator>);
    let scanlines_id = scanlines.id().clone();
    let overscan = CheckMenuItem::new("Hide Overscan", true, true, None::<Accelerator>);
    let overscan_id = overscan.id().clone();
    let correct_aspect_ratio = CheckMenuItem::new(
        "Correct Aspect Ratio (8:7)",
        true,
        true,
        None::<Accelerator>,
    );
    let correct_aspect_ratio_id = correct_aspect_ratio.id().clone();
    let scale_up = MenuItem::new("Increase Window Size", true, None::<Accelerator>);
    scale_up
        .set_key_accelerator(Some(KeyAccelerator::new(
            Some(CMD_OR_CTRL),
            Key::Character("+".into()),
        )))
        .unwrap();
    let scale_up_id = scale_up.id().clone();
    let scale_down = MenuItem::new("Decrease Window Size", true, None::<Accelerator>);
    scale_down
        .set_key_accelerator(Some(KeyAccelerator::new(
            Some(CMD_OR_CTRL),
            Key::Character("-".into()),
        )))
        .unwrap();
    let scale_down_id = scale_down.id().clone();
    view_menu.append(&fullscreen).unwrap();
    view_menu.append(&scaling).unwrap();
    view_menu.append(&scanlines).unwrap();
    view_menu.append(&overscan).unwrap();
    view_menu.append(&correct_aspect_ratio).unwrap();
    view_menu.append(&PredefinedMenuItem::separator()).unwrap();
    view_menu.append(&scale_up).unwrap();
    view_menu.append(&scale_down).unwrap();

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
        app_menu.append(&quit).unwrap();
        menu.append(&app_menu).unwrap();
    }

    menu.append(&file_menu).unwrap();
    menu.append(&emu_menu).unwrap();
    menu.append(&view_menu).unwrap();
    menu.append(&help_menu).unwrap();

    let ids = MenuIds {
        open_rom: open_rom_id,
        quit: quit_id,
        reset: reset_id,
        save_state: save_state_id,
        load_state: load_state_id,
        cycle_slot: cycle_slot_id,
        input_settings: input_settings_id,
        debug_view: debug_view_id,
        pause: pause_id,
        fullscreen: fullscreen_id,
        scaling: scaling_id,
        scanlines: scanlines_id,
        overscan: overscan_id,
        correct_aspect_ratio: correct_aspect_ratio_id,
        scale_up: scale_up_id,
        scale_down: scale_down_id,
    };
    let items = MenuItems {
        debug_view,
        pause,
        fullscreen,
        scaling,
        scanlines,
        overscan,
        correct_aspect_ratio,
        recent_submenu,
        recent_items,
    };
    (menu, ids, items)
}

pub(crate) fn open_rom_dialog() -> Option<String> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open NES ROM")
        .add_filter("NES ROMs", &["nes", "zip"])
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
}

pub struct TurboState {
    frame_counter: u8,
}

impl TurboState {
    pub fn new() -> Self {
        Self { frame_counter: 0 }
    }

    fn tick(&mut self) {
        self.frame_counter = self.frame_counter.wrapping_add(1);
    }

    fn is_active(&self) -> bool {
        self.frame_counter % 2 == 0
    }
}

pub(crate) fn apply_gamepad(
    gamepads: &mut Gamepads,
    bindings: &InputBindings,
    p1_kb_state: u8,
    p2_kb_state: u8,
    p1_turbo_kb: u8,
    p2_turbo_kb: u8,
    turbo: &mut TurboState,
    mem: &mut dyn memory::MemoryMapper,
    result: &mut PollResult,
) {
    let gp = gamepads.poll(bindings);
    for msg in gp.toasts {
        result.toasts.push(msg);
    }

    let mut p0_state = p1_kb_state;
    let mut p1_state = p2_kb_state;
    let mut turbo_p0: u8 = p1_turbo_kb;
    let mut turbo_p1: u8 = p2_turbo_kb;

    for s in gp.states.iter().flatten() {
        p0_state |= s.p1_bits;
        p1_state |= s.p2_bits;
        turbo_p0 |= s.turbo_p1_bits;
        turbo_p1 |= s.turbo_p2_bits;
        if s.save_state {
            result.save_state = true;
        }
        if s.load_state {
            result.load_state = true;
        }
        if s.cycle_slot {
            result.cycle_slot = true;
        }
        if s.rewind {
            result.rewind = true;
        }
        if s.fast_forward {
            result.fast_forward = true;
        }
    }

    if turbo.is_active() {
        p0_state |= turbo_p0;
        p1_state |= turbo_p1;
    }
    turbo.tick();

    mem.controllers()[0].load_status(p0_state);
    mem.controllers()[1].load_status(p1_state);
}

pub(crate) fn frame_pace(
    last_frame_time: &mut Instant,
    fast_forward: bool,
    frame_duration: Duration,
) -> f64 {
    let elapsed = last_frame_time.elapsed();
    let frame_ms = elapsed.as_secs_f64() * 1000.0;
    if !fast_forward && elapsed < frame_duration {
        let sleep_duration = frame_duration - elapsed;
        if sleep_duration > Duration::from_millis(1) {
            std::thread::sleep(sleep_duration - Duration::from_millis(1));
        }
        while last_frame_time.elapsed() < frame_duration {
            std::hint::spin_loop();
        }
    }
    *last_frame_time = Instant::now();
    frame_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_pace_fast_forward_skips_sleep() {
        let mut t = Instant::now();
        let ms = frame_pace(&mut t, true, NTSC_FRAME_DURATION);
        assert!(ms < 1.0);
    }

    #[test]
    fn test_frame_pace_normal_sleeps_to_frame_budget() {
        let mut t = Instant::now();
        let before = Instant::now();
        frame_pace(&mut t, false, NTSC_FRAME_DURATION);
        let wall = before.elapsed();
        assert!(
            wall.as_millis() >= 14,
            "wall time {}ms too low — should sleep to ~16.6ms",
            wall.as_millis()
        );
    }
}
