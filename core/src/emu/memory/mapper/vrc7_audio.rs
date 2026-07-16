//! VRC7 expansion audio: 6-channel 2-operator FM synthesizer (Yamaha OPLL
//! derivative, die-marked "VRC VII"). Port of the reverse-engineered YM2413
//! algorithm from emu2413 v1.5.9 (Mitsutaka Okazaki, MIT), the same core Mesen
//! uses, with the rhythm mode and channels 7-9 removed (absent on the VRC7).

use crate::emu::apu::ChannelDebugState;
use crate::emu::savestate::{SavestateReader, SavestateWriter};
use std::sync::OnceLock;

const PG_BITS: u32 = 10;
const PG_WIDTH: usize = 1 << PG_BITS;
const DP_BITS: u32 = 19;
const DP_WIDTH: u32 = 1 << DP_BITS;
const DP_BASE_BITS: u32 = DP_BITS - PG_BITS;

const EG_MUTE: u8 = 127;
const EG_MAX: u8 = EG_MUTE - 4;
const DAMPER_RATE: u8 = 12;

const CPU_CYCLES_PER_SAMPLE: u8 = 36;

// Matches Mesen's mixer ratio: raw OPLL units vs the standard nonlinear APU
// formula scaled by 5000.
const OUTPUT_SCALE: f32 = 1.0 / 5000.0;

const WAVEFORM_SIZE: usize = 512;
const WAVEFORM_DECIMATION: u32 = 2;
const NUM_CHANNELS: usize = 6;

// Built-in instrument ROM, Nuke.YKT's die-derived dump (row 0 = custom patch).
const VRC7_PATCH_ROM: [[u8; 8]; 16] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    [0x03, 0x21, 0x05, 0x06, 0xe8, 0x81, 0x42, 0x27],
    [0x13, 0x41, 0x14, 0x0d, 0xd8, 0xf6, 0x23, 0x12],
    [0x11, 0x11, 0x08, 0x08, 0xfa, 0xb2, 0x20, 0x12],
    [0x31, 0x61, 0x0c, 0x07, 0xa8, 0x64, 0x61, 0x27],
    [0x32, 0x21, 0x1e, 0x06, 0xe1, 0x76, 0x01, 0x28],
    [0x02, 0x01, 0x06, 0x00, 0xa3, 0xe2, 0xf4, 0xf4],
    [0x21, 0x61, 0x1d, 0x07, 0x82, 0x81, 0x11, 0x07],
    [0x23, 0x21, 0x22, 0x17, 0xa2, 0x72, 0x01, 0x17],
    [0x35, 0x11, 0x25, 0x00, 0x40, 0x73, 0x72, 0x01],
    [0xb5, 0x01, 0x0f, 0x0f, 0xa8, 0xa5, 0x51, 0x02],
    [0x17, 0xc1, 0x24, 0x07, 0xf8, 0xf8, 0x22, 0x12],
    [0x71, 0x23, 0x11, 0x06, 0x65, 0x74, 0x18, 0x16],
    [0x01, 0x02, 0xd3, 0x05, 0xc9, 0x95, 0x03, 0x02],
    [0x61, 0x63, 0x0c, 0x00, 0x94, 0xc0, 0x33, 0xf6],
    [0x21, 0x72, 0x0d, 0x00, 0xc1, 0xd5, 0x56, 0x06],
];

const ML_TABLE: [u32; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];

// Additive fnum offset, rough approximation of 14 cents vibrato depth.
// Row = fnum bits 8-6, column = LFO step.
const PM_TABLE: [[i8; 8]; 8] = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, -1, 0],
    [0, 1, 2, 1, 0, -1, -2, -1],
    [0, 1, 3, 1, 0, -1, -3, -1],
    [0, 2, 4, 2, 0, -2, -4, -2],
    [0, 2, 5, 2, 0, -2, -5, -2],
    [0, 3, 6, 3, 0, -3, -6, -3],
    [0, 3, 7, 3, 0, -3, -7, -3],
];

// Tremolo LFO triangle, verified on real YM2413. Each entry lasts 64 samples.
const AM_TABLE_LEN: u32 = 210;
const EG_STEP_TABLES: [[u8; 8]; 4] = [
    [0, 1, 0, 1, 0, 1, 0, 1],
    [0, 1, 0, 1, 1, 1, 0, 1],
    [0, 1, 1, 1, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 1],
];

fn am_value(step: u32) -> u8 {
    // Triangle 0..13..0 over 210 steps: 0-12 ascending x8, 13 x3, 12-0 descending.
    let i = step % AM_TABLE_LEN;
    if i < 104 {
        (i / 8) as u8
    } else if i < 107 {
        13
    } else {
        (12 - (i - 107) / 8) as u8
    }
}

struct Tables {
    fullsin: Vec<u16>,
    halfsin: Vec<u16>,
    exp: Vec<u16>,
    // Index: ((blk_fnum >> 5) << 8) | (tl << 2) | kl
    tll: Vec<u16>,
    rks: [[u8; 2]; 16],
}

static TABLES: OnceLock<Tables> = OnceLock::new();

fn build_tables() -> Tables {
    let mut fullsin = vec![0u16; PG_WIDTH];
    for (x, v) in fullsin.iter_mut().take(PG_WIDTH / 4).enumerate() {
        let s = ((x as f64 + 0.5) * std::f64::consts::PI / (PG_WIDTH / 4) as f64 / 2.0).sin();
        *v = (-s.log2() * 256.0).round() as u16;
    }
    for x in 0..PG_WIDTH / 4 {
        fullsin[PG_WIDTH / 4 + x] = fullsin[PG_WIDTH / 4 - x - 1];
    }
    for x in 0..PG_WIDTH / 2 {
        fullsin[PG_WIDTH / 2 + x] = 0x8000 | fullsin[x];
    }

    let mut halfsin = fullsin.clone();
    for v in halfsin.iter_mut().skip(PG_WIDTH / 2) {
        *v = 0xfff;
    }

    let mut exp = vec![0u16; 256];
    for (x, v) in exp.iter_mut().enumerate() {
        *v = (((x as f64 / 256.0).exp2() - 1.0) * 1024.0).round() as u16;
    }

    const KL_TABLE: [f64; 16] = [
        0.0, 18.0, 24.0, 27.75, 30.0, 32.25, 33.75, 35.25, 36.0, 37.5, 38.25, 39.0, 39.75, 40.5,
        41.25, 42.0,
    ];
    let mut tll = vec![0u16; 128 * 64 * 4];
    for (fnum, &kl_base) in KL_TABLE.iter().enumerate() {
        for block in 0..8usize {
            for tl in 0..64usize {
                for kl in 0..4usize {
                    let idx = (((block << 4) | fnum) << 8) | (tl << 2) | kl;
                    tll[idx] = if kl == 0 {
                        (tl << 1) as u16
                    } else {
                        let tmp = (kl_base - 6.0 * (7 - block) as f64) as i32;
                        if tmp <= 0 {
                            (tl << 1) as u16
                        } else {
                            ((tmp >> (3 - kl)) as f64 / 0.375) as u16 + (tl << 1) as u16
                        }
                    };
                }
            }
        }
    }

    let mut rks = [[0u8; 2]; 16];
    for fnum8 in 0..2usize {
        for block in 0..8usize {
            rks[(block << 1) | fnum8][1] = ((block << 1) + fnum8) as u8;
            rks[(block << 1) | fnum8][0] = (block >> 1) as u8;
        }
    }

    Tables {
        fullsin,
        halfsin,
        exp,
        tll,
        rks,
    }
}

fn tables() -> &'static Tables {
    TABLES.get_or_init(build_tables)
}

// Output range -4095..4095, "andete's expression".
fn lookup_exp(t: &Tables, i: u16) -> i16 {
    let v = (t.exp[((i & 0xff) ^ 0xff) as usize] + 1024) as i16;
    let shift = (i & 0x7f00) >> 8;
    let res = if shift > 15 { 0 } else { v >> shift };
    (if i & 0x8000 != 0 { !res } else { res }) << 1
}

#[derive(Clone, Copy, Default)]
struct Patch {
    am: bool,
    pm: bool,
    eg: bool,
    kr: bool,
    ml: u8,
    kl: u8,
    tl: u8,
    fb: u8,
    ws: u8,
    ar: u8,
    dr: u8,
    sl: u8,
    rr: u8,
}

fn decode_patch(dump: &[u8; 8]) -> [Patch; 2] {
    let mod_p = Patch {
        am: dump[0] & 0x80 != 0,
        pm: dump[0] & 0x40 != 0,
        eg: dump[0] & 0x20 != 0,
        kr: dump[0] & 0x10 != 0,
        ml: dump[0] & 15,
        kl: (dump[2] >> 6) & 3,
        tl: dump[2] & 63,
        fb: dump[3] & 7,
        ws: (dump[3] >> 3) & 1,
        ar: (dump[4] >> 4) & 15,
        dr: dump[4] & 15,
        sl: (dump[6] >> 4) & 15,
        rr: dump[6] & 15,
    };
    let car_p = Patch {
        am: dump[1] & 0x80 != 0,
        pm: dump[1] & 0x40 != 0,
        eg: dump[1] & 0x20 != 0,
        kr: dump[1] & 0x10 != 0,
        ml: dump[1] & 15,
        kl: (dump[3] >> 6) & 3,
        tl: 0,
        fb: 0,
        ws: (dump[3] >> 4) & 1,
        ar: (dump[5] >> 4) & 15,
        dr: dump[5] & 15,
        sl: (dump[7] >> 4) & 15,
        rr: dump[7] & 15,
    };
    [mod_p, car_p]
}

#[derive(Clone, Copy, PartialEq)]
enum EgState {
    Attack,
    Decay,
    Sustain,
    Release,
    Damp,
}

impl EgState {
    fn to_u8(self) -> u8 {
        match self {
            EgState::Attack => 0,
            EgState::Decay => 1,
            EgState::Sustain => 2,
            EgState::Release => 3,
            EgState::Damp => 4,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => EgState::Attack,
            1 => EgState::Decay,
            2 => EgState::Sustain,
            4 => EgState::Damp,
            _ => EgState::Release,
        }
    }
}

const UPDATE_WS: u8 = 1;
const UPDATE_TLL: u8 = 2;
const UPDATE_RKS: u8 = 4;
const UPDATE_EG: u8 = 8;
const UPDATE_ALL: u8 = 255;

#[derive(Clone)]
struct Slot {
    is_carrier: bool,
    patch: Patch,
    wave_half: bool,
    volume: u8, // carrier attenuation index (register value << 2)
    fnum: u16,
    blk: u8,
    blk_fnum: u16,
    sus_flag: bool,
    key_flag: bool,
    pg_phase: u32,
    pg_out: u16,
    output: [i16; 2],
    eg_state: EgState,
    eg_out: u8,
    eg_rate_h: u8,
    eg_rate_l: u8,
    eg_shift: u8,
    rks: u8,
    tll: u16,
    update_requests: u8,
}

impl Slot {
    fn new(is_carrier: bool) -> Self {
        Slot {
            is_carrier,
            patch: Patch::default(),
            wave_half: false,
            volume: 0,
            fnum: 0,
            blk: 0,
            blk_fnum: 0,
            sus_flag: false,
            key_flag: false,
            pg_phase: 0,
            pg_out: 0,
            output: [0; 2],
            eg_state: EgState::Release,
            eg_out: EG_MUTE,
            eg_rate_h: 0,
            eg_rate_l: 0,
            eg_shift: 0,
            rks: 0,
            tll: 0,
            update_requests: 0,
        }
    }

    fn parameter_rate(&self) -> u8 {
        if !self.is_carrier && !self.key_flag {
            return 0;
        }
        match self.eg_state {
            EgState::Attack => self.patch.ar,
            EgState::Decay => self.patch.dr,
            EgState::Sustain => {
                if self.patch.eg {
                    0
                } else {
                    self.patch.rr
                }
            }
            EgState::Release => {
                if self.sus_flag {
                    5
                } else if self.patch.eg {
                    self.patch.rr
                } else {
                    7
                }
            }
            EgState::Damp => DAMPER_RATE,
        }
    }

    fn commit_update(&mut self, t: &Tables) {
        if self.update_requests & UPDATE_WS != 0 {
            self.wave_half = self.patch.ws != 0;
        }

        if self.update_requests & UPDATE_TLL != 0 {
            let tl_idx = if self.is_carrier {
                self.volume
            } else {
                self.patch.tl
            };
            self.tll = t.tll[((self.blk_fnum as usize >> 5) << 8)
                | ((tl_idx as usize) << 2)
                | self.patch.kl as usize];
        }

        if self.update_requests & UPDATE_RKS != 0 {
            self.rks = t.rks[(self.blk_fnum >> 8) as usize][self.patch.kr as usize];
        }

        if self.update_requests & (UPDATE_RKS | UPDATE_EG) != 0 {
            let p_rate = self.parameter_rate();
            if p_rate == 0 {
                self.eg_shift = 0;
                self.eg_rate_h = 0;
                self.eg_rate_l = 0;
                self.update_requests = 0;
                return;
            }
            self.eg_rate_h = (p_rate + (self.rks >> 2)).min(15);
            self.eg_rate_l = self.rks & 3;
            self.eg_shift = if self.eg_state == EgState::Attack {
                if self.eg_rate_h > 0 && self.eg_rate_h < 12 {
                    13 - self.eg_rate_h
                } else {
                    0
                }
            } else {
                13u8.saturating_sub(self.eg_rate_h)
            };
        }

        self.update_requests = 0;
    }

    fn lookup_attack_step(&self, counter: u16) -> u8 {
        match self.eg_rate_h {
            12 => 4 - EG_STEP_TABLES[self.eg_rate_l as usize][((counter & 0xc) >> 1) as usize],
            13 => 3 - EG_STEP_TABLES[self.eg_rate_l as usize][((counter & 0xc) >> 1) as usize],
            14 => 2 - EG_STEP_TABLES[self.eg_rate_l as usize][((counter & 0xc) >> 1) as usize],
            0 | 15 => 0,
            _ => {
                let index = (counter >> self.eg_shift) & 7;
                if EG_STEP_TABLES[self.eg_rate_l as usize][index as usize] != 0 {
                    4
                } else {
                    0
                }
            }
        }
    }

    fn lookup_decay_step(&self, counter: u16) -> u8 {
        match self.eg_rate_h {
            0 => 0,
            13 => {
                EG_STEP_TABLES[self.eg_rate_l as usize]
                    [(((counter & 0xc) >> 1) | (counter & 1)) as usize]
            }
            14 => EG_STEP_TABLES[self.eg_rate_l as usize][((counter & 0xc) >> 1) as usize] + 1,
            15 => 2,
            _ => {
                let index = (counter >> self.eg_shift) & 7;
                EG_STEP_TABLES[self.eg_rate_l as usize][index as usize]
            }
        }
    }

    fn start_envelope(&mut self) {
        if (self.patch.ar + (self.rks >> 2)).min(15) == 15 {
            self.eg_state = EgState::Decay;
            self.eg_out = 0;
        } else {
            self.eg_state = EgState::Attack;
        }
        self.update_requests |= UPDATE_EG;
    }

    fn calc_phase(&mut self, pm_phase: u32) {
        let pm = if self.patch.pm {
            PM_TABLE[((self.fnum >> 6) & 7) as usize][((pm_phase >> 10) & 7) as usize] as i32
        } else {
            0
        };
        let inc = ((((self.fnum & 0x1ff) as i32 * 2 + pm)
            * ML_TABLE[self.patch.ml as usize] as i32)
            << self.blk)
            >> 2;
        self.pg_phase = self.pg_phase.wrapping_add(inc as u32) & (DP_WIDTH - 1);
        self.pg_out = (self.pg_phase >> DP_BASE_BITS) as u16;
    }

    fn to_linear(&self, t: &Tables, h: u16, am: u8) -> i16 {
        if self.eg_out > EG_MAX {
            return 0;
        }
        let att = (self.eg_out as u16 + self.tll + am as u16).min(EG_MUTE as u16) << 4;
        lookup_exp(t, h.wrapping_add(att))
    }

    fn wave_at(&self, t: &Tables, idx: usize) -> u16 {
        if self.wave_half {
            t.halfsin[idx]
        } else {
            t.fullsin[idx]
        }
    }
}

struct WaveformCapture {
    buffers: [Vec<f32>; NUM_CHANNELS],
    write_pos: usize,
    decimation_counter: u32,
}

impl WaveformCapture {
    fn new() -> Self {
        Self {
            buffers: std::array::from_fn(|_| vec![0.0; WAVEFORM_SIZE]),
            write_pos: 0,
            decimation_counter: 0,
        }
    }

    fn push(&mut self, samples: [f32; NUM_CHANNELS]) {
        self.decimation_counter += 1;
        if self.decimation_counter < WAVEFORM_DECIMATION {
            return;
        }
        self.decimation_counter = 0;
        for (i, &s) in samples.iter().enumerate() {
            self.buffers[i][self.write_pos] = s;
        }
        self.write_pos = (self.write_pos + 1) % WAVEFORM_SIZE;
    }

    fn read_buffer(&self, ch: usize) -> Vec<f32> {
        let mut out = vec![0.0; WAVEFORM_SIZE];
        let pos = self.write_pos;
        let (tail, head) = self.buffers[ch].split_at(pos);
        out[..head.len()].copy_from_slice(head);
        out[head.len()..].copy_from_slice(tail);
        out
    }
}

pub struct Vrc7Audio {
    adr: u8,
    regs: [u8; 0x40],
    custom_patch: [Patch; 2],
    slots: [Slot; 12], // mod at 2*ch, carrier at 2*ch+1
    patch_num: [u8; NUM_CHANNELS],
    slot_key_status: u16,
    pm_phase: u32,
    am_phase: u32,
    lfo_am: u8,
    eg_counter: u32,
    divider: u8,
    ch_out: [i16; NUM_CHANNELS],
    mix_out: i16,
    muted: bool,
    debug_capture: Option<WaveformCapture>,
}

impl Vrc7Audio {
    pub fn new() -> Self {
        let mut audio = Vrc7Audio {
            adr: 0,
            regs: [0; 0x40],
            custom_patch: [Patch::default(); 2],
            slots: std::array::from_fn(|i| Slot::new(i % 2 == 1)),
            patch_num: [0; NUM_CHANNELS],
            slot_key_status: 0,
            pm_phase: 0,
            am_phase: 0,
            lfo_am: 0,
            eg_counter: 0,
            divider: 0,
            ch_out: [0; NUM_CHANNELS],
            mix_out: 0,
            muted: false,
            debug_capture: None,
        };
        audio.reset();
        audio
    }

    pub fn reset(&mut self) {
        self.adr = 0;
        self.pm_phase = 0;
        self.am_phase = 0;
        self.lfo_am = 0;
        self.eg_counter = 0;
        self.slot_key_status = 0;
        self.divider = 0;
        self.ch_out = [0; NUM_CHANNELS];
        self.mix_out = 0;
        self.custom_patch = [Patch::default(); 2];
        for (i, slot) in self.slots.iter_mut().enumerate() {
            *slot = Slot::new(i % 2 == 1);
        }
        for ch in 0..NUM_CHANNELS {
            self.set_patch(ch, 0);
        }
        for reg in 0..0x40 {
            self.write_reg(reg as u8, 0);
        }
    }

    // $E000 bit 6: output silenced and register writes disregarded. Verified
    // on hardware to also clear the tremolo LFO phase (but not vibrato).
    pub fn set_halt(&mut self, halt: bool) {
        if halt && !self.muted {
            self.am_phase = 0;
        }
        self.muted = halt;
    }

    pub fn write_addr(&mut self, value: u8) {
        if !self.muted {
            self.adr = value;
        }
    }

    pub fn write_data(&mut self, value: u8) {
        if !self.muted {
            self.write_reg(self.adr, value);
        }
    }

    fn resolve_patch(&self, num: u8) -> [Patch; 2] {
        if num == 0 {
            self.custom_patch
        } else {
            decode_patch(&VRC7_PATCH_ROM[num as usize & 15])
        }
    }

    fn set_patch(&mut self, ch: usize, num: u8) {
        self.patch_num[ch] = num;
        let pair = self.resolve_patch(num);
        self.slots[ch * 2].patch = pair[0];
        self.slots[ch * 2].update_requests |= UPDATE_ALL;
        self.slots[ch * 2 + 1].patch = pair[1];
        self.slots[ch * 2 + 1].update_requests |= UPDATE_ALL;
    }

    fn refresh_custom_patch_users(&mut self, carrier: bool, flags: u8) {
        for ch in 0..NUM_CHANNELS {
            if self.patch_num[ch] == 0 {
                let slot = &mut self.slots[ch * 2 + carrier as usize];
                slot.patch = self.custom_patch[carrier as usize];
                slot.update_requests |= flags;
            }
        }
    }

    fn set_fnumber(&mut self, ch: usize, fnum: u16) {
        for slot in &mut self.slots[ch * 2..ch * 2 + 2] {
            slot.fnum = fnum;
            slot.blk_fnum = (slot.blk_fnum & 0xe00) | (fnum & 0x1ff);
            slot.update_requests |= UPDATE_EG | UPDATE_RKS | UPDATE_TLL;
        }
    }

    fn set_block(&mut self, ch: usize, blk: u8) {
        for slot in &mut self.slots[ch * 2..ch * 2 + 2] {
            slot.blk = blk;
            slot.blk_fnum = (((blk & 7) as u16) << 9) | (slot.blk_fnum & 0x1ff);
            slot.update_requests |= UPDATE_EG | UPDATE_RKS | UPDATE_TLL;
        }
    }

    fn update_key_status(&mut self) {
        let mut new_status: u16 = 0;
        for ch in 0..NUM_CHANNELS {
            if self.regs[0x20 + ch] & 0x10 != 0 {
                new_status |= 3 << (ch * 2);
            }
        }
        let updated = self.slot_key_status ^ new_status;
        if updated != 0 {
            for i in 0..NUM_CHANNELS * 2 {
                if updated & (1 << i) != 0 {
                    let slot = &mut self.slots[i];
                    if new_status & (1 << i) != 0 {
                        slot.key_flag = true;
                        slot.eg_state = EgState::Damp;
                        slot.update_requests |= UPDATE_EG;
                    } else {
                        slot.key_flag = false;
                        if slot.is_carrier {
                            slot.eg_state = EgState::Release;
                            slot.update_requests |= UPDATE_EG;
                        }
                    }
                }
            }
        }
        self.slot_key_status = new_status;
    }

    fn write_reg(&mut self, reg: u8, data: u8) {
        let mut reg = reg as usize;
        if reg >= 0x40 {
            return;
        }
        // Mirror registers ($19-$1F, $29-$2F, $39-$3F)
        if (0x19..=0x1f).contains(&reg)
            || (0x29..=0x2f).contains(&reg)
            || (0x39..=0x3f).contains(&reg)
        {
            reg -= 9;
        }
        self.regs[reg] = data;

        match reg {
            0x00 | 0x01 => {
                let car = reg == 0x01;
                let p = &mut self.custom_patch[car as usize];
                p.am = data & 0x80 != 0;
                p.pm = data & 0x40 != 0;
                p.eg = data & 0x20 != 0;
                p.kr = data & 0x10 != 0;
                p.ml = data & 15;
                self.refresh_custom_patch_users(car, UPDATE_RKS | UPDATE_EG);
            }
            0x02 => {
                self.custom_patch[0].kl = (data >> 6) & 3;
                self.custom_patch[0].tl = data & 63;
                self.refresh_custom_patch_users(false, UPDATE_TLL);
            }
            0x03 => {
                self.custom_patch[1].kl = (data >> 6) & 3;
                self.custom_patch[1].ws = (data >> 4) & 1;
                self.custom_patch[0].ws = (data >> 3) & 1;
                self.custom_patch[0].fb = data & 7;
                self.refresh_custom_patch_users(false, UPDATE_WS);
                self.refresh_custom_patch_users(true, UPDATE_WS | UPDATE_TLL);
            }
            0x04 | 0x05 => {
                let car = reg == 0x05;
                self.custom_patch[car as usize].ar = (data >> 4) & 15;
                self.custom_patch[car as usize].dr = data & 15;
                self.refresh_custom_patch_users(car, UPDATE_EG);
            }
            0x06 | 0x07 => {
                let car = reg == 0x07;
                self.custom_patch[car as usize].sl = (data >> 4) & 15;
                self.custom_patch[car as usize].rr = data & 15;
                self.refresh_custom_patch_users(car, UPDATE_EG);
            }
            0x10..=0x15 => {
                let ch = reg - 0x10;
                self.set_fnumber(ch, data as u16 + (((self.regs[0x20 + ch] & 1) as u16) << 8));
            }
            0x20..=0x25 => {
                let ch = reg - 0x20;
                self.set_fnumber(ch, (((data & 1) as u16) << 8) + self.regs[0x10 + ch] as u16);
                self.set_block(ch, (data >> 1) & 7);
                let sus = (data >> 5) & 1 != 0;
                let car = &mut self.slots[ch * 2 + 1];
                car.sus_flag = sus;
                car.update_requests |= UPDATE_EG;
                self.update_key_status();
            }
            0x30..=0x35 => {
                let ch = reg - 0x30;
                self.set_patch(ch, (data >> 4) & 15);
                let car = &mut self.slots[ch * 2 + 1];
                car.volume = (data & 15) << 2;
                car.update_requests |= UPDATE_TLL;
            }
            _ => {}
        }
    }

    fn calc_envelope(&mut self, i: usize) {
        let cnt = (self.eg_counter & 0xffff) as u16;
        let slot = &mut self.slots[i];
        let mask = (1u16 << slot.eg_shift) - 1;

        if slot.eg_state == EgState::Attack {
            if slot.eg_out > 0 && slot.eg_rate_h > 0 && (cnt & mask & !3) == 0 {
                let s = slot.lookup_attack_step(cnt);
                if s > 0 {
                    slot.eg_out = (slot.eg_out as i32 - (slot.eg_out >> s) as i32 - 1).max(0) as u8;
                }
            }
        } else if slot.eg_rate_h > 0 && (cnt & mask) == 0 {
            slot.eg_out = (slot.eg_out + slot.lookup_decay_step(cnt)).min(EG_MUTE);
        }

        match slot.eg_state {
            EgState::Damp => {
                // DAMP->ATTACK happens at max attenuation, synchronized with
                // the envelope tick; the carrier resets both slots' phases.
                if slot.eg_out >= EG_MAX && (cnt & mask) == 0 {
                    slot.start_envelope();
                    if slot.is_carrier {
                        slot.pg_phase = 0;
                        self.slots[i ^ 1].pg_phase = 0;
                    }
                }
            }
            EgState::Attack => {
                if slot.eg_out == 0 {
                    slot.eg_state = EgState::Decay;
                    slot.update_requests |= UPDATE_EG;
                }
            }
            // Checked every cycle, not synchronized with the rate counter.
            EgState::Decay if (slot.eg_out >> 3) == slot.patch.sl => {
                slot.eg_state = EgState::Sustain;
                slot.update_requests |= UPDATE_EG;
            }
            _ => {}
        }
    }

    fn update_output(&mut self) {
        let t = tables();

        self.pm_phase = self.pm_phase.wrapping_add(1);
        self.am_phase = self.am_phase.wrapping_add(1);
        self.lfo_am = am_value(self.am_phase >> 6);

        self.eg_counter = self.eg_counter.wrapping_add(1);
        for i in 0..NUM_CHANNELS * 2 {
            if self.slots[i].update_requests != 0 {
                self.slots[i].commit_update(t);
            }
            self.calc_envelope(i);
            self.slots[i].calc_phase(self.pm_phase);
        }

        let mut mix: i16 = 0;
        for ch in 0..NUM_CHANNELS {
            // Modulator with feedback, then carrier driven by it.
            let m = &mut self.slots[ch * 2];
            let fb = m.patch.fb;
            let fm = if fb > 0 {
                (m.output[1] + m.output[0]) >> (9 - fb)
            } else {
                0
            };
            let am = if m.patch.am { self.lfo_am } else { 0 };
            let h = m.wave_at(
                t,
                (m.pg_out.wrapping_add(fm as u16) as usize) & (PG_WIDTH - 1),
            );
            let mod_out = m.to_linear(t, h, am);
            m.output[1] = m.output[0];
            m.output[0] = mod_out;

            let c = &mut self.slots[ch * 2 + 1];
            let am = if c.patch.am { self.lfo_am } else { 0 };
            let idx = (c.pg_out as i32 + 2 * (mod_out >> 1) as i32) as usize & (PG_WIDTH - 1);
            let h = c.wave_at(t, idx);
            let car_out = c.to_linear(t, h, am);
            c.output[1] = c.output[0];
            c.output[0] = car_out;

            self.ch_out[ch] = -car_out >> 1;
            mix += self.ch_out[ch];
        }
        self.mix_out = mix;
    }

    pub fn cpu_cycle(&mut self) {
        self.divider += 1;
        if self.divider >= CPU_CYCLES_PER_SAMPLE {
            self.divider = 0;
            self.update_output();
            if let Some(ref mut capture) = self.debug_capture {
                let mut samples = [0.0f32; NUM_CHANNELS];
                for (i, s) in samples.iter_mut().enumerate() {
                    *s = self.ch_out[i] as f32 / 2048.0;
                }
                capture.push(samples);
            }
        }
    }

    pub fn output(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.mix_out as f32 * OUTPUT_SCALE
        }
    }

    #[cfg(test)]
    fn channel_output(&self, ch: usize) -> i16 {
        self.ch_out[ch]
    }

    pub fn set_debug_capture(&mut self, on: bool) {
        if on && self.debug_capture.is_none() {
            self.debug_capture = Some(WaveformCapture::new());
        } else if !on {
            self.debug_capture = None;
        }
    }

    pub fn debug_channels(&self) -> Vec<ChannelDebugState> {
        const NAMES: [&str; NUM_CHANNELS] = ["FM1", "FM2", "FM3", "FM4", "FM5", "FM6"];
        (0..NUM_CHANNELS)
            .map(|ch| ChannelDebugState {
                name: NAMES[ch],
                enabled: self.regs[0x20 + ch] & 0x10 != 0,
                length_counter: 0,
                waveform: self
                    .debug_capture
                    .as_ref()
                    .map(|c| c.read_buffer(ch))
                    .unwrap_or_else(|| vec![0.0; WAVEFORM_SIZE]),
            })
            .collect()
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.adr);
        w.write_bytes(&self.regs);
        for &n in &self.patch_num {
            w.write_u8(n);
        }
        w.write_u16(self.slot_key_status);
        w.write_u32(self.pm_phase);
        w.write_u32(self.am_phase);
        w.write_u8(self.lfo_am);
        w.write_u32(self.eg_counter);
        w.write_u8(self.divider);
        w.write_bool(self.muted);
        for &o in &self.ch_out {
            w.write_u16(o as u16);
        }
        w.write_u16(self.mix_out as u16);
        for slot in &self.slots {
            w.write_u8(slot.volume);
            w.write_u16(slot.fnum);
            w.write_u8(slot.blk);
            w.write_u16(slot.blk_fnum);
            w.write_bool(slot.sus_flag);
            w.write_bool(slot.key_flag);
            w.write_u32(slot.pg_phase);
            w.write_u16(slot.pg_out);
            w.write_u16(slot.output[0] as u16);
            w.write_u16(slot.output[1] as u16);
            w.write_u8(slot.eg_state.to_u8());
            w.write_u8(slot.eg_out);
        }
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.adr = r.read_u8()?;
        r.read_bytes_into(&mut self.regs)?;
        for n in &mut self.patch_num {
            *n = r.read_u8()?;
        }
        self.slot_key_status = r.read_u16()?;
        self.pm_phase = r.read_u32()?;
        self.am_phase = r.read_u32()?;
        self.lfo_am = r.read_u8()?;
        self.eg_counter = r.read_u32()?;
        self.divider = r.read_u8()?;
        self.muted = r.read_bool()?;
        for o in &mut self.ch_out {
            *o = r.read_u16()? as i16;
        }
        self.mix_out = r.read_u16()? as i16;
        for slot in &mut self.slots {
            slot.volume = r.read_u8()?;
            slot.fnum = r.read_u16()?;
            slot.blk = r.read_u8()?;
            slot.blk_fnum = r.read_u16()?;
            slot.sus_flag = r.read_bool()?;
            slot.key_flag = r.read_bool()?;
            slot.pg_phase = r.read_u32()?;
            slot.pg_out = r.read_u16()?;
            slot.output[0] = r.read_u16()? as i16;
            slot.output[1] = r.read_u16()? as i16;
            slot.eg_state = EgState::from_u8(r.read_u8()?);
            slot.eg_out = r.read_u8()?;
        }

        // Rebuild derived state (patches, rates, tables) from registers.
        self.custom_patch = [Patch::default(); 2];
        for reg in 0..8 {
            let data = self.regs[reg];
            let p = &mut self.custom_patch;
            match reg {
                0x00 | 0x01 => {
                    let p = &mut p[reg & 1];
                    p.am = data & 0x80 != 0;
                    p.pm = data & 0x40 != 0;
                    p.eg = data & 0x20 != 0;
                    p.kr = data & 0x10 != 0;
                    p.ml = data & 15;
                }
                0x02 => {
                    p[0].kl = (data >> 6) & 3;
                    p[0].tl = data & 63;
                }
                0x03 => {
                    p[1].kl = (data >> 6) & 3;
                    p[1].ws = (data >> 4) & 1;
                    p[0].ws = (data >> 3) & 1;
                    p[0].fb = data & 7;
                }
                0x04 | 0x05 => {
                    let p = &mut p[reg & 1];
                    p.ar = (data >> 4) & 15;
                    p.dr = data & 15;
                }
                _ => {
                    let p = &mut p[reg & 1];
                    p.sl = (data >> 4) & 15;
                    p.rr = data & 15;
                }
            }
        }
        for ch in 0..NUM_CHANNELS {
            let pair = self.resolve_patch(self.patch_num[ch]);
            self.slots[ch * 2].patch = pair[0];
            self.slots[ch * 2 + 1].patch = pair[1];
        }
        let t = tables();
        for slot in &mut self.slots {
            slot.update_requests = UPDATE_ALL;
            slot.commit_update(t);
        }
        Ok(())
    }
}

impl Default for Vrc7Audio {
    fn default() -> Self {
        Self::new()
    }
}

// The tests below act as a verifier for the FM chip: tables are checked
// against the values literally present in the reverse-engineered YM2413
// sources, and output behavior (pitch, envelope, tremolo, volume steps) is
// checked against datasheet math (F = 49716 * fnum * 2^block / 2^19, 3 dB
// per volume step, ~3.7 Hz tremolo).
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f64 = 49716.0;

    fn w(a: &mut Vrc7Audio, reg: u8, val: u8) {
        a.write_addr(reg);
        a.write_data(val);
    }

    // Custom patch: carrier = plain sustained sine (ML=1, EG hold, AR=15),
    // modulator fully attenuated (TL=63), no feedback, no LFOs.
    fn setup_sine(a: &mut Vrc7Audio) {
        w(a, 0x00, 0x01); // mod: ML=1
        w(a, 0x01, 0x21); // car: EG=1, ML=1
        w(a, 0x02, 0x3F); // mod TL=63 (max attenuation)
        w(a, 0x03, 0x00);
        w(a, 0x04, 0xF0); // mod AR=15
        w(a, 0x05, 0xF0); // car AR=15, DR=0
        w(a, 0x06, 0x0F); // mod SL=0, RR=15
        w(a, 0x07, 0x00); // car SL=0, RR=0
    }

    fn key_on(a: &mut Vrc7Audio, ch: u8, fnum: u16, block: u8) {
        w(a, 0x30 + ch, 0x00); // instrument 0 (custom), volume 0
        w(a, 0x10 + ch, (fnum & 0xFF) as u8);
        w(
            a,
            0x20 + ch,
            0x10 | ((block & 7) << 1) | ((fnum >> 8) & 1) as u8,
        );
    }

    fn key_off(a: &mut Vrc7Audio, ch: u8) {
        let cur = a.regs[0x20 + ch as usize];
        w(a, 0x20 + ch, cur & !0x10);
    }

    fn run_channel(a: &mut Vrc7Audio, ch: usize, samples: usize) -> Vec<i16> {
        let mut out = Vec::with_capacity(samples);
        for _ in 0..samples {
            for _ in 0..CPU_CYCLES_PER_SAMPLE {
                a.cpu_cycle();
            }
            out.push(a.channel_output(ch));
        }
        out
    }

    fn measure_freq_hz(samples: &[i16]) -> f64 {
        let mut crossings = 0u32;
        let mut first = None;
        let mut last = 0usize;
        for i in 1..samples.len() {
            if samples[i - 1] <= 0 && samples[i] > 0 {
                crossings += 1;
                if first.is_none() {
                    first = Some(i);
                }
                last = i;
            }
        }
        if crossings < 2 {
            return 0.0;
        }
        let span = (last - first.unwrap()) as f64 / SAMPLE_RATE;
        (crossings - 1) as f64 / span
    }

    fn peak(samples: &[i16]) -> i16 {
        samples.iter().map(|&s| s.abs() as i16).max().unwrap_or(0)
    }

    #[test]
    fn test_logsin_table_matches_hardware_dump() {
        let t = tables();
        // Spot values from the reverse-engineered YM2413 table (emu2413).
        assert_eq!(t.fullsin[0], 2137);
        assert_eq!(t.fullsin[1], 1731);
        assert_eq!(t.fullsin[2], 1543);
        assert_eq!(t.fullsin[15], 869);
        assert_eq!(t.fullsin[128], 127);
        assert_eq!(t.fullsin[255], 0);
        // Mirror and sign quadrants
        assert_eq!(t.fullsin[256], 0);
        assert_eq!(t.fullsin[511], 2137);
        assert_eq!(t.fullsin[512], 0x8000 | 2137);
        assert_eq!(t.fullsin[1023], 0x8000 | 2137);
        // Rectified wave: second half muted (positive zero)
        assert_eq!(t.halfsin[100], t.fullsin[100]);
        assert_eq!(t.halfsin[512], 0xfff);
        assert_eq!(t.halfsin[1023], 0xfff);
    }

    #[test]
    fn test_exp_table_matches_hardware_dump() {
        let t = tables();
        assert_eq!(t.exp[0], 0);
        assert_eq!(t.exp[1], 3);
        assert_eq!(t.exp[2], 6);
        assert_eq!(t.exp[128], 424);
        assert_eq!(t.exp[254], 1013);
        assert_eq!(t.exp[255], 1018);
    }

    #[test]
    fn test_exp_lookup_full_scale() {
        let t = tables();
        // Zero attenuation -> near max positive output
        assert_eq!(lookup_exp(t, 0), (1018 + 1024) << 1);
        // Sign bit -> negative
        assert!(lookup_exp(t, 0x8000) < 0);
        // Huge attenuation -> silence
        assert_eq!(lookup_exp(t, 0x1F00), 0);
    }

    #[test]
    fn test_output_frequency_middle_c() {
        // F = 49716 * 172 * 2^4 / 2^19 = 260.9 Hz
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        let samples = run_channel(&mut a, 0, 30000);
        let f = measure_freq_hz(&samples[2000..]);
        let expected = 49716.0 * 172.0 * 16.0 / 524288.0;
        assert!(
            (f - expected).abs() < expected * 0.01,
            "measured {f:.1} Hz, expected {expected:.1} Hz"
        );
    }

    #[test]
    fn test_output_frequency_a5() {
        // F = 49716 * 290 * 2^5 / 2^19 = 880.1 Hz
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 290, 5);
        let samples = run_channel(&mut a, 0, 30000);
        let f = measure_freq_hz(&samples[2000..]);
        let expected = 49716.0 * 290.0 * 32.0 / 524288.0;
        assert!(
            (f - expected).abs() < expected * 0.01,
            "measured {f:.1} Hz, expected {expected:.1} Hz"
        );
    }

    #[test]
    fn test_multiplier_doubles_frequency() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        w(&mut a, 0x01, 0x22); // car ML=2
        key_on(&mut a, 0, 172, 4);
        let samples = run_channel(&mut a, 0, 30000);
        let f = measure_freq_hz(&samples[2000..]);
        let expected = 2.0 * 49716.0 * 172.0 * 16.0 / 524288.0;
        assert!(
            (f - expected).abs() < expected * 0.01,
            "measured {f:.1} Hz, expected {expected:.1} Hz"
        );
    }

    #[test]
    fn test_sustained_tone_holds_amplitude() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        let samples = run_channel(&mut a, 0, 20000);
        let early = peak(&samples[2000..4000]);
        let late = peak(&samples[18000..20000]);
        assert!(early > 1000, "sine should be near full scale, got {early}");
        assert_eq!(early, late, "sustained tone must not decay");
    }

    #[test]
    fn test_key_off_releases_to_silence() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        w(&mut a, 0x07, 0x0F); // car SL=0, RR=15 (fast release)
        key_on(&mut a, 0, 172, 4);
        run_channel(&mut a, 0, 5000);
        key_off(&mut a, 0);
        let samples = run_channel(&mut a, 0, 2000);
        assert_eq!(peak(&samples[500..]), 0, "note must fade after key off");
    }

    #[test]
    fn test_slow_release_decays_gradually() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        w(&mut a, 0x07, 0x05); // car RR=5
        key_on(&mut a, 0, 172, 4);
        run_channel(&mut a, 0, 5000);
        key_off(&mut a, 0);
        let samples = run_channel(&mut a, 0, 30000);
        let p0 = peak(&samples[0..1000]);
        let p1 = peak(&samples[8000..9000]);
        let p2 = peak(&samples[25000..26000]);
        assert!(p0 > p1 && p1 > p2, "expected gradual decay: {p0} {p1} {p2}");
        assert!(p1 > 0, "release rate 5 should still sound after 160ms");
    }

    #[test]
    fn test_retrigger_damps_before_attack() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        run_channel(&mut a, 0, 5000);
        // Retrigger: key off, then immediately on again
        key_off(&mut a, 0);
        key_on(&mut a, 0, 172, 4);
        let samples = run_channel(&mut a, 0, 4000);
        // During damp (~5ms = ~250 samples) the old note fades out
        let damp = peak(&samples[100..250]);
        let after = peak(&samples[2000..4000]);
        assert!(
            damp < after,
            "damp phase ({damp}) should be quieter than the new note ({after})"
        );
        assert!(after > 1000, "new note should reach full scale");
    }

    #[test]
    fn test_volume_steps_3db() {
        // Each carrier volume step is 3 dB, so 4 steps = 12 dB = 4x amplitude.
        let mut amp = [0i16; 2];
        for (i, vol) in [0u8, 4].iter().enumerate() {
            let mut a = Vrc7Audio::new();
            setup_sine(&mut a);
            key_on(&mut a, 0, 172, 4);
            w(&mut a, 0x30, *vol); // instrument 0, volume
            let samples = run_channel(&mut a, 0, 10000);
            amp[i] = peak(&samples[5000..]);
        }
        let ratio = amp[0] as f64 / amp[1].max(1) as f64;
        assert!(
            (3.0..5.7).contains(&ratio),
            "12 dB should be ~4x amplitude, got {ratio:.2} ({} vs {})",
            amp[0],
            amp[1]
        );
    }

    #[test]
    fn test_tremolo_modulates_amplitude() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        w(&mut a, 0x01, 0xA1); // car: AM=1, EG=1, ML=1
        key_on(&mut a, 0, 172, 4);
        // AM LFO period = 210*64 = 13440 samples (~3.7 Hz). Track amplitude
        // over one period: max/min peaks differ by ~4.9 dB.
        run_channel(&mut a, 0, 1000);
        let samples = run_channel(&mut a, 0, 14000);
        let window = 500;
        let mut peaks = vec![];
        for chunk in samples.chunks(window) {
            peaks.push(peak(chunk) as f64);
        }
        let hi = peaks.iter().cloned().fold(0.0, f64::max);
        let lo = peaks.iter().cloned().fold(f64::MAX, f64::min);
        let db = 20.0 * (hi / lo.max(1.0)).log10();
        assert!(
            (3.0..7.0).contains(&db),
            "tremolo depth should be ~4.9 dB, got {db:.2} dB (hi {hi} lo {lo})"
        );
    }

    #[test]
    fn test_all_instruments_produce_sound() {
        for inst in 1..=15u8 {
            let mut a = Vrc7Audio::new();
            w(&mut a, 0x30, inst << 4);
            w(&mut a, 0x10, 172);
            w(&mut a, 0x20, 0x10 | (4 << 1));
            let samples = run_channel(&mut a, 0, 8000);
            assert!(
                peak(&samples) > 100,
                "instrument {inst} should produce sound"
            );
        }
    }

    #[test]
    fn test_channels_are_independent() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        key_on(&mut a, 2, 290, 5);
        for _ in 0..5000 * CPU_CYCLES_PER_SAMPLE as usize {
            a.cpu_cycle();
        }
        assert!(a.channel_output(0).abs() <= 2048);
        let s0 = run_channel(&mut a, 0, 10000);
        key_off(&mut a, 0);
        w(&mut a, 0x07, 0x0F);
        let s5 = run_channel(&mut a, 5, 1000);
        assert!(peak(&s0) > 1000, "channel 0 should sound");
        assert_eq!(peak(&s5), 0, "channel 5 was never keyed");
    }

    #[test]
    fn test_halt_mutes_and_blocks_writes() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        run_channel(&mut a, 0, 5000);
        assert!(a.output() != 0.0 || a.channel_output(0) != 0);

        a.set_halt(true);
        assert_eq!(a.output(), 0.0);
        assert_eq!(a.am_phase, 0, "halt resets the tremolo LFO phase");
        // Writes are disregarded while halted
        w(&mut a, 0x30, 0x0F);
        assert_eq!(a.regs[0x30], 0x00);

        a.set_halt(false);
        let samples = run_channel(&mut a, 0, 5000);
        assert!(peak(&samples[1000..]) > 1000, "tone resumes after unhalt");
    }

    #[test]
    fn test_output_scale_bounded() {
        // All six channels at max volume must stay within a sane mix range.
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        for ch in 0..6 {
            key_on(&mut a, ch, 172 + ch as u16 * 7, 4);
        }
        let mut max_out = 0.0f32;
        for _ in 0..20000 * CPU_CYCLES_PER_SAMPLE as usize {
            a.cpu_cycle();
            max_out = max_out.max(a.output().abs());
        }
        assert!(max_out > 0.1, "six channels should be clearly audible");
        assert!(max_out < 2.5, "mix must not exceed raw OPLL range");
    }

    #[test]
    fn test_register_mirrors() {
        let mut a = Vrc7Audio::new();
        // $19 mirrors $10
        w(&mut a, 0x19, 0xAB);
        assert_eq!(a.regs[0x10], 0xAB);
        w(&mut a, 0x2A, 0x15);
        assert_eq!(a.regs[0x21], 0x15);
    }

    #[test]
    fn test_savestate_roundtrip_bit_exact() {
        let mut a = Vrc7Audio::new();
        setup_sine(&mut a);
        key_on(&mut a, 0, 172, 4);
        w(&mut a, 0x31, 0x35); // ch1: instrument 3, volume 5
        w(&mut a, 0x11, 0x80);
        w(&mut a, 0x21, 0x1A);
        for _ in 0..12345 {
            a.cpu_cycle();
        }

        let mut wtr = SavestateWriter::new();
        a.save_state(&mut wtr);
        let data = wtr.finish();

        let mut b = Vrc7Audio::new();
        let mut rdr = SavestateReader::new(&data).unwrap();
        b.load_state(&mut rdr).unwrap();

        for i in 0..100000 {
            a.cpu_cycle();
            b.cpu_cycle();
            assert_eq!(
                a.mix_out, b.mix_out,
                "mix output diverged at cycle {i} after savestate load"
            );
        }
    }
}
