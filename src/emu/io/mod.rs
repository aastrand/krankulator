pub mod loader;
pub mod log;

extern crate sdl2;

use sdl2::pixels::Color;
use sdl2::video::Window;

use super::memory;
use pancurses;
use std::{thread, time};

pub trait IOHandler {
    fn init(&self) -> Result<(), String>;
    fn log(&self, logline: &str);
    fn display(&self, mem: &memory::Memory);
    fn input(&mut self, mem: &mut memory::Memory) -> Option<char>;
    fn exit(&self, s: &str);
}

pub struct HeadlessIOHandler {}

impl IOHandler for HeadlessIOHandler {
    fn init(&self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut memory::Memory) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn display(&self, mem: &memory::Memory) {}

    fn exit(&self, s: &str) {
        self.log(s);
    }
}

pub struct CursesIOHandler {
    window: pancurses::Window,
}

impl CursesIOHandler {
    pub fn new() -> CursesIOHandler {
        CursesIOHandler {
            window: pancurses::initscr(),
        }
    }
}

impl IOHandler for CursesIOHandler {
    fn init(&self) -> Result<(), String> {
        self.window.timeout(0);
        Ok(())
    }

    fn log(&self, logline: &str) {
        self.window.mvaddstr(0, 0, logline);
    }

    fn input(&mut self, mem: &mut memory::Memory) -> Option<char> {
        // TODO: Map more input
        let c = match self.window.getch() {
            Some(pancurses::Input::Character(c)) => {
                mem.write_bus(0xff, c.to_ascii_lowercase() as u8);
                Some(c.to_ascii_lowercase())
            }
            _ => None,
        };
        self.window.refresh();
        c
    }

    fn display(&self, mem: &memory::Memory) {
        // Display is stored as a 32x32 screen from 0x200 and onwards
        let base: u16 = 0x0200;
        let offset: i32 = 3;
        self.window.attron(pancurses::A_REVERSE);

        // Apple location is stored at 0x0100
        let apple_addr: u16 = mem.get_16b_addr(0x00);
        for y in 0..31 {
            for x in 0..31 {
                let addr: u16 = base + (y * 32) + x;
                let chr: char = if addr == apple_addr {
                    'O'
                } else {
                    let value: u8 = mem.read_bus(addr);
                    if value == 1 {
                        '#'
                    } else {
                        ' '
                    }
                };
                self.window.mvaddch(offset + y as i32, x as i32, chr);
            }
        }
        self.window.attroff(pancurses::A_REVERSE);
        thread::sleep(time::Duration::from_micros(10));
    }

    fn exit(&self, s: &str) {
        self.window.mvaddstr(1, 0, s);
        pancurses::endwin();
    }
}

pub struct SDLIOHandler {
    //window: Window,
}

impl IOHandler for SDLIOHandler {
    fn init(&self) -> Result<(), String> {
        let sdl_context = sdl2::init()?;
        let video_subsystem = sdl_context.video()?;
        // the window is the representation of a window in your operating system,
        // however you can only manipulate properties of that window, like its size, whether it's
        // fullscreen, ... but you cannot change its content without using a Canvas or using the
        // `surface()` method.
        let window = video_subsystem
            .window("Krankulator", 256, 240)
            .position_centered()
            .build()
            .map_err(|e| e.to_string())?;
        // the canvas allows us to both manipulate the property of the window and to change its content
        // via hardware or software rendering. See CanvasBuilder for more info.
        let mut canvas = window
            .into_canvas()
            .target_texture()
            .present_vsync()
            .build()
            .map_err(|e| e.to_string())?;
        println!("Using SDL_Renderer \"{}\"", canvas.info().name);
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        // clears the canvas with the color we set in `set_draw_color`.
        canvas.clear();
        // However the canvas has not been updated to the window yet, everything has been processed to
        // an internal buffer, but if we want our buffer to be displayed on the window, we need to call
        // `present`. We need to call this everytime we want to render a new frame on the window.
        canvas.present();

        Ok(())
    }

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut memory::Memory) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn display(&self, mem: &memory::Memory) {}

    fn exit(&self, s: &str) {
        self.log(s);
    }
}
