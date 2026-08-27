pub mod app;
pub mod document;
pub mod editor;
pub mod effects;
pub mod io;
mod shortcut_dispatcher;
pub mod tools;
pub mod ui;

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WINDOW_SIZE: [f32; 2] = [1100.0, 800.0];
#[cfg(not(target_arch = "wasm32"))]
const MINIMUM_WINDOW_SIZE: [f32; 2] = [720.0, 540.0];

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    if std::env::args_os().any(|argument| argument == "--version" || argument == "-V") {
        println!("pixelbuddy {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    env_logger::init();

    let icon_result = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"));
    if let Err(e) = &icon_result {
        println!("Failed to load icon: {e:?}");
    }
    let icon = icon_result.ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(DEFAULT_WINDOW_SIZE)
        .with_min_inner_size(MINIMUM_WINDOW_SIZE)
        .with_title("PixelBuddy")
        .with_decorations(false);

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
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::PixelBuddyApp::from_creation_context(
                cc, 64, 64,
            )))
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
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    Ok(Box::new(app::PixelBuddyApp::from_creation_context(
                        cc, 64, 64,
                    )))
                }),
            )
            .await;

        // Log errors if WebRunner failed
        if let Err(e) = start_result {
            log::error!("Failed to start eframe WebRunner: {e:?}");
        }
    });
}
