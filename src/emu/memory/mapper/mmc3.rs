use super::super::super::io;
use super::{mirror_nametable_addr, NametableMirror, RESET_TARGET_ADDR};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct MMC3Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_mem: Vec<[u8; CHR_BANK_SIZE]>,
    chr_is_ram: bool,
    prg_ram: Box<[u8; 0x2000]>,
    has_battery: bool,

    prg_bank_mode: u8,
    chr_bank_mode: u8,
    bank_select: u8,
    bank_regs: [u8; 8],

    // IRQ counter state
    irq_counter: u8,
    irq_latch: u8,
    irq_enable: bool,
    irq_reload: bool,
    irq_pending: bool,
    irq_pending_since_dot: u64,

    // A12 tracking for IRQ
    a12_state: bool,
    last_a12_low_dot: u64,

    // Mirroring
    mirroring: NametableMirror,

    // VRAM for nametables
    vram: Box<[u8; 0x800]>,

    // CPU RAM (0x0000-0x07FF, mirrored to 0x1FFF)
    cpu_ram: Box<[u8; 0x800]>,

    // Palette RAM for colors
    palette_ram: [u8; 32],

    // iNES 2.0 submapper (0 = standard MMC3, 1 = MMC6, etc.)
    submapper: u8,
}

impl MMC3Mapper {
    pub fn new(
        flags: u8,
        prg_banks: Vec<[u8; 16384]>,
        chr_banks: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
        submapper: u8,
    ) -> MMC3Mapper {
        // Flatten PRG/CHR banks into 8K/1K chunks
        let mut prg_rom = vec![];
        for (_i, bank) in prg_banks.iter().enumerate() {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
            // println!("MMC3: Split 16KB PRG bank {} into two 8KB banks ({} and {})",
            //          i, prg_rom.len() - 2, prg_rom.len() - 1);
        }

        if prg_rom.is_empty() {
            panic!("MMC3: No PRG banks loaded!");
        }

        let (chr_mem, chr_is_ram) = if chr_banks.is_empty() {
            (vec![[0; CHR_BANK_SIZE]; 8], true)
        } else {
            let mut chr_mem = vec![];
            for bank in chr_banks {
                for i in 0..8 {
                    chr_mem.push(
                        <[u8; CHR_BANK_SIZE]>::try_from(
                            &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                        )
                        .unwrap(),
                    );
                }
            }
            (chr_mem, false)
        };
        // Creating new MMC3Mapper instance
        MMC3Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_mem,
            chr_is_ram,
            prg_ram: {
                let mut ram = Box::new([0; 0x2000]);
                if let Some(data) = sram_data {
                    let len = data.len().min(0x2000);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            has_battery,
            prg_bank_mode: 0,
            chr_bank_mode: 0,
            bank_select: 0,
            // Initialize bank registers to provide better sprite data
            bank_regs: [0, 2, 4, 5, 6, 7, 0, 1],
            irq_counter: 0,
            irq_latch: 0,
            irq_enable: false,
            irq_reload: false,
            irq_pending: false,
            irq_pending_since_dot: 0,
            a12_state: false,
            last_a12_low_dot: 0,
            mirroring: if flags & 1 == 0 {
                NametableMirror::Vertical
            } else {
                NametableMirror::Horizontal
            },
            vram: Box::new([0; 0x800]),
            cpu_ram: Box::new([0; 0x800]),
            palette_ram: [0x0F; 32],
            submapper,
        }
    }

    fn get_prg_bank(&self, register: usize) -> usize {
        let banks = self.prg_rom.len();
        (self.bank_regs[register] as usize) % banks
    }

    fn get_chr_bank(&self, register: usize) -> usize {
        if self.chr_mem.is_empty() {
            return 0;
        }
        let banks = self.chr_mem.len();
        (self.bank_regs[register] as usize) % banks
    }

    fn map_prg(&self, addr: u16) -> Option<&[u8; PRG_BANK_SIZE]> {
        let bank = match addr {
            0x8000..=0x9FFF => {
                if self.prg_bank_mode == 0 {
                    // Mode 0: R6 at $8000-$9FFF
                    self.get_prg_bank(6)
                } else {
                    // Mode 1: Fixed second-to-last bank at $8000-$9FFF
                    self.prg_rom.len().saturating_sub(2)
                }
            }
            0xA000..=0xBFFF => {
                // R7 always controls $A000-$BFFF
                self.get_prg_bank(7)
            }
            0xC000..=0xDFFF => {
                if self.prg_bank_mode == 0 {
                    // Mode 0: Fixed second-to-last bank at $C000-$DFFF
                    if self.prg_rom.len() >= 2 {
                        self.prg_rom.len() - 2
                    } else {
                        0
                    }
                } else {
                    // Mode 1: R6 at $C000-$DFFF
                    self.get_prg_bank(6)
                }
            }
            0xE000..=0xFFFF => {
                // Always fixed to last bank
                if self.prg_rom.len() >= 1 {
                    self.prg_rom.len() - 1
                } else {
                    0
                }
            }
            _ => return None,
        };

        self.prg_rom.get(bank)
    }

    fn map_chr(&self, addr: u16) -> (usize, usize) {
        let bank_idx = match addr {
            0x0000..=0x03FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 2KB banks at $0000 and $0800
                    self.get_chr_bank(0) & 0xFE
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(2)
                }
            }
            0x0400..=0x07FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: Second half of 2KB bank
                    (self.get_chr_bank(0) & 0xFE) + 1
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(3)
                }
            }
            0x0800..=0x0BFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 2KB banks
                    self.get_chr_bank(1) & 0xFE
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(4)
                }
            }
            0x0C00..=0x0FFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: Second half of 2KB bank
                    (self.get_chr_bank(1) & 0xFE) + 1
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(5)
                }
            }
            0x1000..=0x13FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(2)
                } else {
                    // Mode 1: 2KB banks
                    self.get_chr_bank(0) & 0xFE
                }
            }
            0x1400..=0x17FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(3)
                } else {
                    // Mode 1: Second half of 2KB bank
                    (self.get_chr_bank(0) & 0xFE) + 1
                }
            }
            0x1800..=0x1BFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(4)
                } else {
                    // Mode 1: 2KB banks
                    self.get_chr_bank(1) & 0xFE
                }
            }
            0x1C00..=0x1FFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(5)
                } else {
                    // Mode 1: Second half of 2KB bank
                    (self.get_chr_bank(1) & 0xFE) + 1
                }
            }
            _ => 0,
        };

        let offset = addr as usize % CHR_BANK_SIZE;
        (bank_idx, offset)
    }

    fn check_a12_transition(&mut self, addr: u16, dot: u64) {
        let current_a12 = (addr & 0x1000) != 0;

        if current_a12 {
            if self.last_a12_low_dot > 0 && dot.saturating_sub(self.last_a12_low_dot) >= 16 {
                self.clock_irq_counter(dot);
            }
            self.last_a12_low_dot = 0;
        } else if self.last_a12_low_dot == 0 {
            self.last_a12_low_dot = dot;
        }
    }

    fn clock_irq_counter(&mut self, dot: u64) {
        let old_counter = self.irq_counter;
        let was_reload = self.irq_reload;

        if self.irq_reload || self.irq_counter == 0 {
            self.irq_counter = self.irq_latch;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }

        self.irq_reload = false;

        if self.irq_counter == 0 && self.irq_enable {
            let should_fire = match self.submapper {
                1 => old_counter != 0 || was_reload,
                _ => true,
            };
            if should_fire {
                if !self.irq_pending {
                    self.irq_pending_since_dot = dot;
                }
                self.irq_pending = true;
            }
        }
    }
}

impl MemoryMapper for MMC3Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        match addr {
            0x0000..=0x1FFF => {
                // CPU RAM (mirrored every 0x800 bytes)
                let ram_addr = addr & 0x07FF;
                unsafe { *self.cpu_ram.as_ptr().offset(ram_addr as isize) }
            }
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                if let Some(bank) = self.map_prg(addr) {
                    let value = bank[(addr as usize) % PRG_BANK_SIZE];
                    value
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        match addr {
            0x0000..=0x1FFF => {
                // CPU RAM (mirrored every 0x800 bytes)
                let ram_addr = addr & 0x07FF;
                unsafe {
                    *self.cpu_ram.as_mut_ptr().offset(ram_addr as isize) = value;
                }
            }
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    // Bank select
                    self.bank_select = value & 0x07;
                    self.prg_bank_mode = (value >> 6) & 1;
                    self.chr_bank_mode = (value >> 7) & 1;
                } else {
                    // Bank data - enforce 2KB bank constraints for R0 and R1
                    let reg = self.bank_select as usize;
                    if reg <= 1 {
                        // R0 and R1 are 2KB banks - force even numbers
                        self.bank_regs[reg] = value & 0xFE;
                    } else {
                        // R2-R7 are 1KB banks
                        self.bank_regs[reg] = value;
                    }
                }
            }
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    // Mirroring control
                    self.mirroring = if value & 1 == 0 {
                        NametableMirror::Vertical
                    } else {
                        NametableMirror::Horizontal
                    };
                } else {
                    // PRG RAM protect (ignored for now)
                }
            }
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    // IRQ latch - set the value to reload the counter with
                    //println!("MMC3: Setting IRQ latch to {} (was {})", value, self.irq_latch);
                    self.irq_latch = value;
                } else {
                    // IRQ reload - reload the counter on next tick
                    //println!("MMC3: Setting IRQ reload flag");
                    self.irq_reload = true;
                }
            }
            0xE000..=0xFFFF => {
                if addr & 1 == 0 {
                    self.irq_enable = false;
                    self.irq_pending = false;
                } else {
                    self.irq_enable = true;
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // CHR ROM/RAM access
                let (bank_idx, offset) = self.map_chr(addr);
                if let Some(bank) = self.chr_mem.get(bank_idx) {
                    bank[offset]
                } else {
                    0
                }
            }
            0x2000..=0x3EFF => {
                // Nametable access with mirroring
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                self.vram[vram_addr]
            }
            0x3F00..=0x3FFF => {
                // Palette RAM access with proper mirroring
                let mut palette_addr = (addr as usize - 0x3F00) % 32;
                // NESDev-correct mirroring: $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
                if palette_addr & 0x13 == 0x10 {
                    palette_addr &= !0x10;
                }
                self.palette_ram[palette_addr]
            }
            _ => 0,
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let (bank_idx, offset) = self.map_chr(addr);
                if let Some(bank) = self.chr_mem.get(bank_idx) {
                    let copy_size = std::cmp::min(size, CHR_BANK_SIZE - offset);
                    unsafe {
                        std::ptr::copy(bank.as_ptr().add(offset), dest, copy_size);
                    }
                }
            }
            0x2000..=0x3EFF => {
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                let copy_size = std::cmp::min(size, 0x800 - vram_addr);
                unsafe {
                    std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size);
                }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.chr_is_ram {
                    let (bank_idx, offset) = self.map_chr(addr);
                    if let Some(bank) = self.chr_mem.get_mut(bank_idx) {
                        bank[offset] = value;
                    }
                }
            }
            0x2000..=0x3EFF => {
                // Nametable write with mirroring
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                self.vram[vram_addr] = value;
            }
            0x3F00..=0x3FFF => {
                // Palette RAM write with proper mirroring
                let mut palette_addr = (addr as usize - 0x3F00) % 32;
                // NESDev-correct mirroring: $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
                if palette_addr & 0x13 == 0x10 {
                    palette_addr &= !0x10;
                }
                self.palette_ram[palette_addr] = value;
            }
            _ => {}
        }
    }

    fn code_start(&mut self) -> u16 {
        // Read reset vector through proper mapper CPU read (like other mappers do)
        let lo = self.cpu_read(RESET_TARGET_ADDR);
        let hi = self.cpu_read(RESET_TARGET_ADDR + 1);
        let start_addr = ((hi as u16) << 8) | (lo as u16);

        start_addr
    }

    fn controllers(&mut self) -> &mut [io::controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        // Level-sensitive: stays asserted until $E000-$FFFE acknowledge; CPU I mask prevents re-entrancy.
        self.irq_pending
    }

    fn poll_irq_at_dot(&self, deadline_dot: u64) -> bool {
        self.irq_pending && self.irq_pending_since_dot <= deadline_dot
    }

    fn ppu_fetch(&mut self, addr: u16, dot: u64) -> u8 {
        self.check_a12_transition(addr, dot);
        self.ppu_read(addr)
    }

    fn ppu_cycle_260(&mut self, _scanline: u16) {}

    fn ppu_a12_transition(&mut self, addr: u16, dot: u64) {
        self.check_a12_transition(addr, dot);
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&self.prg_ram[..])
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        4
    }
    fn submapper_id(&self) -> u8 {
        self.submapper
    }

    fn set_submapper(&mut self, submapper: u8) {
        self.submapper = submapper;
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for bank in &self.chr_mem {
            w.write_bytes(bank);
        }
        w.write_u8(self.prg_bank_mode);
        w.write_u8(self.chr_bank_mode);
        w.write_u8(self.bank_select);
        for &reg in &self.bank_regs {
            w.write_u8(reg);
        }
        w.write_u8(self.irq_counter);
        w.write_u8(self.irq_latch);
        w.write_bool(self.irq_enable);
        w.write_bool(self.irq_reload);
        w.write_bool(self.irq_pending);
        w.write_u64(self.irq_pending_since_dot);
        w.write_bool(self.a12_state);
        w.write_u64(self.last_a12_low_dot);
        w.write_u8(self.submapper);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for bank in &mut self.chr_mem {
            r.read_bytes_into(bank)?;
        }
        self.prg_bank_mode = r.read_u8()?;
        self.chr_bank_mode = r.read_u8()?;
        self.bank_select = r.read_u8()?;
        for reg in &mut self.bank_regs {
            *reg = r.read_u8()?;
        }
        self.irq_counter = r.read_u8()?;
        self.irq_latch = r.read_u8()?;
        self.irq_enable = r.read_bool()?;
        self.irq_reload = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        self.irq_pending_since_dot = r.read_u64()?;
        self.a12_state = r.read_bool()?;
        self.last_a12_low_dot = r.read_u64()?;
        self.submapper = r.read_u8()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::emu;
    #[allow(unused_imports)]
    use crate::emu::io::loader;

    fn test_mapper() -> MMC3Mapper {
        MMC3Mapper::new(0, vec![[0; 16384]; 2], vec![[0; 8192]; 1], false, None, 0)
    }

    #[test]
    fn test_cycle_260_does_not_clock_irq_counter() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 1;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        mapper.ppu_cycle_260(0);

        assert_eq!(mapper.irq_counter, 0);
        assert_eq!(mapper.irq_reload, true);
        assert_eq!(mapper.poll_irq(), false);
    }

    #[test]
    fn test_mmc3_a12_edges_from_filtered_ppu_fetches() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 1;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        // Gap of 10 dots (< 16 threshold) — filtered, no clock
        mapper.ppu_fetch(0x0000, 10);
        mapper.ppu_fetch(0x1000, 20);
        assert_eq!(mapper.irq_reload, true);

        // Gap of 20 dots (>= 16 threshold) — clocks, reload from latch
        mapper.ppu_fetch(0x0000, 30);
        mapper.ppu_fetch(0x1000, 50);
        assert_eq!(mapper.irq_counter, 1);
        assert_eq!(mapper.irq_reload, false);
        assert_eq!(mapper.poll_irq(), false);

        // Another valid edge — counter decrements to 0, IRQ fires
        mapper.ppu_fetch(0x0000, 60);
        mapper.ppu_fetch(0x1000, 80);
        assert_eq!(mapper.poll_irq(), true);
    }

    #[test]
    fn test_irq_pending_stays_asserted_until_disabled() {
        let mut mapper = test_mapper();
        mapper.irq_pending = true;
        mapper.irq_enable = true;

        assert_eq!(mapper.poll_irq(), true);
        assert_eq!(mapper.poll_irq(), true);

        mapper.cpu_write(0xE000, 0);

        assert_eq!(mapper.poll_irq(), false);
        assert_eq!(mapper.irq_enable, false);
    }

    #[test]
    fn test_reload_with_zero_latch_can_assert_irq() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 0;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        mapper.ppu_fetch(0x0000, 10);
        mapper.ppu_fetch(0x1000, 30);

        assert_eq!(mapper.irq_counter, 0);
        assert_eq!(mapper.irq_reload, false);
        assert_eq!(mapper.poll_irq(), true);
    }

    #[test]
    fn test_reload_with_nonzero_latch_does_not_immediately_assert_irq() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 2;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        mapper.ppu_fetch(0x0000, 10);
        mapper.ppu_fetch(0x1000, 30);

        assert_eq!(mapper.irq_counter, 2);
        assert_eq!(mapper.irq_reload, false);
        assert_eq!(mapper.poll_irq(), false);
    }

    #[test]
    fn test_mmc3_a12_rising_edge_from_pattern_table_switch_clocks_irq() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 1;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        // Simulate BG fetches at $0xxx then sprite fetches at $1xxx with big gap
        mapper.ppu_fetch(0x0000, 10);
        mapper.ppu_fetch(0x1000, 30);
        mapper.ppu_fetch(0x0000, 40);
        mapper.ppu_fetch(0x1000, 60);

        assert_eq!(mapper.poll_irq(), true);
    }

    #[test]
    fn test_cpu_a12_transition_clocks_irq_with_sufficient_gap() {
        let mut mapper = test_mapper();
        mapper.irq_latch = 1;
        mapper.irq_enable = true;
        mapper.irq_reload = true;

        mapper.ppu_fetch(0x0000, 10);
        mapper.ppu_a12_transition(0x1000, 30);

        assert_eq!(mapper.irq_counter, 1);
        assert_eq!(mapper.irq_reload, false);
    }

    fn run_mmc3_rom(path: &str, name: &str) {
        run_mmc3_rom_with_submapper(path, name, None);
    }

    fn run_mmc3_rom_with_submapper(path: &str, name: &str, submapper: Option<u8>) {
        use crate::util::get_status_str;

        let mut mapper = loader::load_nes(&String::from(path));
        if let Some(sm) = submapper {
            mapper.set_submapper(sm);
        }

        let mut emu: emu::Emulator = emu::Emulator::new_headless(mapper);

        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = format!("\n{}\n\nPassed\n", name);
        let buf = get_status_str(&mut emu, 0x6004, 80);

        println!("{}", buf);
        println!("status: {:02X}", emu.mem.cpu_read(0x6000));

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_mmc3_1_clocking() {
        run_mmc3_rom("input/nes/mappers/mmc3/1-clocking.nes", "1-clocking");
    }

    #[test]
    fn test_mmc3_2_details() {
        run_mmc3_rom("input/nes/mappers/mmc3/2-details.nes", "2-details");
    }

    #[test]
    fn test_mmc3_3_a12_clocking() {
        run_mmc3_rom(
            "input/nes/mappers/mmc3/3-A12_clocking.nes",
            "3-A12_clocking",
        );
    }

    #[test]
    fn test_mmc3_4_scanline_timing() {
        run_mmc3_rom(
            "input/nes/mappers/mmc3/4-scanline_timing.nes",
            "4-scanline_timing",
        );
    }

    #[test]
    fn test_mmc3_5_mmc3() {
        run_mmc3_rom("input/nes/mappers/mmc3/5-MMC3.nes", "5-MMC3");
    }

    #[test]
    fn test_mmc3_6_mmc6() {
        run_mmc3_rom_with_submapper("input/nes/mappers/mmc3/6-MMC6.nes", "6-MMC6", Some(1));
    }
}
