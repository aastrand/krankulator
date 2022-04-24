pub mod controller;
pub mod loader;
pub mod log;

extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::render::TextureCreator;
use sdl2::video::Window;
use sdl2::video::WindowContext;
use sdl2::EventPump;
use sdl2::Sdl;

use super::cpu;
use super::gfx;
use super::memory;

pub trait IOHandler {
    fn init(&mut self) -> Result<(), String>;
    fn log(&self, logline: String);
    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, cpu: &mut cpu::Cpu) -> bool;
    fn render(&mut self, buf: &gfx::buf::Buffer);
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
    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, cpu: &mut cpu::Cpu) -> bool {
        false
    }

    fn render(&mut self, _buf: &gfx::buf::Buffer) {}

    fn exit(&self, s: String) {
        self.log(s);
    }
}

pub struct SDLIOHandler {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    muted: bool,
}

impl SDLIOHandler {
    pub fn new(
        sdl_context: Sdl,
        canvas: Canvas<Window>,
    ) -> SDLIOHandler {
        let event_pump = sdl_context.event_pump().unwrap();
        let texture_creator = canvas.texture_creator();

        SDLIOHandler {
            event_pump: event_pump,
            canvas: canvas,
            texture_creator,
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
    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, cpu: &mut cpu::Cpu) -> bool {
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
                Event::KeyDown {
                    keycode: Some(Keycode::R),
                    ..
                } => {
                    cpu.pc = mem.get_16b_addr(memory::RESET_TARGET_ADDR);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::Z),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::A);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Z),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::A);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::X),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::B);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::X),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::B);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::C),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::START);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::C),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::START);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::V),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::SELECT);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::V),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::SELECT);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::Left),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::LEFT);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Left),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::LEFT);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::RIGHT);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::RIGHT);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::Up),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::UP);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Up),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::UP);
                }

                Event::KeyDown {
                    keycode: Some(Keycode::Down),
                    ..
                } => {
                    mem.controllers()[0].set_pressed(controller::DOWN);
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Down),
                    ..
                } => {
                    mem.controllers()[0].set_not_pressed(controller::DOWN);
                }

                _ => {}
            }
        }

        should_exit
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let mut texture = self.texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB24, buf.width as u32, buf.height as u32)
            .map_err(|e| e.to_string())
            .ok()
            .unwrap();

        texture.update(None, &buf.data, 256 * 3).unwrap();
        self.canvas.copy(&texture, None, None).unwrap();
        self.canvas.present();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }
}
