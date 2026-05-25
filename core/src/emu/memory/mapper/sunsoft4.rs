use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE: usize = 0x0800; // 2KB
const NT_BANK_SIZE: usize = 0x0400; // 1KB

pub struct Sunsoft4Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    chr_rom_1k: Vec<[u8; NT_BANK_SIZE]>,
    prg_ram: Box<[u8; PRG_RAM_8K]>,
    prg_ram_enabled: bool,
    has_battery: bool,

    chr_banks: [u8; 4],
    nt_banks: [u8; 2],
    prg_bank: u8,
    mirroring: u8,
    use_chr_nametables: bool,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Sunsoft4Mapper {
    pub fn new(
        flags: u8,
        prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        let mut chr_rom = vec![];
        let mut chr_rom_1k = vec![];
        for bank in &chr_banks_8k {
            for i in 0..4 {
                chr_rom.push(
                    <[u8; CHR_BANK_SIZE]>::try_from(
                        &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                    )
                    .unwrap(),
                );
            }
            for i in 0..8 {
                chr_rom_1k.push(
                    <[u8; NT_BANK_SIZE]>::try_from(&bank[i * NT_BANK_SIZE..(i + 1) * NT_BANK_SIZE])
                        .unwrap(),
                );
            }
        }

        let mirroring_mode = if flags & 1 != 0 { 0 } else { 1 };

        Sunsoft4Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            chr_rom_1k,
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            prg_ram_enabled: false,
            has_battery,
            chr_banks: [0; 4],
            nt_banks: [0x80; 2],
            prg_bank: 0,
            mirroring: mirroring_mode,
            use_chr_nametables: false,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn mirroring_mode(&self) -> NametableMirror {
        match self.mirroring & 3 {
            0 => NametableMirror::Vertical,
            1 => NametableMirror::Horizontal,
            2 => NametableMirror::Lower,
            3 => NametableMirror::Higher,
            _ => unreachable!(),
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let region = (addr >> 11) as usize & 3;
        let bank = self.chr_banks[region] as usize;
        let bank_idx = bank % self.chr_rom.len().max(1);
        let offset = addr as usize & 0x7FF;
        self.chr_rom.get(bank_idx).map_or(0, |b| b[offset])
    }

    fn read_nametable(&self, addr: u16) -> u8 {
        if self.use_chr_nametables {
            let nt = (addr >> 10) & 3;
            let nt_reg = match self.mirroring & 3 {
                0 => {
                    if nt & 1 == 0 {
                        0
                    } else {
                        1
                    }
                } // vertical
                1 => {
                    if nt < 2 {
                        0
                    } else {
                        1
                    }
                } // horizontal
                2 => 0, // single low
                3 => 1, // single high
                _ => unreachable!(),
            };
            let bank = self.nt_banks[nt_reg] as usize;
            let bank_idx = bank % self.chr_rom_1k.len().max(1);
            let offset = addr as usize & 0x3FF;
            self.chr_rom_1k.get(bank_idx).map_or(0, |b| b[offset])
        } else {
            let mirrored = mirror_nametable_addr(addr, self.mirroring_mode());
            self.vram[(mirrored & 0x7FF) as usize]
        }
    }

    fn write_nametable(&mut self, addr: u16, value: u8) {
        if !self.use_chr_nametables {
            let mirrored = mirror_nametable_addr(addr, self.mirroring_mode());
            self.vram[(mirrored & 0x7FF) as usize] = value;
        }
    }
}

impl MemoryMapper for Sunsoft4Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_rom.len().max(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xC000..=0xFFFF => {
                let bank = self.prg_rom.len().saturating_sub(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xC000) as usize])
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0x8FFF => self.chr_banks[0] = value,
            0x9000..=0x9FFF => self.chr_banks[1] = value,
            0xA000..=0xAFFF => self.chr_banks[2] = value,
            0xB000..=0xBFFF => self.chr_banks[3] = value,
            0xC000..=0xCFFF => self.nt_banks[0] = value | 0x80,
            0xD000..=0xDFFF => self.nt_banks[1] = value | 0x80,
            0xE000..=0xEFFF => {
                self.mirroring = value & 3;
                self.use_chr_nametables = (value & 0x10) != 0;
            }
            0xF000..=0xFFFF => {
                self.prg_bank = value & 0x0F;
                self.prg_ram_enabled = (value & 0x10) != 0;
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.read_chr(addr),
            0x2000..=0x3EFF => self.read_nametable(addr),
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
                if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                    idx &= !PALETTE_MIRROR_CLEAR;
                }
                self.palette_ram[idx]
            }
            _ => 0,
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let region = (addr >> 11) as usize & 3;
                let bank = self.chr_banks[region] as usize % self.chr_rom.len().max(1);
                if let Some(b) = self.chr_rom.get(bank) {
                    let offset = addr as usize & 0x7FF;
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
            0x2000..=0x3EFF => {
                if !self.use_chr_nametables {
                    let mirrored = mirror_nametable_addr(addr, self.mirroring_mode());
                    let vram_addr = (mirrored & 0x7FF) as usize;
                    let copy_size = size.min(VRAM_SIZE as usize - vram_addr);
                    unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
                }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {}
            0x2000..=0x3EFF => self.write_nametable(addr, value),
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
                if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                    idx &= !PALETTE_MIRROR_CLEAR;
                }
                self.palette_ram[idx] = value;
            }
            _ => {}
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
        false
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
        68
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        w.write_u8(self.nt_banks[0]);
        w.write_u8(self.nt_banks[1]);
        w.write_u8(self.prg_bank);
        w.write_u8(self.mirroring);
        w.write_bool(self.use_chr_nametables);
        w.write_bool(self.prg_ram_enabled);
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
        self.nt_banks[0] = r.read_u8()?;
        self.nt_banks[1] = r.read_u8()?;
        self.prg_bank = r.read_u8()?;
        self.mirroring = r.read_u8()?;
        self.use_chr_nametables = r.read_bool()?;
        self.prg_ram_enabled = r.read_bool()?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}
