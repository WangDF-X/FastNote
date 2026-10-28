mod controller;

use controller::AppController;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = AppController::default();
    let initial_page = controller.current_page().to_owned();

    fastnote_ui::run(initial_page, move |action| {
        controller.handle_action(&action).to_owned()
    })?;

    Ok(())
}
