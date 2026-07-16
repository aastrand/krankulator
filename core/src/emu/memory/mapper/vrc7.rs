use super::super::super::io;
use super::vrc7_audio::Vrc7Audio;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::apu::ChannelDebugState;
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct Vrc7Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr: Vec<[u8; CHR_BANK_SIZE]>,
    chr_is_ram: bool,
    prg_ram: Box<[u8; PRG_RAM_8K]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    has_battery: bool,
    vram: Box<[u8; VRAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],

    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    mirroring: NametableMirror,
    wram_enable: bool,

    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_mode_cycle: bool,
    irq_enable: bool,
    irq_enable_after_ack: bool,
    irq_pending: bool,
    irq_pending_since_dot: u64,

    audio: Vrc7Audio,
}

impl Vrc7Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        chr_is_ram: bool,
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

        let mut chr: Vec<[u8; CHR_BANK_SIZE]> = vec![];
        for bank in &chr_banks_8k {
            for i in 0..8 {
                chr.push(
                    <[u8; CHR_BANK_SIZE]>::try_from(
                        &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                    )
                    .unwrap(),
                );
            }
        }
        if chr.is_empty() {
            chr = vec![[0; CHR_BANK_SIZE]; 8];
        }

        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        Vrc7Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            has_battery,
            vram: Box::new([0; VRAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],

            prg_banks: [0; 3],
            chr_banks: [0; 8],
            mirroring,
            wram_enable: false,

            irq_latch: 0,
            irq_counter: 0,
            irq_prescaler: 341,
            irq_mode_cycle: false,
            irq_enable: false,
            irq_enable_after_ack: false,
            irq_pending: false,
            irq_pending_since_dot: 0,

            audio: Vrc7Audio::new(),
        }
    }

    // VRC7a (Lagrange Point) decodes registers on A4/A5 ($x010/$x030); VRC7b
    // (Tiny Toon 2) uses A3 ($x008). OR A3 into A4 so both board wirings work.
    fn decode_addr(addr: u16) -> u16 {
        (addr & 0xF030) | ((addr & 0x0008) << 1)
    }

    fn prg_index(&self, bank: u8) -> usize {
        (bank as usize & 0x3F) % self.prg_rom.len().max(1)
    }

    fn chr_index(&self, bank: u8) -> usize {
        bank as usize % self.chr.len().max(1)
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
        match Self::decode_addr(addr) {
            0x8000 => self.prg_banks[0] = value & 0x3F,
            0x8010 => self.prg_banks[1] = value & 0x3F,
            0x9000 => self.prg_banks[2] = value & 0x3F,

            0x9010 => self.audio.write_addr(value),
            0x9030 => self.audio.write_data(value),

            0xA000 => self.chr_banks[0] = value,
            0xA010 => self.chr_banks[1] = value,
            0xB000 => self.chr_banks[2] = value,
            0xB010 => self.chr_banks[3] = value,
            0xC000 => self.chr_banks[4] = value,
            0xC010 => self.chr_banks[5] = value,
            0xD000 => self.chr_banks[6] = value,
            0xD010 => self.chr_banks[7] = value,

            0xE000 => {
                self.mirroring = match value & 0x03 {
                    0 => NametableMirror::Vertical,
                    1 => NametableMirror::Horizontal,
                    2 => NametableMirror::Lower,
                    3 => NametableMirror::Higher,
                    _ => unreachable!(),
                };
                self.audio.set_halt(value & 0x40 != 0);
                self.wram_enable = value & 0x80 != 0;
            }

            0xE010 => self.irq_latch = value,
            0xF000 => {
                self.irq_pending = false;
                self.irq_enable_after_ack = (value & 0x01) != 0;
                self.irq_enable = (value & 0x02) != 0;
                self.irq_mode_cycle = (value & 0x04) != 0;
                if self.irq_enable {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            0xF010 => {
                self.irq_pending = false;
                self.irq_enable = self.irq_enable_after_ack;
            }

            _ => {}
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_index(self.prg_banks[0]),
            0xA000..=0xBFFF => self.prg_index(self.prg_banks[1]),
            0xC000..=0xDFFF => self.prg_index(self.prg_banks[2]),
            _ => self.prg_rom.len().saturating_sub(1),
        };
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr & 0x1FFF) as usize])
    }
}

impl MemoryMapper for Vrc7Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF if self.wram_enable => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF if self.wram_enable => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF if self.wram_enable => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0xFFFF => self.handle_write(addr, value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_index(self.chr_banks[slot]);
                self.chr.get(bank).map_or(0, |b| b[addr as usize & 0x3FF])
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
                let bank = self.chr_index(self.chr_banks[slot]);
                if let Some(b) = self.chr.get(bank) {
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
            0x0000..=0x1FFF if self.chr_is_ram => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_index(self.chr_banks[slot]);
                if let Some(b) = self.chr.get_mut(bank) {
                    b[addr as usize & 0x3FF] = value;
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

    fn cpu_cycle(&mut self, ppu_dot: u64) {
        if self.irq_enable {
            let was_pending = self.irq_pending;
            if self.irq_mode_cycle {
                self.clock_irq_counter();
            } else {
                self.irq_prescaler -= 3;
                if self.irq_prescaler <= 0 {
                    self.irq_prescaler += 341;
                    self.clock_irq_counter();
                }
            }
            if !was_pending && self.irq_pending {
                self.irq_pending_since_dot = ppu_dot;
            }
        }

        self.audio.cpu_cycle();
    }

    fn audio_expansion_output(&self) -> f32 {
        self.audio.output()
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

    // The CPU samples the IRQ line on the penultimate cycle of each
    // instruction; an IRQ asserted after that point must wait one more
    // instruction. Without this, split-screen effects jitter by a scanline.
    fn poll_irq_at_dot(&self, deadline_dot: u64) -> bool {
        self.irq_pending && self.irq_pending_since_dot <= deadline_dot
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
        85
    }

    fn set_debug_capture(&mut self, on: bool) {
        self.audio.set_debug_capture(on);
    }

    fn expansion_audio_debug(&self) -> Vec<ChannelDebugState> {
        self.audio.debug_channels()
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        if self.chr_is_ram {
            for v in &self.chr {
                w.write_bytes(v);
            }
        }
        for &b in &self.prg_banks {
            w.write_u8(b);
        }
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        super::save_mirroring(w, self.mirroring);
        w.write_bool(self.wram_enable);

        w.write_u8(self.irq_latch);
        w.write_u8(self.irq_counter);
        w.write_u16(self.irq_prescaler as u16);
        w.write_bool(self.irq_mode_cycle);
        w.write_bool(self.irq_enable);
        w.write_bool(self.irq_enable_after_ack);
        w.write_bool(self.irq_pending);

        self.audio.save_state(w);

        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        if self.chr_is_ram {
            for v in &mut self.chr {
                r.read_bytes_into(v)?;
            }
        }
        for b in &mut self.prg_banks {
            *b = r.read_u8()?;
        }
        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        self.mirroring = super::load_mirroring(r)?;
        self.wram_enable = r.read_bool()?;

        self.irq_latch = r.read_u8()?;
        self.irq_counter = r.read_u8()?;
        self.irq_prescaler = r.read_u16()? as i16;
        self.irq_mode_cycle = r.read_bool()?;
        self.irq_enable = r.read_bool()?;
        self.irq_enable_after_ack = r.read_bool()?;
        self.irq_pending = r.read_bool()?;

        self.audio.load_state(r)?;

        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(prg_count: usize, chr_count: usize) -> Vrc7Mapper {
        let mut prg_banks = vec![];
        for i in 0..prg_count {
            let mut bank = [0u8; 16384];
            bank[0] = (i * 2) as u8;
            bank[PRG_BANK_SIZE] = (i * 2 + 1) as u8;
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
        let chr_is_ram = chr_banks.is_empty();
        Vrc7Mapper::new(0, prg_banks, chr_banks, chr_is_ram, true, None)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(8, 4);
        // Power-on: banks 0 everywhere, last bank fixed
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 0);
        assert_eq!(m.cpu_read(0xC000), 0);
        assert_eq!(m.cpu_read(0xE000), 15);

        m.cpu_write(0x8000, 3);
        m.cpu_write(0x8010, 4);
        m.cpu_write(0x9000, 5);
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xA000), 4);
        assert_eq!(m.cpu_read(0xC000), 5);
        assert_eq!(m.cpu_read(0xE000), 15);
    }

    #[test]
    fn test_vrc7b_a3_register_alias() {
        // VRC7b (Tiny Toon 2) uses $x008 instead of $x010
        let mut m = make_mapper(8, 4);
        m.cpu_write(0x8008, 6);
        assert_eq!(m.cpu_read(0xA000), 6);

        m.cpu_write(0xA008, 9);
        assert_eq!(m.ppu_read(0x0400), 9);

        m.cpu_write(0xE008, 0xFE); // IRQ latch via VRC7b address
        assert_eq!(m.irq_latch, 0xFE);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(4, 4);
        m.cpu_write(0xA000, 5);
        m.cpu_write(0xA010, 10);
        m.cpu_write(0xB000, 11);
        m.cpu_write(0xD010, 31);
        assert_eq!(m.ppu_read(0x0000), 5);
        assert_eq!(m.ppu_read(0x0400), 10);
        assert_eq!(m.ppu_read(0x0800), 11);
        assert_eq!(m.ppu_read(0x1C00), 31);
    }

    #[test]
    fn test_chr_ram_write() {
        let mut m = make_mapper(4, 0); // CHR-RAM (Lagrange Point config)
        m.cpu_write(0xA000, 3);
        m.ppu_write(0x0005, 0x77);
        assert_eq!(m.ppu_read(0x0005), 0x77);
        // Same RAM visible through a different slot mapping the same bank
        m.cpu_write(0xA010, 3);
        assert_eq!(m.ppu_read(0x0405), 0x77);
    }

    #[test]
    fn test_mirroring_and_wram() {
        let mut m = make_mapper(4, 4);
        assert_eq!(m.mirroring, NametableMirror::Horizontal); // from iNES flags
        m.cpu_write(0xE000, 0x00);
        assert_eq!(m.mirroring, NametableMirror::Vertical);
        m.cpu_write(0xE000, 0x01);
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
        m.cpu_write(0xE000, 0x02);
        assert_eq!(m.mirroring, NametableMirror::Lower);
        m.cpu_write(0xE000, 0x03);
        assert_eq!(m.mirroring, NametableMirror::Higher);

        // WRAM disabled: writes dropped, reads open
        m.cpu_write(0xE000, 0x00);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0);
        // Enable via bit 7
        m.cpu_write(0xE000, 0x80);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
    }

    #[test]
    fn test_irq_scanline_mode() {
        let mut m = make_mapper(4, 4);
        m.cpu_write(0xE010, 0xFF); // latch: fire after 1 scanline
        m.cpu_write(0xF000, 0x02); // enable, scanline mode
                                   // One scanline is ~113.67 CPU cycles (341 PPU dots / 3)
        for _ in 0..113 {
            m.cpu_cycle(0);
        }
        assert!(!m.poll_irq());
        m.cpu_cycle(0);
        assert!(m.poll_irq());
    }

    #[test]
    fn test_irq_cycle_mode_and_ack() {
        let mut m = make_mapper(4, 4);
        m.cpu_write(0xE010, 0xFE);
        m.cpu_write(0xF000, 0x07); // enable, cycle mode, enable-after-ack
        m.cpu_cycle(0);
        assert!(!m.poll_irq());
        m.cpu_cycle(0);
        assert!(m.poll_irq());
        assert_eq!(m.irq_counter, 0xFE);
        // Acknowledge via $F010 keeps counting (A bit set)
        m.cpu_write(0xF010, 0x00);
        assert!(!m.poll_irq());
        assert!(m.irq_enable);
    }

    #[test]
    fn test_audio_reachable_through_mapper() {
        let mut m = make_mapper(4, 4);
        // Custom sustained tone via $9010/$9030
        let writes: [(u8, u8); 11] = [
            (0x00, 0x01),
            (0x01, 0x21),
            (0x02, 0x3F),
            (0x03, 0x00),
            (0x04, 0xF0),
            (0x05, 0xF0),
            (0x06, 0x0F),
            (0x07, 0x00),
            (0x30, 0x00),
            (0x10, 172),
            (0x20, 0x18),
        ];
        for (reg, val) in writes {
            m.cpu_write(0x9010, reg);
            m.cpu_write(0x9030, val);
        }
        let mut max_out = 0.0f32;
        for _ in 0..200_000 {
            m.cpu_cycle(0);
            max_out = max_out.max(m.audio_expansion_output().abs());
        }
        assert!(max_out > 0.1, "FM tone should reach the APU mix");

        // $E000 bit 6 silences it
        m.cpu_write(0xE000, 0x40);
        m.cpu_cycle(0);
        assert_eq!(m.audio_expansion_output(), 0.0);
    }

    #[test]
    fn test_debug_channels() {
        let mut m = make_mapper(4, 4);
        m.set_debug_capture(true);
        m.cpu_write(0x9010, 0x30);
        m.cpu_write(0x9030, 0x10); // ch0: instrument 1
        m.cpu_write(0x9010, 0x10);
        m.cpu_write(0x9030, 172);
        m.cpu_write(0x9010, 0x20);
        m.cpu_write(0x9030, 0x18); // key on
        for _ in 0..200_000 {
            m.cpu_cycle(0);
        }
        let channels = m.expansion_audio_debug();
        assert_eq!(channels.len(), 6);
        assert_eq!(channels[0].name, "FM1");
        assert!(channels[0].enabled);
        assert!(!channels[1].enabled);
        assert!(
            channels[0].waveform.iter().any(|&s| s.abs() > 0.05),
            "keyed channel should show a waveform"
        );
        assert!(
            channels[5].waveform.iter().all(|&s| s == 0.0),
            "unkeyed channel should be flat"
        );
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut a = make_mapper(8, 0);
        a.cpu_write(0x8000, 3);
        a.cpu_write(0xA010, 7);
        a.cpu_write(0xE000, 0x82);
        a.cpu_write(0x6000, 0x5A);
        a.cpu_write(0xE010, 0xF0);
        a.cpu_write(0xF000, 0x02);
        a.ppu_write(0x0100, 0x33);
        a.cpu_write(0x9010, 0x30);
        a.cpu_write(0x9030, 0x20);
        a.cpu_write(0x9010, 0x10);
        a.cpu_write(0x9030, 200);
        a.cpu_write(0x9010, 0x20);
        a.cpu_write(0x9030, 0x14);
        for _ in 0..5000 {
            a.cpu_cycle(0);
        }

        let mut w = SavestateWriter::new();
        a.save_state(&mut w);
        let data = w.finish();

        let mut b = make_mapper(8, 0);
        let mut r = SavestateReader::new(&data).unwrap();
        b.load_state(&mut r).unwrap();

        assert_eq!(b.cpu_read(0x8000), a.cpu_read(0x8000));
        assert_eq!(b.cpu_read(0x6000), 0x5A);
        assert_eq!(b.ppu_read(0x0100), 0x33);
        assert_eq!(b.mirroring, NametableMirror::Lower);
        for i in 0..200_000u32 {
            a.cpu_cycle(0);
            b.cpu_cycle(0);
            assert_eq!(
                a.audio_expansion_output(),
                b.audio_expansion_output(),
                "audio diverged at cycle {i}"
            );
            assert_eq!(a.poll_irq(), b.poll_irq(), "IRQ diverged at cycle {i}");
        }
    }
}
