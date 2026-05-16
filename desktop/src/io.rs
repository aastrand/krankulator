use std::time::{Duration, Instant};

const NES_FRAME_DURATION: Duration = Duration::from_nanos(16_639_267);

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

use krankulator_core::emu::apu;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::controller;
use krankulator_core::emu::io::{DebugContext, IOHandler, PollResult};
use krankulator_core::emu::memory;
use krankulator_core::emu::dbg;
use krankulator_core::util;

pub struct WinitPixelsIOHandler {
    pixels: Option<Pixels<'static>>,
    event_loop: Option<EventLoop<()>>,
    window: Option<&'static Window>,
    muted: bool,
    last_frame_time: Instant,
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
            last_frame_time: Instant::now(),
        }
    }
}

struct PollHandler<'a> {
    pixels: &'a mut Pixels<'static>,
    mem: &'a mut dyn memory::MemoryMapper,
    apu: &'a mut apu::APU,
    muted: &'a mut bool,
    exit: bool,
    save_state: bool,
    load_state: bool,
    cycle_slot: bool,
    reset: bool,
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

    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, apu: &mut apu::APU) -> PollResult {
        let mut event_loop = self.event_loop.take().unwrap();

        let mut handler = PollHandler {
            pixels: self.pixels.as_mut().unwrap(),
            mem,
            apu,
            muted: &mut self.muted,
            exit: false,
            save_state: false,
            load_state: false,
            cycle_slot: false,
            reset: false,
        };

        event_loop.pump_app_events(Some(Duration::ZERO), &mut handler);
        let result = PollResult {
            exit: handler.exit,
            save_state: handler.save_state,
            load_state: handler.load_state,
            cycle_slot: handler.cycle_slot,
            reset: handler.reset,
        };

        self.event_loop = Some(event_loop);
        result
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let elapsed = self.last_frame_time.elapsed();
        if elapsed < NES_FRAME_DURATION {
            let sleep_duration = NES_FRAME_DURATION - elapsed;
            if sleep_duration > Duration::from_millis(1) {
                std::thread::sleep(sleep_duration - Duration::from_millis(1));
            }
            while self.last_frame_time.elapsed() < NES_FRAME_DURATION {
                std::hint::spin_loop();
            }
        }
        self.last_frame_time = Instant::now();

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

    #[allow(unused_must_use)]
    fn on_debug(&mut self, ctx: &mut DebugContext) {
        use shrust::{ExecError, Shell, ShellIO};
        use std::io::prelude::*;

        let mut shell = Shell::new(ctx);

        shell.new_command("m", "mem read/write: m <addr> [value]", 1, |io, ctx, w| {
            match util::hex_str_to_u16(w[0]) {
                Ok(addr) => {
                    writeln!(io, "mem[0x{:x}] == 0x{:x}", addr, ctx.mem.cpu_read(addr as _))?;
                    if w.len() > 1 {
                        match util::hex_str_to_u8(w[1]) {
                            Ok(v) => {
                                ctx.mem.cpu_write(addr as _, v);
                                writeln!(io, "mem[0x{:x}] = 0x{:x}", addr, v)?;
                            }
                            _ => { writeln!(io, "invalid value: {}", w[1])?; }
                        }
                    }
                }
                _ => { writeln!(io, "invalid address: {}", w[0])?; }
            }
            Ok(())
        });

        shell.new_command("o", "opcode lookup", 1, |io, ctx, w| {
            match util::hex_str_to_u8(w[0]) {
                Ok(o) => { writeln!(io, "0x{:x} => {}", o, ctx.lookup.name(o))?; }
                _ => { writeln!(io, "invalid opcode: {}", w[0])?; }
            };
            Ok(())
        });

        shell.new_command("cpu", "edit cpu register: cpu <reg> <value>", 2, |io, ctx, w| {
            match util::hex_str_to_u16(w[1]) {
                Ok(v) => match w[0] {
                    "a" => { ctx.cpu.a = (v & 0xff) as u8; writeln!(io, "cpu.a = 0x{:x}", ctx.cpu.a)?; }
                    "x" => { ctx.cpu.x = (v & 0xff) as u8; writeln!(io, "cpu.x = 0x{:x}", ctx.cpu.x)?; }
                    "y" => { ctx.cpu.y = (v & 0xff) as u8; writeln!(io, "cpu.y = 0x{:x}", ctx.cpu.y)?; }
                    "sp" => { ctx.cpu.sp = (v & 0xff) as u8; writeln!(io, "cpu.sp = 0x{:x}", ctx.cpu.sp)?; }
                    "status" => { ctx.cpu.status = (v & 0xff) as u8; writeln!(io, "cpu.status = 0x{:x}", ctx.cpu.status)?; }
                    "pc" => { ctx.cpu.pc = v; writeln!(io, "cpu.pc = 0x{:x}", v)?; }
                    _ => { writeln!(io, "invalid register: {}", w[0])?; }
                },
                _ => { writeln!(io, "invalid value: {}", w[1])?; }
            };
            Ok(())
        });

        shell.new_command("b", "add/remove breakpoint", 0, |io, ctx, w| {
            if !w.is_empty() {
                writeln!(io, "{}", dbg::toggle_breakpoint(w[0], ctx.breakpoints));
            }
            writeln!(io, "breakpoints:")?;
            for b in ctx.breakpoints.iter() {
                writeln!(io, "  0x{:x}: {}", b, ctx.lookup.name(ctx.mem.cpu_read(*b as _)))?;
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

        shell.new_command_noargs("c", "continue", |_, _| Err(ExecError::Quit));
        shell.new_command_noargs("q", "quit", |_, _| { std::process::exit(0); });

        shell.run_loop(&mut ShellIO::default());
    }
}
