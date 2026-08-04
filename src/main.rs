pub mod app;
pub mod document;
pub mod editor;
pub mod tools;
pub mod ui;
pub mod io;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_title("PixelBuddy");

    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    
    eframe::run_native(
        "PixelBuddy",
        options,
        Box::new(|cc| {
            ui::theme::setup_theme(&cc.egui_ctx);
            Ok(Box::new(app::PixelBuddyApp::new(64, 64)))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
fn main() {
    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    ui::theme::setup_theme(&cc.egui_ctx);
                    Ok(Box::new(app::PixelBuddyApp::new(64, 64)))
                }),
            )
            .await;
            
        // Log errors if WebRunner failed
        if let Err(e) = start_result {
            log::error!("Failed to start eframe WebRunner: {:?}", e);
        }
    });
}
