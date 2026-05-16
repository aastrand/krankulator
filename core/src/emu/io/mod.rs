pub mod controller;
pub mod loader;
pub mod log;

use super::apu;
use super::gfx;
use super::memory;

#[derive(Default)]
pub struct PollResult {
    pub exit: bool,
    pub save_state: bool,
    pub load_state: bool,
    pub cycle_slot: bool,
    pub reset: bool,
}

pub trait IOHandler {
    fn init(&mut self) -> Result<(), String>;
    fn log(&self, logline: String);
    fn poll(&mut self, mem: &mut dyn memory::MemoryMapper, apu: &mut apu::APU) -> PollResult;
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

    fn poll(&mut self, _mem: &mut dyn memory::MemoryMapper, _apu: &mut apu::APU) -> PollResult {
        PollResult::default()
    }

    fn render(&mut self, _buf: &gfx::buf::Buffer) {}

    fn exit(&self, s: String) {
        self.log(s);
    }
}
