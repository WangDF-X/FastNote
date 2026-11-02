mod error;
mod hotkey;
mod monitors;
mod tray;
mod window;

pub use error::PlatformError;
pub use hotkey::WindowsHotkey;
pub use monitors::{monitor_at_point, monitors};
pub use tray::{install_event_handlers, WindowsTray};
pub use window::{
    capture_geometry, configure_ball_window, configure_menu_window, cursor_position,
    NativeWindowHandle,
};
