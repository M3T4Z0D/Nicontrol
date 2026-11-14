mod gui;
mod hardware;
mod i18n;
mod tray;
mod updater;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--tray") {
        tray::run_tray();
    } else {
        gui::run_gui();
    }
}
