pub mod emu;
pub mod util;

#[macro_export]
macro_rules! test_input {
    ($path:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../input/", $path)
    };
}

#[macro_export]
macro_rules! test_rom {
    ($path:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../test-roms/", $path)
    };
}
