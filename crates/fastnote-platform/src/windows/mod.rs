mod caption;
mod error;
mod hotkey;
mod monitors;
mod resources;
mod theme;
mod tray;
mod window;

pub use caption::configure_custom_caption;
pub use error::PlatformError;
pub use hotkey::WindowsHotkey;
pub use monitors::{monitor_at_point, monitors};
pub use resources::{
    process_resources, uptime_milliseconds, window_belongs_to_process, ProcessResources,
};
pub use theme::{system_theme_is_dark, WindowsThemeMonitor};
pub use tray::{install_event_handlers, WindowsTray};
pub use window::{
    capture_frameless_geometry, capture_geometry, configure_ball_window, configure_menu_window,
    cursor_position, set_window_big_icon, NativeWindowHandle,
};
