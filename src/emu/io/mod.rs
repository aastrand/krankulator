pub mod loader;
pub mod log;

extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;

use std::cell::RefCell;
use std::rc::Rc;

use super::memory;
use super::ppu;

pub trait IOHandler {
    fn init(&mut self) -> Result<(), String>;
    fn log(&self, logline: String);
    fn poll(&mut self, mem: &dyn memory::MemoryMapper) -> bool;
    fn render(&mut self, mem: &dyn memory::MemoryMapper);
    fn input(&mut self, mem: &mut dyn memory::MemoryMapper) -> Option<char>;
    fn exit(&self, s: String);
}

pub struct HeadlessIOHandler {}

impl IOHandler for HeadlessIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut dyn memory::MemoryMapper) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn poll(&mut self, mem: &dyn memory::MemoryMapper) -> bool {
        false
    }

    fn render(&mut self, _mem: &dyn memory::MemoryMapper) {}

    fn exit(&self, s: String) {
        self.log(s);
    }
}

pub struct SDLIOHandler {
    canvas: Canvas<Window>,
    event_pump: EventPump,
    ppu: Rc<RefCell<ppu::PPU>>,
    muted: bool,
}

impl SDLIOHandler {
    pub fn new(
        sdl_context: Sdl,
        canvas: Canvas<Window>,
        ppu: Rc<RefCell<ppu::PPU>>,
    ) -> SDLIOHandler {
        let event_pump = sdl_context.event_pump().unwrap();
        SDLIOHandler {
            event_pump: event_pump,
            canvas: canvas,
            ppu,
            muted: false,
        }
    }
}

impl<'a> IOHandler for SDLIOHandler {
    fn init(&mut self) -> Result<(), String> {
        // the canvas allows us to both manipulate the property of the window and to change its content
        // via hardware or software rendering. See CanvasBuilder for more info.
        println!("Using SDL_Renderer \"{}\"", self.canvas.info().name);
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        // clears the canvas with the color we set in `set_draw_color`.
        self.canvas.clear();
        let _ = self.canvas.set_scale(2.0, 2.0);
        // However the canvas has not been updated to the window yet, everything has been processed to
        // an internal buffer, but if we want our buffer to be displayed on the window, we need to call
        // `present`. We need to call this everytime we want to render a new frame on the window.
        self.canvas.present();

        Ok(())
    }

    fn log(&self, logline: String) {
        if !self.muted {
            println!("{}", logline);
        }
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut dyn memory::MemoryMapper) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn poll(&mut self, mem: &dyn memory::MemoryMapper) -> bool {
        let mut should_exit = false;

        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    should_exit = true;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::M),
                    ..
                } => {
                    self.muted ^= true;
                }
                _ => {}
            }
        }

        should_exit
    }

    fn render(&mut self, mem: &dyn memory::MemoryMapper) {
        self.ppu.borrow_mut().render(&mut self.canvas, mem);
        self.canvas.present();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }
}
