mod clock;
mod controller;
mod desktop;
mod journal;
mod logging;
mod notes;
mod routing;
mod tasks;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hidden = std::env::args()
        .skip(1)
        .any(|argument| argument == "--hidden");
    let _logs = logging::start();
    desktop::run(hidden)
}
