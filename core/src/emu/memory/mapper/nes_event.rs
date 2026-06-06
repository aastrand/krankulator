use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x4000; // 16KB

pub struct NesEventMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_ram: Box<[u8; PRG_RAM_8K]>,
    wram: Box<[u8; PRG_RAM_8K]>,
    wram_enabled: bool,

    shift_register: u8,
    shift_count: u8,

    reg0: u8, // control: mirroring, prg mode
    reg1: u8, // chr0: repurposed for I, O, AA bits
    reg3: u8, // prg: WBBBB

    init_state: u8,

    irq_counter: u32,
    irq_enabled: bool,
    irq_pending: bool,

    mirroring: NametableMirror,
    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl NesEventMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_BANK_SIZE]>) -> Self {
        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        NesEventMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom: prg_banks,
            chr_ram: Box::new([0; PRG_RAM_8K]),
            wram: Box::new([0; PRG_RAM_8K]),
            wram_enabled: true,
            shift_register: 0b10000,
            shift_count: 0,
            reg0: 0x0C,
            reg1: 0x10, // I bit set on init
            reg3: 0,
            init_state: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            mirroring,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn update_state(&mut self) {
        let i_bit = (self.reg1 & 0x10) != 0;

        match self.init_state {
            0 => {
                if !i_bit {
                    self.init_state = 1;
                }
            }
            1 => {
                if i_bit {
                    self.init_state = 2;
                }
            }
            _ => {}
        }

        if i_bit {
            self.irq_counter = 0;
            self.irq_enabled = false;
            self.irq_pending = false;
        } else {
            self.irq_enabled = true;
        }

        self.mirroring = match self.reg0 & 3 {
            0 => NametableMirror::Lower,
            1 => NametableMirror::Higher,
            2 => NametableMirror::Vertical,
            3 => NametableMirror::Horizontal,
            _ => unreachable!(),
        };

        self.wram_enabled = (self.reg3 & 0x10) == 0;
    }

    fn map_prg_read(&self, addr: u16) -> u8 {
        if self.init_state < 2 {
            // Locked: first 32KB (banks 0+1)
            let offset = (addr - 0x8000) as usize;
            if offset < PRG_BANK_SIZE {
                return self.prg_rom.first().map_or(0, |b| b[offset]);
            } else {
                return self.prg_rom.get(1).map_or(0, |b| b[offset - PRG_BANK_SIZE]);
            }
        }

        let o_bit = (self.reg1 & 0x08) != 0;
        let prg_mode = (self.reg0 >> 2) & 3;

        if !o_bit {
            // Chip 1 (banks 0-7), 32KB mode using AA bits
            let base = self.reg1 as usize & 0x06;
            let offset = (addr - 0x8000) as usize;
            if offset < PRG_BANK_SIZE {
                return self.prg_rom.get(base).map_or(0, |b| b[offset]);
            } else {
                return self
                    .prg_rom
                    .get(base + 1)
                    .map_or(0, |b| b[offset - PRG_BANK_SIZE]);
            }
        }

        // Chip 2 (banks 8-15), use MMC1 PRG mode
        let prg_reg = (self.reg3 as usize & 0x07) | 0x08;
        let last_bank = self.prg_rom.len().saturating_sub(1);
        let first_chip2 = 8usize;

        match addr {
            0x8000..=0xBFFF => {
                let bank = match prg_mode {
                    0 | 1 => prg_reg & !1, // 32KB mode, even-aligned
                    2 => first_chip2,      // fixed first bank of chip 2
                    3 => prg_reg,          // switchable
                    _ => unreachable!(),
                };
                let bank = bank.min(last_bank);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xC000..=0xFFFF => {
                let bank = match prg_mode {
                    0 | 1 => (prg_reg & !1) + 1, // 32KB mode, odd half
                    2 => prg_reg,                // switchable
                    3 => last_bank,              // fixed last bank
                    _ => unreachable!(),
                };
                let bank = bank.min(last_bank);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xC000) as usize])
            }
            _ => 0,
        }
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            self.shift_register = 0b10000;
            self.shift_count = 0;
            self.reg0 |= 0x0C;
            self.update_state();
            return;
        }

        self.shift_register >>= 1;
        self.shift_register |= (value & 1) << 4;
        self.shift_count += 1;

        if self.shift_count == 5 {
            let data = self.shift_register;
            match (addr >> 13) & 3 {
                0 => self.reg0 = data,
                1 => self.reg1 = data,
                2 => {} // reg2 unused
                3 => self.reg3 = data,
                _ => unreachable!(),
            }
            self.shift_register = 0b10000;
            self.shift_count = 0;
            self.update_state();
        }
    }
}

impl MemoryMapper for NesEventMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => {
                if self.wram_enabled {
                    self.wram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x8000..=0xFFFF => self.map_prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF => {
                if self.wram_enabled {
                    self.wram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0xFFFF => self.write_register(addr, value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_ram[addr as usize],
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize]
            }
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

    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => unsafe {
                std::ptr::copy(self.chr_ram.as_ptr().add(addr as usize), dest, size);
            },
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored & 0x7FF) as usize;
                let copy_size = size.min(VRAM_SIZE as usize - vram_addr);
                unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.chr_ram[addr as usize] = value,
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize] = value;
            }
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

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if self.irq_enabled {
            self.irq_counter += 1;
            // DIP switch threshold — use tournament default (0x20000000)
            if self.irq_counter >= 0x20000000 {
                self.irq_pending = true;
                self.irq_enabled = false;
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

    fn mapper_id(&self) -> u8 {
        105
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.wram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.shift_register);
        w.write_u8(self.shift_count);
        w.write_u8(self.reg0);
        w.write_u8(self.reg1);
        w.write_u8(self.reg3);
        w.write_u8(self.init_state);
        w.write_u32(self.irq_counter);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_pending);
        w.write_bool(self.wram_enabled);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.wram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.shift_register = r.read_u8()?;
        self.shift_count = r.read_u8()?;
        self.reg0 = r.read_u8()?;
        self.reg1 = r.read_u8()?;
        self.reg3 = r.read_u8()?;
        self.init_state = r.read_u8()?;
        self.irq_counter = r.read_u32()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        self.wram_enabled = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}
