//! Platform integrations for FastNote.

mod events;
#[cfg(windows)]
mod windows;

pub use events::{EntryAction, NativeEvent};
#[cfg(windows)]
pub use windows::{
    capture_frameless_geometry, capture_geometry, configure_ball_window, configure_custom_caption,
    configure_menu_window, cursor_position, install_event_handlers, monitor_at_point, monitors,
    set_window_big_icon, system_theme_is_dark, NativeWindowHandle, PlatformError, WindowsHotkey,
    WindowsThemeMonitor, WindowsTray,
};
