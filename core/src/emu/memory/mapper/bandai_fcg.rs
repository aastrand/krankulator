use super::super::super::io;
use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 1024;
const EEPROM_SIZE_24C02: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
enum EepromState {
    Idle,
    DeviceAddr,
    WordAddr,
    WriteData,
    ReadData,
}

struct Eeprom24C02 {
    data: [u8; EEPROM_SIZE_24C02],
    state: EepromState,
    scl: bool,
    sda_out: bool,
    sda_in: bool,
    bit_count: u8,
    shift_reg: u8,
    word_addr: u8,
    output_bit: bool,
}

impl Eeprom24C02 {
    fn new(data: Option<&[u8]>) -> Self {
        let mut eeprom_data = [0xFF; EEPROM_SIZE_24C02];
        if let Some(d) = data {
            let len = d.len().min(EEPROM_SIZE_24C02);
            eeprom_data[..len].copy_from_slice(&d[..len]);
        }
        Eeprom24C02 {
            data: eeprom_data,
            state: EepromState::Idle,
            scl: false,
            sda_out: true,
            sda_in: true,
            bit_count: 0,
            shift_reg: 0,
            word_addr: 0,
            output_bit: true,
        }
    }

    fn write(&mut self, sda: bool, scl: bool) {
        let old_scl = self.scl;
        let old_sda = self.sda_in;
        self.scl = scl;
        self.sda_in = sda;

        if scl && old_scl {
            if old_sda && !sda {
                // START condition
                self.state = EepromState::DeviceAddr;
                self.bit_count = 0;
                self.shift_reg = 0;
                self.output_bit = true;
                return;
            }
            if !old_sda && sda {
                // STOP condition
                self.state = EepromState::Idle;
                self.output_bit = true;
                return;
            }
        }

        // Clock data on SCL rising edge
        if scl && !old_scl {
            match self.state {
                EepromState::Idle => {}
                EepromState::DeviceAddr => {
                    if self.bit_count < 8 {
                        self.shift_reg = (self.shift_reg << 1) | (sda as u8);
                        self.bit_count += 1;
                        if self.bit_count == 8 {
                            self.output_bit = false; // ACK
                        }
                    } else {
                        self.bit_count += 1; // mark ACK clocked, transition on falling edge
                    }
                }
                EepromState::WordAddr => {
                    if self.bit_count < 8 {
                        self.shift_reg = (self.shift_reg << 1) | (sda as u8);
                        self.bit_count += 1;
                        if self.bit_count == 8 {
                            self.word_addr = self.shift_reg;
                            self.output_bit = false; // ACK
                        }
                    } else {
                        self.bit_count += 1;
                    }
                }
                EepromState::WriteData => {
                    if self.bit_count < 8 {
                        self.shift_reg = (self.shift_reg << 1) | (sda as u8);
                        self.bit_count += 1;
                        if self.bit_count == 8 {
                            self.data[self.word_addr as usize] = self.shift_reg;
                            self.word_addr = self.word_addr.wrapping_add(1);
                            self.output_bit = false; // ACK
                        }
                    } else {
                        self.bit_count += 1;
                    }
                }
                EepromState::ReadData => {
                    if self.bit_count < 8 {
                        self.output_bit =
                            (self.data[self.word_addr as usize] >> (7 - self.bit_count)) & 1 != 0;
                        self.bit_count += 1;
                    } else {
                        self.bit_count += 1;
                    }
                }
            }
        }

        // SCL falling edge after ACK cycle: transition to next state
        if !scl && old_scl && self.bit_count > 8 {
            match self.state {
                EepromState::DeviceAddr => {
                    let rw = self.shift_reg & 1;
                    self.bit_count = 0;
                    if rw == 0 {
                        self.state = EepromState::WordAddr;
                        self.shift_reg = 0;
                        self.output_bit = true;
                    } else {
                        self.state = EepromState::ReadData;
                        self.output_bit = (self.data[self.word_addr as usize] >> 7) & 1 != 0;
                    }
                }
                EepromState::WordAddr => {
                    self.bit_count = 0;
                    self.shift_reg = 0;
                    self.output_bit = true;
                    self.state = EepromState::WriteData;
                }
                EepromState::WriteData => {
                    self.bit_count = 0;
                    self.shift_reg = 0;
                    self.output_bit = true;
                }
                EepromState::ReadData => {
                    self.word_addr = self.word_addr.wrapping_add(1);
                    self.bit_count = 0;
                    self.output_bit = (self.data[self.word_addr as usize] >> 7) & 1 != 0;
                }
                _ => {}
            }
        }
    }

    fn read_bit(&self) -> bool {
        self.output_bit
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Submapper {
    Fcg,
    Lz93d50,
}

pub struct BandaiFcgMapper {
    _cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    cpu_ram_ptr: *mut u8,

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    prg_bank_idx: usize,

    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    chr_regs: [usize; 8],

    mirroring: NametableMirror,

    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    submapper: Submapper,

    eeprom: Option<Eeprom24C02>,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vram_ptr: *mut u8,
    palette_ram: [u8; PALETTE_SIZE],

    pub controllers: [controller::Controller; 2],
}

impl BandaiFcgMapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; PRG_BANK_SIZE]>,
        chr_banks_8k: Vec<[u8; io::loader::CHR_BANK_SIZE]>,
        submapper: u8,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        let mut chr_rom: Vec<[u8; CHR_BANK_SIZE]> = Vec::new();
        for bank in &chr_banks_8k {
            for i in 0..8 {
                let mut kb = [0u8; CHR_BANK_SIZE];
                kb.copy_from_slice(&bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE]);
                chr_rom.push(kb);
            }
        }

        let mut cpu_ram = Box::new([0u8; CPU_RAM_SIZE as usize]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut vram = Box::new([0u8; VRAM_SIZE as usize]);
        let vram_ptr = vram.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        let variant = if submapper == 4 {
            Submapper::Fcg
        } else {
            Submapper::Lz93d50
        };

        let eeprom = if variant == Submapper::Lz93d50 {
            Some(Eeprom24C02::new(sram_data.as_deref()))
        } else {
            None
        };

        BandaiFcgMapper {
            _cpu_ram: cpu_ram,
            cpu_ram_ptr,
            prg_rom: prg_banks_16k,
            prg_bank_idx: 0,
            chr_rom,
            chr_regs: [0; 8],
            mirroring,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            submapper: variant,
            eeprom,
            _vram: vram,
            vram_ptr,
            palette_ram: [0x0F; PALETTE_SIZE],
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn chr_bank(&self, idx: usize) -> usize {
        if self.chr_rom.is_empty() {
            return 0;
        }
        self.chr_regs[idx] % self.chr_rom.len()
    }

    fn write_register(&mut self, reg: u8, value: u8) {
        match reg {
            0..=7 => self.chr_regs[reg as usize] = value as usize,
            8 => {
                self.prg_bank_idx = if self.prg_rom.is_empty() {
                    0
                } else {
                    (value as usize & 0x0F) % self.prg_rom.len()
                };
            }
            9 => {
                self.mirroring = match value & 0x03 {
                    0 => NametableMirror::Vertical,
                    1 => NametableMirror::Horizontal,
                    2 => NametableMirror::Lower,
                    _ => NametableMirror::Higher,
                };
            }
            0x0A => {
                self.irq_pending = false;
                if self.submapper == Submapper::Lz93d50 {
                    self.irq_counter = self.irq_latch;
                }
                self.irq_enabled = value & 0x01 != 0;
            }
            0x0B => {
                if self.submapper == Submapper::Fcg {
                    self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
                } else {
                    self.irq_latch = (self.irq_latch & 0xFF00) | value as u16;
                }
            }
            0x0C => {
                if self.submapper == Submapper::Fcg {
                    self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                } else {
                    self.irq_latch = (self.irq_latch & 0x00FF) | ((value as u16) << 8);
                }
            }
            0x0D => {
                if let Some(ref mut eeprom) = self.eeprom {
                    let sda = (value >> 6) & 1 != 0;
                    let scl = (value >> 5) & 1 != 0;
                    eeprom.write(sda, scl);
                }
            }
            _ => {}
        }
    }
}

impl MemoryMapper for BandaiFcgMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 if self.submapper == Submapper::Lz93d50 => {
                if let Some(ref eeprom) = self.eeprom {
                    let bit = eeprom.read_bit();
                    if bit {
                        0x10
                    } else {
                        0x00
                    }
                } else {
                    0
                }
            }
            0x80 | 0x90 | 0xA0 | 0xB0 => {
                if self.prg_rom.is_empty() {
                    return 0;
                }
                self.prg_rom[self.prg_bank_idx][(addr - 0x8000) as usize]
            }
            0xC0 | 0xD0 | 0xE0 | 0xF0 => {
                if self.prg_rom.is_empty() {
                    return 0;
                }
                let fixed = self.prg_rom.len() - 1;
                self.prg_rom[fixed][(addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 if self.submapper == Submapper::Lz93d50 => {
                if let Some(ref eeprom) = self.eeprom {
                    let bit = eeprom.read_bit();
                    if bit {
                        0x10
                    } else {
                        0x00
                    }
                } else {
                    0
                }
            }
            0x80 | 0x90 | 0xA0 | 0xB0 => {
                if self.prg_rom.is_empty() {
                    return 0;
                }
                self.prg_rom[self.prg_bank_idx][(addr - 0x8000) as usize]
            }
            0xC0 | 0xD0 | 0xE0 | 0xF0 => {
                if self.prg_rom.is_empty() {
                    return 0;
                }
                let fixed = self.prg_rom.len() - 1;
                self.prg_rom[fixed][(addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) = value },
            0x20 | 0x40 | 0x50 => {}
            0x60 | 0x70 if self.submapper == Submapper::Fcg => {
                let reg = (addr & 0x0F) as u8;
                self.write_register(reg, value);
            }
            0x80..=0xF0 if self.submapper == Submapper::Lz93d50 => {
                let reg = (addr & 0x0F) as u8;
                self.write_register(reg, value);
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            return self.palette_ram[idx];
        }
        match addr_to_page(addr) {
            0x00 | 0x10 => {
                if self.chr_rom.is_empty() {
                    return 0;
                }
                let chr_idx = (addr as usize) / CHR_BANK_SIZE;
                let bank = self.chr_bank(chr_idx);
                self.chr_rom[bank][(addr as usize) % CHR_BANK_SIZE]
            }
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { *self.vram_ptr.offset(a as _) }
            }
            _ => 0,
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % MAX_VRAM_ADDR;
        match addr_to_page(addr) {
            0x00 | 0x10 => {
                if self.chr_rom.is_empty() {
                    return;
                }
                let chr_idx = (addr as usize) / CHR_BANK_SIZE;
                let bank = self.chr_bank(chr_idx);
                let offset = (addr as usize) % CHR_BANK_SIZE;
                unsafe { std::ptr::copy(self.chr_rom[bank].as_ptr().add(offset), dest, size) }
            }
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { std::ptr::copy(self.vram_ptr.offset(a as _), dest, size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            self.palette_ram[idx] = value;
            return;
        }
        match addr_to_page(addr) {
            0x00 | 0x10 => {}
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { *self.vram_ptr.offset(a as _) = value }
            }
            _ => {}
        }
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8)
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if !self.irq_enabled {
            return;
        }
        if self.irq_counter == 0 {
            self.irq_pending = true;
        } else {
            self.irq_counter -= 1;
        }
    }

    fn mapper_id(&self) -> u8 {
        16
    }

    fn submapper_id(&self) -> u8 {
        match self.submapper {
            Submapper::Fcg => 4,
            Submapper::Lz93d50 => 5,
        }
    }

    fn sram_data(&self) -> Option<&[u8]> {
        self.eeprom.as_ref().map(|e| e.data.as_slice())
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        self.eeprom.as_mut().map(|e| e.data.as_mut_slice())
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let cpu_ram =
            unsafe { std::slice::from_raw_parts(self.cpu_ram_ptr, CPU_RAM_SIZE as usize) };
        w.write_bytes(cpu_ram);
        w.write_u8(self.prg_bank_idx as u8);
        for r in &self.chr_regs {
            w.write_u8(*r as u8);
        }
        save_mirroring(w, self.mirroring);
        w.write_u16(self.irq_latch);
        w.write_u16(self.irq_counter);
        w.write_u8(self.irq_enabled as u8);
        w.write_u8(self.irq_pending as u8);
        w.write_u8(self.submapper as u8);
        if let Some(ref eeprom) = self.eeprom {
            w.write_u8(1);
            w.write_bytes(&eeprom.data);
            w.write_u8(eeprom.state as u8);
            w.write_u8(eeprom.scl as u8);
            w.write_u8(eeprom.sda_out as u8);
            w.write_u8(eeprom.sda_in as u8);
            w.write_u8(eeprom.bit_count);
            w.write_u8(eeprom.shift_reg);
            w.write_u8(eeprom.word_addr);
            w.write_u8(eeprom.output_bit as u8);
        } else {
            w.write_u8(0);
        }
        let vram = unsafe { std::slice::from_raw_parts(self.vram_ptr, VRAM_SIZE as usize) };
        w.write_bytes(vram);
        w.write_bytes(&self.palette_ram);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let cpu_ram =
            unsafe { std::slice::from_raw_parts_mut(self.cpu_ram_ptr, CPU_RAM_SIZE as usize) };
        r.read_bytes_into(cpu_ram)?;
        self.prg_bank_idx = r.read_u8()? as usize;
        for reg in &mut self.chr_regs {
            *reg = r.read_u8()? as usize;
        }
        self.mirroring = load_mirroring(r)?;
        self.irq_latch = r.read_u16()?;
        self.irq_counter = r.read_u16()?;
        self.irq_enabled = r.read_u8()? != 0;
        self.irq_pending = r.read_u8()? != 0;
        let _submapper_byte = r.read_u8()?;
        let has_eeprom = r.read_u8()? != 0;
        if has_eeprom {
            if let Some(ref mut eeprom) = self.eeprom {
                r.read_bytes_into(&mut eeprom.data)?;
                let state_byte = r.read_u8()?;
                eeprom.state = match state_byte {
                    0 => EepromState::Idle,
                    1 => EepromState::DeviceAddr,
                    2 => EepromState::WordAddr,
                    3 => EepromState::WriteData,
                    _ => EepromState::ReadData,
                };
                eeprom.scl = r.read_u8()? != 0;
                eeprom.sda_out = r.read_u8()? != 0;
                eeprom.sda_in = r.read_u8()? != 0;
                eeprom.bit_count = r.read_u8()?;
                eeprom.shift_reg = r.read_u8()?;
                eeprom.word_addr = r.read_u8()?;
                eeprom.output_bit = r.read_u8()? != 0;
            }
        }
        let vram = unsafe { std::slice::from_raw_parts_mut(self.vram_ptr, VRAM_SIZE as usize) };
        r.read_bytes_into(vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper_fcg(prg_16k_count: usize, chr_8k_count: usize) -> BandaiFcgMapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_16k_count {
            let mut bank = [0u8; PRG_BANK_SIZE];
            bank[0] = i as u8;
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_8k_count {
            let mut bank = [0u8; io::loader::CHR_BANK_SIZE];
            for k in 0..8 {
                bank[k * 1024] = (i * 8 + k) as u8;
            }
            chr_banks.push(bank);
        }
        BandaiFcgMapper::new(0x01, prg_banks, chr_banks, 4, None)
    }

    fn make_mapper_lz93d50(prg_16k_count: usize, chr_8k_count: usize) -> BandaiFcgMapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_16k_count {
            let mut bank = [0u8; PRG_BANK_SIZE];
            bank[0] = i as u8;
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_8k_count {
            let mut bank = [0u8; io::loader::CHR_BANK_SIZE];
            for k in 0..8 {
                bank[k * 1024] = (i * 8 + k) as u8;
            }
            chr_banks.push(bank);
        }
        BandaiFcgMapper::new(
            0x01,
            prg_banks,
            chr_banks,
            5,
            Some(vec![0xFF; EEPROM_SIZE_24C02]),
        )
    }

    #[test]
    fn test_fcg_prg_banking() {
        let mut m = make_mapper_fcg(8, 8);
        m.cpu_write(0x6008, 3);
        assert_eq!(m.prg_bank_idx, 3);
    }

    #[test]
    fn test_fcg_chr_banking() {
        let mut m = make_mapper_fcg(8, 32);
        m.cpu_write(0x6000, 5);
        assert_eq!(m.chr_regs[0], 5);
        m.cpu_write(0x6003, 10);
        assert_eq!(m.chr_regs[3], 10);
    }

    #[test]
    fn test_fcg_mirroring() {
        let mut m = make_mapper_fcg(8, 8);
        m.cpu_write(0x6009, 0);
        assert_eq!(m.mirroring, NametableMirror::Vertical);
        m.cpu_write(0x6009, 1);
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
        m.cpu_write(0x6009, 2);
        assert_eq!(m.mirroring, NametableMirror::Lower);
        m.cpu_write(0x6009, 3);
        assert_eq!(m.mirroring, NametableMirror::Higher);
    }

    #[test]
    fn test_fcg_irq_direct_counter() {
        let mut m = make_mapper_fcg(8, 8);
        // FCG writes directly to counter
        m.cpu_write(0x600B, 0x03); // low byte
        m.cpu_write(0x600C, 0x00); // high byte
        m.cpu_write(0x600A, 0x01); // enable

        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 3 -> 2
        m.cpu_cycle(0); // 2 -> 1
        m.cpu_cycle(0); // 1 -> 0
        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 0 -> fires
        assert!(m.irq_pending);
    }

    #[test]
    fn test_lz93d50_prg_banking() {
        let mut m = make_mapper_lz93d50(8, 8);
        m.cpu_write(0x8008, 5);
        assert_eq!(m.prg_bank_idx, 5);
    }

    #[test]
    fn test_lz93d50_irq_latched() {
        let mut m = make_mapper_lz93d50(8, 8);
        // LZ93D50 writes to latch, copies on enable
        m.cpu_write(0x800B, 0x03); // latch low
        m.cpu_write(0x800C, 0x00); // latch high
        assert_eq!(m.irq_counter, 0); // counter unchanged
        m.cpu_write(0x800A, 0x01); // copies latch to counter + enable
        assert_eq!(m.irq_counter, 3);

        m.cpu_cycle(0);
        m.cpu_cycle(0);
        m.cpu_cycle(0);
        assert!(!m.irq_pending);
        m.cpu_cycle(0);
        assert!(m.irq_pending);
    }

    #[test]
    fn test_lz93d50_eeprom_read() {
        let mut m = make_mapper_lz93d50(8, 8);
        // Initial EEPROM data is 0xFF
        // Read $6000 should return EEPROM output on bit 4
        let val = m.cpu_read(0x6000);
        assert_eq!(val & 0x10, 0x10); // output_bit defaults to true
    }

    #[test]
    fn test_eeprom_write_and_read() {
        let mut eeprom = Eeprom24C02::new(None);

        // Helper to clock a bit
        fn clock_bit(eeprom: &mut Eeprom24C02, bit: bool) {
            eeprom.write(bit, false); // SDA setup
            eeprom.write(bit, true); // SCL rise - clock in
            eeprom.write(bit, false); // SCL fall
        }

        fn start(eeprom: &mut Eeprom24C02) {
            eeprom.write(true, true);
            eeprom.write(false, true); // SDA fall while SCL high
            eeprom.write(false, false);
        }

        fn stop(eeprom: &mut Eeprom24C02) {
            eeprom.write(false, true);
            eeprom.write(true, true); // SDA rise while SCL high
        }

        fn clock_byte(eeprom: &mut Eeprom24C02, byte: u8) {
            for i in (0..8).rev() {
                clock_bit(eeprom, (byte >> i) & 1 != 0);
            }
            // Read ACK
            clock_bit(eeprom, true);
        }

        // Write 0x42 to address 0x00
        start(&mut eeprom);
        clock_byte(&mut eeprom, 0xA0); // device addr, write
        clock_byte(&mut eeprom, 0x00); // word addr
        clock_byte(&mut eeprom, 0x42); // data
        stop(&mut eeprom);

        assert_eq!(eeprom.data[0], 0x42);
    }

    #[test]
    fn test_eeprom_ack_visible_during_9th_clock() {
        let mut eeprom = Eeprom24C02::new(None);

        fn start(eeprom: &mut Eeprom24C02) {
            eeprom.write(true, true);
            eeprom.write(false, true);
            eeprom.write(false, false);
        }

        // Send START
        start(&mut eeprom);

        // Clock 8 bits of device address 0xA0 (write mode)
        for i in (0..8).rev() {
            let bit = (0xA0u8 >> i) & 1 != 0;
            eeprom.write(bit, false);
            eeprom.write(bit, true);
            eeprom.write(bit, false);
        }

        // After 8th bit clocked, output_bit should be false (ACK ready)
        assert!(!eeprom.output_bit, "ACK should be asserted after 8 bits");

        // 9th clock rising edge: ACK must remain visible
        eeprom.write(true, false); // SDA released by master
        eeprom.write(true, true); // SCL rise — game reads here
        assert!(
            !eeprom.output_bit,
            "ACK must remain visible while SCL is high on 9th clock"
        );

        // SCL falling edge: now state transitions
        eeprom.write(true, false);
        assert_eq!(eeprom.state, EepromState::WordAddr);
    }

    #[test]
    fn test_fcg_registers_at_6000() {
        let mut m = make_mapper_fcg(8, 8);
        // FCG: writes to $6000-$600D are register writes
        m.cpu_write(0x6008, 2);
        assert_eq!(m.prg_bank_idx, 2);
        // $8000 writes should be ignored for FCG
        m.cpu_write(0x8008, 5);
        assert_eq!(m.prg_bank_idx, 2);
    }

    #[test]
    fn test_lz93d50_registers_at_8000() {
        let mut m = make_mapper_lz93d50(8, 8);
        // LZ93D50: writes to $8000-$800D are register writes
        m.cpu_write(0x8008, 2);
        assert_eq!(m.prg_bank_idx, 2);
        // $6000 writes should be ignored for LZ93D50 (reads return EEPROM)
        m.cpu_write(0x6008, 5);
        assert_eq!(m.prg_bank_idx, 2);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper_lz93d50(8, 8);
        m.cpu_write(0x8008, 3);
        m.cpu_write(0x8000, 7);
        m.cpu_write(0x8009, 1);
        m.irq_pending = true;

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);

        let mut m2 = make_mapper_lz93d50(8, 8);
        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.prg_bank_idx, 3);
        assert_eq!(m2.chr_regs[0], 7);
        assert_eq!(m2.mirroring, NametableMirror::Horizontal);
        assert!(m2.irq_pending);
    }
}
