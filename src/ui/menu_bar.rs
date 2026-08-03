use crate::app::PixelBuddyApp;
use crate::io;


pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    app.show_new_dialog = true;
                    ui.close_menu();
                }
                if ui.button("Open").clicked() {
                    io::trigger_open_file(app.io_handler.sender.clone());
                    ui.close_menu();
                }
                if ui.button("Save as PNG").clicked() {
                    // Export is effectively Save as PNG for now
                    if let Some(png_data) = io::png::export_document_to_png(&app.editor.document) {
                        io::trigger_export_png(png_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
                if ui.button("Export").clicked() {
                    if let Some(png_data) = io::png::export_document_to_png(&app.editor.document) {
                        io::trigger_export_png(png_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
            });
            
            ui.menu_button("Edit", |ui| {
                if ui.button("Undo (Ctrl+Z)").clicked() {
                    app.editor.history.undo(&mut app.editor.document);
                    app.texture_dirty = true;
                    ui.close_menu();
                }
                if ui.button("Redo (Ctrl+Y)").clicked() {
                    app.editor.history.redo(&mut app.editor.document);
                    app.texture_dirty = true;
                    ui.close_menu();
                }
                if ui.button("Swap Colors (X)").clicked() {
                    app.editor.swap_colors();
                    ui.close_menu();
                }
            });
            
            ui.menu_button("View", |ui| {
                if ui.checkbox(&mut app.show_grid, "Toggle Grid").clicked() {
                    ui.close_menu();
                }
                if ui.button("Zoom In").clicked() {
                    app.zoom *= 2.0;
                    if app.zoom > 64.0 { app.zoom = 64.0; }
                    ui.close_menu();
                }
                if ui.button("Zoom Out").clicked() {
                    app.zoom *= 0.5;
                    if app.zoom < 0.5 { app.zoom = 0.5; }
                    ui.close_menu();
                }
                if ui.button("Fit to Window").clicked() {
                    // Handled generally by panning to center
                    app.pan_offset = egui::Vec2::ZERO;
                    app.zoom = 1.0;
                    ui.close_menu();
                }
            });
        });
    });
}
