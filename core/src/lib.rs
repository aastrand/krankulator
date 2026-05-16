pub mod emu;
pub mod util;

#[macro_export]
macro_rules! test_input {
    ($path:expr) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../input/", $path)
    };
}
