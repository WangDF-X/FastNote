//! Platform integrations for FastNote.

mod events;
#[cfg(windows)]
mod windows;

pub use events::{EntryAction, NativeEvent};
#[cfg(windows)]
pub use windows::{
    capture_geometry, configure_ball_window, configure_menu_window, cursor_position,
    install_event_handlers, monitor_at_point, monitors, NativeWindowHandle, PlatformError,
    WindowsHotkey, WindowsTray,
};
