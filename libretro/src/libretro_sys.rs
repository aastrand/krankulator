use std::os::raw::{c_char, c_uint, c_void};

pub const RETRO_API_VERSION: c_uint = 1;

pub const RETRO_REGION_NTSC: c_uint = 0;

pub const RETRO_MEMORY_SAVE_RAM: c_uint = 0;

pub const RETRO_DEVICE_JOYPAD: c_uint = 1;

pub const RETRO_DEVICE_ID_JOYPAD_B: c_uint = 0;
#[allow(dead_code)]
pub const RETRO_DEVICE_ID_JOYPAD_Y: c_uint = 1;
pub const RETRO_DEVICE_ID_JOYPAD_SELECT: c_uint = 2;
pub const RETRO_DEVICE_ID_JOYPAD_START: c_uint = 3;
pub const RETRO_DEVICE_ID_JOYPAD_UP: c_uint = 4;
pub const RETRO_DEVICE_ID_JOYPAD_DOWN: c_uint = 5;
pub const RETRO_DEVICE_ID_JOYPAD_LEFT: c_uint = 6;
pub const RETRO_DEVICE_ID_JOYPAD_RIGHT: c_uint = 7;
pub const RETRO_DEVICE_ID_JOYPAD_A: c_uint = 8;
pub const RETRO_DEVICE_ID_JOYPAD_MASK: c_uint = 256;

pub const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
pub const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: c_uint = 11;
pub const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: c_uint = 27;
pub const RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: c_uint = 51 | 0x10000;

pub const RETRO_PIXEL_FORMAT_XRGB8888: c_uint = 1;

pub const RETRO_LOG_INFO: c_uint = 1;

pub type RetroEnvironmentT = unsafe extern "C" fn(cmd: c_uint, data: *mut c_void) -> bool;
pub type RetroVideoRefreshT =
    unsafe extern "C" fn(data: *const c_void, width: c_uint, height: c_uint, pitch: usize);
pub type RetroAudioSampleT = unsafe extern "C" fn(left: i16, right: i16);
pub type RetroAudioSampleBatchT = unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
pub type RetroInputPollT = unsafe extern "C" fn();
pub type RetroInputStateT =
    unsafe extern "C" fn(port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16;
pub type RetroLogPrintfT = unsafe extern "C" fn(level: c_uint, fmt: *const c_char, ...);

#[repr(C)]
pub struct RetroSystemInfo {
    pub library_name: *const c_char,
    pub library_version: *const c_char,
    pub valid_extensions: *const c_char,
    pub need_fullpath: bool,
    pub block_extract: bool,
}

#[repr(C)]
pub struct RetroGameGeometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

#[repr(C)]
pub struct RetroSystemTiming {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
pub struct RetroSystemAvInfo {
    pub geometry: RetroGameGeometry,
    pub timing: RetroSystemTiming,
}

#[repr(C)]
pub struct RetroGameInfo {
    pub path: *const c_char,
    pub data: *const c_void,
    pub size: usize,
    pub meta: *const c_char,
}

#[repr(C)]
pub struct RetroLogCallback {
    pub log: RetroLogPrintfT,
}

#[repr(C)]
pub struct RetroInputDescriptor {
    pub port: c_uint,
    pub device: c_uint,
    pub index: c_uint,
    pub id: c_uint,
    pub description: *const c_char,
}
