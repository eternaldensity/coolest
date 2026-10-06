//! fidom interactive shell (eframe). See gui::app.

mod app;

fn main() {
    let frames = std::env::args()
        .skip_while(|a| a != "--frames")
        .nth(1)
        .and_then(|n| n.parse::<u32>().ok());
    let opts = eframe::NativeOptions::default();
    if let Err(e) = eframe::run_native(
        "FIDOM",
        opts,
        Box::new(move |_cc| Ok(Box::new(app::Shell::new(frames)))),
    ) {
        eprintln!("shell exited: {e:?}");
        std::process::exit(1);
    }
}
