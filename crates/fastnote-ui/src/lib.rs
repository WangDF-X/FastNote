//! Slint-based user interface for FastNote.

slint::include_modules!();

/// Starts the main window and forwards navigation actions to the application controller.
///
/// The handler returns the page label that the UI should display after each action.
pub fn run(
    initial_page: String,
    mut handle_action: impl FnMut(String) -> String + 'static,
) -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.set_current_page(initial_page.into());

    let weak_window = window.as_weak();
    window.on_route_action(move |action| {
        let current_page = handle_action(action.to_string());
        if let Some(window) = weak_window.upgrade() {
            window.set_current_page(current_page.into());
        }
    });

    window.run()
}
