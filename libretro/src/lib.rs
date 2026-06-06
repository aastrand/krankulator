#![allow(non_upper_case_globals, static_mut_refs, clippy::missing_safety_doc)]

mod libretro_sys;

use krankulator_core::emu;
use krankulator_core::emu::audio::AudioBackend;
use krankulator_core::emu::io::controller;
use krankulator_core::emu::io::loader;
use krankulator_core::emu::io::PollResult;
use krankulator_core::emu::memory::MemoryMapper;
use libretro_sys::*;
use std::cell::RefCell;
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr;
use std::rc::Rc;

const NES_WIDTH: c_uint = 256;
const NES_HEIGHT: c_uint = 240;
const NTSC_FPS: f64 = 60.0988;
const PAL_FPS: f64 = 50.0070;
const SAMPLE_RATE: f64 = 44100.0;
const SERIALIZE_MAX_SIZE: usize = 256 * 1024;

static LIBRARY_NAME: &[u8] = b"Krankulator\0";
static LIBRARY_VERSION: &[u8] = b"1.0.0\0";
static VALID_EXTENSIONS: &[u8] = b"nes\0";

static mut environment_cb: Option<RetroEnvironmentT> = None;
static mut video_refresh_cb: Option<RetroVideoRefreshT> = None;
static mut audio_sample_cb: Option<RetroAudioSampleT> = None;
static mut audio_sample_batch_cb: Option<RetroAudioSampleBatchT> = None;
static mut input_poll_cb: Option<RetroInputPollT> = None;
static mut input_state_cb: Option<RetroInputStateT> = None;
static mut log_cb: Option<RetroLogPrintfT> = None;
static mut support_bitmasks: bool = false;

struct RetroState {
    emulator: emu::Emulator,
    frame_buf: Rc<RefCell<Vec<u32>>>,
    audio_buf: Rc<RefCell<Vec<i16>>>,
    has_battery: bool,
    region: emu::Region,
}

static mut STATE: Option<RetroState> = None;

// --- IOHandler ---

struct LibretroIOHandler {
    frame_buf: Rc<RefCell<Vec<u32>>>,
}

impl emu::io::IOHandler for LibretroIOHandler {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        unsafe {
            if let Some(log) = log_cb {
                let msg = format!("{logline}\n\0");
                let fmt = c"%s".as_ptr() as *const c_char;
                log(RETRO_LOG_INFO, fmt, msg.as_ptr() as *const c_char);
            }
        }
    }

    fn poll(&mut self, _mem: &mut dyn MemoryMapper, _apu: &mut emu::apu::APU) -> PollResult {
        PollResult::default()
    }

    fn render(&mut self, buf: &emu::gfx::buf::Buffer) {
        let mut fb = self.frame_buf.borrow_mut();
        let src = &buf.data;
        for i in 0..(NES_WIDTH * NES_HEIGHT) as usize {
            let off = i * 3;
            let r = src[off] as u32;
            let g = src[off + 1] as u32;
            let b = src[off + 2] as u32;
            fb[i] = (r << 16) | (g << 8) | b;
        }
    }

    fn exit(&self, _s: String) {}
}

// --- AudioBackend ---

struct LibretroAudioBackend {
    buf: Rc<RefCell<Vec<i16>>>,
}

impl AudioBackend for LibretroAudioBackend {
    fn push_samples(&mut self, samples: &[f32]) {
        let mut buf = self.buf.borrow_mut();
        for &s in samples {
            let sample = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
            buf.push(sample); // left
            buf.push(sample); // right (mono → stereo)
        }
    }

    fn clear(&mut self) {
        self.buf.borrow_mut().clear();
    }
}

// --- Input helpers ---

const BUTTON_MAP: [(c_uint, u8); 8] = [
    (RETRO_DEVICE_ID_JOYPAD_A, controller::A),
    (RETRO_DEVICE_ID_JOYPAD_B, controller::B),
    (RETRO_DEVICE_ID_JOYPAD_SELECT, controller::SELECT),
    (RETRO_DEVICE_ID_JOYPAD_START, controller::START),
    (RETRO_DEVICE_ID_JOYPAD_UP, controller::UP),
    (RETRO_DEVICE_ID_JOYPAD_DOWN, controller::DOWN),
    (RETRO_DEVICE_ID_JOYPAD_LEFT, controller::LEFT),
    (RETRO_DEVICE_ID_JOYPAD_RIGHT, controller::RIGHT),
];

unsafe fn poll_port(port: c_uint) -> u8 {
    let input_state = input_state_cb.unwrap();

    if support_bitmasks {
        let mask = input_state(port, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_MASK) as u32;
        let mut state: u8 = 0;
        for &(retro_id, nes_btn) in &BUTTON_MAP {
            if mask & (1 << retro_id) != 0 {
                state |= nes_btn;
            }
        }
        state
    } else {
        let mut state: u8 = 0;
        for &(retro_id, nes_btn) in &BUTTON_MAP {
            if input_state(port, RETRO_DEVICE_JOYPAD, 0, retro_id) != 0 {
                state |= nes_btn;
            }
        }
        state
    }
}

// --- Input descriptors ---

fn set_input_descriptors() {
    let descriptors: &[RetroInputDescriptor] = &[
        desc(0, RETRO_DEVICE_ID_JOYPAD_A, b"A\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_B, b"B\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_SELECT, b"Select\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_START, b"Start\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_UP, b"Up\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_DOWN, b"Down\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_LEFT, b"Left\0"),
        desc(0, RETRO_DEVICE_ID_JOYPAD_RIGHT, b"Right\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_A, b"A\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_B, b"B\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_SELECT, b"Select\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_START, b"Start\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_UP, b"Up\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_DOWN, b"Down\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_LEFT, b"Left\0"),
        desc(1, RETRO_DEVICE_ID_JOYPAD_RIGHT, b"Right\0"),
        RetroInputDescriptor {
            port: 0,
            device: 0,
            index: 0,
            id: 0,
            description: ptr::null(),
        },
    ];
    unsafe {
        if let Some(env) = environment_cb {
            env(
                RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS,
                descriptors.as_ptr() as *mut c_void,
            );
        }
    }
}

const fn desc(port: c_uint, id: c_uint, name: &'static [u8]) -> RetroInputDescriptor {
    RetroInputDescriptor {
        port,
        device: RETRO_DEVICE_JOYPAD,
        index: 0,
        id,
        description: name.as_ptr() as *const c_char,
    }
}

// --- Libretro API exports ---

#[no_mangle]
pub unsafe extern "C" fn retro_api_version() -> c_uint {
    RETRO_API_VERSION
}

#[no_mangle]
pub unsafe extern "C" fn retro_get_system_info(info: *mut RetroSystemInfo) {
    (*info).library_name = LIBRARY_NAME.as_ptr() as *const c_char;
    (*info).library_version = LIBRARY_VERSION.as_ptr() as *const c_char;
    (*info).valid_extensions = VALID_EXTENSIONS.as_ptr() as *const c_char;
    (*info).need_fullpath = false;
    (*info).block_extract = false;
}

#[no_mangle]
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut RetroSystemAvInfo) {
    (*info).geometry = RetroGameGeometry {
        base_width: NES_WIDTH,
        base_height: NES_HEIGHT,
        max_width: NES_WIDTH,
        max_height: NES_HEIGHT,
        aspect_ratio: 4.0 / 3.0,
    };
    let fps = match STATE.as_ref().map(|s| s.region) {
        Some(emu::Region::Pal) => PAL_FPS,
        _ => NTSC_FPS,
    };
    (*info).timing = RetroSystemTiming {
        fps,
        sample_rate: SAMPLE_RATE,
    };
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_environment(cb: RetroEnvironmentT) {
    environment_cb = Some(cb);

    let mut format = RETRO_PIXEL_FORMAT_XRGB8888;
    cb(
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
        &mut format as *mut c_uint as *mut c_void,
    );

    let mut log_callback = std::mem::MaybeUninit::<RetroLogCallback>::uninit();
    if cb(
        RETRO_ENVIRONMENT_GET_LOG_INTERFACE,
        log_callback.as_mut_ptr() as *mut c_void,
    ) {
        log_cb = Some(log_callback.assume_init().log);
    }

    let mut bitmask_supported: bool = false;
    if cb(
        RETRO_ENVIRONMENT_GET_INPUT_BITMASKS,
        &mut bitmask_supported as *mut bool as *mut c_void,
    ) {
        support_bitmasks = bitmask_supported;
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_video_refresh(cb: RetroVideoRefreshT) {
    video_refresh_cb = Some(cb);
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_audio_sample(cb: RetroAudioSampleT) {
    audio_sample_cb = Some(cb);
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_audio_sample_batch(cb: RetroAudioSampleBatchT) {
    audio_sample_batch_cb = Some(cb);
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_input_poll(cb: RetroInputPollT) {
    input_poll_cb = Some(cb);
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_input_state(cb: RetroInputStateT) {
    input_state_cb = Some(cb);
}

#[no_mangle]
pub unsafe extern "C" fn retro_init() {}

#[no_mangle]
pub unsafe extern "C" fn retro_deinit() {
    STATE = None;
}

#[no_mangle]
pub unsafe extern "C" fn retro_load_game(game: *const RetroGameInfo) -> bool {
    if game.is_null() || (*game).data.is_null() || (*game).size == 0 {
        return false;
    }

    let rom_data = std::slice::from_raw_parts((*game).data as *const u8, (*game).size);

    let path = if (*game).path.is_null() {
        None
    } else {
        std::ffi::CStr::from_ptr((*game).path).to_str().ok()
    };
    let filename = path.and_then(|p| p.rsplit(&['/', '\\'][..]).next());
    let region = loader::detect_region_with_filename(rom_data, filename);

    let has_battery = loader::rom_has_battery(rom_data);

    let mapper = match loader::load_nes_from_bytes(rom_data) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let frame_buf = Rc::new(RefCell::new(vec![0u32; (NES_WIDTH * NES_HEIGHT) as usize]));
    let audio_buf = Rc::new(RefCell::new(Vec::with_capacity(2048)));

    let io: Box<dyn emu::io::IOHandler> = Box::new(LibretroIOHandler {
        frame_buf: Rc::clone(&frame_buf),
    });
    let audio: Box<dyn AudioBackend> = Box::new(LibretroAudioBackend {
        buf: Rc::clone(&audio_buf),
    });

    let mut emulator = emu::Emulator::new_with_region(io, mapper, audio, region);
    emulator.cpu.status = 0x34;
    emulator.cpu.sp = 0xfd;
    emulator.toggle_should_trigger_nmi(true);
    emulator.toggle_should_exit_on_infinite_loop(false);

    STATE = Some(RetroState {
        emulator,
        region,
        frame_buf,
        audio_buf,
        has_battery,
    });

    set_input_descriptors();

    true
}

#[no_mangle]
pub unsafe extern "C" fn retro_load_game_special(
    _game_type: c_uint,
    _info: *const RetroGameInfo,
    _num_info: usize,
) -> bool {
    false
}

#[no_mangle]
pub unsafe extern "C" fn retro_unload_game() {
    STATE = None;
}

#[no_mangle]
pub unsafe extern "C" fn retro_run() {
    let state = match STATE.as_mut() {
        Some(s) => s,
        None => return,
    };

    (input_poll_cb.unwrap())();

    let p1 = poll_port(0);
    let p2 = poll_port(1);
    state.emulator.mem.controllers()[0].load_status(p1);
    state.emulator.mem.controllers()[1].load_status(p2);

    state.emulator.run_one_frame();

    let fb = state.frame_buf.borrow();
    (video_refresh_cb.unwrap())(
        fb.as_ptr() as *const c_void,
        NES_WIDTH,
        NES_HEIGHT,
        (NES_WIDTH as usize) * 4,
    );

    let mut ab = state.audio_buf.borrow_mut();
    if !ab.is_empty() {
        let frames = ab.len() / 2;
        (audio_sample_batch_cb.unwrap())(ab.as_ptr(), frames);
        ab.clear();
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_reset() {
    if let Some(state) = STATE.as_mut() {
        state.emulator.reset();
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_serialize_size() -> usize {
    SERIALIZE_MAX_SIZE
}

#[no_mangle]
pub unsafe extern "C" fn retro_serialize(data: *mut c_void, size: usize) -> bool {
    let state = match STATE.as_ref() {
        Some(s) => s,
        None => return false,
    };
    let bytes = state.emulator.save_state_to_bytes();
    if bytes.len() > size {
        return false;
    }
    let dest = data as *mut u8;
    ptr::copy_nonoverlapping(bytes.as_ptr(), dest, bytes.len());
    ptr::write_bytes(dest.add(bytes.len()), 0, size - bytes.len());
    true
}

#[no_mangle]
pub unsafe extern "C" fn retro_unserialize(data: *const c_void, size: usize) -> bool {
    let state = match STATE.as_mut() {
        Some(s) => s,
        None => return false,
    };
    if size > SERIALIZE_MAX_SIZE {
        return false;
    }
    let bytes = std::slice::from_raw_parts(data as *const u8, size);
    state.emulator.load_state_from_bytes(bytes).is_ok()
}

#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_data(id: c_uint) -> *mut c_void {
    if id != RETRO_MEMORY_SAVE_RAM {
        return ptr::null_mut();
    }
    let state = match STATE.as_mut() {
        Some(s) => s,
        None => return ptr::null_mut(),
    };
    if !state.has_battery {
        return ptr::null_mut();
    }
    match state.emulator.mem.sram_data_mut() {
        Some(data) => data.as_mut_ptr() as *mut c_void,
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_size(id: c_uint) -> usize {
    if id != RETRO_MEMORY_SAVE_RAM {
        return 0;
    }
    let state = match STATE.as_ref() {
        Some(s) => s,
        None => return 0,
    };
    if !state.has_battery {
        return 0;
    }
    match state.emulator.mem.sram_data() {
        Some(data) => data.len(),
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_get_region() -> c_uint {
    match STATE.as_ref().map(|s| s.region) {
        Some(emu::Region::Pal) => RETRO_REGION_PAL,
        _ => RETRO_REGION_NTSC,
    }
}

#[no_mangle]
pub unsafe extern "C" fn retro_set_controller_port_device(_port: c_uint, _device: c_uint) {}

#[no_mangle]
pub unsafe extern "C" fn retro_cheat_reset() {}

#[no_mangle]
pub unsafe extern "C" fn retro_cheat_set(_index: c_uint, _enabled: bool, _code: *const c_char) {}
