use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

#[derive(Copy, Clone, Debug, PartialEq)]
enum Variant {
    Vrc2a, // mapper 22: A0=A1, A1=A0, CHR right-shifted
    Vrc2b, // mapper 23 sub 3: A0=A0, A1=A1
    Vrc2c, // mapper 25 sub 3: A0=A1, A1=A0
    Vrc4a, // mapper 21 sub 1: A0=A1, A1=A2
    Vrc4b, // mapper 25 sub 1: A0=A1, A1=A0
    Vrc4c, // mapper 21 sub 2: A0=A6, A1=A7
    Vrc4d, // mapper 25 sub 2: A0=A3, A1=A2
    Vrc4e, // mapper 23 sub 2: A0=A2, A1=A3
    Vrc4f, // mapper 23 sub 1: A0=A0, A1=A1
}

impl Variant {
    fn is_vrc2(self) -> bool {
        matches!(self, Variant::Vrc2a | Variant::Vrc2b | Variant::Vrc2c)
    }

    fn decode_reg(self, addr: u16) -> u8 {
        let (a0_bit, a1_bit) = match self {
            Variant::Vrc2a | Variant::Vrc2c | Variant::Vrc4b => (1, 0),
            Variant::Vrc2b | Variant::Vrc4f => (0, 1),
            Variant::Vrc4a => (1, 2),
            Variant::Vrc4c => (6, 7),
            Variant::Vrc4d => (3, 2),
            Variant::Vrc4e => (2, 3),
        };
        (((addr >> a0_bit) & 1) | (((addr >> a1_bit) & 1) << 1)) as u8
    }

    fn mapper_id(self) -> u8 {
        match self {
            Variant::Vrc4a | Variant::Vrc4c => 21,
            Variant::Vrc2a => 22,
            Variant::Vrc2b | Variant::Vrc4e | Variant::Vrc4f => 23,
            Variant::Vrc2c | Variant::Vrc4b | Variant::Vrc4d => 25,
        }
    }
}

pub struct Vrc2_4Mapper {
    controllers: [io::controller::Controller; 2],
    variant: Variant,

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    chr_is_ram: bool,
    prg_ram: Box<[u8; PRG_RAM_8K]>,
    has_battery: bool,

    prg_bank_0: u8,
    prg_bank_1: u8,
    prg_swap_mode: bool,
    wram_enable: bool,

    chr_lo: [u8; 8],
    chr_hi: [u8; 8],
    mirroring: NametableMirror,

    // VRC2 $6000 latch
    vrc2_latch: u8,

    // VRC4 IRQ
    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_mode_cycle: bool,
    irq_enable: bool,
    irq_enable_after_ack: bool,
    irq_pending: bool,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Vrc2_4Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
        variant: u8,
        submapper: u8,
    ) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let chr_is_ram = chr_banks_8k.is_empty();
        let mut chr_rom: Vec<[u8; CHR_BANK_SIZE]> = vec![];
        if chr_is_ram {
            for _ in 0..8 {
                chr_rom.push([0; CHR_BANK_SIZE]);
            }
        } else {
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
        }

        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        let v = resolve_variant(variant, submapper);

        Vrc2_4Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            variant: v,
            prg_rom,
            chr_rom,
            chr_is_ram,
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            has_battery,
            prg_bank_0: 0,
            prg_bank_1: 0,
            prg_swap_mode: false,
            wram_enable: false,
            chr_lo: [0; 8],
            chr_hi: [0; 8],
            mirroring,
            vrc2_latch: 0,
            irq_latch: 0,
            irq_counter: 0,
            irq_prescaler: 341,
            irq_mode_cycle: false,
            irq_enable: false,
            irq_enable_after_ack: false,
            irq_pending: false,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn prg_index(&self, bank: u8) -> usize {
        (bank as usize & 0x1F) % self.prg_rom.len().max(1)
    }

    fn chr_bank_number(&self, slot: usize) -> usize {
        let combined = ((self.chr_hi[slot] as u16 & 0x1F) << 4) | (self.chr_lo[slot] as u16 & 0x0F);
        let val = if self.variant == Variant::Vrc2a {
            (combined >> 1) as usize
        } else {
            combined as usize
        };
        val % self.chr_rom.len().max(1)
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_pending = true;
        } else {
            self.irq_counter += 1;
        }
    }

    fn handle_write(&mut self, addr: u16, value: u8) {
        let base = addr & 0xF000;
        let reg = self.variant.decode_reg(addr);

        match (base, reg) {
            (0x8000, _) => self.prg_bank_0 = value & 0x1F,
            (0xA000, _) => self.prg_bank_1 = value & 0x1F,

            (0x9000, 0 | 1) if self.variant.is_vrc2() => {
                self.mirroring = if value & 1 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
            }
            (0x9000, 0 | 1) => {
                self.mirroring = match value & 3 {
                    0 => NametableMirror::Vertical,
                    1 => NametableMirror::Horizontal,
                    2 => NametableMirror::Lower,
                    3 => NametableMirror::Higher,
                    _ => unreachable!(),
                };
            }
            (0x9000, 2 | 3) if !self.variant.is_vrc2() => {
                self.prg_swap_mode = (value & 0x02) != 0;
                self.wram_enable = (value & 0x01) != 0;
            }

            (0xB000 | 0xC000 | 0xD000 | 0xE000, r) => {
                let base_slot = ((base - 0xB000) >> 11) as usize;
                let slot = base_slot + (r as usize >> 1);
                if slot < 8 {
                    if r & 1 == 0 {
                        self.chr_lo[slot] = value & 0x0F;
                    } else {
                        self.chr_hi[slot] = value & 0x1F;
                    }
                }
            }

            (0xF000, 0) if !self.variant.is_vrc2() => {
                self.irq_latch = (self.irq_latch & 0xF0) | (value & 0x0F);
            }
            (0xF000, 1) if !self.variant.is_vrc2() => {
                self.irq_latch = (self.irq_latch & 0x0F) | ((value & 0x0F) << 4);
            }
            (0xF000, 2) if !self.variant.is_vrc2() => {
                self.irq_pending = false;
                self.irq_enable_after_ack = (value & 0x01) != 0;
                self.irq_enable = (value & 0x02) != 0;
                self.irq_mode_cycle = (value & 0x04) != 0;
                if self.irq_enable {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            (0xF000, 3) if !self.variant.is_vrc2() => {
                self.irq_pending = false;
                self.irq_enable = self.irq_enable_after_ack;
            }

            _ => {}
        }
    }
}

fn resolve_variant(mapper_id: u8, submapper: u8) -> Variant {
    match (mapper_id, submapper) {
        (22, _) => Variant::Vrc2a,
        (23, 3) => Variant::Vrc2b,
        (25, 3) => Variant::Vrc2c,
        (21, 1) => Variant::Vrc4a,
        (21, 2) => Variant::Vrc4c,
        (25, 1) => Variant::Vrc4b,
        (25, 2) => Variant::Vrc4d,
        (23, 2) => Variant::Vrc4e,
        (23, 1) => Variant::Vrc4f,
        // Without submapper info, pick a common default
        (21, _) => Variant::Vrc4a,
        (23, _) => Variant::Vrc4e,
        (25, _) => Variant::Vrc4b,
        _ => Variant::Vrc4e,
    }
}

impl MemoryMapper for Vrc2_4Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => {
                if self.variant.is_vrc2() {
                    self.vrc2_latch & 0x01
                } else if self.wram_enable {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x8000..=0x9FFF => {
                let bank = if self.prg_swap_mode {
                    self.prg_rom.len().saturating_sub(2)
                } else {
                    self.prg_index(self.prg_bank_0)
                };
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xA000..=0xBFFF => {
                let bank = self.prg_index(self.prg_bank_1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xA000) as usize])
            }
            0xC000..=0xDFFF => {
                let bank = if self.prg_swap_mode {
                    self.prg_index(self.prg_bank_0)
                } else {
                    self.prg_rom.len().saturating_sub(2)
                };
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
                if self.variant.is_vrc2() {
                    self.vrc2_latch = value & 0x01;
                } else if self.wram_enable {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0xFFFF => self.handle_write(addr, value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_bank_number(slot);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
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
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_bank_number(slot);
                if let Some(b) = self.chr_rom.get(bank) {
                    let offset = addr as usize & 0x3FF;
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
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
            0x0000..=0x1FFF => {
                if self.chr_is_ram {
                    let slot = (addr >> 10) as usize & 7;
                    let bank = self.chr_bank_number(slot);
                    if let Some(b) = self.chr_rom.get_mut(bank) {
                        b[addr as usize & 0x3FF] = value;
                    }
                }
            }
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
        if !self.irq_enable || self.variant.is_vrc2() {
            return;
        }
        if self.irq_mode_cycle {
            self.clock_irq_counter();
        } else {
            self.irq_prescaler -= 3;
            if self.irq_prescaler <= 0 {
                self.irq_prescaler += 341;
                self.clock_irq_counter();
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
        if self.has_battery && !self.variant.is_vrc2() {
            Some(&self.prg_ram[..])
        } else {
            None
        }
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        if self.has_battery && !self.variant.is_vrc2() {
            Some(&mut self.prg_ram[..])
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        self.variant.mapper_id()
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for v in &self.chr_rom {
            w.write_bytes(v);
        }
        for &b in &self.chr_lo {
            w.write_u8(b);
        }
        for &b in &self.chr_hi {
            w.write_u8(b);
        }
        w.write_u8(self.prg_bank_0);
        w.write_u8(self.prg_bank_1);
        w.write_bool(self.prg_swap_mode);
        w.write_bool(self.wram_enable);
        w.write_u8(self.vrc2_latch);
        w.write_u8(self.irq_latch);
        w.write_u8(self.irq_counter);
        w.write_u16(self.irq_prescaler as u16);
        w.write_bool(self.irq_mode_cycle);
        w.write_bool(self.irq_enable);
        w.write_bool(self.irq_enable_after_ack);
        w.write_bool(self.irq_pending);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for v in &mut self.chr_rom {
            r.read_bytes_into(v)?;
        }
        for b in &mut self.chr_lo {
            *b = r.read_u8()?;
        }
        for b in &mut self.chr_hi {
            *b = r.read_u8()?;
        }
        self.prg_bank_0 = r.read_u8()?;
        self.prg_bank_1 = r.read_u8()?;
        self.prg_swap_mode = r.read_bool()?;
        self.wram_enable = r.read_bool()?;
        self.vrc2_latch = r.read_u8()?;
        self.irq_latch = r.read_u8()?;
        self.irq_counter = r.read_u8()?;
        self.irq_prescaler = r.read_u16()? as i16;
        self.irq_mode_cycle = r.read_bool()?;
        self.irq_enable = r.read_bool()?;
        self.irq_enable_after_ack = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(
        mapper_id: u8,
        submapper: u8,
        prg_count: usize,
        chr_count: usize,
    ) -> Vrc2_4Mapper {
        let mut prg_banks = vec![];
        for i in 0..prg_count {
            let mut bank = [0u8; 16384];
            let lo = (i * 2) as u8;
            bank[0] = lo;
            bank[PRG_BANK_SIZE] = lo + 1;
            prg_banks.push(bank);
        }
        let mut chr_banks = vec![];
        for i in 0..chr_count {
            let mut bank = [0u8; 8192];
            for k in 0..8 {
                bank[k * CHR_BANK_SIZE] = (i * 8 + k) as u8;
            }
            chr_banks.push(bank);
        }
        Vrc2_4Mapper::new(1, prg_banks, chr_banks, false, None, mapper_id, submapper)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(23, 2, 8, 4);
        // 8 x 16KB = 16 x 8KB banks; byte[0] of 8KB bank N = N
        assert_eq!(m.cpu_read(0x8000), 0); // bank 0
        assert_eq!(m.cpu_read(0xE000), 15); // last bank fixed
        m.cpu_write(0x8000, 3);
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xE000), 15);
    }

    #[test]
    fn test_prg_swap_mode() {
        let mut m = make_mapper(23, 2, 8, 4);
        m.cpu_write(0x8000, 5);
        assert_eq!(m.cpu_read(0x8000), 5);
        // Enable swap mode via VRC4e reg 2: A0=bit2=0, A1=bit3=1 => $9008
        m.cpu_write(0x9008, 0x02);
        assert_eq!(m.cpu_read(0x8000), 14); // second-to-last
        assert_eq!(m.cpu_read(0xC000), 5);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(23, 2, 4, 4);
        // Set CHR slot 0 to bank 5: lo=$B000 reg0, hi=$B000 reg1
        // VRC4e: reg0 => bit2=0,bit3=0 => $B000; reg1 => bit2=1,bit3=0 => $B004
        m.cpu_write(0xB000, 5);
        m.cpu_write(0xB004, 0);
        assert_eq!(m.chr_bank_number(0), 5);
    }

    #[test]
    fn test_vrc2a_chr_shift() {
        let mut m = make_mapper(22, 0, 4, 4);
        // VRC2a right-shifts CHR bank number
        m.cpu_write(0xB000, 0x06); // lo = 6
        m.cpu_write(0xB002, 0x00); // hi = 0 (VRC2a: A0=A1,A1=A0, so reg1 = addr with bit1=1,bit0=0)
                                   // combined = 6, shifted = 3
        assert_eq!(m.chr_bank_number(0), 3);
    }

    #[test]
    fn test_mirroring() {
        let mut m = make_mapper(23, 2, 4, 4);
        // VRC4: write to $9000 reg 0 (bit2=0,bit3=0 => $9000)
        m.cpu_write(0x9000, 0); // vertical
        assert_eq!(m.mirroring, NametableMirror::Vertical);
        m.cpu_write(0x9000, 1); // horizontal
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
        m.cpu_write(0x9000, 2); // lower
        assert_eq!(m.mirroring, NametableMirror::Lower);
        m.cpu_write(0x9000, 3); // higher
        assert_eq!(m.mirroring, NametableMirror::Higher);
    }

    #[test]
    fn test_vrc2_mirroring_ignores_bit1() {
        let mut m = make_mapper(22, 0, 4, 4);
        // VRC2a: A0=A1, A1=A0. reg 0 => bit1=0,bit0=0 => $9000
        m.cpu_write(0x9000, 3); // bit 1 should be ignored, only bit 0 matters
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
    }

    #[test]
    fn test_irq_cycle_mode() {
        let mut m = make_mapper(23, 2, 4, 4);
        // Set latch to 0xFE
        m.cpu_write(0xF000, 0x0E); // lo = E
        m.cpu_write(0xF004, 0x0F); // hi = F => latch = 0xFE
                                   // Enable IRQ in cycle mode
                                   // reg 2 => bit2=0,bit3=1 => $F008
        m.cpu_write(0xF008, 0x06); // E=1, M=1 (cycle mode)
        assert_eq!(m.irq_counter, 0xFE);
        assert!(!m.irq_pending);
        // Clock once: counter goes to 0xFF
        m.cpu_cycle(0);
        assert_eq!(m.irq_counter, 0xFF);
        assert!(!m.irq_pending);
        // Clock again: counter wraps, IRQ fires
        m.cpu_cycle(0);
        assert_eq!(m.irq_counter, 0xFE); // reloaded from latch
        assert!(m.irq_pending);
    }

    #[test]
    fn test_irq_acknowledge() {
        let mut m = make_mapper(23, 2, 4, 4);
        m.irq_pending = true;
        m.irq_enable_after_ack = true;
        // Acknowledge: reg 3 => bit2=1,bit3=1 => $F00C
        m.cpu_write(0xF00C, 0);
        assert!(!m.irq_pending);
        assert!(m.irq_enable); // copied from enable_after_ack
    }

    #[test]
    fn test_vrc2_no_irq() {
        let mut m = make_mapper(22, 0, 4, 4);
        // VRC2 should not have IRQ
        m.irq_enable = true;
        m.cpu_cycle(0);
        assert!(!m.irq_pending);
    }

    #[test]
    fn test_vrc2_latch() {
        let mut m = make_mapper(22, 0, 4, 4);
        m.cpu_write(0x6000, 0xFF);
        assert_eq!(m.cpu_read(0x6000), 1); // only bit 0
        m.cpu_write(0x6000, 0x00);
        assert_eq!(m.cpu_read(0x6000), 0);
    }

    #[test]
    fn test_variant_resolution() {
        assert_eq!(resolve_variant(22, 0), Variant::Vrc2a);
        assert_eq!(resolve_variant(23, 3), Variant::Vrc2b);
        assert_eq!(resolve_variant(25, 3), Variant::Vrc2c);
        assert_eq!(resolve_variant(21, 1), Variant::Vrc4a);
        assert_eq!(resolve_variant(21, 2), Variant::Vrc4c);
        assert_eq!(resolve_variant(25, 1), Variant::Vrc4b);
        assert_eq!(resolve_variant(25, 2), Variant::Vrc4d);
        assert_eq!(resolve_variant(23, 2), Variant::Vrc4e);
        assert_eq!(resolve_variant(23, 1), Variant::Vrc4f);
        // Defaults without submapper
        assert_eq!(resolve_variant(21, 0), Variant::Vrc4a);
        assert_eq!(resolve_variant(23, 0), Variant::Vrc4e);
        assert_eq!(resolve_variant(25, 0), Variant::Vrc4b);
    }
}
