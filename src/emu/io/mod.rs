pub mod controller;
pub mod loader;
pub mod log;

use pixels::{Pixels, SurfaceTexture};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
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
    pub pixels: Pixels,
    pub event_loop: Option<EventLoop<()>>,
    pub window: winit::window::Window,
    pub muted: bool,
}

impl WinitPixelsIOHandler {
    pub fn new(width: u32, height: u32) -> Self {
        let scale = 4.0;
        let window_width = (width as f32 * scale) as u32;
        let window_height = (height as f32 * scale) as u32;
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("krankulator")
            .with_inner_size(LogicalSize::new(window_width, window_height))
            .build(&event_loop)
            .unwrap();

        let surface_texture = SurfaceTexture::new(width, height, &window);
        let pixels = Pixels::new(width, height, surface_texture).unwrap();

        Self {
            event_loop: Some(event_loop),
            window,
            pixels,
            muted: false,
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
        let mut exit = false;

        let mut event_loop = self.event_loop.take().unwrap();

        event_loop.run_return(|event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::MainEventsCleared => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        let _ = self.pixels.resize_surface(size.width, size.height);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        let _ = self
                            .pixels
                            .resize_surface(new_inner_size.width, new_inner_size.height);
                    }
                    WindowEvent::CloseRequested => {
                        exit = true;
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let Some(key) = input.virtual_keycode {
                            let pressed = input.state == ElementState::Pressed;

                            match key {
                                VirtualKeyCode::Escape => {
                                    exit = true;
                                    *control_flow = ControlFlow::Exit;
                                }
                                VirtualKeyCode::M => {
                                    if pressed {
                                        self.muted ^= true;
                                    }
                                }
                                VirtualKeyCode::R => {
                                    if pressed {
                                        cpu.pc = mem.get_16b_addr(memory::RESET_TARGET_ADDR);
                                    }
                                }
                                VirtualKeyCode::Z => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::A);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::A);
                                    }
                                }
                                VirtualKeyCode::X => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::B);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::B);
                                    }
                                }
                                VirtualKeyCode::C => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::START);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::START);
                                    }
                                }
                                VirtualKeyCode::V => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::SELECT);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::SELECT);
                                    }
                                }
                                VirtualKeyCode::Left => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::LEFT);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::LEFT);
                                    }
                                }
                                VirtualKeyCode::Right => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::RIGHT);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::RIGHT);
                                    }
                                }
                                VirtualKeyCode::Up => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::UP);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::UP);
                                    }
                                }
                                VirtualKeyCode::Down => {
                                    if pressed {
                                        mem.controllers()[0].set_pressed(controller::DOWN);
                                    } else {
                                        mem.controllers()[0].set_not_pressed(controller::DOWN);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        });

        self.event_loop = Some(event_loop);
        exit
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let frame = self.pixels.frame_mut();
        let pixel_count = buf.data.len() / 3;
        for i in 0..pixel_count {
            let rgb = &buf.data[i * 3..i * 3 + 3];
            let j = i * 4;
            if j + 3 < frame.len() {
                frame[j] = rgb[0];
                frame[j + 1] = rgb[1];
                frame[j + 2] = rgb[2];
                frame[j + 3] = 255; // Opaque alpha
            }
        }
        self.pixels.render().unwrap();
        self.window.request_redraw();
    }

    fn exit(&self, s: String) {
        self.log(s);
    }
}
