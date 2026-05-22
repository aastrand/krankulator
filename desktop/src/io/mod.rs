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
    accelerator::Accelerator, AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem,
    Submenu,
};

use krankulator_core::emu::io::controller;
use krankulator_core::emu::io::PollResult;
use krankulator_core::emu::memory;

use crate::gamepad::Gamepads;

const NES_FRAME_DURATION: Duration = Duration::from_nanos(16_639_267);

pub(crate) static ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

const MAX_RECENT_ROMS: usize = 10;

#[allow(unused_variables)]
pub(crate) struct MenuIds {
    pub open_rom: muda::MenuId,
    pub quit: muda::MenuId,
    pub reset: muda::MenuId,
    pub save_state: muda::MenuId,
    pub load_state: muda::MenuId,
    pub cycle_slot: muda::MenuId,
    pub fullscreen: muda::MenuId,
    pub scaling: muda::MenuId,
}

pub(crate) struct MenuItems {
    pub fullscreen: CheckMenuItem,
    pub scaling: CheckMenuItem,
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
    let cycle_slot = MenuItem::new("Cycle Save Slot", true, None::<Accelerator>);
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

pub(crate) fn open_rom_dialog() -> Option<String> {
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
}

fn gamepad_state_to_bits(s: &crate::gamepad::GamepadState) -> u8 {
    let mut bits: u8 = 0;
    if s.a {
        bits |= controller::A;
    }
    if s.b {
        bits |= controller::B;
    }
    if s.start {
        bits |= controller::START;
    }
    if s.select {
        bits |= controller::SELECT;
    }
    if s.up {
        bits |= controller::UP;
    }
    if s.down {
        bits |= controller::DOWN;
    }
    if s.left {
        bits |= controller::LEFT;
    }
    if s.right {
        bits |= controller::RIGHT;
    }
    bits
}

pub(crate) fn apply_gamepad(
    gamepads: &mut Gamepads,
    kb_state: u8,
    mem: &mut dyn memory::MemoryMapper,
    result: &mut PollResult,
) {
    let gp = gamepads.poll();
    for msg in gp.toasts {
        result.toasts.push(msg);
    }

    let mut p0_state = kb_state;
    if let Some(s) = &gp.states[0] {
        p0_state |= gamepad_state_to_bits(s);
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

    if let Some(s) = &gp.states[1] {
        mem.controllers()[1].load_status(gamepad_state_to_bits(s));
    }
}

pub(crate) fn frame_pace(last_frame_time: &mut Instant, fast_forward: bool) -> f64 {
    let elapsed = last_frame_time.elapsed();
    let frame_ms = elapsed.as_secs_f64() * 1000.0;
    if !fast_forward && elapsed < NES_FRAME_DURATION {
        let sleep_duration = NES_FRAME_DURATION - elapsed;
        if sleep_duration > Duration::from_millis(1) {
            std::thread::sleep(sleep_duration - Duration::from_millis(1));
        }
        while last_frame_time.elapsed() < NES_FRAME_DURATION {
            std::hint::spin_loop();
        }
    }
    *last_frame_time = Instant::now();
    frame_ms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamepad::GamepadState;

    fn default_gamepad_state() -> GamepadState {
        GamepadState {
            a: false,
            b: false,
            start: false,
            select: false,
            up: false,
            down: false,
            left: false,
            right: false,
            save_state: false,
            load_state: false,
            cycle_slot: false,
        }
    }

    #[test]
    fn test_gamepad_state_to_bits_empty() {
        assert_eq!(gamepad_state_to_bits(&default_gamepad_state()), 0);
    }

    #[test]
    fn test_gamepad_state_to_bits_all() {
        let s = GamepadState {
            a: true,
            b: true,
            start: true,
            select: true,
            up: true,
            down: true,
            left: true,
            right: true,
            save_state: false,
            load_state: false,
            cycle_slot: false,
        };
        let bits = gamepad_state_to_bits(&s);
        assert_eq!(bits, 0xFF);
    }

    #[test]
    fn test_gamepad_state_to_bits_individual() {
        let mut s = default_gamepad_state();
        s.a = true;
        assert_eq!(gamepad_state_to_bits(&s), controller::A);

        let mut s = default_gamepad_state();
        s.left = true;
        s.up = true;
        assert_eq!(gamepad_state_to_bits(&s), controller::LEFT | controller::UP);
    }

    #[test]
    fn test_frame_pace_fast_forward_skips_sleep() {
        let mut t = Instant::now();
        let ms = frame_pace(&mut t, true);
        assert!(ms < 1.0);
    }

    #[test]
    fn test_frame_pace_normal_sleeps_to_frame_budget() {
        let mut t = Instant::now();
        let before = Instant::now();
        frame_pace(&mut t, false);
        let wall = before.elapsed();
        assert!(
            wall.as_millis() >= 14,
            "wall time {}ms too low — should sleep to ~16.6ms",
            wall.as_millis()
        );
    }
}
