mod common;

#[cfg(not(target_os = "linux"))]
mod winit;
#[cfg(not(target_os = "linux"))]
pub use self::winit::DebugUi;

#[cfg(target_os = "linux")]
mod gtk;
#[cfg(target_os = "linux")]
pub use self::gtk::DebugUi;

pub use common::PANEL_WIDTH;
