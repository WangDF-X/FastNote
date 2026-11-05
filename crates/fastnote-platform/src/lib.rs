//! Platform integrations for FastNote.

mod events;
#[cfg(windows)]
mod windows;

pub use events::{EntryAction, NativeEvent};
#[cfg(windows)]
pub use windows::{
    capture_frameless_geometry, capture_geometry, configure_ball_window, configure_custom_caption,
    configure_menu_window, cursor_position, install_event_handlers, monitor_at_point, monitors,
    process_resources, set_window_big_icon, system_theme_is_dark, uptime_milliseconds,
    window_belongs_to_process, NativeWindowHandle, PlatformError, ProcessResources, WindowsHotkey,
    WindowsThemeMonitor, WindowsTray,
};
