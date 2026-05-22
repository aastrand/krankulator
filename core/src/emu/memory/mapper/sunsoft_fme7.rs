use super::super::super::io;
use super::{mirror_nametable_addr, NametableMirror, RESET_TARGET_ADDR};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct SunsoftFme7Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    prg_ram: Box<[u8; 0x2000]>,
    has_battery: bool,

    command: u8,
    chr_banks: [u8; 8],
    prg_banks: [u8; 4],
    work_ram_value: u8,
    mirroring: NametableMirror,

    irq_counter: u16,
    irq_counter_enabled: bool,
    irq_enabled: bool,
    irq_pending: bool,

    vram: Box<[u8; 0x800]>,
    cpu_ram: Box<[u8; 0x800]>,
    palette_ram: [u8; 32],
}

impl SunsoftFme7Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let mut chr_rom = vec![];
        for bank in &chr_banks_8k {
            for i in 0..8 {
                chr_rom.push(
                    <[u8; CHR_BANK_SIZE]>::try_from(
                        &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                    )
                    .unwrap(),
                );
            }
        }

        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        SunsoftFme7Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_ram: {
                let mut ram = Box::new([0; 0x2000]);
                if let Some(data) = sram_data {
                    let len = data.len().min(0x2000);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            has_battery,
            command: 0,
            chr_banks: [0; 8],
            prg_banks: [0; 4],
            work_ram_value: 0,
            mirroring,
            irq_counter: 0,
            irq_counter_enabled: false,
            irq_enabled: false,
            irq_pending: false,
            vram: Box::new([0; 0x800]),
            cpu_ram: Box::new([0; 0x800]),
            palette_ram: [0x0F; 32],
        }
    }

    fn prg_bank_index(&self, slot: usize) -> usize {
        (self.prg_banks[slot] as usize & 0x3F) % self.prg_rom.len().max(1)
    }

    fn execute_command(&mut self, value: u8) {
        match self.command {
            0..=7 => self.chr_banks[self.command as usize] = value,
            8 => self.work_ram_value = value,
            9 => self.prg_banks[1] = value,
            0xA => self.prg_banks[2] = value,
            0xB => self.prg_banks[3] = value,
            0xC => {
                self.mirroring = match value & 3 {
                    0 => NametableMirror::Vertical,
                    1 => NametableMirror::Horizontal,
                    2 => NametableMirror::Lower,
                    3 => NametableMirror::Higher,
                    _ => unreachable!(),
                };
            }
            0xD => {
                self.irq_pending = false;
                self.irq_counter_enabled = (value & 0x80) != 0;
                self.irq_enabled = (value & 0x01) != 0;
            }
            0xE => {
                self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
            }
            0xF => {
                self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
            }
            _ => {}
        }
    }
}

impl MemoryMapper for SunsoftFme7Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => {
                let v = self.work_ram_value;
                if v & 0x40 != 0 {
                    if v & 0x80 != 0 {
                        self.prg_ram[(addr - 0x6000) as usize]
                    } else {
                        0 // RAM disabled
                    }
                } else {
                    let bank = (v as usize & 0x3F) % self.prg_rom.len().max(1);
                    self.prg_rom
                        .get(bank)
                        .map_or(0, |b| b[(addr - 0x6000) as usize])
                }
            }
            0x8000..=0x9FFF => {
                let bank = self.prg_bank_index(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xA000..=0xBFFF => {
                let bank = self.prg_bank_index(2);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xA000) as usize])
            }
            0xC000..=0xDFFF => {
                let bank = self.prg_bank_index(3);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xC000) as usize])
            }
            0xE000..=0xFFFF => {
                let bank = self.prg_rom.len().saturating_sub(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xE000) as usize])
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF => {
                let v = self.work_ram_value;
                if (v & 0x40 != 0) && (v & 0x80 != 0) {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0x9FFF => self.command = value & 0x0F,
            0xA000..=0xBFFF => self.execute_command(value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_banks[slot] as usize % self.chr_rom.len().max(1);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize]
            }
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - 0x3F00) % 32;
                if idx & 0x13 == 0x10 {
                    idx &= !0x10;
                }
                self.palette_ram[idx]
            }
            _ => 0,
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_banks[slot] as usize % self.chr_rom.len().max(1);
                if let Some(b) = self.chr_rom.get(bank) {
                    let offset = addr as usize & 0x3FF;
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored & 0x7FF) as usize;
                let copy_size = size.min(0x800 - vram_addr);
                unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {}
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize] = value;
            }
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - 0x3F00) % 32;
                if idx & 0x13 == 0x10 {
                    idx &= !0x10;
                }
                self.palette_ram[idx] = value;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if self.irq_counter_enabled {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
            if self.irq_counter == 0xFFFF && self.irq_enabled {
                self.irq_pending = true;
            }
        }
    }

    fn code_start(&mut self) -> u16 {
        let lo = self.cpu_read(RESET_TARGET_ADDR);
        let hi = self.cpu_read(RESET_TARGET_ADDR + 1);
        ((hi as u16) << 8) | lo as u16
    }

    fn controllers(&mut self) -> &mut [io::controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        self.irq_pending
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&self.prg_ram[..])
        } else {
            None
        }
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        if self.has_battery {
            Some(&mut self.prg_ram[..])
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        69
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        for &b in &self.prg_banks {
            w.write_u8(b);
        }
        w.write_u8(self.command);
        w.write_u8(self.work_ram_value);
        w.write_u16(self.irq_counter);
        w.write_bool(self.irq_counter_enabled);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_pending);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        for b in &mut self.prg_banks {
            *b = r.read_u8()?;
        }
        self.command = r.read_u8()?;
        self.work_ram_value = r.read_u8()?;
        self.irq_counter = r.read_u16()?;
        self.irq_counter_enabled = r.read_bool()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}
