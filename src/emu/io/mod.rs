pub mod loader;
pub mod log;

extern crate sdl2;

use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;

use super::memory;

pub trait IOHandler {
    fn init(&mut self) -> Result<(), String>;
    fn log(&self, logline: &str);
    fn display(&mut self, mem: &dyn memory::MemoryMapper);
    fn input(&mut self, mem: &mut dyn memory::MemoryMapper) -> Option<char>;
    fn exit(&self, s: &str);
}

pub struct HeadlessIOHandler {}

impl IOHandler for HeadlessIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem:  &mut dyn memory::MemoryMapper) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn display(&mut self, mem: &dyn memory::MemoryMapper) {}

    fn exit(&self, s: &str) {
        self.log(s);
    }
}

pub struct SDLIOHandler {
    canvas: Canvas<Window>,
    event_pump: EventPump,
}

impl SDLIOHandler {
    pub fn new(sdl_context: Sdl, canvas: Canvas<Window>) -> SDLIOHandler {
        let event_pump = sdl_context.event_pump().unwrap();
        SDLIOHandler {
            event_pump: event_pump,
            canvas: canvas,
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
        // However the canvas has not been updated to the window yet, everything has been processed to
        // an internal buffer, but if we want our buffer to be displayed on the window, we need to call
        // `present`. We need to call this everytime we want to render a new frame on the window.
        self.canvas.present();

        Ok(())
    }

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem:  &mut dyn memory::MemoryMapper) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn display(&mut self, mem: &dyn memory::MemoryMapper) {
        self.event_pump.poll_event();
        self.canvas.present();
    }

    fn exit(&self, s: &str) {
        self.log(s);
    }
}
