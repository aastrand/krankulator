pub mod controller;
pub mod loader;
pub mod log;

use std::time::Duration;

use pixels::{Pixels, SurfaceTexture};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes},
};

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

pub struct WinitPixelsIOHandler {
    pixels: Option<Pixels<'static>>,
    event_loop: Option<EventLoop<()>>,
    window: Option<&'static Window>,
    muted: bool,
}

struct InitHandler {
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    width: u32,
    height: u32,
}

impl ApplicationHandler for InitHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let scale = 4.0;
        let window_width = (self.width as f32 * scale) as u32;
        let window_height = (self.height as f32 * scale) as u32;
        let attrs = WindowAttributes::default()
            .with_title("krankulator")
            .with_inner_size(LogicalSize::new(window_width, window_height));
        let window = event_loop.create_window(attrs).unwrap();
        let window: &'static Window = Box::leak(Box::new(window));
        let surface_texture = SurfaceTexture::new(self.width, self.height, window);
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
    pub fn new(width: u32, height: u32) -> Self {
        let mut event_loop = EventLoop::new().unwrap();

        let mut init = InitHandler {
            window: None,
            pixels: None,
            width,
            height,
        };

        loop {
            event_loop.pump_app_events(Some(Duration::ZERO), &mut init);
            if init.window.is_some() {
                break;
            }
        }

        Self {
            pixels: init.pixels,
            event_loop: Some(event_loop),
            window: init.window,
            muted: false,
        }
    }
}

struct PollHandler<'a> {
    pixels: &'a mut Pixels<'static>,
    mem: &'a mut dyn memory::MemoryMapper,
    cpu: &'a mut cpu::Cpu,
    muted: &'a mut bool,
    exit: bool,
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
                        KeyCode::Escape => {
                            self.exit = true;
                            event_loop.exit();
                        }
                        KeyCode::KeyM => {
                            if pressed {
                                *self.muted ^= true;
                            }
                        }
                        KeyCode::KeyR => {
                            if pressed {
                                self.cpu.pc = self.mem.get_16b_addr(memory::RESET_TARGET_ADDR);
                                self.mem.apu().borrow_mut().reset();
                            }
                        }
                        KeyCode::Digit1 => {
                            if pressed {
                                self.mem.apu().borrow_mut().toggle_mute_bit(0x01, "Pulse1");
                            }
                        }
                        KeyCode::Digit2 => {
                            if pressed {
                                self.mem.apu().borrow_mut().toggle_mute_bit(0x02, "Pulse2");
                            }
                        }
                        KeyCode::Digit3 => {
                            if pressed {
                                self.mem
                                    .apu()
                                    .borrow_mut()
                                    .toggle_mute_bit(0x04, "Triangle");
                            }
                        }
                        KeyCode::Digit4 => {
                            if pressed {
                                self.mem.apu().borrow_mut().toggle_mute_bit(0x08, "Noise");
                            }
                        }
                        KeyCode::Digit5 => {
                            if pressed {
                                self.mem.apu().borrow_mut().toggle_mute_bit(0x10, "DMC");
                            }
                        }
                        KeyCode::Digit0 => {
                            if pressed {
                                let on = !self.mem.apu().borrow().get_master_mute();
                                self.mem.apu().borrow_mut().set_master_mute(on);
                            }
                        }
                        KeyCode::KeyZ => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::A);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::A);
                            }
                        }
                        KeyCode::KeyX => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::B);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::B);
                            }
                        }
                        KeyCode::KeyC => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::START);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::START);
                            }
                        }
                        KeyCode::KeyV => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::SELECT);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::SELECT);
                            }
                        }
                        KeyCode::ArrowLeft => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::LEFT);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::LEFT);
                            }
                        }
                        KeyCode::ArrowRight => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::RIGHT);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::RIGHT);
                            }
                        }
                        KeyCode::ArrowUp => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::UP);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::UP);
                            }
                        }
                        KeyCode::ArrowDown => {
                            if pressed {
                                self.mem.controllers()[0].set_pressed(controller::DOWN);
                            } else {
                                self.mem.controllers()[0].set_not_pressed(controller::DOWN);
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

impl IOHandler for WinitPixelsIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        if !self.muted {
            println!("{}", logline);
        }
    }

    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, cpu: &mut cpu::Cpu) -> bool {
        let mut event_loop = self.event_loop.take().unwrap();

        let mut handler = PollHandler {
            pixels: self.pixels.as_mut().unwrap(),
            mem,
            cpu,
            muted: &mut self.muted,
            exit: false,
        };

        event_loop.pump_app_events(Some(Duration::ZERO), &mut handler);
        let exit = handler.exit;

        self.event_loop = Some(event_loop);
        exit
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let pixels = self.pixels.as_mut().unwrap();
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
}
