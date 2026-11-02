mod clock;
mod controller;
mod routing;

use clock::SystemClock;
use controller::AppController;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = AppController::new(SystemClock);
    let initial_view = controller.shell_view();

    fastnote_ui::run(initial_view, move |action| {
        controller.handle_action(&action)
    })?;

    Ok(())
}
