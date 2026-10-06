//! fidom interactive shell (eframe). Native window + wasm canvas targets.
//! See gui::app for the app itself.

mod app;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        eframe::WebRunner::new()
            .start(canvas, web_options, Box::new(|_cc| Ok(Box::new(app::Shell::new(None)))))
            .await
            .expect("failed to start eframe");
    });
}
