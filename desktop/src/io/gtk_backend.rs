use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gdk::keys::constants as gdk_key;
use gdk::prelude::*;
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
const NES_WIDTH: i32 = 256;
const NES_HEIGHT: i32 = 240;

pub struct GtkPixelsIOHandler {
    window: gtk::Window,
    drawing_area: gtk::DrawingArea,
    surface_buf: Rc<RefCell<Vec<u8>>>,
    gamepads: Gamepads,
    muted: Rc<Cell<bool>>,
    last_frame_time: Instant,
    last_frame_ms: f64,
    kb_state: Rc<Cell<u8>>,
    fast_forward: Rc<Cell<bool>>,
    pixel_perfect: Rc<Cell<bool>>,
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
    _menu: Menu,
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

        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_can_focus(true);
        vbox.pack_end(&drawing_area, true, true, 0);

        // BGRA framebuffer for Cairo (NES_WIDTH x NES_HEIGHT)
        let surface_buf = Rc::new(RefCell::new(vec![
            0u8;
            (NES_WIDTH * NES_HEIGHT * 4) as usize
        ]));

        let pixel_perfect = Rc::new(Cell::new(true));

        // Draw signal — Cairo software render
        {
            let buf = surface_buf.clone();
            let pp = pixel_perfect.clone();
            drawing_area.connect_draw(move |da, cr| {
                let alloc = da.allocation();
                let w = alloc.width() as f64;
                let h = alloc.height() as f64;

                let data = buf.borrow();
                let surface = gtk::cairo::ImageSurface::create_for_data(
                    data.clone(),
                    gtk::cairo::Format::ARgb32,
                    NES_WIDTH,
                    NES_HEIGHT,
                    NES_WIDTH * 4,
                )
                .unwrap();

                let scale_x = w / NES_WIDTH as f64;
                let scale_y = h / NES_HEIGHT as f64;

                let scale = if pp.get() {
                    (scale_x.min(scale_y)).floor().max(1.0)
                } else {
                    scale_x.min(scale_y)
                };
                let render_w = NES_WIDTH as f64 * scale;
                let render_h = NES_HEIGHT as f64 * scale;
                let offset_x = (w - render_w) / 2.0;
                let offset_y = (h - render_h) / 2.0;

                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.paint().unwrap();

                cr.translate(offset_x, offset_y);
                cr.scale(scale, scale);

                cr.set_source_surface(&surface, 0.0, 0.0).unwrap();
                let pattern = cr.source();
                pattern.set_filter(gtk::cairo::Filter::Nearest);
                cr.paint().unwrap();

                glib::Propagation::Stop
            });
        }

        window.show_all();
        drawing_area.grab_focus();

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
            let pp = pixel_perfect.clone();
            window.connect_key_press_event(move |_, event| {
                handle_key(
                    event, true, &kb, &ff, &mt, &save, &load, &cycle, &reset, &overlay, &rw, &fs,
                    &pp,
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
            let pp = pixel_perfect.clone();
            window.connect_key_release_event(move |_, event| {
                handle_key(
                    event, false, &kb, &ff, &mt, &save, &load, &cycle, &reset, &overlay, &rw, &fs,
                    &pp,
                );
                glib::Propagation::Proceed
            });
        }

        menu_items.overscan.set_checked(settings.overscan);

        Self {
            window,
            drawing_area,
            surface_buf,
            gamepads: Gamepads::new(),
            muted,
            last_frame_time: Instant::now(),
            last_frame_ms: 0.0,
            kb_state,
            fast_forward,
            pixel_perfect,
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
            _menu: menu,
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
            self.menu_items.fullscreen.set_checked(false);
            toasts.push("Windowed".into());
        } else {
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
    pixel_perfect: &Rc<Cell<bool>>,
) {
    let key = event.keyval();
    let mut kb = kb_state.get();

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

        // Handle scaling toggle from keyboard
        // (pixel_perfect is toggled directly in handle_key via Rc<Cell>)

        // Clear one-shot flags
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
                // reset already captured from keyboard flag
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
                    scanlines: false,
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

        // Convert RGB to BGRA (Cairo's native format on little-endian)
        {
            let mut surface = self.surface_buf.borrow_mut();
            let pixel_count = buf.data.len() / 3;
            for i in 0..pixel_count {
                let src = i * 3;
                let dst = i * 4;
                if dst + 3 < surface.len() {
                    surface[dst] = buf.data[src + 2]; // B
                    surface[dst + 1] = buf.data[src + 1]; // G
                    surface[dst + 2] = buf.data[src]; // R
                    surface[dst + 3] = 255; // A
                }
            }
        }

        self.drawing_area.queue_draw();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }
}
